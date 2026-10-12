//! Video playback, trim, poster frame and frame capture: the engine half of M14.4 / M14.6
//! (`plan/video-filmcraft.md` §7).
//!
//! The engine never decodes video itself: a host that can installs a [`VideoBackend`] in
//! [`VideoState::backend`] (the desktop app and CLI wrap `dac-video`). Without one every `video.*`
//! command fails with [`UNAVAILABLE`], so the web build and tests without a decoder stay clean.
//!
//! - [`player`]: the playback state machine (play/pause, clock-driven position, frame steps,
//!   trim bounds).
//! - [`adapter`]: the few functions that read and change a clip's [`VideoMeta`]; the only code
//!   that knows where it is stored.
//! - [`mirror`]: TEMPORARY stand-ins for the catalog's video types and ops until they merge.
//!
//! Sources: own design.

pub mod adapter;
pub mod mirror;
pub mod player;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

pub use mirror::{VideoInfo, VideoMeta};
pub use player::Player;

/// What every video command answers when no decoder backend is installed.
pub const UNAVAILABLE: &str = "video playback isn't available in this build";

/// A decoded frame: linear-light sRGB-primaries RGB (what the pipeline takes from a JPEG/TIFF).
pub type FrameImage = dac_raster::Rgb32f;

/// A video decoder the host provides (`dac-video` on native). Errors are user-facing messages.
pub trait VideoBackend: Send + Sync {
    /// Duration, frame rate, size and codec of the clip at `path`.
    fn probe(&self, path: &Path) -> Result<VideoInfo, String>;
    /// The frame shown at `t` seconds, scaled so its longer side is at most `max_side` (None = full size).
    fn frame_at(&self, path: &Path, t: f64, max_side: Option<u32>) -> Result<FrameImage, String>;
}

/// Seconds on a monotonic clock (injectable for tests).
pub type Clock = Box<dyn Fn() -> f64 + Send>;

/// The session's video state.
pub struct VideoState {
    /// The decoder; `None` = video isn't available in this build.
    pub backend: Option<Arc<dyn VideoBackend>>,
    pub player: Player,
    /// Monotonic time source driving playback.
    pub clock: Clock,
    /// TEMPORARY (M14 merge): clip metadata until `Photo::video` lands in the catalog.
    pub(crate) mirror: mirror::Store,
}

impl Default for VideoState {
    fn default() -> Self {
        Self { backend: None, player: Player::default(), clock: default_clock(), mirror: mirror::Store::default() }
    }
}

impl std::fmt::Debug for VideoState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoState").field("backend", &self.backend.is_some()).field("player", &self.player).finish()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn default_clock() -> Clock {
    let start = std::time::Instant::now();
    Box::new(move || start.elapsed().as_secs_f64())
}

/// The web build has no backend, so playback never runs there.
#[cfg(target_arch = "wasm32")]
fn default_clock() -> Clock {
    Box::new(|| 0.0)
}
