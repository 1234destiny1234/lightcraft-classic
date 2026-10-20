# Android

The Android app is being built in stages (plan: Android build in CI, then peer-to-peer catalog sync). This page
covers what exists today.

## Status

| Stage | State |
|---|---|
| Shared crates build for `aarch64-linux-android` (`cargo xtask android check`, in `ci --fast`) | ✅ |
| App host (`apps/android`, NativeActivity; JNI in `crates/android-host`) | 🟡 runs on the emulator: import via the system picker, Develop, library kept across restarts; status-bar insets not handled yet; on-screen keyboard and system font fallback |
| APK / AAB packaging (`cargo xtask android package`), release job (`RELEASE_ANDROID`) | ✅ (arm64 APK 75 MB, AAB 35 MB) |
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
- **Peer discovery** is multicast DNS service discovery (RFC 6762 / 6763, `crates/peer`), the same as Bonjour, so
  a future iOS build can join. Android's Wi-Fi drops multicast unless an app holds a `WifiManager.MulticastLock`:
  the activity acquires one in `onCreate` (permission `CHANGE_WIFI_MULTICAST_STATE`) and releases it in `onDestroy`.
- **On-screen keyboard.** NativeActivity's own soft-input support has no input connection, so predictions, composed
  text and most non-Latin input never arrive. The activity adds a 1×1 transparent `View` over the native content
  (a plain view, not an `EditText`, so no system cursor or selection handles). It takes focus while the app wants
  text; its `InputConnection` keeps no text and forwards `commitText`, `setComposingText`, `finishComposingText`,
  `deleteSurroundingText`, Enter and Backspace to `nativeIme` (registered at start-up beside `nativePicked`). The
  native side queues them (bounded) and wakes the UI; the next frame's `raw_input_hook` turns them into egui text,
  IME pre-edit/commit and key events (`lcc_ui_egui::soft_keyboard`). After each frame the app checks whether a text
  field wants input (egui's IME output) and calls the activity's `setKeyboard(show)` only when that changes.
- **System font fallback.** At start-up the app reads a few faces from `/system/fonts` (Noto Sans CJK or
  DroidSansFallback, `NotoSans<Script>`) for scripts craft-fonts didn't supply, and lists them after every other
  face (`theme::set_system_fallbacks`). At most 12 faces, 32 MB each and 64 MB together; each is parsed before
  egui sees it, and a face that can't be read is logged and skipped. Nothing is bundled.
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

## Building the APK

```sh
.github/scripts/install-android-ndk.sh   # prints ANDROID_NDK_HOME to export
.github/scripts/install-android-sdk.sh   # JDK 17+ and SDK platform 36 / build-tools 36 (prints JAVA_HOME, ANDROID_HOME)
rustup target add aarch64-linux-android  # and x86_64-linux-android for the emulator
cargo xtask android package [--abi arm64] [--abi x86_64] [--aab]
adb install -r target/android/<binary>-<version>-android-arm64-v8a.apk
```

There is no Gradle. The activity is one Java file on the platform's `NativeActivity` (no AndroidX), so xtask
drives the SDK tools itself (`xtask/src/apk.rs`):
1. `javac` and `d8` for the activity, rendered from `packaging/android/*.in` with the `brand.toml` values (the
   package is `android_package` from `brand.toml`: provisional, and permanent once the app is on the Play Store).
2. `aapt2` for the manifest and launcher icon.
3. The native library from `cargo build --release`.
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

`release.yml` has an `android` job. It is off until the repository variable `RELEASE_ANDROID` is `true`. It
installs the pinned NDK, SDK and JDK, builds the APK and AAB, and uploads both to the draft release. Its secrets,
in the `release` environment, are all optional: `ANDROID_KEYSTORE_BASE64` (the upload keystore, base64),
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`. The in-app updater ignores `.apk` and
`.aab` files.

