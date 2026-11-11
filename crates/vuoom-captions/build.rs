//! macOS: whisper.cpp's Metal backend checks the OS version at run time (`@available`),
//! which compiles to a call into clang's runtime library. Clang links that library itself,
//! but the final link here is rustc's, so it is named explicitly. Only needed once a
//! deployment target older than the build machine is set (release builds target 14.0).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out = std::process::Command::new("clang")
        .arg("--print-resource-dir")
        .output();
    let Ok(out) = out else {
        println!("cargo:warning=clang not found; the macOS link may miss clang_rt.osx");
        return;
    };
    let dir = std::path::PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
        .join("lib")
        .join("darwin");
    if dir.join("libclang_rt.osx.a").exists() {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=clang_rt.osx");
    }
}
