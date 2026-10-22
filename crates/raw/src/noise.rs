//! Sensor noise profile estimated from one raw frame.
//!
//! Demosaicing decisions (how hard to threshold a block, how much chroma to smooth) want the
//! sensor's noise level at each signal level, not a constant. A single frame is enough if the
//! samples compared are the *same colour* — two sites of one CFA phase see the same scene through
//! the same filter, so their difference is noise plus whatever the scene does between them. Taking a
//! low percentile of that difference over the whole frame picks the quiet pairs (flat areas) and
//! leaves textured ones out, so the result is a noise floor rather than a measure of detail.
//!
//! The model is the usual Poisson–Gaussian one, `σ²(μ) = a·μ + b`, with `μ` in normalised units
//! (black subtracted, white = 1): `a` is shot noise per unit signal, `b` the read-noise floor.

use crate::Cfa;

/// Noise variance as a function of the normalised signal: `σ²(μ) = a·μ + b`.
///
/// `a` is the shot-noise slope (normalised units per unit signal) and `b` the read-noise variance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseModel {
    pub a: f32,
    pub b: f32,
}

impl NoiseModel {
    /// Standard deviation at normalised signal `mu`, never below a tiny floor.
    pub fn sigma(&self, mu: f32) -> f32 {
        (self.a.max(0.0) * mu.max(0.0) + self.b.max(0.0)).max(1e-10).sqrt()
    }

    /// Whether the generalised Anscombe transform round-trips through `f32` for this model.
    ///
    /// The transform maps a level to `2·sqrt(μ/a + 3/8 + b/a²)`, so a fit with no shot noise (`a → 0`) puts
    /// the read term in the driving seat and the values become enormous; squaring them back in the inverse
    /// then cancels most of their significant digits, and what survives is an error of about `(b/a)·ε`
    /// levels. That has to stay below one 16-bit code (`1/65535`) for the plane to mean anything — a
    /// read-noise-only frame is *not* wrong to fit, but the stabilised domain has nothing to offer it.
    pub fn is_usable(&self) -> bool {
        let (a, b) = (self.a.max(0.0), self.b.max(0.0));
        a > 0.0 && (b / a) * f32::EPSILON <= 1.0 / 65535.0
    }

    /// The generalised Anscombe transform: `y = 2·sqrt(μ/a + 3/8 + b/a²)`, which turns a
    /// Poisson–Gaussian signal into one whose noise has unit variance at every level.
    ///
    /// `b/a²` is the read noise in electrons and `3/8` the usual Poisson correction.
    pub fn anscombe(&self, mu: f32) -> f32 {
        let a = self.a.max(1e-12);
        let read = self.b.max(0.0) / (a * a);
        2.0 * (mu.max(0.0) / a + 0.375 + read).max(0.0).sqrt()
    }

    /// The algebraic inverse of [`anscombe`](Self::anscombe): `μ = a·((y/2)² − 3/8 − b/a²)`.
    ///
    /// Values below the transform's floor (a shrunk coefficient can land there) clamp to zero.
    pub fn anscombe_inverse(&self, y: f32) -> f32 {
        let a = self.a.max(1e-12);
        let read = self.b.max(0.0) / (a * a);
        let v = (y * 0.5) * (y * 0.5) - 0.375 - read;
        (a * v.max(0.0)).max(0.0)
    }
}

const BLOCK: usize = 16;
/// Equal-count level groups the blocks are split into before taking a percentile in each.
const GROUPS: usize = 10;
/// Percentile of the per-block noise estimates taken as that level's noise floor: the quiet blocks.
const FLOOR: f32 = 0.10;
/// For Gaussian noise, the 25th percentile of `|x − y|` (with `σ(x − y) = √2·σ`) is `0.4506·σ`.
const QUANTILE_TO_SIGMA: f32 = 0.4506;

