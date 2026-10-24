# HDR adjustment previews

The main display uses background `RenderJob`s and per-view slots. Standard textures carry
8-bit SDR renditions; HDR views request `OutputDepth::F32Hdr` in linear sRGB primaries,
retain `DeepSamples::F32`, and upload through `HdrOutput`. The desktop output uploads
`Rgba16Float` textures to the existing extended-linear / PQ surface pipeline. Display
headroom and HDR Preview selection continue to control whether these views are requested.
The compositor handles output gamut conversion; extended linear RGB can represent colours
outside the sRGB gamut. No extra display profile conversion is applied to the float samples.

Previously the loupe, zoom window, Compare and second window explicitly suppressed HDR
requests during interactions. Their 8-bit drafts used `sdr_rendition()` instead. Releasing
an adjustment restored float HDR rendering. Exact-key texture selection also discarded
the displayed HDR frame while each asynchronous settings update was pending.

HDR requests now use draft quality during interactions and full quality on release, with
the same float output depth and HDR processing for both. The loupe keeps its existing
resolution planning; zoomed previews render only the visible window. HDR slots reuse stage
caches and accept completed requests newer than the last displayed request, even when a
newer request is still pending. Older completions cannot overwrite newer pixels. Retained
HDR frames cover the same photo and region; navigation invalidates that coverage and
retires older in-flight requests. Whole-frame resolution changes retain HDR until updated.
SDR renditions, export caches, soft proof and diagnostic overlays keep their existing paths.

## Validation

- `cargo test -p lcc-ui-egui --lib hdr`: mocked HDR uploads verify linear samples and live
  adjustment updates, frame retention, out-of-order completion, region invalidation,
  HDR Preview selection, and unchanged SDR thumbnails and renditions.
- Existing GPU HDR equivalence tests compare float output with the CPU implementation.
- `cargo xtask ci` runs the repository quality gates.

## Local results (2026-10-22)

- All 12 HDR UI tests passed, including continuous fit and zoomed adjustments, retained HDR
  frames during release, settings reversals and region invalidation.
- GPU HDR equivalence passed against the CPU renderer.
- The desktop build passed. An isolated demo session on Windows / NVIDIA GeForce RTX 4070
  Ti SUPER (DX12) reported `hdr` in all 70 sampled live adjustment states across fit and
  1:1 views, with no GPU fallback. Screenshots were captured and inspected. The session
  also exercised HDR Preview off, grouped adjustment undo/redo, SDR PNG and float HDR TIFF
  export. This verifies presentation routing, not measured monitor luminance.
- Final-source formatting and Clippy passed. `cargo xtask ci --fast` passed all 14 gates,
  including WASM; Android was skipped because the NDK is absent.
- Full CI stopped at the MCP idle-reconnect socket test (Windows `WouldBlock`). The failing
  test passed in isolation and all 54 MCP tests passed on a separate run (one ignored).
- The workspace run excluding MCP reached the full UI suite: 1,526 passed, four ignored,
  and the unrelated `tests_folder_drop::dont_ask_again_moves_the_next_drop_at_once` failed
  its immediate file-move assertion. That test passed when rerun in isolation. The broad
  runs are therefore recorded as failures, not as a clean full CI result. The remaining
  updater, video, video-export, web-gallery, spooler and xtask tests passed separately.
- Under concurrent compilation, the native smoke run's median sampled UI update time was
  31.4 ms. There is no before/after performance baseline; no 16 ms guarantee is claimed.

## Adjustment performance

HDR stage caches follow the same memory-trimming policy as their corresponding SDR
views. The active loupe cache is retained, and Before/After and zoom-window caches are
evicted only at the existing hard limit. Previously HDR caches were classified as
background work and could be discarded every frame above the soft budget.

HDR texture encoding uses the existing raster worker pool. The transfer curve,
headroom handling and half-float samples are unchanged; regression tests compare the
parallel result bit for bit with the scalar encoder. This reduces serial work during
upload without changing render resolution or processing quality.

Separate SDR and HDR renders, GPU readback, and synchronous texture upload still have
costs. Removing the SDR render needs a separate design for diagnostics, screenshots,
colour readout and fallback display. These changes do not eliminate that work.

Sources: own design, based on the existing cache policy and raster row workers.

Local development-build validation (2026-10-22, RTX 4070 Ti SUPER / DX12): the
2560 × 1707 encoding microbenchmark measured 82.6 ms scalar versus 9.9 ms parallel
(median of nine warmed runs). In a temporary combination with the high-quality
preview PR, the supplied DNG, JPEG and ARW measured 279–300 ms before versus
117–145 ms after at Fit, and 180–230 ms versus 74–107 ms at 100%. Repository
checks ran concurrently, so native timings are indicative, not controlled benchmarks.
All six HDR and six SDR high-quality cases retained identical captured image pixels
on release; the HDR cases retained the HDR presentation route throughout.
The high-quality setting is not included in this branch.

## Physical display checks

Automated tests cannot certify emitted monitor luminance. On an HDR display with OS HDR
and HDR Preview enabled, open an HDR edit with highlights above reference white. Drag
exposure, contrast, highlights, shadows, whites, blacks and colour controls, rapidly reverse
direction, then release. Check both fit and 1:1, Compare, Before/After and the second window.
There should be no SDR transition during the drag or while the final frame completes.
Repeat with HDR Preview disabled, an SDR edit and an SDR-only display. Move the window
between SDR and HDR displays and verify the existing capability decision follows it.
Save, undo/redo and export HDR and SDR outputs and reopen them to check their independent
rendering paths. Diagnostic overlays and soft proof intentionally remain SDR views.

## Branch transfer

The changes were transferred to `fix/hdr-adjustment-previews` in the checkout now using
Kaan88's fork as origin, based on Chromaforge's main at `dfd29a82c`.
The upstream filmstrip additions and newer parity notes were preserved. Formatting and
diff checks passed after the transfer. The test results above were collected on the
original base (`79a0ba0e5`); the runtime suites have not been rerun on this newer base.
