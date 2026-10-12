//! Native implementation: container dispatch ([`crate::demux`]), codec dispatch
//! ([`crate::decode`]), seeking.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use crate::decode::{self, Decoder, Pic};
use crate::demux::{self, Clip};
use crate::{Frame, VideoError, VideoInfo, colour};

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

pub fn probe(path: &Path) -> Result<VideoInfo, VideoError> {
    guarded("demux", || demux::open(path)).map(|c| c.info)
}

pub struct Reader {
    clip: Clip,
    codec: decode::Codec,
    dec: Option<Decoder>,
    /// Next sample (decode order) to feed `dec`; meaningful only while `dec` is Some.
    next: usize,
    /// Sync sample the current decoder started from.
    gop: usize,
    pending: Vec<Pic>,
    /// Pts of the last picture returned by the current decoder.
    last: Option<i64>,
}

impl Reader {
    pub fn open(path: &Path) -> Result<Self, VideoError> {
        let clip = guarded("demux", || demux::open(path))?;
        let Some(codec) = clip.codec.clone() else {
            let why = clip.unsupported.clone().unwrap_or_else(|| format!("{} video", clip.info.codec));
            return Err(VideoError::Unsupported(why));
        };
        Ok(Self { clip, codec, dec: None, next: 0, gop: 0, pending: Vec::new(), last: None })
    }

    pub fn info(&self) -> &VideoInfo {
        &self.clip.info
    }

    /// Pts of the first shown frame: frame times count from it.
    fn origin(&self) -> i64 {
        let t = &self.clip.table;
        t.order.first().and_then(|&i| t.pts(i)).unwrap_or(0)
    }

    pub fn frame_times(&self) -> Vec<f64> {
        let t = &self.clip.table;
        let o = self.origin();
        t.order.iter().filter_map(|&i| t.pts(i)).map(|p| t.seconds(p.saturating_sub(o))).collect()
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
        let origin = self.origin();
        let table = &self.clip.table;
        let t = if t.is_finite() { t.max(0.0) } else { 0.0 };
        let idx = table.sample_at(table.units(t).saturating_add(origin)).ok_or_else(|| VideoError::Invalid("video track has no samples".into()))?;
        let target = table.pts(idx).ok_or_else(|| VideoError::Invalid("sample index".into()))?;
        let gop = table.sync_before(idx);
        let count = table.len();

        if let Some(p) = self.take_pending(target) {
            return self.finish(p, max_side);
        }
        // reuse the decoder when the target lies ahead in the same GOP
        let reuse = self.dec.is_some() && self.gop == gop && self.last.is_none_or(|l| target > l);
        if !reuse {
            self.dec = Some(Decoder::new(&self.codec)?);
            self.next = gop;
            self.gop = gop;
            self.pending.clear();
            self.last = None;
        }
        let colr = self.clip.colr;
        let mut overrun = 0usize;
        while self.next < count {
            let i = self.next;
            self.next += 1;
            let data = self.clip.read(i)?;
            let pts = self.clip.table.pts(i).unwrap_or(0);
            let Some(dec) = self.dec.as_mut() else { break };
            let pics = dec.decode(&data, pts, colr)?;
            self.push(pics);
            if let Some(p) = self.take_pending(target) {
                return self.finish(p, max_side);
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
            Some(p) => self.finish(p, max_side),
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

    fn finish(&self, p: Pic, max_side: Option<u32>) -> Result<Frame, VideoError> {
        let mut f = p.frame;
        if f.width == 0 || f.height == 0 || !decode::complete(&f) {
            return Err(VideoError::Decode("decoder returned an incomplete picture".into()));
        }
        let rot = self.clip.info.rotation;
        if rot != 0 {
            f = f.rotated(rot);
        }
        let n = colour::decimation(f.width, f.height, max_side);
        let image = colour::to_rgb(&f, n);
        let time = self.clip.table.seconds(p.pts.saturating_sub(self.origin()));
        Ok(Frame { image, time: time.max(0.0) })
    }
}
