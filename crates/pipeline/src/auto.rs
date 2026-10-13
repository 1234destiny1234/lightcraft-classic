//! Auto tone (a Lightroom-like recipe, see [`auto_tone`]) and auto white balance (grey-world
//! statistics on a proxy).

use lcc_color::cct::xy_to_temp_tint;
use lcc_color::{REC2020, Xy, bradford, luminance_2020};
use lcc_develop::DevelopSettings;
use lcc_raster::Rgb32f;
use serde::Serialize;

use crate::SourceInfo;
use crate::local::effective_wb;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct AutoTone {
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
    pub vibrance: f64,
    pub saturation: f64,
}

/// Exposure in EV that Auto adds per EV of the scene's log-average luminance (the slope is negative:
/// darker scenes get more). The slope is Lightroom's: regressing the Exposure its Auto chose on 24
/// raw photos against our own [`scene_log_mean`] of each gives -0.41 EV/EV (r = -0.92).
const EXPOSURE_SLOPE: f64 = -0.41;
/// Exposure for a scene whose log-average luminance is middle grey: fitted, with [`CONTRAST`] and
/// [`VIBRANCE`], so our renders match Lightroom's Auto results on those photos (see [`auto_tone`]).
const EXPOSURE_AT_GREY: f64 = -0.58;
/// Highlights and Shadows are Lightroom's own: the average its Auto chose on the 24 photos (our
/// sliders aim at the same as its). Freeing them as well matched no better on held-out photos and
/// left them poorly determined.
const HIGHLIGHTS: f64 = -66.0;
const SHADOWS: f64 = 52.0;
/// Contrast and Vibrance are fitted: our slider scales differ from Lightroom's, and our base tone is
/// flatter, so Contrast is far above its +6. Whites, Blacks and Saturation stay at 0 (Whites and
/// Blacks made no difference to the match; Lightroom leaves Saturation at about 0).
const CONTRAST: f64 = 68.0;
const VIBRANCE: f64 = 30.0;

/// The scene's log-average luminance under the current white balance, in EV relative to middle grey
/// (the mean of log2(Y / 0.18) over a 512 px proxy, scene-linear, before any tone curve). Pixels that
/// aren't finite (a damaged decode) are left out: they get no weight when the proxy is made, and
/// proxy pixels made only of them are skipped. `None` when no pixel is valid.
pub fn scene_log_mean(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings) -> Option<f64> {
    let (mut clean, mut weight) = (src.clone(), Rgb32f::new(src.width, src.height));
    let mut any = false;
    for (p, w) in clean.data.iter_mut().zip(weight.data.iter_mut()) {
        if p.iter().all(|v| v.is_finite()) {
            *w = [1.0; 3];
            any = true;
        } else {
            *p = [0.0; 3];
        }
    }
    if !any {
        return None;
    }
    let fit = |img: &Rgb32f| lcc_raster::resample::fit(img, 512, 512, lcc_raster::resample::Filter::Box);
    let (img, weight) = (fit(&clean), fit(&weight));
    // the mean of the valid pixels under each proxy pixel
    let kept: Vec<[f32; 3]> = img.data.iter().zip(&weight.data).filter(|(_, w)| w[0] > 1e-6).map(|(p, w)| p.map(|v| v / w[0])).collect();
    let mut kept = Rgb32f { width: kept.len(), height: 1, data: kept };
    let base = DevelopSettings { wb: s.wb, process: s.process, ..DevelopSettings::default() };
    // (white balance is per pixel: the valid pixels can be balanced in a row of their own)
    crate::local::scene_linear_pre(&mut kept, info, &base);
    let (mut sum, mut n) = (0f64, 0usize);
    for c in &kept.data {
        let ev = (luminance_2020(*c).max(1e-6) / 0.18).log2();
        if ev.is_finite() {
            sum += ev as f64;
            n += 1;
        }
    }
    (n > 0).then(|| sum / n as f64)
}

/// Auto settings for `src` under the current white balance (ignores current tone values), aiming at
/// what Lightroom's Auto does: the Exposure a scene needs follows its log-average luminance
/// ([`scene_log_mean`], scene-linear, so the rule doesn't depend on the base tone curve), and the
/// other sliders are one fixed recipe. Calibrated black-box against Lightroom's observed behaviour
/// (the values its Auto chose and its exported results) on 24 Sony raw photos, of which 16 have a
/// starting look that Sony's in-camera DRO doesn't brighten. Held out one photo at a time, those 16
/// come out at a median dE76 of 6.4 from Lightroom's Auto results (8.3 unedited; 18.8 with the
/// previous percentile rule, which ended about 16 L* too bright); all 24 at 7.7 (7.9 unedited).
/// Rendered sources (JPEGs) are already toned, so their log-average reads brighter and Auto changes
/// their exposure less. A source without a single valid pixel gets Exposure 0 (and the rest of the
/// recipe).
pub fn auto_tone(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings) -> AutoTone {
    let exposure = scene_log_mean(src, info, s).map_or(0.0, |m| (EXPOSURE_AT_GREY + EXPOSURE_SLOPE * m).clamp(-2.0, 3.0));
    AutoTone {
        exposure: (exposure * 100.0).round() / 100.0,
        contrast: CONTRAST,
        highlights: HIGHLIGHTS,
        shadows: SHADOWS,
        whites: 0.0,
        blacks: 0.0,
        vibrance: VIBRANCE,
        saturation: 0.0,
    }
}

