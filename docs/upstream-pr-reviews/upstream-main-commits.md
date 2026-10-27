# Upstream main: commits reviewed

Non-merge commits on upstream `main` that the fork lacked, judged one by one (policy: no merges, cherry-pick what
helps; `docs/upstream-merge.md`).

**Reviewed up to:** upstream/main `c9860ea7a3fa367451acd9d8cf23538c345192c6` (2026-10-27): nothing new taken (r4): the unlisted commits after `c7961df10` are upstream file-layout refactors (they only move files, and would conflict with every fork file they touch), and the docking follow-ups stay deferred as below. Before that: `c7961df10` against Chromaforge main `be53dd149` (previous pass: `522beb0c3` against `a52c9de11`) (first pass: `31ec62cd0`, 2026-10-25).

Taken commits are cherry-picked with `-x`; crate and app-struct names were rewritten to the fork's, upstream's file
`format` tag for print templates went into `crates/engine/src/legacy.rs` (as for slideshow templates), and doc rows
(`parity-checklist.md`, `gaps.md`, `ui-parity.md`) that conflicted kept the fork's text.

| Upstream sha | Subject | Decision | Reason |
|---|---|---|---|
| 59e9a6a4f | LRC-PRINT-TEMPLATES: print templates export to a file and import from one | taken | engine feature (`print.template.export` / `import`) the fork lacks; usable from CLI / MCP |
| c453cee15 | Contributors: add dotCooCoo (Robert Lee) | skipped | upstream's contributor list |
| adf94a87c | LRC-SS-PLAYBACK: a manual slideshow waits for the arrow keys | taken | engine slideshow playback feature (manual advance) |
| 1c60abb58 | LRC-PRINT-TEMPLATES: template changes that can't be saved are undone | taken | bug fix: failed template saves no longer leave in-memory state diverged |
| 758c5e170 | LRC-DEV-VIEWOPTIONS: the info overlay can show briefly when the photo changes | taken | loupe feature (View Options ▸ Show briefly when photo changes); merged into the fork's loupe |
| a4ddb9198 | LRC-DEV-VIEWOPTIONS: View Options can turn off the loupe's "Rendering…" message | taken | small loupe option; settings field added beside the fork's |
| 0b1b47ee7 | LR-META-INFO: the Info panel can show metadata for the selected photos | skipped | upstream's Info panel (`panels/right.rs`); the fork replaced it with its own Metadata panel |
| d2c8d46a7 | Tests: the launch-list read-only test compares a held lock file without reading it | skipped | the fork already fixed the same Windows lock-file test |
| 9fcf7f5b8 | LRC-DEV-VIEWOPTIONS: switching libraries restarts Show Briefly When Photo Changes | taken | follow-up fix to 758c5e170 |
| a9d2c594b | LRC-PRINT-TEMPLATES: print template tests clean up their folder on failure | taken | test hygiene for the taken print template tests |
| cc557e189 | KEYC-METADATA: Cmd+Alt+Shift+C / V copy and paste metadata | taken | declared shortcuts on the engine commands (the fork's Classic keymap already had them; this covers the Alternative set); fork's `Metadata` menu path kept |
| e25cc3ca0 | LR-META-INFO: Selected Photos reads the selection when the active photo is outside it | skipped | upstream Info panel (replaced in the fork) |
| 8d67b834e | LR-PREF-SHORTCUTS: a declared shortcut yields to a key the saved keymap gives another command | taken | bug fix: one key no longer runs two commands after an upgrade; merged with the fork's Classic-set keys |
| b572dee19 | KEYC-METADATA: Alt+0 / Alt+Shift+0 go to the next / previous keyword set | taken | engine commands `keyword.nextSet` / `previousSet` and keys |
| 7a4e842dd | LR-META-INFO: an Info field saves to the photos selected when its edit started | skipped | upstream Info panel (replaced in the fork) |
| 1b0a9dae1 | LR-META-INFO: the Info panel's GPS field is a TextField, so Esc gives an edit up | skipped | upstream Info panel (replaced in the fork) |
| beb42abb2 | KEYC-METADATA: Cmd+K / Ctrl+K puts the cursor in the Add keyword box | skipped | the fork's Classic keymap already gives ⌘K to the Keywords panel; focusing the box would need porting to the fork's keywording UI (possible follow-up) |
| c3feda15c | LR-META-INFO: the Info panel shows Selected Photos by default | skipped | upstream Info panel (replaced in the fork) |
| 31ec62cd0 | Info panel Target Photo edits, Add Keywords cursor request and Fixed keys follow-ups (#2889) | partly taken | the Fixed keys fix (ignore keys saved for unknown commands) ported by hand; Info panel and Add Keywords parts skipped as above |
| 50df64618 | M0.37: port the app UI, input and capture lifecycle to egui 0.37 | deferred | see *egui 0.37* below |
| c627ce721 | egui-037: preserve modal input ownership and packaged font notices | deferred | part of the egui 0.37 migration |
| 3401286b5 | egui037: preserve modal focus and first-pass text selection | deferred | part of the egui 0.37 migration |
| 03c88c166 | Release 0.14.0 | skipped | upstream version bump; the fork versions its own releases |
| 2b9317519 | feat: startup update prompt modal and direct self-updating | skipped | built on craft-update's built-in suite for upstream, i.e. upstream's release channel; the fork already has its own signed updater (`crates/updater`, `update.*` commands, check at startup, key and repository from `brand.toml`) |
| 8e5a6fc2e | refactor(updates): hold the install back while documents are unsaved | skipped | craft-update follow-up (see 2b9317519); the fork's catalog saves edits as they are made |
| 2d3c7275b | fix(updates): drop the manual window's custom id; pin craft-update to craft-libs main (db19c37) | skipped | craft-update follow-up |
| ee4878509 | fix(updates): address review feedback | skipped | craft-update follow-up |
| 015ca9eeb | fix(updates): localize the update status messages and format the sources | skipped | craft-update follow-up |
| b88f72b9d | sec: bound heic-rs 0.1.4 work around hostile HEIF containers | already in | upstream PR 2770, fork #510 (6e9efdac1) |
| 178eef790 | heif: bound iloc item-extent visits before reconstructing in probe | already in | empty cherry-pick: in fork #510 |
| d00df6319 | security/raw: bound hostile-memory paths in raw decoders | already in | upstream PR 2780, fork #522 (d2fe55732) |
| d8ceb3a6a | security/meta,merge,raw,tiff,denoise: bound memory sized by attacker-shaped inputs | already in | upstream PR 2780, fork #522 (d2fe55732) |
| 269495cd3 | security(heif): budget every reconstruction before it runs | taken | clean pick |
| 0c71f7532 | security(heif): scan the primary item before heic_rs::probe, the alpha plane, and each thumbnail candidate | taken | clean pick |
| 3022baa86 | security(raw,meta): follow-ups from review | taken | deflate row size now follows the bit packing (`packed_row_bytes`, u64); CR3 preview walk skips an over-budget JPEG instead of stopping; the fork's `get`-based slicing and its lossy-JPEG test kept |
| 522beb0c3 | build: pin craft-update to craft-libs main (495e597) (#2990) | skipped | craft-update pin (see 2b9317519) |
| 5ce171da2 | Docking: the workspace, its commands and their parser come from craft-ui (craft-libs 16fb3d4) | taken | Chromaforge PR #612; renames, `show_toast`, the settings writer's issue-#44 fallback kept; the new drag-bar test left out (Classic frames draw no group bar) |
| c964b7f65 | Docking: a dragged divider beside Library or Tools keeps that panel's width | taken | Chromaforge PR #612 |
| a70d7638a | Docking: repin craft-libs 1774443; a dragged divider keeps its panel's width without app code | taken | Chromaforge PR #612 |
| 5b7a4e9a9 | Docs: ui.dock move lists the tab, above and below zones | taken | Chromaforge PR #612 |
| a27d8c05d | Docs: ui.dock above and below name the target's group | taken | Chromaforge PR #612 |
| b2f624631 | Docking: repin craft-libs 6e7f285 (the docking follow-ups) | taken | Chromaforge PR #612 |
| ee1576682 | Docking: a drop at the window's edge makes no dock column | taken | Chromaforge PR #612; its drag test left out |
| e442e3d22 | Docking: a panel with no recorded place opens beside the photo on its side | taken | Chromaforge PR #612 |
| 63ab09257 | View: F8 on a right panel that isn't showing brings it forward in its mode | skipped | Chromaforge's F8 is its own (`right_collapsed` fold of the Classic right side); upstream's hide/restore branch it patches is gone here |
| 1e4cc14df | Docking: group and window menu labels go through tr() in sentence case | taken | Chromaforge PR #612 |
| 02aa1e0f5 | Docs: only the five split zones of ui.dock move have PascalCase names | taken | Chromaforge PR #612 |
| 8e16c05cb | Docking: a panel whose saved place is gone opens beside the photo on its side | taken | Chromaforge PR #612 |
| e8588b925 | Docking: repin craft-ui and craft-docpath to craft-libs 8a503cf | taken | Chromaforge PR #612 |
| 4e6c210a5 | Docking: repin craft-ui and craft-docpath to craft-libs 4dc543c | taken | Chromaforge PR #612 (Chromaforge's craft-ui pin stops here) |
| 8ae15d4f5 | Docking: side panels in dock columns; saved split-frame layouts reset once | deferred | moves Library/Tools out of the Classic side frames into dock columns and resets every saved layout once (the upstream #2839 change skipped before); see *Docking after 4dc543c* below |
| b477efd3d | Docking: repin craft-ui and craft-docpath to craft-libs b6d35e2 | deferred | needs 8ae15d4f5 |
| 4bc5d2b58 | Docking: the web app resets a saved split-frame layout once too | deferred | needs 8ae15d4f5 |
| 577b6cfce | Top bar: the title-bar drag uses craft-window's title_bar, and a panel over the bar keeps the window still on macOS | deferred | egui 0.37 no longer blocks it (craft-window at ea4c823/5bbe9f6 is on egui 0.36), but Chromaforge's top bar diverged (search-field press handling, own `titlebar.rs`): a hand port plus a windowed check of drag and double-click on macOS, Windows and Linux |
| 19ab82147 | Top bar: repin craft-window to craft-libs 4093ea4; the title bar is announced from empty_area's response | deferred | with 577b6cfce |
| 901a115bf | Top bar: repin craft-window to craft-libs main ea4c823 | deferred | with 577b6cfce |
| 1291d391d | Docking: repin craft-ui and craft-docpath to craft-libs main 5bbe9f6 | deferred | after 8ae15d4f5 |
| 649e84b0e | Top bar: repin craft-window to craft-libs main 5bbe9f6 | deferred | with 577b6cfce |
| d4cfd2e6d | LR-BEHAV-TITLEBAR: the checklist row follows craft-window's title_bar | deferred | with 577b6cfce |
| df05e5947 | Docking: the Info fields test window is tall enough for the place fields in the dock column | deferred | after 8ae15d4f5 |
| 387bed199 | chore(deps): refresh safe dependency bumps (#2994) | skipped | harfrust 0.13 would duplicate epaint 0.36's 0.12 and brotli 9 adds a second brotli-decompressor; miniz_oxide 0.9 in engine removes no copy (png keeps 0.8); the lockfile refresh is regenerated by Chromaforge's own updates |
| 8020cd081 | showcase: check the file is readable without an empty with-block (#3001) | taken | run.py only; upstream's stray `__pycache__` .pyc left out |
| c7961df10 | Camera profile: Sony ILCE-7RM5 (α7R V) (#3022) | taken | contributor's own fit (MIT OR Apache-2.0); README and raw-parity counts follow, attribution row uses `<cli>` |

### egui 0.37 (deferred, 2026-10-26)

Upstream's migration (50df64618, c627ce721, 3401286b5: ~9,500 lines over ~190 files) is not ported. Blockers:

- **Pinned forks.** Upstream patches 15 egui-family crates to `storytold/egui@6d518fc` and `winit` to
  `storytold/winit@a517b38` (`[patch.crates-io]`), not crates.io 0.37.0. The fork would build its UI on unreleased
  third-party commits it does not control, and would have to review them (and every later re-pin) itself.
- **HDR output.** The fork's `vendor/egui-wgpu` (0.36.2 + HDR surface, upstream PR 1700) must be rebased
  onto the pinned 0.37 egui-wgpu; HDR was verified by hand on all three OSes and needs that again.
- **Diverged UI.** The fork's Classic panels, menus (`menus_fork.rs`), keywording and Info panel replace much of what
  the migration edits; the commits would be a hand port, not a cherry-pick.
- **Verification** needs the windowed app on macOS, Windows and Linux (input capture, modal focus, IME, text
  selection), which an unattended agent should not drive.

Plan when it is taken up: (1) wait until egui 0.37.x on crates.io carries the fixes upstream pins (or review the
`storytold/egui` diff against 0.37.0 and vendor only what the fork needs, as with egui-wgpu); (2) rebase
`vendor/egui-wgpu` HDR onto it; (3) bump `egui`/`eframe`/`egui_extras`/`egui_kittest` with the `default_fonts` +
`monochrome_emoji_fonts` features; (4) port the API changes crate by crate, using 50df64618 as the map, then
c627ce721 and 3401286b5 for modal input ownership and text selection; (5) manual check of HDR, modals, text fields
and shortcuts on the three OSes before a release.

### Docking after 4dc543c (deferred, 2026-10-26)

Chromaforge takes upstream's move of the workspace, `ui.dock` commands and parser into craft-ui up to the craft-libs
`4dc543c` pin. The next step, 8ae15d4f5, puts Library and Tools into dock columns instead of the side frames the
Classic layout is built on, and resets every saved split-frame layout once. A later port needs: (1) a decision that
the Classic side panels may live in dock columns (folding headers, the Presets fold and `right_collapsed` must work
there); (2) the one-time layout reset replaced by a migration that keeps a person's panel widths and visibility;
(3) the craft-ui repins b6d35e2 and 5bbe9f6 with 8ae15d4f5, b477efd3d, 4bc5d2b58, 1291d391d and df05e5947 together;
(4) a windowed check of drag, float and re-dock in Library and Develop. The title-bar series (577b6cfce, 19ab82147,
901a115bf, 649e84b0e, d4cfd2e6d) can go separately: it adds `craft-window` at 5bbe9f6 (egui 0.36) and replaces
`crates/ui-egui/src/titlebar.rs`; port the search-field press rule into the `title_bar` call, then check drag and
double-click by hand on all three OSes.
