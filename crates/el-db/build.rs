//! Link the Windows system libraries that a static OpenSSL (SQLCipher) build
//! needs. `libsqlite3-sys` links `libcrypto` directly when the
//! `bundled-sqlcipher` feature is used, so `openssl-sys` never gets a chance
//! to emit these.

fn main() {
    let sqlcipher = std::env::var_os("CARGO_FEATURE_SQLCIPHER").is_some();
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if sqlcipher && windows {
        for lib in ["user32", "crypt32", "secur32", "gdi32"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }
    }
}
