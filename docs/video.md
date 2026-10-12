# Video

> **Last updated:** 2026-10-12 · **Change:** M14.8 core: `dac-video-export` crate (S3: option A)

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

## Next

Catalog format 10 + import (M14.2), grid posters (M14.3), player (M14.4), trim/poster (M14.6), develop on video
(M14.7), export (M14.8). See `plan/video-filmcraft.md` §9.
