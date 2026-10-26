# Upstream open PRs 2930 and up: review

Reviewed 2026-10-26 against fork `main`. ADOPT = ported (adapted to the fork's names), LEARN = idea taken in
the fork's own form (or noted for later), SKIP = not taken. "Stacked" = built on another open upstream PR.

| PR | Title | Decision | Reason | Fork PR |
|---|---|---|---|---|
| 2952 | LR-EDIT-COLOR-TEMP: a one-field white balance partial keeps the other field's look (#2910) | ADOPT | a one-field white balance partial keeps the other field's look | fork #554 |
| 2963 | Accept a UTF-8 BOM at the start of the CLI --script files | ADOPT | scripts saved by Windows editors start with a BOM | fork #554 |
| 2964 | LR-IMP-ADD-DIALOG: handle rename sequence exhaustion | ADOPT | rename sequence exhaustion is an error, not a wrap | fork #554 |
| 2969 | Report a photo's custom white balance left out of an imported look | ADOPT | the import reports a custom white balance it left out | fork #554 |
| 2977 | LR-META-EXIF-FULL: reject malformed EXIF clocks | ADOPT | hostile EXIF clocks rejected (never-crash) | fork #554 |
| 2986 | Issue #2980: Clear Color Label can be given a keyboard shortcut | ADOPT | missing shortcut slot | fork #554 |
| 2942 | Issue #2936: build the GPU kernels on parallel threads | ADOPT | faster GPU start-up | fork #557 |
| 2989 | GPU-TESTS: every WGSL kernel parses and validates in the test run, without a GPU | ADOPT | WGSL kernels validated without a GPU | fork #557 |
| 2974 | LR-BEHAV-WINDOW: on Windows a maximized window opens maximized | ADOPT | Windows window state fix | fork #558 |
| 2979 | LRC-LIB-CATALOG-SWITCH: a library's catalog.snap or folder given at launch opens that library | ADOPT | a library given at launch opens | fork #558 |
| 2988 | Export: Lanczos-3 downscale and a Gaussian luma unsharp mask for crisper exports | ADOPT | crisper exports (Lanczos-3 downscale, luma unsharp mask) | fork #559 |
| 2966 | LR-EDIT-GEOM-UPRIGHT: Upright corrects posts among people, and says why it found nothing | ADOPT | measured on real frames: fewer frames made worse; Upright says why it found nothing | fork #561 |
| 2991 | KEYC-METADATA: Cmd+Alt+Shift+K / Ctrl+Alt+Shift+K sets the keyword shortcut, and Shift+K adds or removes it | LEARN | the fork already had a single keyword shortcut (⌥⇧K / ⇧K) in its library prefs; extended to several keywords, the dialog and the Keyword List mark instead of upstream's catalog-mark storage | fork #564 |
| 2955 | LR-EDIT-COLOR-MIXER-HSL: the Color Mixer's HSL mode, laid out as in Lightroom Classic | LEARN | the fork already had Mixer / Point Color tabs; added the HSL mode, groups and `view.colorMixer` onto them | fork #572 |
| 2932 | LR-EDIT-DETAIL-DENOISE: AI Denoise follows Auto Sync in one undo step, starts at import from an ISO, one batch row (stacked on #2929) | LEARN | builds on #2929 (fork #507, merged); AI Denoise follows Auto Sync in one undo step | fork #602 |
| 2982 | LR-EDIT-DETAIL-DENOISE: built-in BM3D denoising with GPU acceleration | LEARN | built-in BM3D denoising with GPU acceleration | fork #599 |
| 2971 | LR-IMP-DUPES: a photo browsed in Local is offered by Import and no longer blocks the library's own file | LEARN | a photo browsed in Local no longer counts as library content in the Import review | fork #579 |
| 2944 | Import review: say what the folder scan left out | LEARN | the import scan says what it left out | fork #589 |
| 2940 | Import review: an optional batch-edit recipe for newly imported photos | LEARN | optional batch-edit recipe in the import review | fork #594 |
| 2941 | LR-META-XMP: Save and Read Metadata can run on a whole folder, with a row in the activity stack | LEARN | Save / Read Metadata for a whole folder with an activity row | fork #583 |
| 2935 | LRC-LIB-LIBFOLDERS: a folder's menu has one grouped order in Folders and Local | LEARN | one grouped order for the folder menus | fork #592 |
| 2956 | LRC-LIB-LIBFOLDERS: a folder can be exported from its menu (stacked on #2935) | LEARN | Export Folder from its menu (stacked on #2935) | pending (stacked on fork #601) |
| 2959 | LRC-LIB-LIBFOLDERS: every folder menu can copy the folder's path and make a folder on disk (stacked on #2935) | LEARN | Copy Path and New Folder on Disk in every folder menu (stacked on #2935) | fork #601 |
| 2961 | LRC-LIB-LIBFOLDERS: a folder's menu can bring photos in (stacked on #2935) | LEARN | Import to This Folder from a folder's menu (stacked on #2935) | pending (stacked on fork #601) |
| 2981 | Issue #2819: the histogram stays SDR when HDR is switched off | SKIP | already fixed in the fork | — |
| 2973 | MENU-BAR: clippy passes on macOS again (Quit, About and Settings share one branch) | SKIP | the fork's native_menu differs; its macOS clippy already passes | — |
| 2954 | LR-IMP-ADD-DIALOG: an import checks its probes and looks up sidecars several at a time | SKIP | the fork already looks up probes and sidecars in parallel | — |
| 2972 | chore(deps): bump the cargo-dependencies group across 1 directory with 16 updates | SKIP | dependency bump; the fork bumps its own dependencies | — |
| 2994 | chore(deps): refresh safe dependency bumps | SKIP | dependency bump; the fork bumps its own dependencies | — |
| 2264 | ci(deps): bump cachix/install-nix-action from 31.11.1 to 31.11.2 in the actions group across 1 directory | SKIP | dependency bump (closed upstream) | — |
| 2265 | chore(deps): bump the cargo-dependencies group across 1 directory with 17 updates | SKIP | dependency bump (closed upstream) | — |
| 2642 | chore(deps): bump the cargo-dependencies group across 1 directory with 18 updates | SKIP | dependency bump (closed upstream) | — |
| 2948 | PERF-CATALOG: a library states what its missing develop settings read as (stacked on #2867) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2951 | PERF-CATALOG: the catalog formats are one table, and every format from 6 on has a fixture (stacked on #2948) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2957 | PERF-CATALOG: develop settings can be stored as their difference from the defaults (stacked on #2951) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2958 | PERF-CATALOG: a put-off upgrade keeps one copy per attempt that worked and none for one that did not (stacked on #2968) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2968 | PERF-CATALOG: catalog records state only what was set (catalog format 10) (stacked on #2957) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2978 | PERF-CATALOG: the segments of a snapshot, with no caller yet (stacked on #2958 and #2871) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2983 | PERF-CATALOG: the catalog saves its photos in segment files and opens them on several threads (catalog format 11, stacked on #2978) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2984 | PERF-CATALOG: segment files nothing lists are removed when a library is opened, and segments without a snapshot are refused (stacked on #2983) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2985 | PERF-CATALOG: a compaction writes only the segments whose photos changed (stacked on #2984) | SKIP | PERF-CATALOG stack: the fork has its own v4 store | — |
| 2931 | LR-LIB-DELETE: photo.trashFiles moves photos' files to the system Trash (stacked on #2927) | SKIP | stacked on #2927, which the fork skipped | — |
| 2934 | LR-LIB-DELETE: Move Files to Trash can run in the background, and the commands that move files wait while it does (stacked on #2931) | SKIP | stacked on #2931 (base #2927 skipped) | — |
| 2943 | LR-LIB-DELETE: Move Files to Trash… asks, moves the files in the background, and asks again before erasing (stacked on #2934) | SKIP | stacked on #2934 (base #2927 skipped) | — |
| 2930 | Top bar: the title-bar drag uses craft-window's title_bar; a panel over the bar keeps the window still | SKIP | needs egui 0.37, deferred (plan in upstream-main-commits.md) | — |
| 2950 | Native iPad app (ios-port) | SKIP | out of scope (native iPad app) | — |
| 2947 | HW-SURFACE: control surfaces (Logitech MX Creative Console), engine ported from TactileBridge | SKIP | out of scope (hardware control surfaces) | — |
| 2975 | Preserve queued file drops across dialogs and restore Flatpak sources | SKIP | out of scope (Flatpak / drop-queue rework) | — |
| 2946 | LR-LIB-FOLDER: cargo xtask refusals keeps new silent command calls out of the interface (stacked on #777) | SKIP | stacked on a draft base (#777) | — |
| 2945 | Issue #1184: the template allows a tested core whose caller comes in the next pull request | SKIP | upstream PR template only | — |
| 2987 | build: pin craft-update to craft-libs main (495e597) | SKIP | craft-update pin for upstream's build (closed) | — |

## Closed

Closed upstream without merging.

| PR | Title | Decision | Reason | Fork PR |
|---|---|---|---|---|
| 435 | Reuse verified HDR merge results without changing existing edits | LEARN | reuse an identical HDR merge result instead of writing a duplicate DNG (#430); upstream asked for a minimal fix and closed it: worth doing in the fork as that minimal fix | — |
| 1589 | Shrink the app chrome to PhotoCraft control scale | SKIP | shrinks the chrome to another app's control scale; closed upstream pending its scale decision (#2962), and the fork keeps the Lightroom-parity scale | — |
| 1848 | gpu: renders queue for device memory; out of memory never retires the GPU (#1764) | LEARN | GPU out-of-memory should queue renders and fall back to the CPU instead of retiring the GPU (#1764); closed unmerged; the fork has its own `crates/gpumem` budget, revisit if OOM retirements are reported | — |
