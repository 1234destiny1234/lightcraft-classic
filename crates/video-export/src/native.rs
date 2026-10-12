//! The export loop (native targets only).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use dac_raster::Rgb32f;
use dac_video::FrameReader;
use filmcraft_color::{Matrix, from_linear, rgb_to_ycbcr};
use filmcraft_isobmff::{AvcConfig, Brand, CodecConfig, FourCc, Mp4File, Mp4Writer, SampleEntry, TrackConfig, TrackKind, WriteSample, WriterOptions};

/// Largest output side (H.264 level 6.2 tops out at 8192×4320).
pub const MAX_SIDE: u32 = 8192;
/// Most frames one export writes (24 h at 60 fps): guards against a hostile duration.
pub const MAX_FRAMES: i64 = 24 * 3600 * 60;
/// Most audio channels written (more are dropped: stereo delivery).
const MAX_AUDIO_CHANNELS: usize = 2;
/// AAC bitrate per channel.
const AAC_BPS_PER_CHANNEL: u32 = 96_000;

/// The frame type the grade callback receives: [`dac_raster::Rgb32f`] as `dac_video` decodes it,
/// **linear light, Rec.709 primaries, D65, display-referred** (1.0 = the source's SDR peak white).
/// The grade may write any value; afterwards values are clamped to 0..=1 and re-encoded with the
/// sRGB curve (the inverse of the decode), so an identity grade round-trips. The grade must not
/// change the frame size.
pub type GradeFrame = Rgb32f;

/// Output format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClipFormat {
    /// MPEG-4: H.264 High 8-bit 4:2:0 + AAC-LC.
    #[default]
    H264Mp4,
    /// QuickTime: ProRes 422 (HQ at quality ≥ 80, Standard ≥ 50, else LT) 10-bit + AAC-LC.
    ProResMov,
}

impl ClipFormat {
    /// The file extension (without the dot).
    pub fn extension(self) -> &'static str {
        match self {
            ClipFormat::H264Mp4 => "mp4",
            ClipFormat::ProResMov => "mov",
        }
    }
}

/// Export options.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipExportOptions {
    pub format: ClipFormat,
    /// 1–100 (clamped): maps to H.264 CRF 40…14 or the ProRes flavour.
    pub quality: u8,
    /// Longest output side in pixels (`None` = source size). The other side keeps the aspect;
    /// both are rounded down to even. Never upscales.
    pub max_side: Option<u32>,
    /// Destination file. Overwritten on success; untouched on failure.
    pub out: PathBuf,
}

/// What an export wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipReport {
    pub frames: u64,
    /// Seconds (frames / frame rate).
    pub duration: f64,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportClipError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("cannot open the video: {0}")]
    Open(String),
    #[error("the file has no video stream")]
    NoVideo,
    #[error("invalid trim: {0}")]
    BadTrim(String),
    #[error("invalid options: {0}")]
    BadOptions(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("encode error: {0}")]
    Encode(String),
    #[error("export cancelled")]
    Cancelled,
    #[error("internal error while exporting the video")]
    Internal,
}

type Result<T> = std::result::Result<T, ExportClipError>;

fn io(e: impl std::fmt::Display) -> ExportClipError {
    ExportClipError::Io(e.to_string())
}
fn enc(e: impl std::fmt::Display) -> ExportClipError {
    ExportClipError::Encode(e.to_string())
}

/// Export `src` from `trim.0` to `trim.1` (seconds; `None` = end of clip) to `opts.out`.
///
/// `grade` runs on every output frame (sequentially for now; `Sync` so a later version can grade
/// frames in parallel). `progress` receives 0..=1. Setting `cancel` stops within one frame with
/// [`ExportClipError::Cancelled`] and removes the partial file.
pub fn export_clip(
    src: &Path,
    trim: (f64, Option<f64>),
    grade: &(dyn Fn(&mut Rgb32f) + Sync),
    opts: &ClipExportOptions,
    cancel: &AtomicBool,
    progress: &dyn Fn(f64),
) -> Result<ClipReport> {
    let partial = partial_path(&opts.out)?;
    let r = catch_unwind(AssertUnwindSafe(|| run(src, trim, grade, opts, cancel, progress, &partial))).unwrap_or(Err(ExportClipError::Internal));
    match r {
        Ok(mut rep) => {
            rep.path = opts.out.clone();
            std::fs::rename(&partial, &opts.out).map_err(|e| {
                let _ = std::fs::remove_file(&partial);
                io(e)
            })?;
            Ok(rep)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&partial);
            Err(e)
        }
    }
}

