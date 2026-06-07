// Copyright The pipewire-rs Contributors.
// SPDX-License-Identifier: MIT

use std::env;
use std::path::PathBuf;

fn main() {
    let libs = system_deps::Config::new()
        .probe()
        .expect("Cannot find libraries");

    println!("cargo:rerun-if-changed=wrapper.h");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    let builder = bindgen::builder()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .size_t_is_usize(true)
        .allowlist_function("spa_.*")
        .allowlist_type("spa_.*")
        .allowlist_var("SPA_.*")
        .prepend_enum_name(false)
        .derive_eq(true)
        // Required for spa_pod_builder fields to be generated with newer Clang.
        .clang_macro_fallback()
        .wrap_static_fns(true)
        .wrap_static_fns_suffix("_libspa_rs")
        .wrap_static_fns_path(out_path.join("static_fns"));

    let builder = libs
        .iter()
        .iter()
        .flat_map(|(_, lib)| lib.include_paths.iter())
        .fold(builder, |builder, l| {
            let arg = format!("-I{}", l.to_string_lossy());
            builder.clang_arg(arg)
        });

    let bindings = builder.generate().expect("Unable to generate bindings");

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
    const FILES: &[&str] = &["src/type-info.c"];
    let cc_files = &[PathBuf::from(FILES[0]), out_path.join("static_fns.c")];

    for file in FILES {
        println!("cargo:rerun-if-changed={file}");
    }

    let mut cc = cc::Build::new();
    cc.files(cc_files);
    cc.include(env!("CARGO_MANIFEST_DIR"));
    cc.includes(libs.all_include_paths());

    #[cfg(feature = "v0_3_65")]
    cc.define("FEATURE_0_3_65", "1");

    cc.compile("libspa-rs-reexports");
}
