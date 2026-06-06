use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let web_dir = manifest_dir.join("../../failsafe-web-ui");
    let dist_dir = web_dir.join("dist");

    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("package.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("package-lock.json").display()
    );
    println!("cargo:rerun-if-changed={}", web_dir.join("src").display());
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("vite.config.ts").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        web_dir.join("index.html").display()
    );

    if std::env::var("FAILSAFE_SKIP_WEB_BUILD").is_ok() {
        if !dist_dir.join("index.html").exists() {
            panic!(
                "FAILSAFE_SKIP_WEB_BUILD is set but {} does not exist; run `npm run build` in failsafe-web-ui first",
                dist_dir.join("index.html").display()
            );
        }
        return;
    }

    if !web_dir.join("package.json").exists() {
        panic!(
            "failsafe-web-ui project not found at {}; run shadcn init in failsafe-web-ui first",
            web_dir.display()
        );
    }

    if let Err(error) = run_npm(&web_dir, &["ci"]).or_else(|_| run_npm(&web_dir, &["install"])) {
        panic!("{error}");
    }
    if let Err(error) = run_npm(&web_dir, &["run", "build"]) {
        panic!("{error}");
    }

    if !dist_dir.join("index.html").exists() {
        panic!(
            "frontend build did not produce {}; check failsafe-web-ui build output",
            dist_dir.join("index.html").display()
        );
    }
}

fn run_npm(web_dir: &PathBuf, args: &[&str]) -> Result<(), String> {
    let status = Command::new("npm")
        .args(args)
        .current_dir(web_dir)
        .status()
        .map_err(|error| format!("failed to run `npm {}`: {error}", args.join(" ")))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`npm {}` failed with status {status}",
            args.join(" ")
        ))
    }
}
