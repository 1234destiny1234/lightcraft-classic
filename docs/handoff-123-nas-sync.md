# Handoff: issue #123, editing in place on a NAS

Branch: `feat/123-nas-sync`, based on `origin/main` at 24ef6508. Written 2026-10-21. Delete this file before
merging.

Issue: https://github.com/emmertex/lightcraft-classic/issues/123. Work on moving disk access off the UI thread
(`with_disk_folders`, the grid's sidecar checks, the automatic XMP writes) belongs to @endthestart and is
**not** part of this branch.

## What's on this branch

Three feature branches merged one at a time (`feat/123-copy-skip`, `feat/123-xmp-autoread`,
`feat/123-folder-poll`). The only conflicts were in the `docs/parity-checklist.md` header and summary, and
`cargo xtask parity --write` refreshed it afterwards.

1. **Skip files that are still being copied** (`feat/123-copy-skip`, 1df0c7c8)
   - `crates/catalog/src/incoming.rs` (tests in `tests_incoming.rs`): `still_being_written(path)`:
     - **Temp names:** dotfiles (which cover rsync's `.name.XXXXXX`), `.part`, `.crdownload`, `~$`, and similar.
     - **Writer check:** on macOS/BSD, an `O_SHLOCK|O_NONBLOCK` open, where EAGAIN means a writer is active. On
       Windows, a share-mode open, where a sharing violation means a writer is active. On Linux, no check.
     - **Backstop:** the TIFF/raw header's declared data end lies past the file's current size.
   - Wired into the probe in `crates/engine/src/files.rs`, `import::scan_files`, `sync.rs` and
     `ImportJob::prepare_files`. A skipped file is picked up by a later scan.
   - **Open decision:** a genuinely corrupt TIFF/raw whose header points past its end is skipped quietly on every
     scan, never reported as unreadable. A possible fix is to report it after N scans where it hasn't changed.
2. **Read other apps' XMP automatically** (`feat/123-xmp-autoread`, 36c91c50 + 2e0621f0)
   - **Preference:** `XmpPrefs.auto_read`, **on by default**, set with `library.xmpPreferences {autoRead}` and the
     Settings ▸ XMP checkbox. The reasoning is in `docs/xmp-interop.md`.
   - **Command `photo.autoReadMetadata`:**
     - A sidecar that changed only on disk is read through `photo.readMetadataFromFile` (undoable, journaled).
     - If both sides changed, the photo stays a conflict.
     - Run by `folder.sync` (when `readMetadata` isn't given) and by the grid's existing sidecar checks.
   - **Fix in 2e0621f0:** an empty or cut-off sidecar (one another app is still writing) is reported as `failed`
     and not read. Before the fix, such a file was read as "no metadata". `Session::sidecar_complete` in
     `crates/engine/src/sidecar.rs` does the check.
   - **Todo:** the new checkbox text isn't translated.
3. **Folder polling, new-file policy and "sync this folder now"** (`feat/123-folder-poll`, fc431c18)
   - **Polling:** `crates/engine/src/folder_watch.rs` and `crates/ui-egui/src/folder_poll.rs`.
     - Opt-in, off by default, set under Settings ▸ Library ▸ Folder Polling (interval of 1, 5, 15 or 60 min).
     - Runs on open, on focus and at each interval, as an activity-stack row (`folderPoll`).
     - Stamps each folder with its mtime and a hash of its entry names, saved in `folder-stamps.json`, and rescans
       only the folders whose stamp changed.
   - **New-file policy:** "ask" (the default) shows an "N new" badge, a notice and a right-click menu entry "Add N
     New Photos…". "Add automatically" imports in place in the background.
   - **Commands:** `folder.watchPreferences`, `folder.poll` and `folder.newFiles`.
   - **Hand-off:** `<cli> sync-folder [--connect ADDR | --library DIR] FOLDER`, and `<cli> open-url
     <scheme>://sync?path=…` (`crates/engine/src/link.rs`, which allow-lists commands). The desktop app opens the
     Synchronize Folder dialog instead of syncing straight away.
   - **Known gaps:**
     - The URL scheme isn't registered with the OS, and a link isn't forwarded to an app that's already running.
       Both are gap 37 in `docs/gaps.md`.
     - A new folder created *next to* the library's folders isn't noticed.
     - The first poll only records stamps.

Related: PR #375 (`fix/smb-fsync`) ports upstream PR 1409, the plain-`fsync` fallback on SMB shares, for the
catalog store and `safe_file.rs`. It's separate from this branch and has its own CI.

## Verification so far (Windows aarch64, clang 23 installed)

- **Each branch:**
  - copy-skip: clippy clean, 119 engine `files` tests plus 6 catalog `incoming` tests pass.
  - xmp-autoread: 4 auto-read tests and 55 xmp tests pass.
  - folder-poll: clippy clean (engine, ui-egui, cli), 10 `folder_watch` tests, 6 `link` tests and 15 UI
    `folder_poll` tests pass. The agent also reported the SIL suite and a CLI e2e test passing, but it ran them
    with SFTP stubbed out.
- **Merged branch, `cargo xtask ci --fast`:** fmt, brand, quarantine, docs, notes, clippy, heif, parity,
  shared-check, layers, assets and deny all pass. **The wasm check fails for all 58 crates**, almost certainly
  because the wasm32 target isn't installed on this machine. Re-run it where the target is installed.
- **Merged branch, `cargo test -p lcc-engine -p lcc-catalog`:** catalog 285 passed. Engine 1536 passed and 1
  failed, `publish_to_immich_albums_renders_and_stacks`, which already failed before this work (see below).
- **Already failing before this work:** `tests_immich::sync_tests::publish_to_immich_albums_renders_and_stacks`
  and the two `tests_i18n_coverage` tests (missing zh-hans strings in other files).

## Next steps

1. `git fetch && git switch feat/123-nas-sync`
2. `cargo xtask ci --fast`. All of it, including wasm, must pass.
3. Run the targeted tests: `cargo test -p lcc-engine -p lcc-catalog -p lcc-ui-egui` and the `apps/cli`
   e2e `sync_folder_and_open_url_hand_offs`, plus `cargo xtask sil`.
4. Run the app (`cargo xtask run -- --control 18xxx`) and look at the Folders panel's "N new" badge, the
   settings for polling and XMP auto-read, and a `sync-folder` call from the CLI.
5. Decide on the corrupt-raw case (item 1), and add the translations for the new XMP checkbox.
6. Delete this file, then open a PR to `main` that references #123. Merge PR #375 on its own.
