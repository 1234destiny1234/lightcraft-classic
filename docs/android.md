# Android

The Android app is being built in stages (plan: Android build in CI, then peer-to-peer catalog sync). This page
covers what exists today.

## Status

| Stage | State |
|---|---|
| Shared crates build for `aarch64-linux-android` (`cargo xtask android check`, in `ci --fast`) | ✅ |
| App host (`apps/android`, NativeActivity; JNI in `crates/android-host`) | 🟡 builds; Java activity and pickers next |
| APK / AAB packaging (`cargo xtask android package`), release job, Play Store upload | not started |
| Peer-to-peer catalog sync between desktop and Android | not started |
| Touch / phone-sized layout | not started |

## Design

- **NativeActivity, not GameActivity.** android-activity's GameActivity backend compiles C++ from the Android Games
  SDK; NativeActivity is pure Rust on our side. The Java side is one small `NativeActivity` subclass
  (`packaging/android`) that runs the system pickers and opens links.
- **Folders.** The host calls `lcc_brand::set_platform_dirs` before anything else. Settings go in the app's internal
  data folder, and the library in its external data folder (`Android/data/<app_id>/files/<library name>`), which
  the Files app can reach and which is removed with the app. No environment variables are set (`set_var` is unsafe
  and racy once the activity's threads run).
- **Pickers.** The shared UI's asynchronous `Services::picker` is answered by the Storage Access Framework. The
  activity copies picked documents into the app's folders and answers with paths through a native callback that it
  registers at start-up, so no symbol depends on the package name. For a save, it watches the path it handed out
  and copies the written file to the chosen document.
- **Settings** are written within seconds of a change (`apps/android/src/prefs.rs`), because Android can stop a
  background app without calling `on_exit`.

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
