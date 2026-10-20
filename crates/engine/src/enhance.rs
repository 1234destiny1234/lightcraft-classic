//! Photo ▸ Enhance: Raw Details and Super Resolution, each written as a new linear DNG next to the source photo
//! (`<name>-Enhanced-RD.dng`, `<name>-Enhanced-SR.dng`), imported, given the source's develop settings and stacked
//! with it, as Lightroom's Enhance does.
//!
//! - **Raw Details** demosaics a raw again with the slower edge-aware [`lcc_raw::Method::Detail`].
//! - **Super Resolution** does the same and then doubles each side ([`lcc_merge::super_resolve`]); it also takes
//!   non-raw photos (JPEG, TIFF, …), whose linear pixels it enlarges.
//!
//! The pixels are made on a worker thread ([`plan`] → [`EnhanceWork`], `Send`), with a row in the activity stack for
//! the whole run; `enhance.pump` (called every frame by the app) writes and imports the finished ones. `wait: true`
//! (scripts, CLI, MCP; always on the web) runs inline instead.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use lcc_catalog::{PhotoId, Source};
use serde_json::{Value, json};

use crate::activity::{Cancel, TaskGuard};
use crate::{EngineError, Result, Session};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnhanceKind {
    RawDetails,
    SuperResolution,
}

impl EnhanceKind {
    pub fn label(self) -> &'static str {
        match self {
            EnhanceKind::RawDetails => "Raw Details",
            EnhanceKind::SuperResolution => "Super Resolution",
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            EnhanceKind::RawDetails => "Enhanced-RD",
            EnhanceKind::SuperResolution => "Enhanced-SR",
        }
    }
}

/// Finished and failed photos `enhance.pump` reports (the most recent).
const KEEP: usize = 50;

type ByteReader = Arc<dyn Fn(&str) -> std::result::Result<Vec<u8>, String> + Send + Sync>;

/// One photo to enhance, self-contained and `Send`.
pub struct EnhanceWork {
    pub kind: EnhanceKind,
    pub id: PhotoId,
    pub path: String,
    read: ByteReader,
}

/// The finished pixels of one photo.
pub struct Enhanced {
    pub id: PhotoId,
    pub path: String,
    pub dng: Vec<u8>,
    pub size: (usize, usize),
}

impl EnhanceWork {
    /// Decode, enhance and encode. `cancelled()` is checked between stages and Super Resolution's passes.
    pub fn run(&self, cancelled: &(dyn Fn() -> bool + Sync)) -> std::result::Result<Enhanced, String> {
        let bytes = (self.read)(&self.path)?;
        let is_raw = lcc_raw::probe(&bytes).is_some();
        if self.kind == EnhanceKind::RawDetails && !is_raw {
            return Err(format!("{}: Raw Details needs a raw photo", self.path));
        }
        let frame = lcc_merge::load_frame_full(&bytes, &crate::camera_preview::starting_colour_tags, lcc_raw::Method::Detail)
            .map_err(|e| format!("{}: {e}", self.path))?;
        drop(bytes);
        if cancelled() {
            return Err("cancelled".into());
        }
        let mut color = frame.color.clone();
        if let Some(m) = frame.wb_multipliers {
            color.set_as_shot_multipliers(m);
        }
        let image = match self.kind {
            EnhanceKind::RawDetails => frame.image,
            EnhanceKind::SuperResolution => lcc_merge::super_resolve(&frame.image, &|_| !cancelled()).map_err(|e| e.to_string())?,
        };
        let size = (image.width, image.height);
        let dng = lcc_merge::write_linear_dng(
            &image,
            &color,
            frame.orientation,
            &frame.metadata,
            frame.baseline_exposure,
            lcc_merge::DngSamples::Half,
            &frame.lens,
        )
        .map_err(|e| e.to_string())?;
        Ok(Enhanced { id: self.id, path: self.path.clone(), dng, size })
    }
}

/// Side, in source pixels, of the square the dialog's preview enhances.
pub const PREVIEW_CROP: usize = 256;