fn partial_path(out: &Path) -> Result<PathBuf> {
    let name = out.file_name().ok_or_else(|| ExportClipError::BadOptions("the output path has no file name".into()))?;
    let mut n = name.to_os_string();
    n.push(".partial");
    Ok(out.with_file_name(n))
}

/// The frames to export: indices `[k0, k1)` into the clip's presentation-ordered frame times. The
/// first is the frame displayed at `trim.0` (which may sit mid-GOP: the reader decodes from the
/// previous sync sample); the last is the final frame that starts before the out point.
fn select_frames(times: &[f64], duration: f64, trim: (f64, Option<f64>)) -> Result<(usize, usize)> {
    let (tin, tout) = (trim.0, trim.1.unwrap_or(duration));
    if !tin.is_finite() || !tout.is_finite() || !duration.is_finite() || tin < 0.0 {
        return Err(ExportClipError::BadTrim(format!("in {tin} / out {tout}")));
    }
    let tout = tout.min(duration);
    if tout <= tin {
        return Err(ExportClipError::BadTrim(format!("out ({tout:.3} s) must be after in ({tin:.3} s)")));
    }
    const EPS: f64 = 1e-6;
    let k0 = times.iter().rposition(|t| *t <= tin + EPS).unwrap_or(0);
    let k1 = times.iter().position(|t| *t >= tout - EPS).unwrap_or(times.len()).max(k0.saturating_add(1)).min(times.len());
    if k0 >= k1 {
        return Err(ExportClipError::BadTrim("no frames in the range".into()));
    }
    if (k1 - k0) as i64 > MAX_FRAMES {
        return Err(ExportClipError::BadTrim("the range is too long".into()));
    }
    Ok((k0, k1))
}

/// Output size: fit the longest side in `max_side`, keep the aspect, round down to even.
fn output_size(w: u32, h: u32, max_side: Option<u32>) -> Result<(u32, u32)> {
    if w < 2 || h < 2 {
        return Err(ExportClipError::Decode(format!("frame size {w}×{h} is too small")));
    }
    let long = w.max(h);
    let limit = max_side.unwrap_or(long).clamp(2, MAX_SIDE).min(long);
    let s = f64::from(limit) / f64::from(long);
    let even = |v: u32| -> u32 { ((f64::from(v) * s).round() as u32).max(2) & !1 };
    Ok((even(w), even(h)))
}

fn video_err(e: dac_video::VideoError) -> ExportClipError {
    match e {
        dac_video::VideoError::Io(m) => ExportClipError::Io(m),
        dac_video::VideoError::Decode(m) => ExportClipError::Decode(m),
        other => ExportClipError::Open(other.to_string()),
    }
}

