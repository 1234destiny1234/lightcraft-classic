# Android

The Android app is being built in stages (plan: Android build in CI, then peer-to-peer catalog sync). This page
covers what exists today.

## Status

| Stage | State |
|---|---|
| Shared crates build for `aarch64-linux-android` (`cargo xtask android check`, in `ci --fast`) | ✅ |
| App host (`apps/android`, GameActivity, Storage Access Framework pickers) | not started |
| APK / AAB packaging (`cargo xtask android package`), release job, Play Store upload | not started |
| Peer-to-peer catalog sync between desktop and Android | not started |
| Touch / phone-sized layout | not started |

## Toolchain

- Rust target: `rustup target add aarch64-linux-android`.
- Android NDK r28c (minimum API 28, Android 9). `cargo xtask android check` finds it through `ANDROID_NDK_HOME`,
  `ANDROID_NDK_ROOT`, or the newest `~/Android/android-ndk-*`. CI installs the pinned, sha256-verified NDK with
  `.github/scripts/install-android-ndk.sh` (run it locally as well; it prints the `ANDROID_NDK_HOME` to export).
- Without the target or the NDK the check is skipped with a message. In CI (`CI` set) a missing toolchain fails.

The NDK's clang is needed for one C dependency, `ring` (used by SFTP publishing through `russh`). Everything else
in the graph is pure Rust.

## What is off on Android

- **Delete from Disk**: the `trash` crate has no Android backend, so `Session::trash` stays `None` (as on the web).
- **USB tethering**: `nusb` can't list devices on Android, where USB access needs a device handle from Java. Tether
  reports it as unsupported.
- **Updates**: the in-app updater has no Android target. Updates come from the Play Store or the GitHub release.
- **Sound out** (`audio-out` feature): stays off, because `cpal` would pull in the C++ `oboe` library.

## Rules for shared code

Android is `unix`. Code meant for Linux and the BSDs names them (`target_os = "linux"`, …), never
`all(unix, not(target_os = "macos"))`. `cargo xtask android check` catches mistakes.