/// Estimate the profile from a single-channel mosaic laid out as `cfa`.
/// Invalid dimensions, sample counts or CFA layouts are reported to stderr and return `None`.
/// Valid frames with insufficient size or signal variation also return `None`.
pub fn estimate(data: &[f32], w: usize, h: usize, cfa: &Cfa) -> Option<NoiseModel> {
    if w.checked_mul(h).is_none_or(|n| n == 0 || n > data.len()) || !cfa.valid() || cfa.pattern.iter().any(|&c| c > 2) {
        eprintln!("lightcraft: cannot estimate noise: invalid dimensions, sample count or CFA layout");
        return None;
    }
    if w < 4 * BLOCK || h < 4 * BLOCK {
        return None;
    }
    // Nearest same-colour neighbour for every CFA phase: within one 6×6 X-Trans cell every site
    // has a partner at ≤ 3 px. Comparing the same colour keeps the CFA filters out of the statistic.
    let mut partner = vec![None; cfa.width * cfa.height];
    for py in 0..cfa.height {
        for px in 0..cfa.width {
            let c = cfa.color_at(px, py);
            'find: for d in 1..=cfa.width.max(cfa.height) {
                for dy in 0..=d {
                    let dx = d - dy;
                    if cfa.color_at(px + dx, py + dy) == c {
                        partner[py * cfa.width + px] = Some((dx, dy));
                        break 'find;
                    }
                }
            }
        }
    }
    let mut blocks: Vec<(f32, f32)> = Vec::new();
    let mut diffs: Vec<f32> = Vec::with_capacity(BLOCK * BLOCK);
    for by in (0..h.saturating_sub(BLOCK - 1)).step_by(BLOCK * 2) {
        for bx in (0..w.saturating_sub(BLOCK - 1)).step_by(BLOCK * 2) {
            let (mut sum, mut n) = (0.0f64, 0f64);
            diffs.clear();
            for y in by..by + BLOCK {
                for x in bx..bx + BLOCK {
                    let v = data[y * w + x] as f64;
                    if !v.is_finite() {
                        continue;
                    }
                    sum += v;
                    n += 1.0;
                    let Some((dx, dy)) = partner[(y % cfa.height) * cfa.width + (x % cfa.width)] else {
                        continue;
                    };
                    if x + dx >= bx + BLOCK || y + dy >= by + BLOCK {
                        continue;
                    }
                    let pair = data[(y + dy) * w + x + dx] as f64;
                    if pair.is_finite() {
                        diffs.push((v - pair).abs() as f32);
                    }
                }
            }
            if diffs.len() < BLOCK * BLOCK / 2 {
                continue;
            }
            let k = diffs.len() / 4;
            let (_, p25, _) = diffs.select_nth_unstable_by(k, |a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            let sigma = *p25 / QUANTILE_TO_SIGMA;
            if sigma > 0.0 && sigma.is_finite() {
                blocks.push(((sum / n) as f32, sigma));
            }
        }
    }
    if blocks.len() < GROUPS * 20 {
        return None;
    }
    // Split by level into equal-count groups and keep the quiet tenth of each: flat blocks, so what
    // is left in the differences is noise rather than detail.
    blocks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
    let mut points: Vec<(f64, f64)> = Vec::new();
    for group in blocks.chunks(blocks.len() / GROUPS) {
        if group.len() < 20 {
            continue;
        }
        let mut s: Vec<f32> = group.iter().map(|b| b.1).collect();
        let k = (s.len() as f32 * FLOOR) as usize;
        let (_, sigma, _) = s.select_nth_unstable_by(k, |a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let sigma = *sigma;
        let mu = group[group.len() / 2].0;
        if mu.is_finite() && sigma > 0.0 {
            points.push((mu as f64, (sigma as f64) * (sigma as f64)));
        }
    }
    if points.len() < 4 {
        return None;
    }
    // Least squares for σ² = a·μ + b over the group floors.
    let n = points.len() as f64;
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for &(x, y) in &points {
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    // Without a spread of signal levels the slope is not identifiable: say so instead of extrapolating a
    // wild line from blocks that are all at the same brightness.
    let spread = sxx / n - (sx / n) * (sx / n);
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-12 || spread < 4e-6 {
        return None;
    }
    let a = (n * sxy - sx * sy) / denom;
    let b = (sy - a * sx) / n;
    // Plausibility clamp: a fit outside these bounds is a bad fit, not a sensor.
    let model = NoiseModel { a: a.clamp(0.0, 0.05) as f32, b: b.clamp(1e-9, 1e-2) as f32 };
    (model.a.is_finite() && model.b.is_finite()).then_some(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dimensions_and_cfa_are_refused_without_panicking() {
        assert!(estimate(&[], usize::MAX, 64, &Cfa::xtrans()).is_none());
        assert!(estimate(&[], 0, 64, &Cfa::xtrans()).is_none());
        assert!(estimate(&[], 64, 64, &Cfa::xtrans()).is_none());
        let data = vec![0.2; 64 * 64];
        for cfa in [
            Cfa { width: 0, height: 6, pattern: vec![] },
            Cfa { width: 6, height: 6, pattern: vec![1] },
            Cfa { width: usize::MAX, height: 6, pattern: vec![] },
            Cfa { width: 1, height: 1, pattern: vec![3] },
        ] {
            assert!(estimate(&data, 64, 64, &cfa).is_none());
        }
    }

    /// Deterministic uniform in [-1, 1).
    fn rng(state: &mut u32) -> f32 {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    #[test]
    fn estimate_recovers_a_known_profile() {
        let (w, h) = (640, 640);
        let cfa = Cfa::xtrans();
        let (a, b) = (0.0015f32, 4e-6f32);
        let mut state = 12345u32;
        let mut data = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                // A gentle ramp in both axes: same-colour neighbours see nearly the same scene.
                let mu = 0.1 + 0.75 * (x as f32 / w as f32 * 0.6 + y as f32 / h as f32 * 0.4);
                let sigma = (a * mu + b).sqrt();
                data[y * w + x] = mu + rng(&mut state) * sigma * 1.732;
            }
        }
        let m = estimate(&data, w, h, &cfa).expect("profile");
        let want = NoiseModel { a, b };
        for mu in [0.15f32, 0.3, 0.5, 0.7, 0.85] {
            let (got, want_sigma) = (m.sigma(mu), want.sigma(mu));
            assert!((got / want_sigma - 1.0).abs() < 0.25, "sigma({mu}) = {got:.5} vs {want_sigma:.5} (a={:.3e}, b={:.3e})", m.a, m.b);
        }
    }

    /// Standard normal sample from the deterministic uniform generator.
    fn gauss(state: &mut u32) -> f32 {
        let u1 = (rng(state) + 1.0) * 0.5;
        let u2 = (rng(state) + 1.0) * 0.5;
        (-2.0 * u1.max(1e-7).ln()).sqrt() * (core::f32::consts::TAU * u2).cos()
    }

    #[test]
    fn anscombe_round_trips() {
        let m = NoiseModel { a: 0.002, b: 3e-6 };
        for v in [0.0f32, 1e-4, 0.01, 0.2, 0.5, 0.9] {
            let back = m.anscombe_inverse(m.anscombe(v));
            assert!((back - v).abs() < 1e-5, "v = {v}, back = {back}");
        }
        // A shrunk coefficient can land below the transform's floor: that clamps to zero, never below.
        assert_eq!(m.anscombe_inverse(0.0), 0.0);
        assert!(m.anscombe_inverse(-2.0) >= 0.0);
    }

    /// The whole point of the transform: noise variance is ~1 at every signal level, so one threshold fits all.
    #[test]
    fn anscombe_stabilises_variance_across_levels() {
        let m = NoiseModel { a: 0.002, b: 3e-6 };
        let mut state = 9u32;
        for mu in [0.05f32, 0.15, 0.4, 0.8] {
            let sigma = m.sigma(mu);
            let n = 20_000;
            let mut ys = Vec::with_capacity(n);
            for _ in 0..n {
                ys.push(m.anscombe(mu + gauss(&mut state) * sigma));
            }
            let mean = ys.iter().sum::<f32>() / n as f32;
            let var = ys.iter().map(|y| (y - mean).powi(2)).sum::<f32>() / n as f32;
            assert!((var - 1.0).abs() < 0.12, "variance at mu = {mu} is {var}");
        }
    }

    /// A fit with no shot noise is a valid description of the sensor, but the stabilised domain cannot hold
    /// it in `f32`: the read term forces enormous values and the inverse then cancels their digits. The
    /// pipeline falls back to the plain refinement for such a frame instead of returning a wrecked plane.
    #[test]
    fn a_read_noise_only_profile_is_not_usable_for_anscombe() {
        let mut m = NoiseModel { a: 0.002, b: 3e-6 };
        assert!(m.is_usable());
        m.a = 0.0;
        assert!(!m.is_usable());
        m.a = 1e-12;
        assert!(!m.is_usable());
        // Prove the damage the guard prevents: levels one 16-bit code apart land on the same float.
        let y = m.anscombe(0.3);
        let step = (m.anscombe(0.3 + 1.0 / 65535.0) - y).abs();
        assert!(step < y * f32::EPSILON, "one code is {step:.3} in a value of {y:.3e}");
    }

    #[test]
    fn flat_and_tiny_images_are_refused() {
        let cfa = Cfa::xtrans();
        assert!(estimate(&[0.5; 64 * 64], 64, 64, &cfa).is_none());
        assert!(estimate(&[0.5; 8 * 8], 8, 8, &cfa).is_none());
        assert!(estimate(&[f32::NAN; 256 * 256], 256, 256, &cfa).is_none());
    }

    #[test]
    fn sigma_is_finite_at_the_extremes() {
        let m = NoiseModel { a: 0.01, b: 0.0 };
        assert!(m.sigma(0.0).is_finite() && m.sigma(0.0) > 0.0);
        assert!(m.sigma(f32::NAN).is_finite());
        assert!(m.sigma(1e30).is_finite());
    }

    #[test]
    fn estimate_accuracy_across_synthetic_gain_levels() {
        let (w, h) = (512, 512);
        let cfa = Cfa::xtrans();
        let isos = [(200, 3e-5f32, 2e-7f32), (400, 6e-5f32, 4e-7f32), (1600, 2.4e-4f32, 1e-6f32), (6400, 9.6e-4f32, 4e-6f32)];
        let mut prev_sigma = 0.0f32;
        for (iso, a, b) in isos {
            let mut state = 42u32 + iso as u32;
            let mut data = vec![0.0f32; w * h];
            for y in 0..h {
                for x in 0..w {
                    let mu = 0.1 + 0.75 * (x as f32 / w as f32 * 0.6 + y as f32 / h as f32 * 0.4);
                    let sigma = (a * mu + b).sqrt();
                    data[y * w + x] = (mu + rng(&mut state) * sigma * 1.732).clamp(0.0, 1.0);
                }
            }
            let m = estimate(&data, w, h, &cfa).unwrap_or_else(|| panic!("failed to estimate profile for ISO {iso}"));
            assert!(m.is_usable(), "ISO {iso} profile must be usable for Anscombe");
            let sig50 = m.sigma(0.5);
            assert!(sig50 > prev_sigma, "sigma at ISO {iso} ({sig50}) must exceed previous ({prev_sigma})");
            prev_sigma = sig50;

            for mu in [0.15, 0.3, 0.5, 0.7, 0.85] {
                // The estimator is not an exact calibration: on these synthetic gains the fitted
                // sigma is 10.88 % low to 4.13 % high across all levels and ISO labels (measured
                // when this test last changed). Keep the budget at 15 % — headroom over the observed
                // worst case, but tight enough to fail if the fit degrades by a few more points.
                const SIGMA_ERROR: f32 = 0.15;
                let known_sigma = (a * mu + b).sqrt();
                let relative_error = m.sigma(mu) / known_sigma - 1.0;
                assert!(relative_error.abs() < SIGMA_ERROR, "ISO label {iso}, level {mu}: sigma error {relative_error}");
                let mut validation_seed = 73_921 + iso as u32;
                let n = 20_000;
                let ys: Vec<_> = (0..n).map(|_| m.anscombe(mu + gauss(&mut validation_seed) * known_sigma)).collect();
                let mean = ys.iter().sum::<f32>() / n as f32;
                let var = ys.iter().map(|y| (y - mean).powi(2)).sum::<f32>() / n as f32;
                eprintln!("ISO label {iso}, level {mu}: sigma error {relative_error:.4}, held-out Anscombe variance {var:.4}");
                // A fitted noise floor is not an exact calibration. Bound the independent
                // variance by the allowed sigma error, plus Monte Carlo sampling error.
                let variance_bounds = (1.0 / (1.0 + SIGMA_ERROR).powi(2) - 0.05)..(1.0 / (1.0 - SIGMA_ERROR).powi(2) + 0.05);
                assert!(variance_bounds.contains(&var), "ISO label {iso}, level {mu}: variance {var} outside {variance_bounds:?}");
            }
        }
    }
}