/// What the Enhance dialog shows: the middle of the photo before and after, and an estimate of the full run.
pub struct EnhancePreview {
    /// The crop as the library shows the photo now (standard demosaic, source size).
    pub before: lcc_raster::Rgba8,
    /// The same crop enhanced (twice the size for Super Resolution).
    pub after: lcc_raster::Rgba8,
    /// The source's size and the enhanced photo's.
    pub source_size: (usize, usize),
    pub out_size: (usize, usize),
    /// Estimated seconds to enhance the whole photo (measured on this photo and the crop, scaled up).
    pub estimate_secs: f64,
}

fn render_linear(
    image: &lcc_raster::Rgb32f,
    frame: &lcc_merge::Frame,
    color: &lcc_merge::FrameColor,
) -> std::result::Result<lcc_raster::Rgba8, String> {
    let edge = image.width.max(image.height).max(1);
    let dng = lcc_merge::write_linear_dng(
        image,
        color,
        frame.orientation,
        &frame.metadata,
        frame.baseline_exposure,
        lcc_merge::DngSamples::Half,
        &frame.lens,
    )
    .map_err(|e| e.to_string())?;
    let (src, info) = crate::files::load_bytes(&dng, edge)?;
    let s = lcc_develop::DevelopSettings::for_raw(info.as_shot_temp, info.as_shot_tint);
    let req = lcc_pipeline::RenderRequest { quality: lcc_pipeline::Quality::Full, apply_crop: false, ..lcc_pipeline::RenderRequest::fit(edge, edge) };
    Ok(lcc_pipeline::render(&src, &info, &s, &req).image)
}

/// The middle `side` × `side` pixels (or less of a smaller image).
fn middle(img: &lcc_raster::Rgb32f, side: usize) -> lcc_raster::Rgb32f {
    let (w, h) = (side.min(img.width), side.min(img.height));
    img.crop(img.width.saturating_sub(w) / 2, img.height.saturating_sub(h) / 2, w, h)
}

impl EnhanceWork {
    /// The dialog's preview: the middle [`PREVIEW_CROP`] pixels before and after, and an estimate of the full run.
    pub fn preview(&self, cancelled: &(dyn Fn() -> bool + Sync)) -> std::result::Result<EnhancePreview, String> {
        let bytes = (self.read)(&self.path)?;
        let is_raw = lcc_raw::probe(&bytes).is_some();
        if self.kind == EnhanceKind::RawDetails && !is_raw {
            return Err(format!("{}: Raw Details needs a raw photo", self.path));
        }
        let colour = &crate::camera_preview::starting_colour_tags;
        let load = |m| lcc_merge::load_frame_full(&bytes, colour, m).map_err(|e| format!("{}: {e}", self.path));
        let base = load(lcc_raw::Method::Ahd)?;
        if cancelled() {
            return Err("cancelled".into());
        }
        let t0 = std::time::Instant::now();
        let detail = if is_raw { Some(load(lcc_raw::Method::Detail)?) } else { None };
        let decode = t0.elapsed().as_secs_f64();
        drop(bytes);
        if cancelled() {
            return Err("cancelled".into());
        }
        let frame = detail.as_ref().unwrap_or(&base);
        let mut color = frame.color.clone();
        if let Some(m) = frame.wb_multipliers {
            color.set_as_shot_multipliers(m);
        }
        let source_size = (frame.image.width, frame.image.height);
        let crop = middle(&frame.image, PREVIEW_CROP);
        let t1 = std::time::Instant::now();
        let (after, out_size, estimate_secs) = match self.kind {
            EnhanceKind::RawDetails => (crop, source_size, decode),
            EnhanceKind::SuperResolution => {
                let up = lcc_merge::super_resolve(&crop, &|_| !cancelled()).map_err(|e| e.to_string())?;
                let area = |s: (usize, usize)| (s.0 as f64 * s.1 as f64).max(1.0);
                let scale = area(source_size) / area((crop.width, crop.height));
                (up, (source_size.0.saturating_mul(2), source_size.1.saturating_mul(2)), decode + t1.elapsed().as_secs_f64() * scale)
            }
        };
        let before = render_linear(&middle(&base.image, PREVIEW_CROP), &base, &color)?;
        let after = render_linear(&after, frame, &color)?;
        Ok(EnhancePreview { before, after, source_size, out_size, estimate_secs })
    }
}