/// An automatic black & white mix (slider values, red … magenta) for `src` under `s`'s white
/// balance and tone: each hue band's colourful pixels are pushed away from the image's mean
/// lightness — bands brighter than average get brighter, darker ones darker — so areas that
/// differ only in colour stay apart in grey. Bands with almost no colourful pixels stay at 0.
pub fn auto_bw_mix(src: &Rgb32f, info: &SourceInfo, s: &DevelopSettings) -> [f64; 8] {
    use lcc_color::perceptual::{lab_to_lch, oklab_from_2020};
    let mut img = lcc_raster::resample::fit(src, 512, 512, lcc_raster::resample::Filter::Box);
    let base = DevelopSettings { wb: s.wb, light: s.light, process: s.process, ..DevelopSettings::default() };
    crate::local::scene_linear_pre(&mut img, info, &base);
    let gain = 2f32.powf(base.light.exposure as f32);
    let (mut mass, mut sum_l) = ([0f64; 8], [0f64; 8]);
    let (mut all_l, mut n) = (0f64, 0usize);
    for p in &img.data {
        let lch = lab_to_lch(oklab_from_2020(p.map(|v| (v * gain).max(0.0))));
        all_l += lch[0] as f64;
        n += 1;
        let k = (lch[1] / 0.2).min(1.0) as f64; // as in the B&W conversion
        if k < 0.05 {
            continue;
        }
        let w = crate::colorops::band_weights(lch[2]);
        for i in 0..8 {
            mass[i] += w[i] as f64 * k;
            sum_l[i] += w[i] as f64 * k * lch[0] as f64;
        }
    }
    if n == 0 {
        return [0.0; 8];
    }
    let mean = all_l / n as f64;
    let total: f64 = mass.iter().sum();
    std::array::from_fn(|i| {
        if total <= 0.0 || mass[i] / total < 0.01 {
            return 0.0;
        }
        let sep = sum_l[i] / mass[i] - mean;
        (sep * 400.0).clamp(-60.0, 60.0).round()
    })
}

/// Grey-world white balance weighted towards mid-tone, low-chroma pixels. Returns (temp, tint).
pub fn auto_wb(src: &Rgb32f, info: &SourceInfo) -> (f64, f64) {
    let img = lcc_raster::resample::fit(src, 256, 256, lcc_raster::resample::Filter::Box);
    let (mut acc, mut wsum) = ([0.0f64; 3], 0.0f64);
    for c in &img.data {
        let y = luminance_2020(*c);
        if !(0.01..=2.0).contains(&y) {
            continue;
        }
        let mx = c[0].max(c[1]).max(c[2]);
        let mn = c[0].min(c[1]).min(c[2]);
        let chroma = (mx - mn) / (mx + 1e-6);
        let w = (1.0 - chroma).powi(2) as f64 * (1.0 - ((y.log2() + 2.5) / 4.0).abs().min(1.0)) as f64;
        for i in 0..3 {
            acc[i] += c[i] as f64 * w;
        }
        wsum += w;
    }
    if wsum <= 0.0 {
        return (info.as_shot_temp, info.as_shot_tint);
    }
    let avg = acc.map(|v| v / wsum);
    // the white that makes `avg` neutral, through the same model the white balance renders with
    // (`local::wb_matrix_for`): the camera's own colour model, else a Bradford adaptation
    let camera = info.camera_color.as_deref().filter(|_| info.raw && !info.relative_wb);
    let seen = match camera.and_then(|cc| lcc_raw::color::neutral_white(&cc.tags, cc.developed_for, avg)) {
        Some(white) => white,
        None => {
            let shot = lcc_color::cct::temp_tint_to_xy(info.as_shot_temp, info.as_shot_tint);
            Xy::from_xyz(bradford(REC2020.white, shot).apply(REC2020.to_xyz().apply(avg)))
        }
    };
    let (t, tint) = xy_to_temp_tint(seen);
    (t.clamp(2000.0, 50000.0).round(), tint.clamp(-150.0, 150.0).round())
}

/// Temperature/tint currently in effect (for UI display).
pub fn current_wb(info: &SourceInfo, s: &DevelopSettings) -> (f64, f64) {
    effective_wb(info, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_that_are_not_finite_are_left_out() {
        let auto = |img: &Rgb32f| auto_tone(img, &SourceInfo::default(), &DevelopSettings::default());
        // half middle grey, half NaN (or infinite): the grey half alone decides, as if the rest weren't there
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let half = Rgb32f::from_fn(64, 64, |x, _| if x < 32 { [0.18; 3] } else { [bad; 3] });
            assert_eq!(auto(&half).exposure, -0.58, "{bad}");
            // also through the resampling of a source larger than the proxy (proxy pixels on the border mix both halves)
            let big = Rgb32f::from_fn(1030, 700, |x, _| if x < 515 { [0.045; 3] } else { [bad; 3] });
            assert!((auto(&big).exposure - (-0.58 + 2.0 * 0.41)).abs() < 1e-9, "{bad}");
            // a few broken pixels scattered over a grey image change nothing
            let speckled = Rgb32f::from_fn(1030, 700, |x, y| if (x * 7 + y * 13) % 97 == 0 { [bad; 3] } else { [0.18; 3] });
            assert_eq!(auto(&speckled).exposure, -0.58, "{bad}");
        }
        // with no valid pixel at all (or none at all) there is nothing to measure: Exposure 0, the recipe as usual
        for img in [Rgb32f::filled(8, 8, [f32::NAN; 3]), Rgb32f::filled(600, 600, [f32::INFINITY; 3]), Rgb32f::new(0, 0)] {
            let a = auto(&img);
            assert_eq!((a.exposure, a.contrast, a.vibrance), (0.0, 68.0, 30.0));
        }
    }
}
