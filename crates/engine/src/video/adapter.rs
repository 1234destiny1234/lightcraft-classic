//! Where a clip's [`VideoMeta`] lives and how it changes. The only module that knows; at the M14
//! merge its bodies switch to `photo.video` and the catalog's `Op::SetVideoMeta` /
//! `Op::SetVideoTrim` / `Op::SetVideoPoster` (a few lines each), the commands stay as they are.

use std::path::PathBuf;

use dac_catalog::{MediaKind, PhotoId, Source};

use super::mirror::{VideoMeta, VideoOp};
use super::{UNAVAILABLE, VideoBackend};
use crate::{EngineError, Result, Session};

/// The photo is a video clip.
pub fn is_video(s: &Session, id: PhotoId) -> bool {
    s.catalog.photo(id).is_some_and(|p| p.kind == MediaKind::Video)
}

/// The installed decoder, or the "not available in this build" error.
pub fn backend(s: &Session) -> Result<std::sync::Arc<dyn VideoBackend>> {
    s.video.backend.clone().ok_or_else(|| EngineError::Other(UNAVAILABLE.into()))
}

/// The clip's file on disk.
pub fn clip_path(s: &Session, id: PhotoId) -> Result<PathBuf> {
    match s.catalog.photo(id).map(|p| &p.source) {
        Some(Source::File { path }) => Ok(PathBuf::from(path)),
        Some(Source::Demo { .. }) => Err(EngineError::Other("a generated demo photo is not a video".into())),
        None => Err(EngineError::Other(format!("no photo {}", id.0))),
    }
}

/// The clip's metadata: as stored, or probed now (and kept, not as an undo step).
pub fn meta(s: &mut Session, id: PhotoId) -> Result<VideoMeta> {
    if !is_video(s, id) {
        return Err(EngineError::Other("the photo isn't a video".into()));
    }
    if let Some(m) = s.video.mirror.meta.get(&id) {
        return Ok(m.clone());
    }
    let be = backend(s)?;
    let path = clip_path(s, id)?;
    let info = be.probe(&path).map_err(|e| EngineError::Other(format!("can't read the video: {e}")))?;
    if !(info.duration.is_finite() && info.duration > 0.0) || info.fps_num == 0 || info.fps_den == 0 {
        return Err(EngineError::Other("can't read the video: no duration or frame rate".into()));
    }
    let m = VideoMeta::from_info(&info);
    s.video.mirror.meta.insert(id, m.clone());
    Ok(m)
}

/// Set the trim (an undo step labelled `label`).
pub fn commit_trim(s: &mut Session, id: PhotoId, trim_in: f64, trim_out: Option<f64>, label: &str) -> Result<()> {
    s.commit_video_mirror(label, VideoOp::SetTrim { id, trim_in, trim_out })
}

/// Set the poster frame time (an undo step labelled `label`).
pub fn commit_poster(s: &mut Session, id: PhotoId, time: Option<f64>, label: &str) -> Result<()> {
    s.commit_video_mirror(label, VideoOp::SetPoster { id, time })
}
