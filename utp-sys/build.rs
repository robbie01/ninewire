fn main() {
    let mut cc = cc::Build::new();

    cc
        .cpp(true)
        .warnings(false)
        .files([
            "libutp/utp_api.cpp",
            "libutp/utp_callbacks.cpp",
            "libutp/utp_hash.cpp",
            "libutp/utp_internal.cpp",
            "libutp/utp_packedsockaddr.cpp",
            "libutp/utp_utils.cpp"
        ]);

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        cc.define("WIN32", None);
        cc.define("_WIN32_WINNT", "0x600");
        cc.define("_CRT_SECURE_NO_WARNINGS", None);
    } else {
        cc.define("POSIX", None);
    }
    
    cc.compile("utp");
}