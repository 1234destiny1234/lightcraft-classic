# High quality adjustment previews

Settings > Performance > Rendering > **High quality adjustment previews** keeps the
Detail view at its settled rendering quality while adjusting sliders. It applies
immediately and is saved as an application preference. It is off by default.

With it enabled, Fit uses the same render size, full-quality processing and display
downsampling as the settled view, including Lanczos fitting for the SDR view.
At 100% zoom and above, normal zoomed views use the existing visible-window
renderer, keeping native detail and surrounding filter context instead of
rendering the entire original for each update. Before/After views use the same
quality choice. Views that already require a whole-frame fallback retain their
settled rendering behavior and can be substantially slower.

The **Preview size** preference remains a size limit. High quality adjustments do
not override it, hardware texture limits, source availability, or color management.
They do not change exported images, undo history, or the selected HDR output mode.
This preference does not enable HDR adjustment output on a baseline that only
presents HDR at rest. Combined with the separate HDR-preview fix, both quality
modes retain that fix's continuous floating-point HDR presentation.

High quality can update less frequently with expensive edits, CPU rendering, or
large displays. The UI continues to accept adjustments while rendering. Queued
obsolete updates are replaced and older results cannot overwrite newer displayed
results; the last completed image remains visible until a newer one is ready.

With the option off, the existing adaptive previews remain: drafts may use 60% of
the width and height, cheaper processing, and a faster display sampling path.
Disabling only the resolution reduction would not reproduce settled sharpness.

## Validation

Procedural tests cover preference persistence and defaults, the settings control,
switching quality during a drag, 4:3 and 3:2 images at 100%, 125%, 150% and 200%
display scaling, preview limits, proofed CPU rendering, Before/After layouts, and
native-resolution windows. Fine-detail image tests compare actual texture pixels
while adjusting and after release, with noise reduction and tonal edits enabled.
Renderer tests exercise out-of-order completion at both draft and full quality.

For native validation, use the supplied CHDK DNG, JPEG and Sony ARW outside version
control. At Fit and 100%, capture rest, adjustment and release in both quality
modes. Measure warmed update latency separately from initial decode/render time.
Repeat on the base branch and in temporary combinations with the pixel-placement
and HDR-preview changes, with HDR display output on and off. On the HDR combination,
verify float highlight values remain above reference white during the drag.
Screenshots and software tests cannot measure actual HDR monitor luminance.

### Native measurements

On the supplied files at Fit in a 1600 x 1000 window, using an RTX 4070 Ti SUPER
(DX12), the following warmed measurements were recorded with HDR display output
disabled. Latency includes control-channel dispatch and waiting for completion;
update rates are observed Main-render completions during a burst, not compositor
FPS. These are local measurements, not performance guarantees; repository checks
were also running during this validation.

| File | Adaptive latency | High quality latency | Adaptive updates/s | High quality updates/s |
|---|---:|---:|---:|---:|
| CHDK DNG, 3264 x 2448 | 35 ms | 70 ms | 40.5 | 21.4 |
| JPEG, 3264 x 2448 | 35 ms | 70 ms | 40.6 | 21.1 |
| Sony ARW, 6000 x 4000 | 35 ms | 85 ms | 28.6 | 18.4 |

With high quality enabled and HDR output disabled, the captured image pixels
matched exactly between the final adjustment update and release for all three
files, at Fit and 100% zoom. The baseline's separate HDR-on-release transition
remains until the HDR-preview fix is applied.

The temporary integration checkout combined HDR-preview commit `29859f267` and
pixel-placement commit `bc52e269d` with this feature. Its eight quality tests,
existing HDR tests, pixel-placement tests, Before/After tests and geometry tests
passed, as did Clippy, formatting and its desktop build. Native captures repeated
all three files at Fit and 100%, in both quality modes, with HDR output enabled
and disabled: 192 capture states across standalone and combined configurations.
In the combined HDR run, HDR remained active throughout adjustments in both modes;
high quality image pixels matched exactly on release at both zoom levels.

Fast repository CI passed all 14 gates, with the Android NDK and dependency-license
checks skipped because their tools were unavailable. Full repository CI's UI suite
reported 1537 passed, four ignored and two failures: the previously reproduced
`click_widget_right_clicks` context-menu failure, and
`dont_ask_again_moves_the_next_drop_at_once`, which passed in isolation on both an
earlier binary and this feature build. All eight new tests passed in the full run.
The external camera corpus and optional font bundle were unavailable.

The HDR-preview patch overlaps the HDR job's draft-quality selection. When merging
the two changes, retain the quality-aware condition from this feature so high
quality HDR jobs remain full quality.

Sources: own design, using the existing rendering and scheduling architecture.
