//! Native implementation: MP4/MOV demux, H.264/HEVC decode, seeking.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;

use filmcraft_color::ColorInfo;
use filmcraft_frame::{Chroma, PixelData, VideoFrame};
use filmcraft_isobmff::{CodecConfig, Mp4File, Track, TrackKind};

use crate::colour::{self, Codes};
use crate::source::FileSource;
use crate::{Frame, VideoError, VideoInfo};

/// Decoded pictures kept while searching for the target (B-frame reordering needs a few).
const MAX_PENDING: usize = 32;
/// Samples decoded past the target before giving up on finding its picture.
const MAX_OVERRUN: usize = 64;

/// Run a FilmCraft call; a panic inside it becomes [`VideoError::Internal`].
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T, VideoError>) -> Result<T, VideoError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_default();
            Err(VideoError::Internal(format!("{what}: {msg}")))
        }
    }
}

fn io(path: &Path, e: std::io::Error) -> VideoError {
    VideoError::Io(format!("{}: {e}", path.display()))
}

fn demux_error(e: filmcraft_isobmff::Error) -> VideoError {
    match e {
        filmcraft_isobmff::Error::Io(e) => VideoError::Io(e.to_string()),
        e => VideoError::Invalid(e.to_string()),
    }
}

fn h264_error(e: filmcraft_h264::Error) -> VideoError {
    match e {
        filmcraft_h264::Error::Unsupported(w) => VideoError::Unsupported(format!("H.264: {w}")),
        e => VideoError::Decode(format!("H.264: {e}")),
    }
}

fn hevc_error(e: filmcraft_hevc::Error) -> VideoError {
    VideoError::Decode(format!("HEVC: {e}"))
}

struct Opened {
    src: FileSource,
    file: Mp4File,
    track: usize,
    info: VideoInfo,
    colr: Option<Codes>,
}

fn codec_name(c: &CodecConfig, fourcc: [u8; 4]) -> (String, bool) {
    match c {
        CodecConfig::Avc(_) => ("h264".into(), true),
        CodecConfig::Hevc(_) => ("hevc".into(), true),
        CodecConfig::Av1(_) => ("av1".into(), false),
        CodecConfig::Vp9(_) => ("vp9".into(), false),
        CodecConfig::ProRes { .. } => ("prores".into(), false),
        _ => (String::from_utf8_lossy(&fourcc).trim().to_string(), false),
    }
}

