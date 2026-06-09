# iroh-live compatibility spike

Spiked against [n0-computer/iroh-live](https://github.com/n0-computer/iroh-live) (main, June 2026).

## Version mismatch

| Component | iroh-live | Failsafe |
|-----------|-----------|----------|
| `iroh` | 0.98.0 | 1.0.0-rc.1 |
| MoQ stack | `deps/iroh-098` git branches | N/A |

`iroh-live`, `iroh-moq`, and the MoQ git dependencies are pinned to iroh 0.98. Failsafe's transport (`failsafe-transport`) uses iroh 1.0 APIs (`Endpoint::builder(presets::N0)`, etc.). **These cannot coexist in one workspace without downgrading Failsafe's iroh or waiting for upstream iroh-live 1.0 support.**

## What builds standalone

- `moq-media` compiles without iroh when built from the iroh-live workspace (`cargo check -p moq-media --no-default-features`).
- `rusty-capture` and `rusty-codecs` are workspace path crates; pulling them into Failsafe requires vendoring the full iroh-live workspace or publishing forks.

## Current Failsafe approach

Phase 1 uses **xcap** screen capture, **JPEG** frames over a custom Iroh bi-stream protocol (`FDT1` handshake), and a **minifb** native viewer. This avoids the iroh version conflict and keeps dependencies light.

## Migration path

When iroh-live ships iroh 1.0 support:

1. Add `iroh-live` / `moq-media` as git dependencies with `[patch.crates-io]` if needed.
2. Replace `failsafe-desktop` capture/encode in `host.rs` with `ScreenCapturer` + H.264 from moq-media.
3. Replace JPEG frame relay with MoQ publish/subscribe on the existing Iroh endpoint.
4. Keep the Failsafe control socket + feature registry integration unchanged.

## Validation commands

```sh
git clone https://github.com/n0-computer/iroh-live.git
cd iroh-live
cargo check -p moq-media --no-default-features   # OK (no iroh)
cargo check -p iroh-live                          # requires iroh 0.98 toolchain
```

On Linux with PipeWire dev headers, `irl publish --video screen` validates end-to-end capture outside Failsafe.