struct Job {
    kind: EnhanceKind,
    rx: mpsc::Receiver<std::result::Result<Enhanced, String>>,
    left: usize,
    total: usize,
    stack: bool,
    guard: TaskGuard,
}

/// The running Enhance jobs and the outcome of the finished ones.
#[derive(Default)]
pub struct EnhanceJobs {
    jobs: Vec<Job>,
    made: Vec<Value>,
    failed: Vec<Value>,
}

fn output_names(first: &str, suffix: &str) -> impl Iterator<Item = std::path::PathBuf> {
    let p = std::path::Path::new(first);
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "photo".into());
    (0..10_000usize).map(move |i| if i == 0 { dir.join(format!("{stem}-{suffix}.dng")) } else { dir.join(format!("{stem}-{suffix}-{i}.dng")) })
}

impl Session {
    /// The work for each of `ids` (a photo named twice is enhanced once).
    pub fn plan_enhance(&self, kind: EnhanceKind, ids: &[PhotoId]) -> Result<Vec<EnhanceWork>> {
        let read: ByteReader = match &self.media.file_bytes {
            Some(r) => r.clone(),
            None => Arc::new(|path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"))),
        };
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for id in ids.iter().filter(|id| seen.insert(**id)) {
            let p = self.catalog.photo(*id).ok_or(lcc_catalog::CatalogError::NoPhoto(*id))?;
            match &p.source {
                Source::File { path } => out.push(EnhanceWork { kind, id: *id, path: path.clone(), read: read.clone() }),
                Source::Demo { .. } => {
                    return Err(EngineError::Other(format!("{} is a demo photo; {} needs the photo's file", p.file_name, kind.label())));
                }
            }
        }
        if out.is_empty() {
            return Err(EngineError::Other(format!("select a photo for {}", kind.label())));
        }
        Ok(out)
    }

    /// Write a finished photo next to its source, import it, give it the source's develop settings and (with
    /// `stack`) stack it with the source. Returns `{id, source, path, size}`.
    pub fn finish_enhance(&mut self, kind: EnhanceKind, out: Enhanced, stack: bool) -> Result<Value> {
        // the same photo enhanced again gives the same bytes: import would refuse the copy, so the one there is kept
        let hash = lcc_preview::hash_bytes(&out.dng).to_string();
        let existing = self.catalog.photos().find_map(|p| match &p.source {
            Source::File { path } if !p.deleted && p.content_hash.as_deref() == Some(hash.as_str()) && std::path::Path::new(path).exists() => {
                Some((p.id, path.clone()))
            }
            _ => None,
        });
        if let Some((id, path)) = existing {
            return Ok(
                json!({"id": id.0, "source": out.id.0, "path": path, "size": [out.size.0, out.size.1], "bytes": out.dng.len(), "reused": true}),
            );
        }
        let path = lcc_catalog::safe_file::write_new_unique(&mut output_names(&out.path, kind.suffix()), &out.dng)
            .map_err(|e| EngineError::Other(format!("could not write the enhanced DNG next to {}: {e}", out.path)))?
            .to_string_lossy()
            .to_string();
        let report = match crate::import::import(self, std::slice::from_ref(&path), crate::import::ImportMode::Add) {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_file(&path);
                return Err(e);
            }
        };
        let Some(id) = report.imported.first().copied().map(PhotoId) else {
            let _ = std::fs::remove_file(&path);
            return Err(EngineError::Other(format!("the enhanced file {path} could not be imported: {:?}", report.failed)));
        };
        if let Some(d) = self.develop_of(out.id) {
            let d = (*d).clone();
            self.set_develop(id, d, kind.label())?;
        }
        if stack && let Some(op) = self.catalog.stack_with_ops(id, &[out.id]) {
            self.commit("Stack with Source", op)?;
        }
        Ok(json!({"id": id.0, "source": out.id.0, "path": path, "size": [out.size.0, out.size.1], "bytes": out.dng.len()}))
    }

