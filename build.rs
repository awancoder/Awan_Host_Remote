fn main() {
    // Windows gives the main thread only 1 MB of stack, and GPUI renders the
    // whole UI on it; debug builds keep every builder temporary on the stack,
    // so deep views can overflow it. 8 MB matches what Zed (same UI stack)
    // sets for its Windows binary.
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" {
        if env == "msvc" {
            println!("cargo:rustc-link-arg-bins=/STACK:8388608");
        } else {
            println!("cargo:rustc-link-arg-bins=-Wl,--stack,8388608");
        }
    }
}
