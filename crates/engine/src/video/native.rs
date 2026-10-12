//! The `dac-video` backend (pure-Rust FilmCraft decoders). Keeps the last clip's reader open so
//! scrubbing and frame steps don't re-open the file.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::{FrameImage, FrameIter, VideoBackend, VideoInfo};

#[derive(Default)]
pub struct DacVideo {
    reader: Mutex<Option<(PathBuf, dac_video::FrameReader)>>,
}

impl VideoBackend for DacVideo {
    fn probe(&self, path: &Path) -> Result<VideoInfo, String> {
        let i = dac_video::probe(path).map_err(|e| e.to_string())?;
        if !i.decodable {
            return Err(format!("unsupported video: {} can't be decoded", i.codec));
        }
        Ok(VideoInfo {
            duration: i.duration,
            fps_num: i.fps_num,
            fps_den: i.fps_den,
            width: i.width,
            height: i.height,
            codec: i.codec,
            has_audio: i.has_audio,
        })
    }

    fn frame_at(&self, path: &Path, t: f64, max_side: Option<u32>) -> Result<FrameImage, String> {
        let mut slot = self.reader.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.as_ref().is_none_or(|(p, _)| p != path) {
            *slot = None;
            let r = dac_video::FrameReader::open(path).map_err(|e| e.to_string())?;
            *slot = Some((path.to_path_buf(), r));
        }
        let Some((_, reader)) = slot.as_mut() else { return Err("the video reader isn't open".into()) };
        match reader.frame_at(t, max_side) {
            Ok(f) => Ok(f.image),
            Err(e) => {
                // a failed decode may leave the reader mid-stream: open afresh next time
                *slot = None;
                Err(e.to_string())
            }
        }
    }

    fn frames_from(&self, path: &Path, t: f64, max_side: Option<u32>) -> Result<FrameIter, String> {
        let mut r = dac_video::FrameReader::open(path).map_err(|e| e.to_string())?;
        r.seek(t);
        let mut failed = false;
        Ok(Box::new(std::iter::from_fn(move || {
            if failed {
                return None;
            }
            match r.next_frame(max_side) {
                Ok(Some(f)) => Some(Ok((f.time, f.image))),
                Ok(None) => None,
                Err(e) => {
                    failed = true;
                    Some(Err(e.to_string()))
                }
            }
        })))
    }
}