    /// `enhance.rawDetails` / `enhance.superResolution` `{ids?, stack?: true, wait?: false}`.
    pub(crate) fn enhance_command(&mut self, kind: EnhanceKind, p: &Value) -> Result<Value> {
        let ids = self.targets(p)?;
        let work = self.plan_enhance(kind, &ids)?;
        let stack = p.get("stack").and_then(Value::as_bool).unwrap_or(true);
        let wait = p.get("wait").and_then(Value::as_bool).unwrap_or(false) || cfg!(target_arch = "wasm32");
        let cancel = Arc::new(AtomicBool::new(false));
        let guard = self.activity.start("enhance", kind.label(), Cancel::Flag(cancel.clone()));
        let total = work.len();
        guard.progress(0, total as u64);
        if wait {
            let (mut made, mut failed) = (Vec::new(), Vec::new());
            for (i, w) in work.iter().enumerate() {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.run(&|| cancel.load(Ordering::Relaxed))))
                    .unwrap_or_else(|_| Err(format!("{}: {} failed unexpectedly", w.path, kind.label())));
                match r.map_err(EngineError::Other).and_then(|out| self.finish_enhance(kind, out, stack)) {
                    Ok(v) => made.push(v),
                    Err(e) => failed.push(json!({"id": w.id.0, "error": e.to_string()})),
                }
                guard.progress(i as u64 + 1, total as u64);
            }
            drop(guard);
            if made.is_empty()
                && let Some(e) = failed.first()
            {
                return Err(EngineError::Other(e["error"].as_str().unwrap_or("failed").to_string()));
            }
            return Ok(json!({"made": made, "failed": failed}));
        }
        let (tx, rx) = mpsc::channel();
        let flag = cancel.clone();
        std::thread::Builder::new()
            .name("enhance".into())
            .spawn(move || {
                for w in work {
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.run(&|| flag.load(Ordering::Relaxed))))
                        .unwrap_or_else(|_| Err(format!("{}: failed unexpectedly", w.path)));
                    if tx.send(r).is_err() || flag.load(Ordering::Relaxed) {
                        return;
                    }
                }
            })
            .map_err(|e| EngineError::Other(format!("could not start {}: {e}", kind.label())))?;
        self.enhance.jobs.push(Job { kind, rx, left: total, total, stack, guard });
        Ok(json!({"queued": total}))
    }

    /// `enhance.pump`: take in finished photos. Cheap when idle.
    pub(crate) fn enhance_pump(&mut self) -> Value {
        let mut done = Vec::new();
        let mut i = 0;
        while i < self.enhance.jobs.len() {
            let mut finished = false;
            while let Some(job) = self.enhance.jobs.get_mut(i) {
                match job.rx.try_recv() {
                    Ok(r) => {
                        job.left = job.left.saturating_sub(1);
                        done.push((job.kind, job.stack, r));
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            if let Some(job) = self.enhance.jobs.get(i) {
                finished |= job.left == 0;
            }
            if finished {
                self.enhance.jobs.remove(i);
            } else {
                i += 1;
            }
        }
        for (kind, stack, r) in done {
            let id = r.as_ref().map(|o| o.id.0).ok();
            match r.map_err(EngineError::Other).and_then(|out| self.finish_enhance(kind, out, stack)) {
                Ok(v) => self.enhance.made.push(v),
                Err(e) => self.enhance.failed.push(json!({"id": id, "error": e.to_string()})),
            }
        }
        for job in &self.enhance.jobs {
            job.guard.progress(job.total.saturating_sub(job.left) as u64, job.total as u64);
        }
        for list in [&mut self.enhance.made, &mut self.enhance.failed] {
            let extra = list.len().saturating_sub(KEEP);
            list.drain(..extra);
        }
        let running: usize = self.enhance.jobs.iter().map(|j| j.left).sum();
        json!({"active": running > 0, "running": running, "made": self.enhance.made, "failed": self.enhance.failed})
    }
}
