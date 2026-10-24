# High quality adjustment previews

Settings ▸ Performance ▸ **High quality adjustment previews** keeps the Detail view at its settled
rendering quality while a slider is dragged, instead of the faster adaptive drafts. It applies at
once (also in the middle of a drag), is saved as an application preference (`highQualityAdjustmentPreviews`
in `ui.set` / the settings file) and is off by default. Contributed by Kaan88 (fork PR #426).

## What it renders

With the setting on, a drag renders what the view shows at rest:

- **Fit:** the same render size, full-quality processing and Lanczos display fit as the settled
  view, so the picture does not soften while the slider moves and does not change when it is let go.
- **Zoomed in (100 % and above):** the existing zoomed-window renderer, one window of the visible
  part per pane at native resolution with the filter context around it, not the whole original per
  update. The window reuses its stage cache through the drag.
- **Before/After, the HDR view, Compare / Reference zoomed panes and the second window** follow the
  same choice. HDR edits stay on the float HDR display path throughout (fork PR #423).

The **Preview size** preference, the GPU's texture limit, source availability and colour management
still apply. Exports, undo history and the HDR output mode are unchanged. Updates are still
coalesced: a newer value replaces queued ones, an older result never replaces a newer one on screen,
and the last finished image stays visible until the next is ready.

With the setting off, the adaptive drafts remain: up to 60 % of the width and height on the CPU (or
after slow drags), cheaper processing and no display fit (see `region::DragScale`).

## When full quality is too slow

A heavy edit (many masks, AI denoise, the CPU path, a 6K display) can take hundreds of milliseconds
per full-quality update, which leaves the slider dragging a frozen picture. So a drag falls back
(`region::HqDrag`):

1. Every drag starts at full quality. Its updates are timed as their render plus the frame that
   shows them, leaving out the first (which may rebuild stage caches). What is timed is what shows
   the drag: the zoomed window when there is one, otherwise the whole frame.
2. Once the median is over **100 ms** (fewer than ten updates a second), or one update is over
   **300 ms**, the drag falls back for the rest of the drag.
3. In the fallback, while the slider **moves**, the view renders the adaptive drafts the setting
   otherwise replaces. Once the slider **holds still for 0.2 s**, mouse button still down, it
   renders full quality again, so the picture is sharp whenever you stop to look. Moving again
   drafts again. Letting go renders full quality as always.
4. A slider that fell back on a photo starts its next drags on that photo in the fallback (no
   frozen updates to find out again), for 30 s; then a drag tries full quality again. A retry that
   is still slow doubles the wait (up to 4 min); one that keeps up forgets it. Other sliders and
   other photos are timed afresh.

`ui.inspect` → `perf.adjustQuality` reports the decision: `idle` (no drag, or the setting is off),
`full`, `draft` (falling back, slider moving) or `settled` (falling back, slider still).

## Measurements

Kaan88's warmed measurements at Fit in a 1600 × 1000 window on an RTX 4070 Ti SUPER (DX12), HDR
output off. Latency includes control-channel dispatch; update rates are Main-render completions
during a burst, not compositor FPS. Local measurements, not guarantees.

| File | Adaptive latency | High quality latency | Adaptive updates/s | High quality updates/s |
|---|---:|---:|---:|---:|
| CHDK DNG, 3264 × 2448 | 35 ms | 70 ms | 40.5 | 21.4 |
| JPEG, 3264 × 2448 | 35 ms | 70 ms | 40.6 | 21.1 |
| Sony ARW, 6000 × 4000 | 35 ms | 85 ms | 28.6 | 18.4 |

These stay at full quality: they are inside the 100 ms budget. With high quality on, the pixels of
the last update and of the release matched exactly for all three files at Fit and 100 %.

## Tests

`crates/ui-egui/src/tests_adjustment_quality.rs` (headless, procedural photo): preference round
trip and default, the settings control, switching during a drag, 4:3 and 3:2 photos at 100–200 %
display scaling, preview and texture limits, proofed CPU rendering, every Before/After layout,
native-resolution windows instead of tiles, HDR float output, and the fallback with renders that
report as slow (whole frame and zoomed window: drafts while moving, settled render while held
still, the next drag starting in the fallback) or fast (never falls back). `region.rs` unit tests
cover `HqDrag` itself, including hostile clocks and timings.

Sources: own design, on the existing render scheduling (`DragScale`'s timing and verdicts).
