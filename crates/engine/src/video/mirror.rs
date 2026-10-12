//! TEMPORARY (M14 merge): local stand-ins for what the `catalog` and `video` agents add
//! (`dac_catalog::VideoMeta`, `Photo::video`, `Op::SetVideoTrim` / `Op::SetVideoPoster`,
//! `dac_video::VideoInfo`). The lead replaces this file at merge: [`super::adapter`] then reads
//! `photo.video` and commits the catalog ops, and `UndoEntry::video` goes away.

use std::collections::HashMap;

use dac_catalog::PhotoId;
use serde::{Deserialize, Serialize};

/// Mirror of `dac_catalog::VideoMeta` (same fields).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VideoMeta {
    pub duration: f64,
    pub fps_num: u32,
    pub fps_den: u32,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub has_audio: bool,
    pub trim_in: f64,
    pub trim_out: Option<f64>,
    pub poster_time: Option<f64>,
}

/// Mirror of `dac_video::VideoInfo`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VideoInfo {
    pub duration: f64,
    pub fps_num: u32,
    pub fps_den: u32,
    pub width: u32,
    pub height: u32,
    pub codec: String,
    pub has_audio: bool,
    pub created: Option<String>,
    pub gps: Option<(f64, f64)>,
}

impl VideoMeta {
    /// A freshly probed clip: no trim, default poster.
    pub fn from_info(i: &VideoInfo) -> Self {
        Self {
            duration: i.duration,
            fps_num: i.fps_num,
            fps_den: i.fps_den,
            width: i.width,
            height: i.height,
            codec: i.codec.clone(),
            has_audio: i.has_audio,
            trim_in: 0.0,
            trim_out: None,
            poster_time: None,
        }
    }
}

/// Mirror of the catalog's `Op::SetVideoTrim` / `Op::SetVideoPoster`.
#[derive(Clone, Debug, PartialEq)]
pub enum VideoOp {
    SetTrim { id: PhotoId, trim_in: f64, trim_out: Option<f64> },
    SetPoster { id: PhotoId, time: Option<f64> },
}

/// Clip metadata by photo.
#[derive(Default)]
pub(crate) struct Store {
    pub(crate) meta: HashMap<PhotoId, VideoMeta>,
}

impl Store {
    /// Apply `op`, returning its inverse (a clip not in the store is left alone: inverse = same op).
    pub(crate) fn apply(&mut self, op: VideoOp) -> VideoOp {
        match op {
            VideoOp::SetTrim { id, trim_in, trim_out } => match self.meta.get_mut(&id) {
                Some(m) => VideoOp::SetTrim {
                    id,
                    trim_in: std::mem::replace(&mut m.trim_in, trim_in),
                    trim_out: std::mem::replace(&mut m.trim_out, trim_out),
                },
                None => VideoOp::SetTrim { id, trim_in, trim_out },
            },
            VideoOp::SetPoster { id, time } => match self.meta.get_mut(&id) {
                Some(m) => VideoOp::SetPoster { id, time: std::mem::replace(&mut m.poster_time, time) },
                None => VideoOp::SetPoster { id, time },
            },
        }
    }
}
