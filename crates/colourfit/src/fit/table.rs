//! Hue/saturation tables (DNG `HueSatMap` / `LookTable`, ProPhoto HSV) fitted by kernel regression with a smoothness
//! term: each sample's needed correction is splatted at its table input, then the grid is solved by Gauss–Seidel.
use lcc_raw::profile::HsvTable;

pub const HUES: usize = 90;
pub const SATS: usize = 30;

/// HSV of a linear RGB colour as the DNG tables read it (hue in degrees).
pub fn hsv(c: [f32; 3]) -> Option<(f64, f64, f64)> {
    let max = c[0].max(c[1]).max(c[2]);
    if !(max.is_finite() && max > 1e-6) {
        return None;
    }
    let min = c[0].min(c[1]).min(c[2]);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    Some((f64::from(h) * 60.0, f64::from(d / max), f64::from(max)))
}

pub fn identity() -> HsvTable {
    HsvTable { hue_divisions: HUES, sat_divisions: SATS, val_divisions: 1, data: vec![[0.0, 1.0, 1.0]; HUES * SATS], srgb_value: false }
}

fn wrap180(d: f64) -> f64 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// Accumulated corrections (hue shift °, ln saturation ratio, ln value ratio) on the table grid.
#[derive(Clone, Debug)]
pub struct Splat {
    w: Vec<f64>,
    v: Vec<[f64; 3]>,
}

impl Default for Splat {
    fn default() -> Self {
        Splat { w: vec![0.0; HUES * SATS], v: vec![[0.0; 3]; HUES * SATS] }
    }
}

impl Splat {
    /// Add one sample: the table input `input`, the colour the table currently gives `out` and the wanted one
    /// `target` (all linear ProPhoto), with weight `weight`.
    pub fn add(&mut self, input: [f32; 3], out: [f32; 3], target: [f32; 3], weight: f64) {
        let (Some((hi, si, _)), Some((ho, so, vo)), Some((ht, st, vt))) = (hsv(input), hsv(out), hsv(target)) else {
            return;
        };
        if weight.is_nan() || weight <= 0.0 || so < 0.02 || st < 0.0 {
            return;
        }
        // hue is meaningless for near-neutral colours: they correct value only, a little saturation
        let hue_weight = ((so.min(st) - 0.02) / 0.08).clamp(0.0, 1.0);
        let corr = [wrap180(ht - ho) * hue_weight, (st.max(1e-4) / so).ln().clamp(-1.0, 1.0), (vt / vo).ln().clamp(-1.0, 1.0)];
        let hf = hi / 360.0 * HUES as f64;
        let h0 = (hf.floor() as isize).rem_euclid(HUES as isize) as usize;
        let th = hf - hf.floor();
        let sf = si.clamp(0.0, 1.0) * (SATS - 1) as f64;
        let s0 = (sf as usize).min(SATS - 2);
        let ts = sf - s0 as f64;
        for (dh, wh) in [(0, 1.0 - th), (1, th)] {
            for (ds, ws) in [(0, 1.0 - ts), (1, ts)] {
                let k = ((h0 + dh) % HUES) * SATS + s0 + ds;
                let w = weight * wh * ws;
                if let (Some(cw), Some(cv)) = (self.w.get_mut(k), self.v.get_mut(k)) {
                    *cw += w;
                    for i in 0..3 {
                        cv[i] += w * corr[i];
                    }
                }
            }
        }
    }

    pub fn total(&self) -> f64 {
        self.w.iter().sum()
    }

    /// The smooth correction grid: data term, Laplacian smoothness (`smooth` × mean cell weight) and a weak pull
    /// to no correction where there is no data.
    pub fn solve(&self, smooth: f64) -> Vec<[f64; 3]> {
        let n = HUES * SATS;
        let total = self.total();
        let mut x = vec![[0.0; 3]; n];
        if total.is_nan() || total <= 0.0 {
            return x;
        }
        let kappa = smooth * total / n as f64;
        let eps = 1e-3 * total / n as f64;
        for _ in 0..400 {
            for h in 0..HUES {
                for s in 0..SATS {
                    let k = h * SATS + s;
                    let mut nb = vec![((h + 1) % HUES) * SATS + s, ((h + HUES - 1) % HUES) * SATS + s];
                    if s > 0 {
                        nb.push(k - 1);
                    }
                    if s + 1 < SATS {
                        nb.push(k + 1);
                    }
                    let (w, v) = (self.w.get(k).copied().unwrap_or(0.0), self.v.get(k).copied().unwrap_or([0.0; 3]));
                    let den = w + kappa * nb.len() as f64 + eps;
                    let mut new = [0.0; 3];
                    for i in 0..3 {
                        let sum: f64 = nb.iter().map(|j| x.get(*j).map_or(0.0, |p: &[f64; 3]| p[i])).sum();
                        new[i] = (v[i] + kappa * sum) / den;
                    }
                    if let Some(p) = x.get_mut(k) {
                        *p = new;
                    }
                }
            }
        }
        x
    }
}

/// `table` with `corr` composed on top (hue shifts add, scales multiply), clamped to the format's bounds.
pub fn compose(table: &HsvTable, corr: &[[f64; 3]], shrink: f64) -> HsvTable {
    let mut t = table.clone();
    for (e, c) in t.data.iter_mut().zip(corr) {
        let k = shrink.clamp(0.0, 1.0);
        e[0] = (f64::from(e[0]) + k * c[0]).clamp(-170.0, 170.0) as f32;
        e[1] = (f64::from(e[1]) * (k * c[1]).exp()).clamp(0.0, 8.0) as f32;
        e[2] = (f64::from(e[2]) * (k * c[2]).exp()).clamp(0.0, 8.0) as f32;
    }
    t
}

/// Whether a table changes anything noticeable.
pub fn is_identity(t: &HsvTable) -> bool {
    t.data.iter().all(|e| e[0].abs() < 1e-3 && (e[1] - 1.0).abs() < 1e-4 && (e[2] - 1.0).abs() < 1e-4)
}
