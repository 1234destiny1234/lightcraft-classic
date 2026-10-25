//! Pairing raws with Lightroom exports, decoding, registration and sampling. OWNER: dataset agent.
use crate::Result;
use std::path::PathBuf;

/// One export folder: a Lightroom profile's exports (folder name or each file's `crs:CameraProfile` names it).
#[derive(Clone, Debug)]
pub struct ExportSet {
    pub dir: PathBuf,
}

/// What `fit` needs about one photo's white balance.
#[derive(Clone, Debug)]
pub struct White {
    /// Camera neutral (AsShotNeutral, or 1 / multipliers), green-normalised.
    pub neutral: [f64; 3],
    /// Lightroom's temperature and tint for this export (crs:Temperature / crs:Tint), when the export carries it.
    pub lightroom_temp_tint: Option<(f64, f64)>,
    /// File's baseline exposure (EV).
    pub baseline_exposure: f64,
}

/// One colour pair: our camera RGB (black-subtracted, white = 1, not white-balanced, no matrix) and Lightroom's
/// colour at the same place as linear XYZ D50 (decoded from the export's ICC profile).
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub camera: [f32; 3],
    pub lightroom_xyz: [f32; 3],
    /// Weight (bin balancing).
    pub weight: f32,
}

/// One raw paired with one export of one look.
#[derive(Clone, Debug)]
pub struct PhotoSamples {
    /// Stable anonymous id within this run (index), never a file name.
    pub photo: usize,
    /// `crs:CameraProfile` name of the export, e.g. "Adobe Standard".
    pub look: String,
    pub make: String,
    pub model: String,
    pub white: White,
    pub samples: Vec<Sample>,
    /// Held out for validation (not fitted on).
    pub held_out: bool,
    /// Path of the raw (local only: report and closed loop; never written to the calibration).
    pub raw_path: PathBuf,
    /// Path of the export (local only).
    pub export_path: PathBuf,
}

/// Options for collecting.
#[derive(Clone, Debug)]
pub struct CollectOptions {
    /// Long edge of the sampling proxies (default 256).
    pub proxy: usize,
    /// Share of photos held out (default 0.2).
    pub hold_out: f64,
    /// Cap of samples per photo (default 3000).
    pub per_photo: usize,
}

impl Default for CollectOptions {
    fn default() -> Self {
        CollectOptions { proxy: 256, hold_out: 0.2, per_photo: 3000 }
    }
}

/// What collection did, for the console (counts and reasons, never file contents).
#[derive(Clone, Debug, Default)]
pub struct CollectReport {
    pub paired: usize,
    /// (file name, why skipped): printed locally only.
    pub skipped: Vec<(String, String)>,
}

/// Pair, decode, register and sample. Errors only when nothing usable is found.
pub fn collect(raws: &[PathBuf], exports: &[ExportSet], opts: &CollectOptions) -> Result<(Vec<PhotoSamples>, CollectReport)> {
    let _ = (raws, exports, opts);
    Err(crate::Error("collect: not implemented yet".into()))
}
