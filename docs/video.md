# Video

> **Last updated:** 2026-10-12 · **Change:** M14.8: video export wired through `dac_engine::export`

Video support is milestone M14 (`plan/video-filmcraft.md`, gaps.md row 18, parity rows LR-VID-*). Containers and
codecs come from [FilmCraft](https://github.com/storytold/filmcraft) (pure Rust, clean-room, `MIT OR Apache-2.0`),
used as git dependencies pinned to one full revision. The app writes no codecs or containers of its own.

## `crates/video` (`dac-video`, L3, native only)

| API | What it does |
|---|---|
| `probe(path) -> Result<VideoInfo, VideoError>` | Container and stream headers, no decoding: `duration` (s), `fps_num`/`fps_den`, display `width`/`height` (after rotation), `codec` (`h264`, `hevc`, or the fourcc), `has_audio`, `created` (QuickTime `creationdate`/`©day`, as written), `gps` (ISO 6709), `rotation`, `hdr`, `decodable`. |
| `FrameReader::open(path)` | Opens a clip with a decodable video track (H.264 or HEVC in MP4/MOV). |
| `FrameReader::frame_at(t, max_side) -> Result<Frame, VideoError>` | The frame displayed at `t` seconds (clamped to the clip; NaN = 0), box-decimated so the longer side is at most `max_side`. `Frame { image: dac_raster::Rgb32f, time }`: linear Rec.709 RGB, the pipeline's JPEG/TIFF input type; `time` is the frame's presentation time. |
| `FrameReader::frame_times()` | Every frame's presentation time, sorted (frame stepping). |
| `poster(path, t, max_side)` | One frame for thumbnails; `t = None` means `default_poster_time(duration)` = 10 % in. |

**Seeking.** `frame_at` finds the sample presented at `t` (edit lists applied), decodes from the preceding sync sample,
and keeps the decoder while later times stay in the same GOP, so playback and frame stepping don't re-decode from the
keyframe. B-frame reordering is handled by matching the decoder's output pts.

**Colour.** The stream's colour comes from size-based defaults (BT.601 matrix up to 1024×576, else BT.709, limited
range), overridden by the bitstream VUI, overridden by the container's `colr` box. FilmCraft's converter turns Y'CbCr
into linear light; the crate then converts BT.2020/P3 primaries to Rec.709 and, for HLG (BT.2100 OOTF, 1000 cd/m²) and
PQ, tone-maps to SDR with the BT.2390 EETF, reference white (203 cd/m²) at 1.0. True HDR output waits for gap Q.

**Errors, never panics.** `VideoError::{Io, Invalid, Unsupported, Decode, Internal}`. Every call into FilmCraft runs
under `catch_unwind` (the safety net the never-crash standard allows), and a failed decoder is discarded. Tests sweep
truncations, random byte corruption and garbage files.

**Web build.** On wasm32 the API exists and returns `Unsupported`; the FilmCraft crates aren't linked.

## `crates/video-export` (`dac-video-export`, L3, native only)

`export_clip(src, (trim_in, trim_out), grade, &ClipExportOptions { format, quality, max_side, out }, cancel, progress)
-> Result<ClipReport, ExportClipError>`: decodes the trimmed range with `dac_video::FrameReader` (first frame = the one
displayed at the in point, decoded from the previous sync sample), runs `grade(&mut dac_raster::Rgb32f)` on every
frame (linear Rec.709, display-referred), converts to BT.709 limited-range Y'CbCr and encodes H.264 High 4:2:0
(`filmcraft-h264enc`, CRF from quality) into MP4 or ProRes 422 10-bit (`filmcraft-prores`, HQ/Standard/LT) into MOV.
A source AAC track is decoded (with pre-roll, honouring its edit list) and re-encoded as AAC-LC for the same range,
the encoder delay hidden by an edit list. Output goes to `<out>.partial` and is renamed on success; errors, panics and
cancel remove it. Other source audio codecs export without audio for now.

