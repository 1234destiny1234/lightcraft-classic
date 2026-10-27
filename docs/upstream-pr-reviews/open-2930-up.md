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
| 2949 | LR-PREF-GENERAL: the Settings tab bar, filter, choices and questions can be read in every theme (stacked on #2906) | SKIP | draft stacked on a chain of unmerged settings/theme PRs; revisit once they land |  |
| 2998 | KEYC-COMPARE: Shift+Up/Down and Ctrl+Shift+arrows extend the grid selection | ADOPT | keyboard selection of grid rows | fork #611 |
| 3002 | LR-META-EXIF: TIFF keeps shared child IFDs and multi-string ASCII values | ADOPT | mostly already in Chromaforge; the copy's entry and byte charge added | fork #609 |
| 3003 | LR-EDIT-EFFECTS-GRAIN: grain skips a degenerate frame instead of producing NaN pixels | SKIP | already in Chromaforge |  |
| 3004 | LR-IMP-FORMATS: decode compressed Samsung SRW (32770, 32772, 32773) | SKIP | Chromaforge already decodes compressed SRW (`vendor::srwc`) |  |
| 3005 | fix(gpu): LR-BEHAV-GPU: bounded blur work per pixel so large radii can't trigger a GPU reset | SKIP | already in Chromaforge |  |
| 3006 | CLI-HOSTILE: render --set refuses inf / NaN instead of dropping the value | SKIP | already in Chromaforge |  |
| 3007 | Never crash: CR2 slice widths that overflow are an error; XMP attribute unescape in one pass | ADOPT | remaining parts: usize conversion of slice widths, leading-zero references | fork #609 |
| 3008 | security(raster): fallible image allocation; no panic on empty images or zip_map size mismatch | ADOPT | remaining parts: checked zip_map index, 32-bit-safe MAX_IMAGE_BYTES | fork #609 |
| 3009 | LR-MERGE-HDRPANO: a repeated identical HDR merge reuses the verified output | ADOPT | same idea as #435, already in Chromaforge; the `reused` result field added | fork #611 |
| 3010 | #366: SAM 3 mirrors accept a local folder; tokenizer files pinned | ADOPT | local mirrors and pins were in; `file://` percent escapes added | fork #611 |
| 3011 | security(codecs): refuse decodes with no pixels; checked JPEG sample counts | ADOPT | code already in; the zero-size header test added | fork #609 |
| 3012 | #2361: lens corrections crop inside the corrected photo | ADOPT | already in except the estimated-CA constrain | fork #611 |
| 3013 | LR-EDIT-OPTICS-PROFILE: lens profiles for compact cameras with a built-in lens | ADOPT | remaining parts: no-make cameras, overflow guard, exact-name lookup without the mount fallback | fork #611 |
| 3014 | MENU-CTX-GRID: the photo menu's rows, built from the menu bar's model | LEARN | a photo context menu built from the menu bar's model is the right shape; too large to port while the docking rework lands, revisit with #3026 |  |
| 3015 | Never crash: an oversize export size is an error, not a multiply overflow | SKIP | already in Chromaforge |  |
| 3016 | LRC-LIB-LIBFOLDERS: a date folder inside the library can be renamed | ADOPT | imported date folders can be renamed; refused prompts say why | fork #611 |
| 3017 | LR-IMP-FORMATS: decode AVIF on import (pure-Rust AV1 decoder) | SKIP | Chromaforge already decodes AVIF (`codecs::avif`) |  |
| 3018 | perf(catalog): box Photo::embedded_lens (~240 MB less per 1M photos) | LEARN | worthwhile memory saving; touches many call sites, do it with the next catalog memory pass |  |
| 3019 | LR-BEHAV-WINDOW: remember where the window was, not only its size (#2967) | SKIP | already in Chromaforge (`window_place`) |  |
| 3020 | Originals: on Windows a hard link to an original counts as that original | ADOPT | Windows hard links via the same-file crate | fork #611 |
| 3021 | Clean-room Canon CRW decode, EOS D60 colour matrices | SKIP | Chromaforge already decodes CRW (`vendor::crw`) and ships dcraw's matrices |  |
| 3023 | LR-MASK-OBJECTS: recover from SAM worker accept errors | SKIP | already in Chromaforge (accept errors are logged and retried) |  |
| 3024 | LRC-LIB-PEOPLE: Start the face scan's progress over in another library (#2995) | ADOPT | progress row resets with the library | fork #611 |
| 3026 | MENU-CTX-GRID: the photo menu offers the Photo menu's actions, grouped by task (stacked on #3014) | LEARN | see #3014 |  |
| 3027 | LR-PROF-CAMERACOLOR: docs count 9 bundled camera profiles, not 5 | SKIP | upstream's doc counts; Chromaforge's docs count its own bundle |  |
| 3030 | LR-BEHAV-MULTISELECT: click selection as in Lightroom Classic (frame click selects alone, Shift/Ctrl keep the active photo) | LEARN | Classic click semantics worth matching; grid/filmstrip code is being reworked by the docking cherry-picks, port after |  |
| 3031 | KEY-INFO, LR-BEHAV-PANEL-RESIZE: panels come back as they were (closing a tool, pressing a panel key again) | LEARN | panel restore behaviour; collides with the docking rework, revisit after it lands |  |
| 3034 | LR-PREF-INTERFACE: Settings › Interface size, Auto or 50–300% | LEARN | an interface size setting is useful; settings UI is in flux with the docking/craft-ui work, revisit |  |
| 3035 | i18n(it): launch library chooser, Lightroom import destination and new settings | ADOPT | only the strings Chromaforge has, product name as `{app}` | fork #611 |
|---|---|---|---|---|
| 435 | Reuse verified HDR merge results without changing existing edits | LEARN | reuse an identical HDR merge result instead of writing a duplicate DNG (#430); upstream asked for a minimal fix and closed it: worth doing in the fork as that minimal fix; already in Chromaforge (commits 101efb1ae, 6459531cf), `reused` field added with #3009 | fork #611 |
| 2999 | Clean room crw format decode | SKIP | closed; Chromaforge already decodes CRW | |
| 1589 | Shrink the app chrome to PhotoCraft control scale | SKIP | shrinks the chrome to another app's control scale; closed upstream pending its scale decision (#2962), and the fork keeps the Lightroom-parity scale | — |
| 1848 | gpu: renders queue for device memory; out of memory never retires the GPU (#1764) | LEARN | GPU out-of-memory should queue renders and fall back to the CPU instead of retiring the GPU (#1764); closed unmerged; the fork has its own `crates/gpumem` budget, revisit if OOM retirements are reported; already in Chromaforge as `crates/gpu/src/budget.rs` (commit 6c4864f6a) | in main (6c4864f6a) |

## Review r4 (2026-10-27): open PRs not yet listed, up to 3119

| PR | Title | Decision | Reason | Fork PR |
|---|---|---|---|---|
| 3038 | macOS: CFBundleLocalizations | ADOPT | AppKit's own dialogs follow the app's languages | this bundle |
| 3041 | Detail startup view for launch files | LEARN | the fork's startup path differs; port only if it shows the same bug | — |
| 3042 | Windows: gvfs unused variable | SKIP | already in Chromaforge | — |
| 3043 | Dependabot bumps (quick-xml, jixel, candle 0.11) | SKIP | Chromaforge pins its own versions; candle / jixel bumps are a regression risk | — |
| 3045 | Windows: folder trees hide hidden folders | ADOPT | small isolated fix; non-Windows unchanged | this bundle |
| 3047 | Web: craftStorage | SKIP | needs craft-libs' `craft-web-host` | — |
| 3054 | Reject invalid XMP GPS subdivisions | ADOPT | `crates/meta` validation only; its UI / locale parts left out | this bundle |
| 3055 | PERF-CULL: wedding culling | LEARN | large multi-crate perf work; mine piecewise later | — |
| 3073 | Lens DB auto-download on first run | LEARN | Chromaforge has its own `lens_db` commands; upstream's adds network on first run and a catalog format bump | — |
| 3074 | Integration: 23 approved PRs | SKIP | a bundle of other PRs, reviewed one by one | — |
| 3076 | `--connect` resends a command that crossed the idle-close notice (#3069) | ADOPT | control client / MCP retry bug fix | this bundle |
| 3077 | Culling: loupe uses built previews | SKIP | stacked on #3055 | — |
| 3078 | Text box: an outside press ends the edit with the menu open | LEARN | the fork's `TextField` diverged; recheck there | — |
| 3079 | `cargo xtask test` under ci's caps (#3058) | SKIP | upstream test infrastructure | — |
| 3080 | Culling: camera JPEG first | SKIP | stacked on #3077 | — |
| 3083 | testkit deadline helpers and watchdog (#3062) | LEARN | bounded test waits; upstream's harness | — |
| 3084 | Settings: a tab click keeps typed text | SKIP | Settings redesigned in Chromaforge | — |
| 3085 | CI: simplify clippy-flagged conditions | SKIP | Chromaforge's clippy doesn't flag these lines | — |
| 3086 | Plain cargo test on 4 threads (#3059) | SKIP | test infrastructure | — |
| 3087 | wgpu compatibility fork for OCLP Macs (#1939) | SKIP | forked wgpu dependency | — |
| 3088 | Settings test restores the memory budget (#3065) | SKIP | upstream test harness | — |
| 3089 | An album shown is not also a filter | LEARN | check the fork's filter model first | — |
| 3090 | Export-dialog test waits with the background bound | SKIP | upstream test flake | — |
| 3091 | SAM 3 model can be removed in Settings | LEARN | engine remove command worth having; Settings UI is Chromaforge's | — |
| 3092 | YuNet looks at photos up to 1024 px in their own shape | ADOPT | finds small faces; isolated to `crates/faces` | this bundle |
| 3093 | Undo of a keyword delete / folder removal restores filters | LEARN | depends on upstream's #2758 filter work | — |
| 3095 | Settings tab opened by a command keeps text | SKIP | Settings redesigned in Chromaforge | — |
| 3096 | Tests stop asserting wall-clock time (#3064) | SKIP | test infrastructure | — |
| 3097 | CR3 white level saturation (#2053) | ADOPT | CR3 highlights clipped at the real sensor ceiling | this bundle |
| 3098 | CLI refused-connection tests use a free port (#3070) | ADOPT | 18437 clashes with agents' control ports | this bundle |
| 3099 | xtask bounds each test binary (#3060) | SKIP | draft test infrastructure | — |
| 3100 | ui-egui test caches sized (#3066) | SKIP | draft; its own PR doubts the value | — |
| 3102 | AGENTS.md test rules (#3061) | SKIP | upstream docs | — |
| 3104 | Windows installation policy | SKIP | upstream packaging (draft) | — |
| 3105 | JPEG size limit reports its quality (#3033) | LEARN | engine report worth having; the export dialog is Chromaforge's | — |
| 3106 | Sidebar multi-select | SKIP | stacked on #1153 / #2707 | — |
| 3107 | Sidebar multi-select actions | SKIP | stacked on #3106 | — |
| 3108 | Cut / Copy / Paste albums and keywords | SKIP | stacked on #3107 | — |
| 3109 | SAM 3 masking, model preload, a6400 lens | SKIP | mixed PR; conflicts with Chromaforge's segment work | — |
| 3110 | ai-bench: WIDER FACE | SKIP | benchmark tooling | — |
| 3111 | Enforce coding standards via clippy | SKIP | workspace lint policy change | — |
| 3112 | ai-bench: LFW | SKIP | benchmark tooling | — |
| 3113 | ui-egui test apps render on two workers (#3067) | LEARN | test memory; check the production render pool is unchanged | — |
| 3115 | Batch keyword delete / move by level | SKIP | stacked on #2757 | — |
| 3116 | Transparent sources keep alpha in PNG, TIFF and WebP (#2976) | ADOPT | export correctness | this bundle |
| 3117 | GPU device creation gives up after a time limit (#3068) | ADOPT | hang fix | this bundle |
| 3118 | Memory sizing honours cgroup limits (#3072) | ADOPT | containers, Flatpak, systemd scopes | this bundle |
| 3119 | A cut-short folder move resumes at the next open | SKIP | stacked on #2914 / #1925 | — |
