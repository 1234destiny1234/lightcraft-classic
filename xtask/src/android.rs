//! `cargo xtask android check`: the crates the Android app is built from must keep building for
//! Android (`aarch64-linux-android`). Run by `ci` next to the wasm check.
//!
//! Android counts as `unix`, so a cfg written as "unix but not macOS" to mean Linux silently
//! takes it in; this check catches that the day it happens. The C parts of the graph (`ring`, via
//! SFTP publishing) need the NDK's clang, found through `ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT`
//! or the newest `~/Android/android-ndk-*`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{cargo, layers, metadata, run as step};

/// The Rust target the Android app ships for.
pub const TARGET: &str = "aarch64-linux-android";
/// The oldest Android API level the app supports (Android 9); picks the NDK's clang wrapper.
pub const MIN_API: u32 = 28;

pub fn run(args: &[&str]) -> Result<(), String> {
    match args.first().copied() {
        Some("check") | None => check(),
        Some(other) => Err(format!("android: unknown subcommand `{other}` (expected `check`)")),
    }
}

/// Workspace packages that must build for Android: every L0–L5 and standalone crate except the
/// web app, plus the Android app itself.
fn android_set() -> Result<Vec<String>, String> {
    let crates = layers::from_metadata(&metadata()?)?;
    Ok(crates
        .into_iter()
        .filter(|c| c.name != "lcc-web")
        .filter(|c| match layers::classify(&c.name) {
            Some(layers::Class::Layer(l)) => l <= 5,
            Some(layers::Class::Standalone) => true,
            _ => false,
        })
        .map(|c| c.name)
        .chain(std::iter::once("lcc-android".to_string()))
        .collect())
}

/// The NDK root: `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, else the newest `~/Android/android-ndk-*`.
fn ndk_root() -> Option<PathBuf> {
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Some(p) = std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_dir()) {
            return Some(p);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let mut found: Vec<PathBuf> = std::fs::read_dir(home.join("Android"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("android-ndk-")))
        .collect();
    found.sort();
    found.pop()
}

/// The NDK's prebuilt LLVM `bin` directory for this host.
fn ndk_bin(ndk: &Path) -> Option<PathBuf> {
    let prebuilt = ndk.join("toolchains/llvm/prebuilt");
    std::fs::read_dir(prebuilt).ok()?.filter_map(|e| e.ok().map(|e| e.path().join("bin"))).find(|p| p.is_dir())
}

/// Points cargo and the `cc` crate at the NDK's clang for [`TARGET`].
pub fn configure(c: &mut Command, bin: &Path) {
    let clang = bin.join(format!("aarch64-linux-android{MIN_API}-clang"));
    let triple = TARGET.replace('-', "_");
    c.env(format!("CC_{triple}"), &clang)
        .env(format!("AR_{triple}"), bin.join("llvm-ar"))
        .env(format!("CARGO_TARGET_{}_LINKER", triple.to_uppercase()), &clang);
}

fn target_installed() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == TARGET))
}

/// A missing toolchain is a skip on a developer machine and a failure in CI (`CI` is set there).
fn missing(what: &str) -> Result<(), String> {
    if std::env::var_os("CI").is_some() {
        Err(format!("android: {what}"))
    } else {
        eprintln!("android: SKIPPED, {what}; CI runs it (see docs/android.md)");
        Ok(())
    }
}

fn check() -> Result<(), String> {
    if !target_installed() {
        return missing(&format!("the Rust target is not installed (`rustup target add {TARGET}`)"));
    }
    let Some(bin) = ndk_root().as_deref().and_then(ndk_bin) else {
        return missing("the Android NDK was not found (set ANDROID_NDK_HOME)");
    };
    let set = android_set()?;
    let mut results = Vec::new();
    for pkg in &set {
        let mut c = cargo();
        c.args(["check", "--target", TARGET, "-p", pkg]);
        configure(&mut c, &bin);
        let ok = step(c, &format!("cargo check --target {TARGET} -p {pkg}")).is_ok();
        results.push((pkg.clone(), ok));
    }
    println!("\n{TARGET} check:");
    for (p, ok) in &results {
        println!("  {:<6} {p}", if *ok { "ok" } else { "FAIL" });
    }
    let failed = results.iter().filter(|r| !r.1).count();
    if failed == 0 { Ok(()) } else { Err(format!("{failed} crate(s) failed the Android check")) }
}