**Spike S3 (export option A vs B):** option A. `filmcraft-export`'s `FrameRenderer` hook is a global registry of
plain `fn()` factories rendering a `filmcraft-project` sequence, so a per-export grade closure cannot be passed
without global state, and the crate pulls `project`/`render`/`text`/`audio-dsp` plus `filmcraft-media`/`-codecs`
(symphonia, MPL-2.0: rejected by `cargo deny`). Option A is ~600 lines over isobmff, h264enc, prores and aac.

## Export through the engine (M14.8)

`dac_engine::export` is the one export path for app, CLI and MCP (`crates/engine/src/export_video.rs`). A clip
(`Photo.video` set) in a batch exported to a rendered format becomes a movie; `Original` copies its file. It keeps
the clip's trim (`trim_in`/`trim_out`) and its develop settings reduced by `video_settings`, graded on every frame at
full quality by `dac_pipeline::grade::grade_frame_linear` (float, linear Rec.709 in and out, so the export crate's
clamp + sRGB encode after the grade is the only encode; an 8-bit round trip would band ProRes). A default grade is the
identity and skips the pipeline. Options (`ExportOptions`, validated like every export key): `videoFormat`
(`h264` default = H.264 + AAC MP4, `prores` = ProRes 422 + AAC MOV), `videoQuality` 1–100 (default 80), `videoMaxSide`
16–8192 (0 = the clip's size). The file name keeps the naming template with `.mp4` / `.mov`.

The movie is written to a temporary file on the worker (`PreparedExport::run`, beside the batch's other items) and
moved into place in batch order (`Exported::video_file`, `Exported::save`); a skipped, failed or cancelled clip
leaves nothing behind. `set_video_hooks(items, &VideoHooks { cancel, progress })` gives clips the batch's
activity-row flag (checked once per frame) and a per-clip progress callback: the desktop background export and the
headless MCP export show it as the row's detail (`clip.mp4 42%`). The Export dialog shows a Video section (format,
quality, long edge) when the selection holds clips. CLI: `<cli> render clip.mp4 -o out.mp4` (or `.mov`).

Limits: the import probe that fills `Photo.video` lands with the engine wiring of M14.2; frames are graded one at a
time on the CPU (~0.7 s per 1080p frame, see Grading performance); no "export current frame as JPEG" command yet.

## Licences: why not `filmcraft-codecs`

The plan named `filmcraft-codecs` (the container/codec hub with GOP caching). At rev 156086dc it depends
unconditionally on `symphonia` (MPL-2.0, audio decoding), which `deny.toml` rejects (`cargo deny check licenses`
fails). `dac-video` therefore uses the crates below it directly: `filmcraft-isobmff` (MP4/MOV demux),
`filmcraft-h264`, `filmcraft-hevc`, `filmcraft-frame`, `filmcraft-color`, `filmcraft-time`, which pass the licence
check. Matroska/WebM, MXF, MPEG-TS, AV1, VP9 and ProRes decode need either `symphonia` made optional upstream
(FilmCraft feature) or the same direct approach per container/codec. `filmcraft-h264enc` is a dev-dependency only:
the tests generate their clips in-test, so no media is committed.

## Cost (spike S1, 2026-10-12, 32-core Linux)

- Cold release build of `dac-video` and its FilmCraft dependencies: ~20 s wall.
- The spike example (`examples/frame_png.rs`, probe + decode + PNG write), stripped: 2.1 MB including `image`'s PNG
  encoder.
- Decoding vs the `ffmpeg` oracle (dev only, not in CI): a 1080p libx264 clip with B-frames, frame at 1.5 s,
  PSNR 48.9 dB in sRGB; a seek that decodes 15 frames from the keyframe took ~480 ms (single reader, no cache).

```sh
cargo run --release -p dac-video --example frame_png -- clip.mp4 1.5 out.png [max_side]
```

## Revision history

- 2026-10-12: M14.8 core, `dac-video-export` (S3: option A).
- 2026-10-12: M14.8 export through the engine (options, dialog, CLI, MCP, activity row).

## Next

Catalog format 10 + import (M14.2), grid posters (M14.3), player (M14.4), trim/poster (M14.6), develop on video
(M14.7), export (M14.8). See `plan/video-filmcraft.md` §9.
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
