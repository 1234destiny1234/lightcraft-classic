# Video

> **Last updated:** 2026-10-12 · **Change:** minor (engine commands for playback, trim, poster, Capture Frame)

Video clips in the library: plan in `plan/video-filmcraft.md` (M14), rows in
[parity-checklist.md](parity-checklist.md) → R. Video, gap 18 in [gaps.md](gaps.md).

## Engine: playback, trim, poster, Capture Frame (M14.4 / M14.6)

`crates/engine/src/video/` holds the state, `crates/engine/src/cmd/video.rs` the commands. The engine
does not decode video: the host installs a `VideoBackend` (`probe`, `frame_at`) in
`session.video.backend`. Without one (the web build, a host without the decoder) every `video.*`
command is disabled with "video playback isn't available in this build". All commands act on the
active photo and are enabled only when it is a video.

| Command | Params | Result |
|---|---|---|
| `video.get` | — | `{id, duration, fps, frameRate, width, height, codec, hasAudio, canPlayAudio, current, frame, playing, looping, trimIn, trimOut, trimmed, poster, posterSet}` |
| `video.play` | `{loop?}` | plays from the current position (from the trim start when at the end) |
| `video.pause`, `video.togglePlay` | — | |
| `video.seek` | `{time}` | snaps to the frame start, clamped into the trim |
| `video.stepFrame` | `{delta?}` (whole frames, default 1, at most ±1 000 000) | pauses, moves by frames inside the trim |
| `video.setTrim` | `{in, out?}` | `0 ≤ in < out ≤ duration`, otherwise an error; one undo step |
| `video.resetTrim` | — | undoable; no step when the clip isn't trimmed |
| `video.setPoster` | `{time \| null}` | snapped to a frame; undoable; `null` = default (10 % in) |
| `video.captureFrame` | `{time?}` (default: current position) | JPEG next to the clip (`<name>-<m>m<ss>s<mmm>.jpg`, never overwriting), imported and stacked under the clip, one undo step, an activity row while it runs → `{id, path, time, clip}` |

Playback position is driven by `session.video.clock` (monotonic seconds; injectable in tests): it
advances while playing, stops at the trim end, or wraps to the trim start with `loop`. Playback is
muted (`canPlayAudio` is false). Every command answers the `video.get` state, so UI, CLI and MCP all
see the same thing.

Merge note: trim and poster live in `video::mirror` until the catalog's `VideoMeta` /
`Op::SetVideoTrim` / `Op::SetVideoPoster` land; `video::adapter` is the only code that reads or
changes them.
