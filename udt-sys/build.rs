fn main() {
    let mut build = cxx_build::bridge("src/lib.rs");

    build
        .std("c++11")
        .includes(["udt", "bridge"])
        .files([
            "udt/api.cpp",
            "udt/buffer.cpp",
            "udt/cache.cpp",
            "udt/ccc.cpp",
            "udt/channel.cpp",
            "udt/core.cpp",
            "udt/list.cpp",
            "udt/packet.cpp",
            "udt/queue.cpp",
            "udt/udtCommon.cpp",
            "udt/window.cpp"
        ]);

    build.flag_if_supported("-pthread");
    if std::env::var_os("CARGO_CFG_UNIX").is_some() {
        println!("cargo::rustc-link-lib=pthread");
        println!("cargo::rustc-link-lib=m");
    }

    if std::env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "x86_64" {
        build.define("AMD64", None);
    }

    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();

    if os == "macos" {
        build.define("MACOSX", None);
    } else if os == "linux" {
        build.define("LINUX", None);
        println!("cargo::rustc-link-lib=dl");
    } else if os.contains("bsd") {
        build.define("BSD", None);
    } else {
        panic!("unsupported platform");
    }

    build.compile("udt");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=udt");
    println!("cargo:rerun-if-changed=bridge");
}