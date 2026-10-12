//! Frames for the loupe while a clip plays or sits paused (M14.4).
//!
//! - [`Playback`]: a decode-ahead worker for one playing clip. It decodes sequentially
//!   ([`VideoBackend::frames_from`]) at display size, grades every frame through a
//!   [`GradeLut`] baked from `video_settings(develop)` (rebaked only when the settings change),
//!   and hands frames over a bounded queue ([`QUEUE`] frames), so it runs at most that far ahead.
//!   Frames already behind the playhead the host reports ([`Playback::set_target`]) are skipped
//!   before grading, which holds real time on a slow decoder. The worker never blocks its host:
//!   the host polls with [`Playback::poll`].
//! - [`spawn_still`]: one full-quality graded frame (the real pipeline, `grade_frame`) for a
//!   paused or scrubbed clip, on a thread of its own.
//!
//! Both are long-lived interactive work, not activity rows; decode errors are logged and reported
//! once to the host. Sources: own design.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use dac_develop::DevelopSettings;
use dac_pipeline::Quality;
use dac_pipeline::grade::{GradeLut, grade_frame, video_settings};
use dac_raster::Rgba8;

use super::VideoBackend;

/// Most frames the worker decodes ahead of the screen.
pub const QUEUE: usize = 8;
/// Longest side of a frame decoded for playback.
pub const PLAY_MAX_SIDE: u32 = 960;

/// A graded frame ready to show.
#[derive(Clone, Debug)]
pub struct ShownFrame {
    /// Presentation time, seconds.
    pub time: f64,
    pub image: Arc<Rgba8>,
    /// Rendered by the full pipeline (paused) rather than the playback LUT.
    pub full: bool,
}

/// What the worker sends.
enum Msg {
    Frame(ShownFrame),
    End,
    Error(String),
}

/// What [`Playback::poll`] reports.
#[derive(Debug, Default)]
pub struct Poll {
    /// The newest frame due at the playhead (earlier due frames were dropped as late).
    pub frame: Option<ShownFrame>,
    /// The worker reached the clip's end.
    pub ended: bool,
    /// Decoding failed; the worker has stopped.
    pub error: Option<String>,
}

struct Shared {
    settings: Mutex<(u64, DevelopSettings)>,
    target: Mutex<f64>,
    stop: AtomicBool,
}

/// The decode-ahead worker of one playing clip. Dropping it stops the worker.
pub struct Playback {
    rx: Receiver<Msg>,
    shared: Arc<Shared>,
    /// A frame received but not yet due.
    ahead: Option<ShownFrame>,
    /// Where it started decoding (seconds).
    pub start: f64,
    pub path: PathBuf,
    ended: bool,
}

impl std::fmt::Debug for Playback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Playback").field("path", &self.path).field("start", &self.start).finish()
    }
}

fn settings_key(d: &DevelopSettings) -> u64 {
    video_settings(d).hash64()
}

impl Playback {
    /// Start decoding `path` from `start` at most `max_side` px, graded with `develop`.
    pub fn start(backend: Arc<dyn VideoBackend>, path: PathBuf, start: f64, max_side: u32, develop: &DevelopSettings) -> Result<Playback, String> {
        let shared = Arc::new(Shared {
            settings: Mutex::new((settings_key(develop), develop.clone())),
            target: Mutex::new(start),
            stop: AtomicBool::new(false),
        });
        let (tx, rx) = sync_channel(QUEUE);
        let (sh, p) = (shared.clone(), path.clone());
        let side = max_side.clamp(16, PLAY_MAX_SIDE);
        std::thread::Builder::new()
            .name("video-playback".into())
            .spawn(move || worker(backend, p, start, side, sh, tx))
            .map_err(|e| format!("can't start video playback: {e}"))?;
        Ok(Playback { rx, shared, ahead: None, start, path, ended: false })
    }

    /// New develop settings: the worker rebakes its LUT when the video grade changed.
    pub fn set_settings(&self, develop: &DevelopSettings) {
        let key = settings_key(develop);
        let mut g = self.shared.settings.lock().unwrap_or_else(PoisonError::into_inner);
        if g.0 != key {
            *g = (key, develop.clone());
        }
    }

