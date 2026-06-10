#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

out_dir="${OUT_DIR:-$root/dist}"
packages=(failsafe failsafe-server)

default_targets=(
    x86_64-unknown-linux-gnu
    aarch64-apple-darwin
    x86_64-pc-windows-gnu
)

if (($# > 0)); then
    targets=("$@")
else
    targets=("${default_targets[@]}")
fi

if ! command -v cross >/dev/null 2>&1; then
    echo "error: cross is not installed (cargo install cross)" >&2
    exit 1
fi

darwin_image_available() {
    local target="$1"
    local image

    image="$(awk -v t="$target" '
        $0 ~ "^\\[target\\." t "\\]" { in_target=1; next }
        /^\[/ { in_target=0 }
        in_target && /^image[[:space:]]*=/ {
            sub(/^image[[:space:]]*=[[:space:]]*"/, "")
            sub(/".*$/, "")
            print
            exit
        }
    ' "$root/Cross.toml")"
    if [[ -n "$image" ]]; then
        return 0
    fi

    for image in "${target}-cross" "${target}-cross:local"; do
        if docker image inspect "$image" >/dev/null 2>&1; then
            return 0
        fi
    done

    return 1
}

for target in "${targets[@]}"; do
    if [[ "$target" == *-apple-darwin ]]; then
        if ! darwin_image_available "$target"; then
            cat >&2 <<EOF
error: no cross image for $target

cross does not ship macOS images. Build one locally with cross-toolchains, then set it in Cross.toml:

  [target.$target]
  image = "${target}-cross:local"

See https://github.com/cross-rs/cross-toolchains#apple-targets
EOF
            exit 1
        fi
    fi
done

if [[ -f failsafe-web-ui/dist/index.html ]]; then
    echo "==> web UI dist present, skipping npm build"
else
    echo "==> building web UI"
    npm --prefix failsafe-web-ui ci || npm --prefix failsafe-web-ui install
    npm --prefix failsafe-web-ui run build
fi

export FAILSAFE_SKIP_WEB_BUILD=1

# Host-built artifacts in target/{debug,release} use the local glibc and break cross containers.
rm -rf target/debug target/release

mkdir -p "$out_dir"

cargo_config="${HOME}/.cargo/config.toml"
cargo_config_backup=""

isolate_host_cargo_config() {
    if [[ ! -f "$cargo_config" ]]; then
        return
    fi

    cargo_config_backup="$(mktemp)"
    mv "$cargo_config" "$cargo_config_backup"
}

restore_host_cargo_config() {
    if [[ -n "$cargo_config_backup" && -f "$cargo_config_backup" ]]; then
        mv "$cargo_config_backup" "$cargo_config"
    fi
}

trap restore_host_cargo_config EXIT INT TERM
isolate_host_cargo_config

for target in "${targets[@]}"; do
    echo "==> cross build ($target)"
    for package in "${packages[@]}"; do
        cross build --release --target "$target" -p "$package"
    done

    dest="$out_dir/$target"
    mkdir -p "$dest"

    for package in "${packages[@]}"; do
        suffix=""
        if [[ "$target" == *-windows-* ]]; then
            suffix=".exe"
        fi

        artifact=""
        for candidate in \
            "$root/target/$target/release/$package$suffix" \
            "$root/target/release/$package$suffix"; do
            if [[ -f "$candidate" ]]; then
                artifact="$candidate"
                break
            fi
        done

        if [[ -z "$artifact" ]]; then
            echo "error: missing $package binary for $target" >&2
            exit 1
        fi

        cp "$artifact" "$dest/"
    done

    echo "    -> $dest"
done

echo "==> done: $out_dir"
