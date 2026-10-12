# Video

> **Last updated:** 2026-10-12 · **Change:** minor (grading performance measured)

## Grading performance

A clip has one `DevelopSettings`, applied to every frame through the display-referred (JPEG/TIFF) input path of the
photo pipeline: `dac_pipeline::grade::grade_frame(&Rgb32f, &DevelopSettings, Quality) -> Rgba8` (CPU) and
`dac_gpu::grade_frame(&Arc<Rgb32f>, &DevelopSettings, Quality) -> (Rgba8, bool)` (GPU when available, CPU fallback;
the bool says whether the GPU rendered it). `Quality::Draft` fits 960 px wide, `Quality::Full` keeps the frame size.
`video_settings` keeps only the video controls: white balance (relative), Light, tone curve, colour (vibrance,
saturation, HSL), mixer / B&W mix, colour grading, profile and treatment. Everything else (texture, clarity, dehaze,
vignette, grain, detail, optics, geometry, crop, masks, spots, red eye, lens blur, enhance, point colour, calibration,
HDR) is dropped.

Measured 2026-10-12 on a 32-thread Linux box **under heavy load from other jobs (load average ≈ 90)**, so treat
these as upper bounds; synthetic `dac-scenes` frames, exposure / contrast / highlights / shadows / vibrance / WB set.
Draft rows take the full-size frame and resample it; playback should decode at display size instead.

| Frame | Quality | CPU pipeline | GPU pipeline | CPU 3D LUT |
|---|---|---|---|---|
| 960×540 input | Draft/Full | — | — | 13–18 ms (56–79 fps) |
| 1920×1080 | Draft (960 wide) | 440 ms | 35 ms (29 fps) | 69 ms (resample dominates) |
| 1920×1080 | Full | 664 ms | 41 ms (24 fps) | 38 ms (27 fps) |
| 3840×2160 | Draft (960 wide) | 530 ms | 177 ms | 70 ms |
| 3840×2160 | Full | 1515 ms | 157 ms (6 fps) | 99 ms (10 fps) |

`<PREFIX>_PROFILE=1` on the 1080p CPU draft: sample 60–110 ms, white balance 15–25 ms, log luminance 12–40 ms,
the edge-aware Highlights/Shadows base 175–275 ms, finish 46–60 ms. The CPU pipeline is far from 30 fps, so playback
uses a baked LUT:

**3D LUT playback path** (`dac_pipeline::grade::GradeLut`): `GradeLut::bake(&settings)` evaluates the video
controls on a 33³ lattice (sRGB-curve spacing of the linear input, ~30 ms) and `apply(&frame, quality)` /
`lookup(rgb)` interpolate it tetrahedrally (one thread: ~107 ns/px, 55 ms per 960×540 frame; parallel as above).
Highlights/Shadows enter it point-wise (their base taken as the pixel's own luminance); texture, clarity and dehaze
never do (stripped for video anyway). Accuracy vs the real grade on a 480×270 frame (CIE76 ΔE, tested): point-wise
controls mean 0.34 / p99 1.26; with Highlights/Shadows mean 0.61 / p99 2.24. Use the LUT while playing and the real
pipeline (`grade_frame`) on pause and export.

Reproduce: `cargo test --release -p dac-gpu --test grade -- --ignored --nocapture` and
`cargo test --release -p dac-pipeline grade::tests::lut_timings -- --ignored --nocapture`.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-12 | minor | Grading performance: CPU / GPU / LUT timings at 1080p and 4K |
