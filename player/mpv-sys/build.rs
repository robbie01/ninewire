use std::{env, path::PathBuf};

fn main() {
    let mpv = pkg_config::Config::new()
        .atleast_version("2.5.0")
        .probe("mpv")
        .unwrap();

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        .clang_args(mpv.include_paths.into_iter().map(|p| format!("-I{}", p.to_str().unwrap())))
        .clang_args(mpv.defines.into_iter().map(|(k, v)| match v {
            None => format!("-D{k}"),
            Some(v) => format!("-D{k}={v}")
        }))
        .header("wrapper.h")
        .allowlist_function("mpv_.*")
        .allowlist_type("mpv_.*")
        .prepend_enum_name(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}