fn run(
    src: &Path,
    trim: (f64, Option<f64>),
    grade: &(dyn Fn(&mut Rgb32f) + Sync),
    opts: &ClipExportOptions,
    cancel: &AtomicBool,
    progress: &dyn Fn(f64),
    partial: &Path,
) -> Result<ClipReport> {
    let mut reader = FrameReader::open(src).map_err(video_err)?;
    let info = reader.info().clone();
    if !info.decodable {
        return Err(ExportClipError::Open(format!("{} video cannot be decoded", info.codec)));
    }
    let (fnum, fden) = (info.fps_num, info.fps_den);
    if fnum == 0 || fden == 0 {
        return Err(ExportClipError::Decode("invalid frame rate".into()));
    }
    let times = reader.frame_times();
    let (k0, k1) = select_frames(&times, info.duration, trim)?;
    let total = k1 - k0;
    let t0 = times.get(k0).copied().unwrap_or(0.0);
    let (w, h) = output_size(info.width, info.height, opts.max_side)?;
    let quality = opts.quality.clamp(1, 100);

    let file = File::create(partial).map_err(io)?;
    let brand = match opts.format {
        ClipFormat::H264Mp4 => Brand::Mp4,
        ClipFormat::ProResMov => Brand::Mov,
    };
    let mut mux = Mp4Writer::new(BufWriter::new(file), WriterOptions::new(brand)).map_err(enc)?;
    let mut video = VideoOut::new(opts.format, quality, w, h, fnum, fden)?;
    let vtrack = mux.add_track(video.track_config()).map_err(enc)?;
    let end = t0 + total as f64 * f64::from(fden) / f64::from(fnum);
    let mut audio = AudioOut::open(src, t0, end)?;
    let atrack = match &audio {
        Some(a) => Some(mux.add_track(a.track_config()).map_err(enc)?),
        None => None,
    };

    for (i, k) in (k0..k1).enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportClipError::Cancelled);
        }
        let t = times.get(k).copied().unwrap_or(t0);
        let decoded = reader.frame_at(t, Some(w.max(h))).map_err(video_err)?;
        let mut frame = fit(&decoded.image, w as usize, h as usize);
        grade(&mut frame);
        if frame.width != w as usize || frame.height != h as usize || frame.data.len() != w as usize * h as usize {
            return Err(ExportClipError::Encode("the grade changed the frame size".into()));
        }
        for p in video.encode(&frame, i as i64)? {
            mux.write_sample(vtrack, WriteSample { data: &p.data, duration: fden, composition_offset: p.cto, is_sync: p.key }).map_err(enc)?;
        }
        if let (Some(a), Some(tr)) = (audio.as_mut(), atrack) {
            let until = t0 + (i as f64 + 1.0) * f64::from(fden) / f64::from(fnum);
            for au in a.encode_until(until)? {
                mux.write_sample(tr, WriteSample { data: &au, duration: 1024, composition_offset: 0, is_sync: true }).map_err(enc)?;
            }
        }
        progress((i as f64 + 1.0) / total as f64);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(ExportClipError::Cancelled);
    }
    for p in video.flush() {
        mux.write_sample(vtrack, WriteSample { data: &p.data, duration: fden, composition_offset: p.cto, is_sync: p.key }).map_err(enc)?;
    }
    if let (Some(a), Some(tr)) = (audio.as_mut(), atrack) {
        for au in a.finish()? {
            mux.write_sample(tr, WriteSample { data: &au, duration: 1024, composition_offset: 0, is_sync: true }).map_err(enc)?;
        }
    }
    let mut out = mux.finish().map_err(enc)?;
    out.flush().map_err(io)?;
    let file = out.into_inner().map_err(|e| io(e.error()))?;
    file.sync_all().map_err(io)?;
    let frames = total as u64;
    Ok(ClipReport {
        frames,
        duration: frames as f64 * f64::from(fden) / f64::from(fnum),
        path: PathBuf::new(),
        width: w,
        height: h,
        has_audio: audio.is_some(),
    })
}

/// Resample to exactly `w × h` (bilinear; a copy when the size already matches).
fn fit(img: &Rgb32f, w: usize, h: usize) -> Rgb32f {
    if img.width == w && img.height == h {
        return img.clone();
    }
    let (sx, sy) = (img.width.max(1) as f32 / w.max(1) as f32, img.height.max(1) as f32 / h.max(1) as f32);
    Rgb32f::from_fn(w, h, |x, y| img.sample_bilinear((x as f32 + 0.5) * sx, (y as f32 + 0.5) * sy))
}

/// Linear → encoded R'G'B' (0..1) → normalised Y'CbCr, per pixel.
fn ycbcr(frame: &Rgb32f) -> Vec<[f32; 3]> {
    frame
        .data
        .iter()
        .map(|p| {
            let e = |v: f32| from_linear(if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 }, filmcraft_color::Transfer::Bt709);
            rgb_to_ycbcr(e(p[0]), e(p[1]), e(p[2]), Matrix::Bt709)
        })
        .collect()
}

/// Limited-range code value of a normalised luma / chroma sample at `bits`.
fn code_y(v: f32, bits: u32) -> u16 {
    let s = (1u32 << (bits - 8)) as f32;
    let max = ((1u32 << bits) - 1) as f32;
    (16.0 * s + v.clamp(0.0, 1.0) * 219.0 * s).round().clamp(0.0, max) as u16
}
fn code_c(v: f32, bits: u32) -> u16 {
    let s = (1u32 << (bits - 8)) as f32;
    let max = ((1u32 << bits) - 1) as f32;
    (128.0 * s + v.clamp(-0.5, 0.5) * 224.0 * s).round().clamp(0.0, max) as u16
}

struct Packet {
    data: Vec<u8>,
    key: bool,
    cto: i32,
}

enum VideoOut {
    H264 { enc: Box<filmcraft_h264enc::Encoder>, w: u32, h: u32, den: u32 },
    ProRes { enc: filmcraft_prores::Encoder, profile: filmcraft_prores::Profile, w: u32, h: u32, num: u32 },
}

