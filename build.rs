//! Compiles `swift/shim.swift` into a static library on macOS.
//!
//! SF Symbols are only reachable as vector paths through SwiftUI, so the
//! rendering lives in a small Swift shim with a C ABI. Elsewhere (and on
//! docs.rs) nothing is compiled and the crate renders no symbols.

use std::{env, path::PathBuf, process::Command};

/// `ImageRenderer` needs macOS 13, but the shim checks for it at run time, so
/// apps can still deploy as far back as SF Symbols themselves (macOS 11).
const MIN_DEPLOYMENT_TARGET: (u32, u32) = (11, 0);

fn main() {
    println!("cargo::rustc-check-cfg=cfg(sf_symbols_shim)");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=swift/shim.swift");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    println!("cargo:rerun-if-env-changed=SWIFTC");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") || env::var_os("DOCS_RS").is_some()
    {
        return;
    }

    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        Ok(other) => panic!("unsupported macOS architecture: {other}"),
        Err(_) => panic!("CARGO_CFG_TARGET_ARCH is not set"),
    };
    let deployment_target = deployment_target();
    let target = format!("{arch}-apple-macosx{deployment_target}");

    let sdk = xcrun(&["--sdk", "macosx", "--show-sdk-path"]);
    let swiftc = env::var("SWIFTC").unwrap_or_else(|_| xcrun(&["--find", "swiftc"]));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let library = out.join("libgpui_sf_symbols_shim.a");

    let opt = if env::var("PROFILE").as_deref() == Ok("release") {
        "-O"
    } else {
        "-Onone"
    };
    let status = Command::new(&swiftc)
        .args(["-emit-library", "-static", "-parse-as-library", opt])
        .args(["-module-name", "GpuiSfSymbolsShim"])
        .args(["-target", &target, "-sdk", &sdk])
        .arg("swift/shim.swift")
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {swiftc}: {error}"));
    assert!(status.success(), "compiling swift/shim.swift failed");

    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=gpui_sf_symbols_shim");
    for framework in ["SwiftUI", "AppKit", "CoreGraphics", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    // The Swift runtime ships with macOS; the toolchain supplies the static
    // back-deployment libraries the shim's object file asks for.
    println!("cargo:rustc-link-search=native={sdk}/usr/lib/swift");
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    let toolchain = PathBuf::from(&swiftc)
        .parent()
        .and_then(|bin| bin.parent())
        .map(|usr| usr.join("lib/swift/macosx"));
    if let Some(toolchain) = toolchain.filter(|dir| dir.is_dir()) {
        println!("cargo:rustc-link-search=native={}", toolchain.display());
    }
    println!("cargo:rustc-cfg=sf_symbols_shim");
}

fn deployment_target() -> String {
    let parse = |version: &str| -> Option<(u32, u32)> {
        let mut parts = version.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().map_or(Some(0), |minor| minor.parse().ok())?;
        Some((major, minor))
    };
    let requested = env::var("MACOSX_DEPLOYMENT_TARGET")
        .ok()
        .and_then(|version| parse(&version))
        .unwrap_or(MIN_DEPLOYMENT_TARGET);
    let (major, minor) = requested.max(MIN_DEPLOYMENT_TARGET);
    format!("{major}.{minor}")
}

fn xcrun(args: &[&str]) -> String {
    let output = Command::new("xcrun")
        .args(args)
        .output()
        .expect("failed to run xcrun; install the Xcode command line tools");
    assert!(
        output.status.success(),
        "xcrun {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("UTF-8 xcrun output")
        .trim()
        .to_owned()
}
