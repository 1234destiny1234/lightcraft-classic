# Android

The Android app is being built in stages (plan: Android build in CI, then peer-to-peer catalog sync). This page
covers what exists today.

## Status

| Stage | State |
|---|---|
| Shared crates build for `aarch64-linux-android` (`cargo xtask android check`, in `ci --fast`) | ✅ |
| App host (`apps/android`, NativeActivity; JNI in `crates/android-host`) | 🟡 runs on the emulator: import via the system picker or by sharing / dropping from other apps, Develop, library kept across restarts; on-screen keyboard and system font fallback; status-bar insets not handled yet |
| Storage: folders read in place with All files access, copies in the activity stack, stable staging and cleanup | 🟡 built and unit-tested; not yet tried on a device |
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
  activity answers with paths (the real path where it can read a document in place, else a copy in the app's
  folders; see [Storage](#storage)) through a native callback that it
  registers at start-up, so no symbol depends on the package name. For a save, it watches the path it handed out
  and copies the written file to the chosen document.
- **Opening, sharing and dropping photos from other apps.** The activity takes `VIEW` intents for `content://`
  images and `SEND` / `SEND_MULTIPLE` for `image/*` (raw files arrive as `image/x-…` types, so `image/*` covers
  them; `application/octet-stream` would offer the app for every file). It is `singleTask`, so a share while the
  app runs arrives in `onNewIntent`. Drag and drop from another app (split screen) goes through a drag listener on
  the window, with `requestDragAndDropPermissions` held until the copy is done. In every case the documents are
  staged like picked files ([Storage](#storage): real path or a reused copy in `Incoming/Picked`) off the UI thread and the paths go to `nativeOpened` (registered with
  `nativePicked`). The native side queues them and the next frame hands them to the shared UI as dropped files, so
  they take the desktop's drop path: photos open the import review, presets import as presets. A share that arrives
  before the native side is up waits in the activity until `Host::new` calls `hostReady()`. Copy failures are logged
  and shown as a toast.
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

## Storage

The Storage Access Framework hands out `content://` documents, but the engine reads files by path. The activity
(`MainActivity`, Storage section) answers each pick with paths:

- **In place.** A folder or file on the device's own storage (the external storage provider's `primary:<path>`)
  is answered with its real path under `Environment.getExternalStorageDirectory()` when the app can read it. Then
  nothing is copied, and Library ▸ Browse, folder scans and Synchronize work on the real folder. Reading it needs
  **"All files access"** (`MANAGE_EXTERNAL_STORAGE`, Android 11+) or, on Android 9–10, `READ_EXTERNAL_STORAGE`
  (with `requestLegacyExternalStorage` for 10). `READ_MEDIA_IMAGES` is declared for Android 13+.
- **Asking.** The activity asks once, at the first start (remembered in its `storage` SharedPreferences): the
  system's All files access page on 11+, the runtime permission before. `app.allFilesAccess` (Settings ▸ Folders ▸
  Device storage ▸ Change…) asks again; `Services::storage_access` gives the shared UI the state and the question.
  The Play Store allows `MANAGE_EXTERNAL_STORAGE` only for apps whose core purpose needs it; a photo library
  managing the user's own folders is the case argued for, but Play may still ask for a declaration.
- **Copies.** Anything else (cloud providers, SD cards and USB drives, which are other volumes) is copied into
  `<external data>/Incoming`: single files into `Incoming/Picked`, folders into `Incoming/<folder name>`. The
  document's URI is remembered with its copy's path, so picking the same document again reuses the copy (when the
  size still matches) or overwrites it, and picking the same folder again copies only files whose size changed. A
  document whose name has no extension gets one from its MIME type (raw types first), so the importer accepts it.
- **Progress.** A copy that copies at least one file reports through JNI (`nativeCopyProgress`, `nativeCopyEnded`)
  to `apps/android/src/copies.rs`, which holds an `import` row ("Copying picked files") in the activity stack. The
  row's ✕ answers the copy loop's next report with "stop" (checked between files and every 8 MB).
- **Cleanup.** At start, off the UI thread, `apps/android/src/staging.rs` deletes files under `Incoming` that no
  library photo refers to (keeping a kept file's sidecars, files younger than a day, and never following links or
  leaving the folder), then the folders left empty.
- **Import ▸ Copy of a staged file.** When the import dialog's mode is Copy for files that were staged, the file is
  copied a second time into the library's folders. The staged copy then has no photo and is deleted by the next
  cleanup a day later; use Add for picked files to keep one copy.

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

