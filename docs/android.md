# Android

The Android app is being built in stages (plan: Android build in CI, then peer-to-peer catalog sync). This page
covers what exists today.

## Status

| Stage | State |
|---|---|
| Shared crates build for `aarch64-linux-android` (`cargo xtask android check`, in `ci --fast`) and `x86_64-linux-android` (the full pre-release run) | ✅ |
| App host (`apps/android`, NativeActivity; JNI in `crates/android-host`) | 🟡 runs on the emulator: import via the system picker, Develop, library kept across restarts; status-bar insets not handled yet |
| APK / AAB packaging (`cargo xtask android package`), release job (on by default; `RELEASE_ANDROID=false` turns it off) | ✅ arm64-v8a and x86_64 APKs, one AAB with both (arm64 APK 75 MB) |
| Play Store upload | not started (needs the Play developer account) |
| Peer-to-peer catalog sync between desktop and Android | 🟡 pairing (Settings ▸ Sync, `crates/peer`; discovery over mDNS / DNS-SD); catalog exchange next |
| Touch / phone-sized layout | not started |

## Design

- **NativeActivity, not GameActivity.** android-activity's GameActivity backend compiles C++ from the Android Games
  SDK; NativeActivity is pure Rust on our side. The Java side is one small `NativeActivity` subclass
  (`packaging/android`) that runs the system pickers and opens links.
- **Folders.** The host calls `lcc_brand::set_platform_dirs` before anything else. Settings go in the app's internal
  data folder, and the library in its external data folder (`Android/data/<android_package>/files/<library name>`), which
  the Files app can reach and which is removed with the app. No environment variables are set (`set_var` is unsafe
  and racy once the activity's threads run).
- **Pickers.** The shared UI's asynchronous `Services::picker` is answered by the Storage Access Framework. The
  activity copies picked documents into the app's folders and answers with paths through a native callback that it
  registers at start-up, so no symbol depends on the package name. For a save, it watches the path it handed out
  and copies the written file to the chosen document.
- **Picked folders stay linked.** The activity keeps a picked folder's read grant
  (`takePersistableUriPermission`, up to 16 folders) and rescans it every 4 s while the app is in front, copying
  files that appeared since into its copy: never overwriting or deleting, skipping hidden files (the camera's
  `.pending-…` shots), and only once a file's size held between two scans, under a hidden name renamed into place.
  So a studio capture session watching the copy of `DCIM/Camera` imports each new shot, with no photo permission.
  Deleting the copy, or revoking the grant, unlinks the folder. Not built: a `MediaStore` observer behind
  `READ_MEDIA_IMAGES`, and scanning while the app is in the background (gap 34).
- **Peer discovery** is multicast DNS service discovery (RFC 6762 / 6763, `crates/peer`), the same as Bonjour, so
  a future iOS build can join. Android's Wi-Fi drops multicast unless an app holds a `WifiManager.MulticastLock`:
  the activity acquires one in `onCreate` (permission `CHANGE_WIFI_MULTICAST_STATE`) and releases it in `onDestroy`.
- **Settings** are written within seconds of a change (`apps/android/src/prefs.rs`), because Android can stop a
  background app without calling `on_exit`.

## Toolchain

- Rust targets: `rustup target add aarch64-linux-android x86_64-linux-android` (x86_64 for the emulator and
  Chromebooks; releases ship both).
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

## Building the APK

```sh
.github/scripts/install-android-ndk.sh   # prints ANDROID_NDK_HOME to export
.github/scripts/install-android-sdk.sh   # JDK 17+ and SDK platform 36 / build-tools 36 (prints JAVA_HOME, ANDROID_HOME)
rustup target add aarch64-linux-android  # and x86_64-linux-android for the emulator
cargo xtask android package [--abi arm64] [--abi x86_64] [--split] [--aab]
adb install -r target/android/<binary>-<version>-android-arm64-v8a.apk
```

There is no Gradle. The activity is one Java file on the platform's `NativeActivity` (no AndroidX), so xtask
drives the SDK tools itself (`xtask/src/apk.rs`):
1. `javac` and `d8` for the activity, rendered from `packaging/android/*.in` with the `brand.toml` values (the
   package is `android_package` from `brand.toml`: provisional, and permanent once the app is on the Play Store).
2. `aapt2` for the manifest and launcher icon.
3. The native library from `cargo build --profile dist`, every `--abi` in one cargo run (one `--target` each), so
   the host-side build is shared and all cores stay busy. One APK holds every ABI, or with `--split` one APK per
   ABI. The bundle always holds them all, and the Play Store gives each device only its own.
4. `zipalign -P 16` (16 KB pages, which Play requires) and `apksigner`.
5. With `--aab`, the same parts go into a Play Store bundle through `bundletool` (downloaded once, pinned by sha256)
   and are signed with `jarsigner`.

The version code is derived from the workspace version: `X.Y.Z` → `X·10⁶ + Y·10⁴ + Z·100 + 99`, and a prerelease
`-rcN` uses `N` in place of `99`.

**Signing.** Set `ANDROID_KEYSTORE` (a keystore file), `ANDROID_KEYSTORE_PASSWORD`, and optionally
`ANDROID_KEY_ALIAS` (default `upload`) and `ANDROID_KEY_PASSWORD`. Without them a debug key in
`<target>/android/debug.keystore` signs the build, and a warning says so. A debug-signed build installs for testing,
but the Play Store refuses it.

## Releases

`release.yml` has an `android` job, on unless the repository variable `RELEASE_ANDROID` is `false`. On gpu0 it
installs the pinned NDK, SDK and JDK (cached between runs), cross-compiles arm64-v8a and x86_64 in one cargo run,
and uploads an APK per ABI and one AAB to the draft release. Its build cache (`~/.cache/lcc-release-android`) is
shared by both gpu0 runner instances and backed by sccache. To try it without releasing, dispatch the Release
workflow as a dry run with `only: android`. Its secrets,
in the `release` environment, are all optional: `ANDROID_KEYSTORE_BASE64` (the upload keystore, base64),
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`. The in-app updater ignores `.apk` and
`.aab` files.

Without those secrets the release is signed with the debug key kept in that build cache, so successive releases
share it and update each other. Android refuses an update signed with a different key: when the release key is
added, or if the cache is wiped, people uninstall the old build first. Set up the upload key before Android
builds are announced. Create it with `keytool -genkeypair -v -keystore upload.jks -alias upload -keyalg RSA
-keysize 4096 -validity 10000`, keep it safe, and add it base64-encoded as `ANDROID_KEYSTORE_BASE64`.