/// Frame rate as (num, den) from the track timescale and the most common sample duration.
fn frame_rate(t: &Track) -> (u32, u32) {
    let mut durs: Vec<u32> = t.samples.iter().map(|s| s.duration).filter(|d| *d > 0).collect();
    durs.sort_unstable();
    // most common duration (runs in the sorted list)
    let mut best = (0u32, 0usize);
    let mut i = 0;
    while let Some(&d) = durs.get(i) {
        let run = durs.get(i..).map_or(0, |r| r.iter().take_while(|&&x| x == d).count()).max(1);
        if run > best.1 {
            best = (d, run);
        }
        i += run;
    }
    let (num, den) = (t.timescale, best.0);
    if num == 0 || den == 0 {
        return (30, 1);
    }
    let g = gcd(num, den);
    (num / g, den / g)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

fn opened(path: &Path) -> Result<Opened, VideoError> {
    let src = FileSource::open(path).map_err(|e| io(path, e))?;
    let file = guarded("demux", || filmcraft_isobmff::open(&src).map_err(demux_error))?;
    let track = file.track_of_kind(TrackKind::Video).ok_or_else(|| VideoError::Invalid("no video track".into()))?;
    let t = file.tracks.get(track).ok_or_else(|| VideoError::Invalid("no video track".into()))?;
    let entry = t.entries.first().ok_or_else(|| VideoError::Invalid("video track without a sample description".into()))?;
    let (codec, decodable) = codec_name(&entry.codec, entry.format.0);
    let vp = entry.video.as_ref();
    let colr = vp.and_then(|v| v.color.as_ref()).and_then(|c| match *c {
        filmcraft_isobmff::ColorInfo::Nclx { primaries, transfer, matrix, full_range } => {
            Some(Codes { primaries, transfer, matrix, full_range: Some(full_range) })
        }
        filmcraft_isobmff::ColorInfo::Nclc { primaries, transfer, matrix } => Some(Codes { primaries, transfer, matrix, full_range: None }),
        filmcraft_isobmff::ColorInfo::Icc { .. } => None,
    });
    let (mut w, mut h) = vp.map_or((t.width, t.height), |v| (u32::from(v.width), u32::from(v.height)));
    if w == 0 || h == 0 {
        (w, h) = (t.width, t.height);
    }
    let rotation = t.display_rotation().unwrap_or(0);
    if rotation % 2 == 1 {
        (w, h) = (h, w);
    }
    let duration = if file.timescale > 0 && file.duration > 0 {
        file.duration as f64 / f64::from(file.timescale)
    } else if t.timescale > 0 {
        t.duration as f64 / f64::from(t.timescale)
    } else {
        0.0
    };
    let (fps_num, fps_den) = frame_rate(t);
    let md = &file.metadata;
    let created = md.get("com.apple.quicktime.creationdate").or_else(|| md.get("©day")).or_else(|| md.get("date")).map(str::to_string);
    let gps = md.get("com.apple.quicktime.location.ISO6709").or_else(|| md.get("©xyz")).and_then(crate::parse_iso6709);
    let has_audio = file.track_of_kind(TrackKind::Audio).is_some();
    let hdr = colr.is_some_and(|c| colour::is_hdr(&colour::resolve(w, h, None, Some(c))));
    let decodable = decodable && !t.samples.is_empty();
    let info = VideoInfo { duration, fps_num, fps_den, width: w, height: h, codec, has_audio, created, gps, rotation, hdr, decodable };
    Ok(Opened { src, file, track, info, colr })
}

pub fn probe(path: &Path) -> Result<VideoInfo, VideoError> {
    opened(path).map(|o| o.info)
}

enum Decoder {
    H264(Box<filmcraft_h264::Decoder>),
    Hevc(Box<filmcraft_hevc::Decoder>),
}

/// A decoded picture with its raw (pre-edit) pts.
struct Pic {
    pts: i64,
    frame: VideoFrame,
}

fn tight<T: Copy>(plane: &[T], stride: usize, w: usize, h: usize) -> Vec<T> {
    if stride == w && plane.len() >= w * h {
        return plane.get(..w * h).map(<[T]>::to_vec).unwrap_or_default();
    }
    let mut out = Vec::with_capacity(w.saturating_mul(h));
    for y in 0..h {
        let start = y.saturating_mul(stride);
        match plane.get(start..start.saturating_add(w)) {
            Some(row) => out.extend_from_slice(row),
            None => break,
        }
    }
    out
}

impl Decoder {
    fn new(cfg: &CodecConfig) -> Result<Self, VideoError> {
        match cfg {
            CodecConfig::Avc(a) => filmcraft_h264::Decoder::from_avcc(&a.to_bytes()).map(|d| Decoder::H264(Box::new(d))).map_err(h264_error),
            CodecConfig::Hevc(c) => filmcraft_hevc::Decoder::from_hvcc(&c.to_bytes()).map(|d| Decoder::Hevc(Box::new(d))).map_err(hevc_error),
            _ => Err(VideoError::Unsupported("codec".into())),
        }
    }

    fn decode(&mut self, data: &[u8], pts: i64, colr: Option<Codes>) -> Result<Vec<Pic>, VideoError> {
        Ok(match self {
            Decoder::H264(d) => d.decode(data, pts).map_err(h264_error)?.into_iter().map(|p| h264_pic(p, colr)).collect(),
            Decoder::Hevc(d) => d.decode(data, pts).map_err(hevc_error)?.into_iter().map(|p| hevc_pic(p, colr)).collect(),
        })
    }

    fn flush(&mut self, colr: Option<Codes>) -> Vec<Pic> {
        match self {
            Decoder::H264(d) => d.flush().into_iter().map(|p| h264_pic(p, colr)).collect(),
            Decoder::Hevc(d) => d.flush().into_iter().map(|p| hevc_pic(p, colr)).collect(),
        }
    }
}

fn vui(primaries: u8, transfer: u8, matrix: u8, full_range: bool) -> Codes {
    Codes { primaries: primaries.into(), transfer: transfer.into(), matrix: matrix.into(), full_range: Some(full_range) }
}

fn h264_pic(p: filmcraft_h264::Picture, colr: Option<Codes>) -> Pic {
    let (w, h, cw, ch) = (p.width as usize, p.height as usize, p.chroma_width as usize, p.chroma_height as usize);
    let c = p.color;
    let color = colour::resolve(p.width, p.height, Some(vui(c.primaries, c.transfer, c.matrix, c.full_range)), colr);
    let planes = [tight(&p.y, p.y_stride, w, h), tight(&p.u, p.uv_stride, cw, ch), tight(&p.v, p.uv_stride, cw, ch)].map(Arc::new);
    Pic { pts: p.pts, frame: frame(p.width, p.height, PixelData::Yuv8 { planes, chroma: Chroma::C420, alpha: None }, color) }
}

fn hevc_pic(p: filmcraft_hevc::Picture, colr: Option<Codes>) -> Pic {
    use filmcraft_hevc::Plane;
    let (w, h, cw, ch) = (p.width as usize, p.height as usize, p.chroma_width as usize, p.chroma_height as usize);
    let c = p.color;
    let color = colour::resolve(p.width, p.height, Some(vui(c.primaries, c.transfer, c.matrix, c.full_range)), colr);
    let (ys, uvs) = (p.y_stride, p.uv_stride);
    let wide = |pl: &Plane| -> Vec<u16> {
        match pl {
            Plane::U16(v) => v.clone(),
            Plane::U8(v) => v.iter().map(|&b| u16::from(b)).collect(),
        }
    };
    let data = match (&p.y, &p.u, &p.v) {
        (Plane::U8(y), Plane::U8(u), Plane::U8(v)) => PixelData::Yuv8 {
            planes: [tight(y, ys, w, h), tight(u, uvs, cw, ch), tight(v, uvs, cw, ch)].map(Arc::new),
            chroma: Chroma::C420,
            alpha: None,
        },
        (y, u, v) => PixelData::Yuv16 {
            planes: [tight(&wide(y), ys, w, h), tight(&wide(u), uvs, cw, ch), tight(&wide(v), uvs, cw, ch)].map(Arc::new),
            chroma: Chroma::C420,
            bits: p.bit_depth,
            alpha: None,
        },
    };
    Pic { pts: p.pts, frame: frame(p.width, p.height, data, color) }
}

fn frame(width: u32, height: u32, data: PixelData, color: ColorInfo) -> VideoFrame {
    VideoFrame { width, height, data, color, par: (1, 1), pts: filmcraft_time::Tick::ZERO }
}

/// Whether a frame's planes hold as many samples as its size says (the converter assumes so).
fn complete(f: &VideoFrame) -> bool {
    let (w, h) = (f.width as usize, f.height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    match &f.data {
        PixelData::Yuv8 { planes, .. } => planes[0].len() >= w * h && planes[1].len() >= cw * ch && planes[2].len() >= cw * ch,
        PixelData::Yuv16 { planes, .. } => planes[0].len() >= w * h && planes[1].len() >= cw * ch && planes[2].len() >= cw * ch,
        _ => true,
    }
}

pub struct Reader {
    o: Opened,
    config: CodecConfig,
    dec: Option<Decoder>,
    /// Next sample (decode order) to feed `dec`; meaningful only while `dec` is Some.
    next: usize,
    /// Sync sample the current decoder started from.
    gop: usize,
    pending: Vec<Pic>,
    /// Raw pts of the last picture returned by the current decoder.
    last: Option<i64>,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Self, VideoError> {
        let o = opened(path)?;
        if !o.info.decodable {
            return Err(VideoError::Unsupported(format!("{} video", o.info.codec)));
        }
        let config =
            o.file.tracks.get(o.track).and_then(|t| t.codec()).cloned().ok_or_else(|| VideoError::Invalid("no codec configuration".into()))?;
        Ok(Self { o, config, dec: None, next: 0, gop: 0, pending: Vec::new(), last: None })
    }

    pub fn info(&self) -> &VideoInfo {
        &self.o.info
    }

    fn track(&self) -> Result<&Track, VideoError> {
        self.o.file.tracks.get(self.o.track).ok_or_else(|| VideoError::Invalid("no video track".into()))
    }

    pub fn frame_times(&self) -> Vec<f64> {
        let Ok(t) = self.track() else { return Vec::new() };
        if t.timescale == 0 {
            return Vec::new();
        }
        let mut v: Vec<f64> = (0..t.samples.len()).filter_map(|i| t.presentation_pts(i)).map(|p| p as f64 / f64::from(t.timescale)).collect();
        v.sort_by(f64::total_cmp);
        v
    }

    pub fn frame_at(&mut self, t: f64, max_side: Option<u32>) -> Result<Frame, VideoError> {
        let r = guarded("decode", || self.frame_at_inner(t, max_side));
        if r.is_err() {
            // a failed or panicked decoder is in an unknown state: start over next time
            self.dec = None;
            self.pending.clear();
        }
        r
    }

    fn frame_at_inner(&mut self, t: f64, max_side: Option<u32>) -> Result<Frame, VideoError> {
        let track = self.track()?;
        let scale = track.timescale;
        if scale == 0 {
            return Err(VideoError::Invalid("video track timescale is 0".into()));
        }
        let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
        let target_t = (t * f64::from(scale)).round().min(i64::MAX as f64 / 2.0) as i64;
        let idx = track.sample_at_presentation_time(target_t).ok_or_else(|| VideoError::Invalid("video track has no samples".into()))?;
        let target = track.samples.get(idx).ok_or_else(|| VideoError::Invalid("sample index".into()))?.pts;
        let edit = track.edit_offset;
        let gop = track.sync_sample_before(idx);
        let count = track.samples.len();

        if let Some(p) = self.take_pending(target) {
            return self.finish(p, edit, scale, max_side);
        }
        // reuse the decoder when the target lies ahead in the same GOP
        let reuse = self.dec.is_some() && self.gop == gop && self.last.is_none_or(|l| target > l);
        if !reuse {
            self.dec = Some(Decoder::new(&self.config)?);
            self.next = gop;
            self.gop = gop;
            self.pending.clear();
            self.last = None;
        }
        let colr = self.o.colr;
        let mut overrun = 0usize;
        while self.next < count {
            let i = self.next;
            self.next += 1;
            let data = self.o.file.read_sample(&self.o.src, self.o.track, i).map_err(demux_error)?;
            let pts = self.track()?.samples.get(i).map_or(0, |s| s.pts);
            let Some(dec) = self.dec.as_mut() else { break };
            let pics = dec.decode(&data, pts, colr)?;
            self.push(pics);
            if let Some(p) = self.take_pending(target) {
                return self.finish(p, edit, scale, max_side);
            }
            if i >= idx {
                overrun += 1;
                if overrun > MAX_OVERRUN {
                    break;
                }
            }
        }
        if let Some(dec) = self.dec.as_mut() {
            let pics = dec.flush(colr);
            self.push(pics);
            self.dec = None;
        }
        // end of stream: the target's picture, else the nearest one decoded
        let best = self.take_pending(target).or_else(|| {
            let k = self.pending.iter().enumerate().min_by_key(|(_, p)| p.pts.abs_diff(target)).map(|(k, _)| k)?;
            Some(self.pending.swap_remove(k))
        });
        match best {
            Some(p) => self.finish(p, edit, scale, max_side),
            None => Err(VideoError::Decode("no picture decoded at this time".into())),
        }
    }

    fn push(&mut self, pics: Vec<Pic>) {
        self.pending.extend(pics);
        if self.pending.len() > MAX_PENDING {
            self.pending.sort_by_key(|p| p.pts);
            let drop = self.pending.len() - MAX_PENDING;
            self.pending.drain(..drop);
        }
    }

    fn take_pending(&mut self, target: i64) -> Option<Pic> {
        let k = self.pending.iter().position(|p| p.pts == target)?;
        let p = self.pending.swap_remove(k);
        self.last = Some(target);
        // pictures before the target are no longer needed for forward playback
        self.pending.retain(|q| q.pts > target);
        Some(p)
    }

    fn finish(&self, p: Pic, edit: i64, scale: u32, max_side: Option<u32>) -> Result<Frame, VideoError> {
        let mut f = p.frame;
        if f.width == 0 || f.height == 0 || !complete(&f) {
            return Err(VideoError::Decode("decoder returned an incomplete picture".into()));
        }
        let rot = self.o.info.rotation;
        if rot != 0 {
            f = f.rotated(rot);
        }
        let n = colour::decimation(f.width, f.height, max_side);
        let image = colour::to_rgb(&f, n);
        let time = p.pts.saturating_add(edit) as f64 / f64::from(scale);
        Ok(Frame { image, time: time.max(0.0) })
    }
}
