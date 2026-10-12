# Video

> **Last updated:** 2026-10-12 · **Change:** minor (M14.1: Matroska/WebM, MPEG-TS, MXF; VP9, AV1, ProRes, DNxHR, APV, MPEG-2)

Video support is milestone M14 (`plan/video-filmcraft.md`, gaps.md row 18, parity rows LR-VID-*). Containers and
codecs come from [FilmCraft](https://github.com/storytold/filmcraft) (pure Rust, clean-room, `MIT OR Apache-2.0`),
used as git dependencies pinned to one full revision. The app writes no codecs or containers of its own.

## `crates/video` (`dac-video`, L3, native only)

| API | What it does |
|---|---|
| `probe(path) -> Result<VideoInfo, VideoError>` | Container and stream headers, no decoding: `duration` (s), `fps_num`/`fps_den`, display `width`/`height` (after rotation), `codec` (`h264`, `hevc`, `vp9`, `av1`, `prores`, `dnxhd`, `apv`, `mpeg2`, or the container's name for codecs we don't decode), `has_audio`, `created` (QuickTime `creationdate`/`©day` as written; Matroska `DateUTC` as RFC 3339), `gps` (ISO 6709), `rotation`, `hdr`, `decodable`. |
| `FrameReader::open(path)` | Opens a clip with a decodable video track ([supported formats](#supported-formats)); a recognised but undecodable stream is `Unsupported` with the reason. |
| `FrameReader::frame_at(t, max_side) -> Result<Frame, VideoError>` | The frame displayed at `t` seconds (clamped to the clip; NaN = 0), box-decimated so the longer side is at most `max_side`. `Frame { image: dac_raster::Rgb32f, time }`: linear Rec.709 RGB, the pipeline's JPEG/TIFF input type; `time` is the frame's presentation time. |
| `FrameReader::frame_times()` | Every frame's presentation time, sorted (frame stepping). |
| `poster(path, t, max_side)` | One frame for thumbnails; `t = None` means `default_poster_time(duration)` = 10 % in. |

**Seeking.** Each container is reduced to one sample table (decode order, presentation times, sync flags), so seeking
is container-independent: `frame_at` finds the sample presented at `t` (MP4 edit lists applied; frame times count
from the first shown frame), decodes from the preceding sync sample (one GOP further back for an open GOP's leading
pictures), and keeps the decoder while later times stay in the same GOP, so playback and frame stepping don't
re-decode from the keyframe. B-frame reordering is handled by matching the decoder's output pts. MPEG-TS/PS and MXF
times are display positions at the stream's frame rate (TS: the coded rate unless the PTS say otherwise).

**Colour.** The stream's colour comes from size-based defaults (BT.601 matrix up to 1024×576, else BT.709, limited
range), overridden by the bitstream (H.264/HEVC VUI, AV1 sequence header, VP9 colour space, APV/ProRes/MPEG-2 headers),
overridden by the container (`colr` box, `vpcC`, Matroska `Colour`, MXF full-range coding). FilmCraft's converter turns Y'CbCr
into linear light; the crate then converts BT.2020/P3 primaries to Rec.709 and, for HLG (BT.2100 OOTF, 1000 cd/m²) and
PQ, tone-maps to SDR with the BT.2390 EETF, reference white (203 cd/m²) at 1.0. True HDR output waits for gap Q.

**Errors, never panics.** `VideoError::{Io, Invalid, Unsupported, Decode, Internal}`. Every call into FilmCraft runs
under `catch_unwind` (the safety net the never-crash standard allows), and a failed decoder is discarded. Tests sweep
truncations, random byte corruption and garbage files.

**Web build.** On wasm32 the API exists and returns `Unsupported`; the FilmCraft crates aren't linked.

## Supported formats

Probe works for every container below; frames decode when the codec cell says so. "Unsupported" streams probe with
`decodable: false`, and `FrameReader::open` returns `VideoError::Unsupported(reason)`.

| Container | Extensions | Decoded video |
|---|---|---|
| MP4 / QuickTime | `.mp4` `.m4v` `.mov` | H.264, HEVC, VP9, AV1, ProRes, DNxHD/DNxHR, APV, MPEG-1/2 (XDCAM, IMX, HDV fourccs) |
| Matroska / WebM | `.mkv` `.webm` | H.264, HEVC, VP9, AV1, ProRes, APV, MPEG-1/2 |
| MPEG-TS / M2TS | `.ts` `.m2ts` `.mts` | H.264, HEVC, MPEG-1/2 (Annex B, in-band parameter sets) |
| MPEG program / elementary stream | `.mpg` `.vob` `.m2v` | MPEG-1/2 |
| MXF (OP1a, OP-Atom) | `.mxf` | H.264, DNxHD/DNxHR (VC-3), ProRes, MPEG-2 (D-10/IMX, XDCAM) |

| Codec | FilmCraft crate | Notes |
|---|---|---|
| H.264 | `filmcraft-h264` | progressive 8-bit 4:2:0 only. **Interlaced** (field / MBAFF: AVCHD 1080i, broadcast) is detected from the SPS and reported as `Unsupported("interlaced H.264 …")`; High 4:2:2 / 10-bit likewise unsupported |
| HEVC | `filmcraft-hevc` | Main, Main 10 |
| VP9 | `filmcraft-vp9` | profiles 0–3, 8/10/12-bit; transfer/primaries from `vpcC` / Matroska `Colour` |
| AV1 | `filmcraft-av1` | slowest decoder: ~50 ms per 1080p frame on the reference machine |
| ProRes | `filmcraft-prores` | all 422/4444 profiles (alpha ignored) |
| DNxHD / DNxHR | `filmcraft-dnx` | incl. RGB 4:4:4 |
| APV | `filmcraft-apv` | |
| MPEG-1/2 | `filmcraft-mpeg2v` | interlaced pictures come out as woven frames (no deinterlacing yet) |
| VP8, MJPEG, others | – | probed, unsupported |

Pixel aspect ratio is not applied yet (anamorphic HDV / DV / DNxHD 1440 show unstretched). Only the first video track
is read; audio is detected (`has_audio`) but not decoded (playback is muted in v1).

## Licences: why not `filmcraft-codecs`

The plan named `filmcraft-codecs` (the container/codec hub with GOP caching). At rev 156086dc it depends
unconditionally on `symphonia` (MPL-2.0, audio decoding), which `deny.toml` rejects (`cargo deny check licenses`
fails). `dac-video` therefore uses the crates below it directly: the demuxers `filmcraft-isobmff`, `-matroska`,
`-mpegts`, `-mxf`; the decoders `filmcraft-h264`, `-hevc`, `-vp9`, `-av1`, `-prores`, `-dnx`, `-apv`, `-mpeg2v`;
and `filmcraft-bitstream`, `-frame`, `-color`, `-time`. Each is std-only (plus `thiserror` and `rayon`), `MIT OR
Apache-2.0`, with no `unsafe` and no C; `cargo deny check licenses` passes. The per-codec frame conversions follow
`filmcraft-codecs` (same licence) with bounds-checked plane access. `filmcraft-h264enc` is a dev-dependency only:
the tests generate their clips in-test, so no media is committed.

## Cost (2026-10-12, 32-core Linux)

- Cold release build of `dac-video` and its FilmCraft dependencies: ~8 s wall with MP4 + H.264/HEVC only (spike S1
  measured ~20 s with the example), ~15 s with every container and codec above.
- A stripped binary using the API (the `clips` test binary): 2.6 MB → 5.2 MB with every container and codec.
- Decoding vs the `ffmpeg` oracle (dev only, not in CI), frame at 4 s, PSNR in sRGB: Big Buck Bunny 1080p excerpts
  H.264 38.6 dB, HEVC 38.6 dB, VP9 38.5 dB, AV1 38.5 dB (the four agree with each other at 32–34 dB, the excerpts
  being separate encodes of one source); the NPS 720p H.264 clip 47.3 dB. Spike S1: a 1080p libx264 clip with
  B-frames, 48.9 dB.
- First frame 2 s into a 1080p clip (decodes from the keyframe, single reader, no cache): H.264 ~0.6 s, HEVC ~0.5 s,
  VP9 ~0.7 s, AV1 ~3.1 s.

**Tests.** `tests/clips.rs` (synthetic H.264 MP4: colour, seeking, fuzz), `tests/formats.rs` (synthetic ProRes,
DNxHR, APV in MOV/MP4, ProRes and DNxHR in MXF, H.264 with B-frames in Matroska and in a hand-muxed transport stream:
probe fields, frame times, seeking, truncation and corruption sweeps), `tests/corpus.rs` (the clips from `cargo xtask
corpus --download`, skipped when absent: probe fields, neighbour/far-frame PSNR, the four Big Buck Bunny decoders
against each other, AVCHD 1080i reported interlaced, truncated and corrupted copies of each). There is no synthetic
MPEG-2, VP9 or AV1 clip (FilmCraft has no encoder for them); VP9 and AV1 are covered by the corpus clips, MPEG-2
only by FilmCraft's own tests.

```sh
cargo run --release -p dac-video --example frame_png -- clip.mp4 1.5 out.png [max_side]
```

## Next

Catalog format 10 + import (M14.2), grid posters (M14.3), player (M14.4), trim/poster (M14.6), develop on video
(M14.7), export (M14.8). See `plan/video-filmcraft.md` §9.
