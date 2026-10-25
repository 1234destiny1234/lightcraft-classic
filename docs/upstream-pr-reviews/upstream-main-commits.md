# Upstream main: commits reviewed

Non-merge commits on upstream `main` that the fork lacked, judged one by one (policy: no merges, cherry-pick what
helps; `docs/upstream-merge.md`).

**Reviewed up to:** upstream/main `31ec62cd0387abecd8f2a2da034aee591447897d` (2026-10-25), against fork main `ffc249d03`.

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
