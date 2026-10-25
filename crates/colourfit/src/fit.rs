//! The fit. OWNER: fit agent.
use crate::Result;
use crate::dataset::PhotoSamples;
use lcc_camdb::calibration::Calibration;

/// Options for the fit.
#[derive(Clone, Debug, Default)]
pub struct FitOptions {
    pub contributor: Option<String>,
    /// Closed-loop refinement rounds through the real pipeline (default 3 when 0).
    pub rounds: usize,
}

/// Fit the calibration from the collected photos (all of one camera model). The result passes
/// `Calibration::validate` and carries held-out ΔE00 before/after per look in `evidence`.
pub fn fit(photos: &[PhotoSamples], opts: &FitOptions) -> Result<Calibration> {
    let _ = (photos, opts);
    Err(crate::Error("fit: not implemented yet".into()))
}