impl VideoOut {
    fn new(format: ClipFormat, quality: u8, w: u32, h: u32, num: u32, den: u32) -> Result<Self> {
        Ok(match format {
            ClipFormat::H264Mp4 => {
                let mut cfg = filmcraft_h264enc::EncoderConfig::new(w, h, num, den);
                cfg.format = filmcraft_h264enc::PacketFormat::LengthPrefixed;
                cfg.aud = false;
                cfg.keyint = ((f64::from(num) / f64::from(den)) * 2.0).round().clamp(1.0, 600.0) as u32;
                cfg.rate = filmcraft_h264enc::RateControl::Crf(40.0 - f32::from(quality - 1) * 26.0 / 99.0);
                let enc = filmcraft_h264enc::Encoder::new(cfg).map_err(enc)?;
                VideoOut::H264 { enc: Box::new(enc), w, h, den }
            }
            ClipFormat::ProResMov => {
                use filmcraft_prores::Profile;
                let profile = match quality {
                    80.. => Profile::Hq,
                    50.. => Profile::Standard,
                    _ => Profile::Lt,
                };
                let mut cfg = filmcraft_prores::EncoderConfig::new(profile, w, h);
                cfg.encode_alpha = false;
                VideoOut::ProRes { enc: filmcraft_prores::Encoder::with_config(cfg), profile, w, h, num }
            }
        })
    }

    fn track_config(&self) -> TrackConfig {
        match self {
            VideoOut::H264 { enc, w, h, den } => {
                let cfg = AvcConfig::parse(&enc.avcc()).unwrap_or_else(|_| {
                    let (sps, pps) = enc.sps_pps();
                    AvcConfig::new(vec![sps], vec![pps], 4)
                });
                let mut t = TrackConfig::new(SampleEntry::avc(cfg, *w as u16, *h as u16), enc.config().fps_num);
                // with B-frames the first DTS is one frame before the first PTS
                if enc.delay() > 0 {
                    t.media_start = Some(i64::from(*den));
                }
                t
            }
            VideoOut::ProRes { profile, w, h, num, .. } => {
                TrackConfig::new(SampleEntry::prores(FourCc(profile.fourcc()), *w as u16, *h as u16), *num)
            }
        }
    }

    fn encode(&mut self, frame: &Rgb32f, index: i64) -> Result<Vec<Packet>> {
        let yc = ycbcr(frame);
        match self {
            VideoOut::H264 { enc, w, h, den } => {
                let (w, h) = (*w as usize, *h as usize);
                let (cw, ch) = (w / 2, h / 2);
                let ys: Vec<u8> = yc.iter().map(|p| code_y(p[0], 8) as u8).collect();
                let (mut us, mut vs) = (vec![0u8; cw * ch], vec![0u8; cw * ch]);
                for cy in 0..ch {
                    for cx in 0..cw {
                        let (mut cb, mut cr) = (0.0, 0.0);
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            if let Some(p) = yc.get((cy * 2 + dy) * w + cx * 2 + dx) {
                                cb += p[1];
                                cr += p[2];
                            }
                        }
                        let i = cy * cw + cx;
                        if let (Some(u), Some(v)) = (us.get_mut(i), vs.get_mut(i)) {
                            *u = code_c(cb / 4.0, 8) as u8;
                            *v = code_c(cr / 4.0, 8) as u8;
                        }
                    }
                }
                let yuv = filmcraft_h264enc::YuvFrame { y: &ys, u: &us, v: &vs, y_stride: w, uv_stride: cw };
                let ps = enc.try_encode(&yuv, index.saturating_mul(i64::from(*den))).map_err(self::enc)?;
                Ok(h264_packets(ps))
            }
            VideoOut::ProRes { enc, w, h, .. } => {
                let mut fr = filmcraft_prores::Frame::new(*w, *h, filmcraft_prores::ChromaFormat::Yuv422, 10, false);
                let (w, h) = (*w as usize, *h as usize);
                let cw = w / 2;
                for (d, p) in fr.y.iter_mut().zip(&yc) {
                    *d = code_y(p[0], 10);
                }
                for y in 0..h {
                    for cx in 0..cw {
                        let a = yc.get(y * w + cx * 2);
                        let b = yc.get(y * w + cx * 2 + 1).or(a);
                        let (Some(a), Some(b)) = (a, b) else { continue };
                        let i = y * cw + cx;
                        if let Some(d) = fr.cb.get_mut(i) {
                            *d = code_c((a[1] + b[1]) / 2.0, 10);
                        }
                        if let Some(d) = fr.cr.get_mut(i) {
                            *d = code_c((a[2] + b[2]) / 2.0, 10);
                        }
                    }
                }
                let data = enc.encode(&fr).map_err(self::enc)?;
                Ok(vec![Packet { data, key: true, cto: 0 }])
            }
        }
    }

    fn flush(&mut self) -> Vec<Packet> {
        match self {
            VideoOut::H264 { enc, .. } => h264_packets(enc.flush()),
            VideoOut::ProRes { .. } => Vec::new(),
        }
    }
}