    /// The playhead now: frames before it (by more than `slack`) aren't graded.
    pub fn set_target(&self, t: f64) {
        *self.shared.target.lock().unwrap_or_else(PoisonError::into_inner) = t;
    }

    /// Take what has arrived: the newest frame whose time is at or before `now + slack`.
    pub fn poll(&mut self, now: f64, slack: f64) -> Poll {
        let mut out = Poll::default();
        loop {
            if let Some(f) = self.ahead.take() {
                if f.time <= now + slack {
                    out.frame = Some(f);
                } else {
                    self.ahead = Some(f);
                    break;
                }
            }
            match self.rx.try_recv() {
                Ok(Msg::Frame(f)) => self.ahead = Some(f),
                Ok(Msg::End) => {
                    self.ended = true;
                }
                Ok(Msg::Error(e)) => {
                    out.error = Some(e);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.ended {
                        out.error = Some("video playback stopped unexpectedly".into());
                        self.ended = true;
                    }
                    break;
                }
            }
        }
        out.ended = self.ended && self.ahead.is_none();
        out
    }

    /// The time of the next frame waiting (not yet due), if any.
    pub fn next_time(&self) -> Option<f64> {
        self.ahead.as_ref().map(|f| f.time)
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

fn worker(backend: Arc<dyn VideoBackend>, path: PathBuf, start: f64, side: u32, sh: Arc<Shared>, tx: SyncSender<Msg>) {
    let fail = |tx: &SyncSender<Msg>, e: String| {
        log::warn!("video playback of {}: {e}", path.display());
        let _ = tx.send(Msg::Error(e));
    };
    let frames = match backend.frames_from(&path, start, Some(side)) {
        Ok(f) => f,
        Err(e) => return fail(&tx, e),
    };
    let mut baked: Option<(u64, Arc<GradeLut>)> = None;
    let mut last: Option<f64> = None;
    for item in frames {
        if sh.stop.load(Ordering::Relaxed) {
            return;
        }
        let (time, img) = match item {
            Ok(f) => f,
            Err(e) => return fail(&tx, e),
        };
        // late: a frame further behind the playhead than one frame interval isn't worth grading
        let target = *sh.target.lock().unwrap_or_else(PoisonError::into_inner);
        let interval = last.map(|l| (time - l).abs()).filter(|d| *d > 0.0).unwrap_or(1.0 / 30.0);
        last = Some(time);
        if time + interval < target {
            continue;
        }
        let (key, develop) = sh.settings.lock().unwrap_or_else(PoisonError::into_inner).clone();
        if baked.as_ref().is_none_or(|(k, _)| *k != key) {
            match GradeLut::bake(&video_settings(&develop)) {
                Ok(l) => baked = Some((key, Arc::new(l))),
                Err(e) => return fail(&tx, format!("can't grade the clip: {e}")),
            }
        }
        let Some((_, lut)) = baked.as_ref() else { return };
        let image = Arc::new(lut.apply(&img, Quality::Full));
        if tx.send(Msg::Frame(ShownFrame { time, image, full: false })).is_err() {
            return;
        }
    }
    let _ = tx.send(Msg::End);
}

/// A full-quality graded frame at `t` (longest side ≤ `max_side`), rendered on its own thread.
/// The receiver gets exactly one answer.
pub fn spawn_still(
    backend: Arc<dyn VideoBackend>,
    path: PathBuf,
    t: f64,
    max_side: u32,
    develop: &DevelopSettings,
) -> Receiver<Result<ShownFrame, String>> {
    let (tx, rx) = sync_channel(1);
    let develop = develop.clone();
    let tx2 = tx.clone();
    let spawned = std::thread::Builder::new().name("video-still".into()).spawn(move || {
        let r = backend.frame_at(&path, t, Some(max_side.max(16))).map(|img| ShownFrame {
            time: t,
            image: Arc::new(grade_frame(&img, &video_settings(&develop), Quality::Full)),
            full: true,
        });
        if let Err(e) = &r {
            log::warn!("video frame of {} at {t:.3} s: {e}", path.display());
        }
        let _ = tx.send(r);
    });
    if let Err(e) = spawned {
        let _ = tx2.send(Err(format!("can't render the video frame: {e}")));
    }
    rx
}