fn h264_packets(ps: Vec<filmcraft_h264enc::Packet>) -> Vec<Packet> {
    ps.into_iter().map(|p| Packet { data: p.data, key: p.keyframe, cto: i32::try_from(p.pts.saturating_sub(p.dts)).unwrap_or(0) }).collect()
}

/// The primary AAC audio track of the source for `[start, end)` seconds, re-encoded to AAC-LC as
/// the video frames go by. Other audio codecs are not exported yet (the movie is written without
/// audio).
struct AudioOut {
    file: Mp4File,
    src: File,
    track: usize,
    timescale: i64,
    edit: i64,
    dec: filmcraft_aac::Decoder,
    enc: filmcraft_aac::Encoder,
    sr: u32,
    channels: usize,
    /// Next source sample to decode.
    next: usize,
    /// Output position (samples at `sr`, presentation time) of the next sample to emit.
    cursor: i64,
    /// Absolute end (samples at `sr`).
    end: i64,
    /// Decoded samples from `cursor` on, per channel.
    buf: Vec<Vec<f32>>,
}

/// Packets decoded before the first needed one (AAC's overlapped MDCT needs one).
const AAC_PREROLL: usize = 2;

impl AudioOut {
    fn open(path: &Path, start: f64, end: f64) -> Result<Option<Self>> {
        let src = File::open(path).map_err(io)?;
        let Ok(file) = filmcraft_isobmff::open(&src) else { return Ok(None) };
        let Some(track) = file.track_of_kind(TrackKind::Audio) else { return Ok(None) };
        let Some(t) = file.tracks.get(track) else { return Ok(None) };
        let Some(CodecConfig::Aac(cfg)) = t.codec() else { return Ok(None) };
        let Ok(dec) = filmcraft_aac::Decoder::new(&cfg.asc) else { return Ok(None) };
        let (sr, channels) = (dec.sample_rate(), dec.channels().clamp(1, MAX_AUDIO_CHANNELS));
        if !(8_000..=96_000).contains(&sr) || t.timescale == 0 || t.samples.is_empty() {
            return Ok(None);
        }
        let timescale = i64::from(t.timescale);
        let edit = t.edit_offset;
        let at = |s: f64| (s.max(0.0) * f64::from(sr)).round().min(i64::MAX as f64 / 4.0) as i64;
        let (cursor, end) = (at(start), at(end));
        // first packet whose presentation end lies after `cursor`, minus the pre-roll
        let to_sr =
            |ts: i64| -> i64 { (i128::from(ts) * i128::from(sr) / i128::from(timescale)).clamp(i64::MIN as i128 / 4, i64::MAX as i128 / 4) as i64 };
        let first = t
            .samples
            .iter()
            .position(|s| to_sr(s.pts.saturating_add(edit).saturating_add(i64::from(s.duration))) > cursor)
            .unwrap_or(t.samples.len());
        let bps = AAC_BPS_PER_CHANNEL * channels as u32;
        let enc = filmcraft_aac::Encoder::new(filmcraft_aac::EncoderConfig::cbr(sr, channels, bps)).map_err(enc)?;
        Ok(Some(Self {
            file,
            src,
            track,
            timescale,
            edit,
            dec,
            enc,
            sr,
            channels,
            next: first.saturating_sub(AAC_PREROLL),
            cursor,
            end,
            buf: vec![Vec::new(); channels],
        }))
    }

    fn track_config(&self) -> TrackConfig {
        let entry = SampleEntry::aac(self.enc.audio_specific_config(), self.channels as u32, self.sr);
        let mut t = TrackConfig::new(entry, self.sr);
        t.media_start = Some(i64::from(self.enc.priming_samples()));
        t
    }

    /// Decode source packets until `buf` holds `n` samples (or the stream ends).
    fn fill(&mut self, n: usize) -> Result<()> {
        let count = self.file.tracks.get(self.track).map_or(0, |t| t.samples.len());
        while self.buf.first().map_or(0, Vec::len) < n && self.next < count {
            let i = self.next;
            self.next += 1;
            let Some(s) = self.file.tracks.get(self.track).and_then(|t| t.samples.get(i)).copied() else { break };
            let data = self.file.read_sample(&self.src, self.track, i).map_err(|e| ExportClipError::Decode(format!("audio: {e}")))?;
            // a damaged packet becomes silence rather than failing the export
            let pcm = self.dec.decode(&data).unwrap_or_default();
            let frames = pcm.first().map_or(0, Vec::len);
            if frames == 0 {
                continue;
            }
            let pos = (i128::from(s.pts.saturating_add(self.edit)) * i128::from(self.sr) / i128::from(self.timescale)) as i64;
            let have = self.cursor.saturating_add(self.buf.first().map_or(0, Vec::len) as i64);
            // skip what lies before what we already hold; pad a gap with silence
            let skip = usize::try_from(have.saturating_sub(pos)).unwrap_or(0);
            let gap = usize::try_from(pos.saturating_sub(have)).unwrap_or(0).min(self.sr as usize * 10);
            for (c, out) in self.buf.iter_mut().enumerate() {
                let ch = pcm.get(c).or(pcm.first());
                out.extend(std::iter::repeat_n(0.0, gap));
                if let Some(ch) = ch {
                    out.extend(ch.iter().skip(skip));
                }
            }
        }
        Ok(())
    }

    /// Encode audio up to `until` seconds (clamped to the export end).
    fn encode_until(&mut self, until: f64) -> Result<Vec<Vec<u8>>> {
        let target = ((until.max(0.0) * f64::from(self.sr)).round() as i64).min(self.end);
        let n = usize::try_from(target.saturating_sub(self.cursor)).unwrap_or(0);
        if n == 0 {
            return Ok(Vec::new());
        }
        self.fill(n)?;
        let mut planar: Vec<Vec<f32>> = Vec::with_capacity(self.channels);
        for ch in &mut self.buf {
            let mut part: Vec<f32> = ch.drain(..n.min(ch.len())).collect();
            part.resize(n, 0.0);
            planar.push(part);
        }
        self.cursor = target;
        let refs: Vec<&[f32]> = planar.iter().map(Vec::as_slice).collect();
        Ok(self.enc.encode(&refs))
    }

    fn finish(&mut self) -> Result<Vec<Vec<u8>>> {
        let end = self.end as f64 / f64::from(self.sr);
        let mut out = self.encode_until(end)?;
        out.extend(self.enc.flush());
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_selection_is_frame_accurate_and_validated() {
        let times: Vec<f64> = (0..100).map(|i| f64::from(i) / 25.0).collect();
        assert_eq!(select_frames(&times, 4.0, (1.0, Some(2.0))).unwrap(), (25, 50));
        assert_eq!(select_frames(&times, 4.0, (0.0, None)).unwrap(), (0, 100));
        assert_eq!(select_frames(&times, 4.0, (1.01, Some(1.05))).unwrap(), (25, 27));
        assert_eq!(select_frames(&times, 4.0, (3.9, Some(99.0))).unwrap(), (97, 100));
        assert!(select_frames(&times, 4.0, (2.0, Some(1.0))).is_err());
        assert!(select_frames(&times, 4.0, (f64::NAN, None)).is_err());
        assert!(select_frames(&times, 4.0, (-1.0, None)).is_err());
        assert!(select_frames(&times, 4.0, (5.0, None)).is_err());
        assert!(select_frames(&times, 4.0, (0.0, Some(f64::INFINITY))).is_err());
        assert!(select_frames(&[], 4.0, (0.0, None)).is_err());
    }

    #[test]
    fn output_size_fits_keeps_aspect_and_is_even() {
        assert_eq!(output_size(1920, 1080, None).unwrap(), (1920, 1080));
        assert_eq!(output_size(1920, 1080, Some(1280)).unwrap(), (1280, 720));
        assert_eq!(output_size(1080, 1920, Some(1280)).unwrap(), (720, 1280));
        assert_eq!(output_size(641, 481, None).unwrap(), (640, 480));
        assert_eq!(output_size(640, 480, Some(4000)).unwrap(), (640, 480));
        assert_eq!(output_size(640, 480, Some(0)).unwrap(), (2, 2));
        assert!(output_size(1, 480, None).is_err());
    }

    #[test]
    fn partial_path_sits_next_to_the_output() {
        assert_eq!(partial_path(Path::new("/a/b.mp4")).unwrap(), PathBuf::from("/a/b.mp4.partial"));
        assert!(partial_path(Path::new("/")).is_err());
    }
}
