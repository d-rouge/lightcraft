//! Demosaicing for non-Bayer patterns (Fujifilm X-Trans and others): our own edge-weighted colour-difference
//! interpolation.
//!
//! 1. Green at non-green sites: robust weighted mean of the green samples in the 3×3 neighbourhood (5×5 if
//!    none), each weighted by `1 / (ε + |g − median|)` so samples across an edge contribute little.
//! 2. **X-Trans only**: the interpolated green plane is refined in the DCT domain — overlapping 8×8 blocks are
//!    transformed, their coefficients shrunk toward zero by a block-adaptive threshold (DC untouched), and the
//!    blocks averaged back where they overlap. This removes the checkerboard ("maze") residue that a purely
//!    spatial green interpolation leaves in fine detail.
//! 3. Red and blue everywhere: `G(p) + Σ w_q (C(q) − G(q)) / Σ w_q` over same-colour samples `q` in the 5×5
//!    neighbourhood, with `w_q = 1 / (d²(p, q) · (ε + |G(q) − G(p)|))` — spatially close samples on the same
//!    side of an edge dominate.

use super::Mosaic;
use crate::noise::{self, NoiseModel};
use crate::{Cfa, Rgb32f};
use lightcraft_raster::par_rows;
use rayon::prelude::*;

const EPS: f32 = 1e-3;

/// Side of a DCT refinement block.
const REFINE_BLOCK: usize = 8;
/// Block step: 6 leaves a 2-pixel overlap in both directions.
const REFINE_STRIDE: usize = 6;
/// Largest block threshold, in normalised sample units: 4096/65535 of full scale.
const REFINE_CAP: f32 = 4096.0 / 65535.0;

#[derive(Clone, Copy)]
struct DirGreen {
    hl_off: isize,
    hr_off: isize,
    w_hl: f32,
    w_hr: f32,
    vu_off: isize,
    vd_off: isize,
    w_vu: f32,
    w_vd: f32,
}

#[derive(Clone, Copy)]
struct RbNeighbour {
    dx: isize,
    dy: isize,
    d2: f32,
    kind: u8, // 0: H (ax > ay), 1: V (ay > ax), 2: Diag (ax == ay)
}

/// Same-colour neighbour offsets for every pattern phase, in scan order (so sums run in the same
/// order as a plain 2-D loop and interior pixels match border pixels bit for bit).
struct Offsets {
    pw: usize,
    ph: usize,
    /// Per phase: green offsets at radius 1, then those at radius 2 (ring order as the loop below).
    green: Vec<(Vec<(isize, isize)>, Vec<(isize, isize)>)>,
    /// Per phase and colour (0 = red, 1 = blue): 5×5 offsets of that colour.
    rb: Vec<[Vec<RbNeighbour>; 2]>,
    /// Per phase: orthogonal horizontal and vertical green pairs for directional interpolation.
    dir_green: Vec<Option<DirGreen>>,
}

impl Offsets {
    fn new(m: &Mosaic) -> Offsets {
        let (pw, ph) = (m.cfa.width.max(1), m.cfa.height.max(1));
        let w = m.w as isize;
        // phases are evaluated far from the borders (pattern coordinates wrap)
        let col = |x: isize, y: isize| m.cfa.color_at(x.rem_euclid(pw as isize) as usize, y.rem_euclid(ph as isize) as usize);
        let mut green = Vec::new();
        let mut rb = Vec::new();
        let mut dir_green = Vec::new();
        for py in 0..ph as isize {
            for px in 0..pw as isize {
                let ring = |r: isize| {
                    let mut v = Vec::new();
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if (dx != 0 || dy != 0) && col(px + dx, py + dy) == 1 {
                                v.push((dx, dy));
                            }
                        }
                    }
                    v
                };
                green.push((ring(1), ring(2)));
                let of = |c: u8| {
                    let mut v = Vec::new();
                    for dy in -2isize..=2 {
                        for dx in -2isize..=2 {
                            if col(px + dx, py + dy) == c {
                                let (ax, ay) = (dx.abs(), dy.abs());
                                let kind = if ax > ay {
                                    0
                                } else if ay > ax {
                                    1
                                } else {
                                    2
                                };
                                let d2 = (dx * dx + dy * dy) as f32;
                                v.push(RbNeighbour { dx, dy, d2, kind });
                            }
                        }
                    }
                    v
                };
                rb.push([of(0), of(2)]);
                if col(px, py) == 1 {
                    dir_green.push(None);
                } else {
                    let hl = [-1isize, -2].into_iter().find(|&dx| col(px + dx, py) == 1);
                    let hr = [1isize, 2].into_iter().find(|&dx| col(px + dx, py) == 1);
                    let vu = [-1isize, -2].into_iter().find(|&dy| col(px, py + dy) == 1);
                    let vd = [1isize, 2].into_iter().find(|&dy| col(px, py + dy) == 1);
                    let pair = match (hl, hr, vu, vd) {
                        (Some(hl), Some(hr), Some(vu), Some(vd)) => {
                            let dl = hl.abs() as f32;
                            let dr = hr.abs() as f32;
                            let du = vu.abs() as f32;
                            let dd = vd.abs() as f32;
                            Some(DirGreen {
                                hl_off: hl,
                                hr_off: hr,
                                w_hl: dr / (dl + dr),
                                w_hr: dl / (dl + dr),
                                vu_off: vu * w,
                                vd_off: vd * w,
                                w_vu: dd / (du + dd),
                                w_vd: du / (du + dd),
                            })
                        }
                        _ => None,
                    };
                    dir_green.push(pair);
                }
            }
        }
        Offsets { pw, ph, green, rb, dir_green }
    }
    #[inline]
    fn phase(&self, x: usize, y: usize) -> usize {
        (y % self.ph) * self.pw + x % self.pw
    }
}

/// Robust weighted mean of `vals` around their median.
#[inline]
fn robust_mean(vals: &[f32]) -> f32 {
    let mut sorted = [0f32; 24];
    let sorted = &mut sorted[..vals.len()];
    sorted.copy_from_slice(vals);
    sorted.sort_unstable_by(|a, b| a.total_cmp(b));
    let med = sorted[sorted.len() / 2];
    let (mut s, mut ws) = (0.0f32, 0.0f32);
    for &v in vals {
        let wt = 1.0 / (EPS + (v - med).abs());
        s += wt * v;
        ws += wt;
    }
    s / ws
}

fn green(m: &Mosaic, off: &Offsets) -> Vec<f32> {
    let (w, h) = (m.w, m.h);
    let mut g = vec![0f32; w * h];
    par_rows(&mut g, w, |y, row| {
        let interior_y = y >= 2 && y + 2 < h;
        let yi = y as isize;
        let mut vals: Vec<f32> = Vec::with_capacity(24);
        for (x, o) in row.iter_mut().enumerate() {
            if interior_y && x >= 2 && x + 2 < w {
                // fast path: precomputed offsets, no reflection
                let ph = off.phase(x, y);
                let i = y * w + x;
                if m.cfa.color_at(x, y) == 1 {
                    *o = m.data[i];
                    continue;
                }
                if let Some(dir) = off.dir_green[ph] {
                    let hl = m.data[(i as isize + dir.hl_off) as usize];
                    let hr = m.data[(i as isize + dir.hr_off) as usize];
                    let gh = hl * dir.w_hl + hr * dir.w_hr;
                    let dh = (hl - hr).abs();

                    let vu = m.data[(i as isize + dir.vu_off) as usize];
                    let vd = m.data[(i as isize + dir.vd_off) as usize];
                    let gv = vu * dir.w_vu + vd * dir.w_vd;
                    let dv = (vu - vd).abs();

                    let wh = 1.0 / (EPS + dh);
                    let wv = 1.0 / (EPS + dv);
                    *o = (wh * gh + wv * gv) / (wh + wv);
                    continue;
                }
                let (r1, r2) = &off.green[ph];
                let ring = if r1.is_empty() { r2 } else { r1 };
                if ring.is_empty() {
                    *o = m.data[i];
                    continue;
                }
                vals.clear();
                vals.extend(ring.iter().map(|&(dx, dy)| m.data[(i as isize + dy * w as isize + dx) as usize]));
                *o = robust_mean(&vals);
                continue;
            }
            let x = x as isize;
            if m.color(x, yi) == 1 {
                *o = m.get(x, yi);
                continue;
            }
            let hl = [-1isize, -2].into_iter().find(|&dx| m.color(x + dx, yi) == 1);
            let hr = [1isize, 2].into_iter().find(|&dx| m.color(x + dx, yi) == 1);
            let vu = [-1isize, -2].into_iter().find(|&dy| m.color(x, yi + dy) == 1);
            let vd = [1isize, 2].into_iter().find(|&dy| m.color(x, yi + dy) == 1);
            if let (Some(hl), Some(hr), Some(vu), Some(vd)) = (hl, hr, vu, vd) {
                let l = m.get(x + hl, yi);
                let r = m.get(x + hr, yi);
                let dl = hl.abs() as f32;
                let dr = hr.abs() as f32;
                let gh = (l * dr + r * dl) / (dl + dr);
                let dh = (l - r).abs();

                let u = m.get(x, yi + vu);
                let d = m.get(x, yi + vd);
                let du = vu.abs() as f32;
                let dd = vd.abs() as f32;
                let gv = (u * dd + d * du) / (du + dd);
                let dv = (u - d).abs();

                let wh = 1.0 / (EPS + dh);
                let wv = 1.0 / (EPS + dv);
                *o = (wh * gh + wv * gv) / (wh + wv);
                continue;
            }
            vals.clear();
            for r in [1isize, 2] {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if (dx != 0 || dy != 0) && m.color(x + dx, yi + dy) == 1 {
                            vals.push(m.get(x + dx, yi + dy));
                        }
                    }
                }
                if !vals.is_empty() {
                    break;
                }
            }
            if vals.is_empty() {
                *o = m.get(x, yi);
                continue;
            }
            *o = robust_mean(&vals);
        }
    });
    g
}

/// X-Trans is the only 6×6 CFA we know: 20 green sites, 8 red and 8 blue per 36. Gating on the layout (not on
/// "not Bayer") keeps the refinement off other non-Bayer patterns, which have no maze residue to remove.
fn is_xtrans(cfa: &Cfa) -> bool {
    if cfa.width != 6 || cfa.height != 6 || cfa.pattern.len() != 36 {
        return false;
    }
    let mut counts = [0usize; 3];
    for &c in &cfa.pattern {
        match counts.get_mut(c as usize) {
            Some(n) => *n += 1,
            None => return false,
        }
    }
    counts == [8, 20, 8]
}

/// DCT threshold in units of the sensor noise at the block's level (`mean|AC| ≈ 0.8 σ` for white noise).
const THRESHOLD_SIGMAS: f32 = 2.0;

type Block8 = [[f32; REFINE_BLOCK]; REFINE_BLOCK];

/// Orthonormal DCT-II matrix: `c[n][k] = s_k · cos(π(2n+1)k / 16)`, `s_0 = √(1/8)`, `s_k = √(2/8)`.
fn dct_matrix() -> Block8 {
    let mut c = [[0.0f32; REFINE_BLOCK]; REFINE_BLOCK];
    for (n, row) in c.iter_mut().enumerate() {
        for (k, v) in row.iter_mut().enumerate() {
            let scale = if k == 0 { (1.0 / 8.0f32).sqrt() } else { (2.0 / 8.0f32).sqrt() };
            let angle = core::f32::consts::PI * (2 * n + 1) as f32 * k as f32 / 16.0;
            *v = scale * angle.cos();
        }
    }
    c
}

/// Forward 2-D DCT-II (rows, then columns). The transform is orthonormal, so `idct8x8` inverts it.
fn dct8x8(b: &mut Block8, c: &Block8) {
    let mut t: Block8 = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
    for r in 0..REFINE_BLOCK {
        for k in 0..REFINE_BLOCK {
            let mut s = 0.0f32;
            for n in 0..REFINE_BLOCK {
                s += b[r][n] * c[n][k];
            }
            t[r][k] = s;
        }
    }
    for col in 0..REFINE_BLOCK {
        for k in 0..REFINE_BLOCK {
            let mut s = 0.0f32;
            for n in 0..REFINE_BLOCK {
                s += t[n][col] * c[n][k];
            }
            b[k][col] = s;
        }
    }
}

/// Inverse 2-D DCT (columns, then rows): `x[n] = Σ_k c[n][k] · X[k]`.
fn idct8x8(b: &mut Block8, c: &Block8) {
    let mut t: Block8 = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
    for col in 0..REFINE_BLOCK {
        for n in 0..REFINE_BLOCK {
            let mut s = 0.0f32;
            for k in 0..REFINE_BLOCK {
                s += b[k][col] * c[n][k];
            }
            t[n][col] = s;
        }
    }
    for r in 0..REFINE_BLOCK {
        for n in 0..REFINE_BLOCK {
            let mut s = 0.0f32;
            for k in 0..REFINE_BLOCK {
                s += t[r][k] * c[n][k];
            }
            b[r][n] = s;
        }
    }
}

/// Thresholding mode for DCT green refinement.
#[derive(Clone, Copy)]
enum RefineMode<'a> {
    /// Constant noise standard deviation across all blocks (used in the Anscombe variance-stabilised domain).
    Constant(f32),
    /// Level-adaptive physical noise model evaluated in the linear domain (`sigma(dc / 8)` per block).
    Model(&'a NoiseModel),
    /// Fallback for frames too small to estimate a profile: threshold from the block's own AC energy.
    BlockAc,
}

/// Shrink AC coefficients toward zero, leaving DC alone so the block mean (and therefore the tone) is unchanged.
///
/// In the Anscombe domain [`RefineMode::Constant`] provides a constant `THRESHOLD_SIGMAS · sigma` threshold.
/// In the linear domain with a fitted noise profile [`RefineMode::Model`] computes the sensor noise for each
/// block from its DC level, `THRESHOLD_SIGMAS · sigma(mu)`. Without a profile [`RefineMode::BlockAc`] uses the
/// block's own AC content as a fallback.
/// Modelled modes use nonnegative-garrote shrinkage: strong texture loses `threshold² / |coefficient|`
/// rather than a fixed threshold. The unmodelled fallback keeps its soft threshold.
fn soften(b: &mut Block8, mode: RefineMode<'_>) {
    let dc = b[0][0];
    let mut ac = 0.0f32;
    for (i, row) in b.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            if (i | j) != 0 {
                ac += v.abs();
            }
        }
    }
    let mean_ac = ac / (REFINE_BLOCK * REFINE_BLOCK - 1) as f32;
    let t0 = match mode {
        RefineMode::Constant(s) => THRESHOLD_SIGMAS * s.max(0.0),
        RefineMode::Model(model) => THRESHOLD_SIGMAS * model.sigma(dc * (1.0 / 8.0)),
        RefineMode::BlockAc => (dc.abs() * 0.01).min(mean_ac).min(REFINE_CAP),
    };
    for (i, row) in b.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            if (i | j) == 0 {
                continue;
            }
            let m = match mode {
                RefineMode::BlockAc => i.max(j) as f32 * t0 / (REFINE_BLOCK - 1) as f32,
                // A sensor-derived threshold is already in orthonormal coefficient units.
                // Scaling it down at low frequencies leaves correlated demosaic noise untouched.
                RefineMode::Constant(_) | RefineMode::Model(_) => t0,
            };
            let x = *v;
            *v = if x.abs() <= m {
                0.0
            } else {
                match mode {
                    RefineMode::BlockAc => x - m.copysign(x),
                    RefineMode::Constant(_) | RefineMode::Model(_) => {
                        let ratio = m / x;
                        x * (1.0 - ratio * ratio)
                    }
                }
            };
        }
    }
}

/// DCT-domain refinement of the interpolated green plane.
///
/// Blocks of [`REFINE_BLOCK`]² step by [`REFINE_STRIDE`], so every interior pixel is covered by up to four
/// blocks; the inverse-transformed blocks are averaged where they overlap. Pixels no block covers (the last
/// few rows and columns) keep their original value. Blocks holding a non-finite sample are skipped: a NaN
/// would otherwise spread across the whole transform.
fn refine_green(src: &[f32], w: usize, h: usize, mode: RefineMode<'_>) -> Vec<f32> {
    let mut dst = src.to_vec();
    if w < REFINE_BLOCK || h < REFINE_BLOCK || w.checked_mul(h) != Some(src.len()) {
        return dst;
    }
    let c = dct_matrix();
    // Strips are whole multiples of the block step: a block that straddles two strips is recomputed by each
    // (the source is read-only) and each strip writes only its own rows.
    let strip = REFINE_STRIDE * REFINE_BLOCK;
    let chunk = strip.saturating_mul(w);
    if chunk == 0 {
        return dst;
    }
    dst.par_chunks_mut(chunk).enumerate().for_each(|(i, out)| {
        refine_strip(src, out, w, h, i.saturating_mul(strip), &c, mode);
    });
    dst
}

fn refine_strip(src: &[f32], out: &mut [f32], w: usize, h: usize, y0: usize, c: &Block8, mode: RefineMode<'_>) {
    let rows = out.len() / w;
    if rows == 0 {
        return;
    }
    let y1 = y0.saturating_add(rows).min(h);
    let mut sum = vec![0.0f32; out.len()];
    let mut count = vec![0.0f32; out.len()];
    // First block row whose [by, by + BLOCK) range intersects [y0, y1).
    let mut by = if y0 + 1 >= REFINE_BLOCK { (y0 + 1 - REFINE_BLOCK).div_ceil(REFINE_STRIDE) * REFINE_STRIDE } else { 0 };
    while by < y1 && by + REFINE_BLOCK <= h {
        let mut bx = 0usize;
        while bx + REFINE_BLOCK <= w {
            let mut block: Block8 = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
            let mut usable = true;
            'load: for (r, row) in block.iter_mut().enumerate() {
                let base = (by + r) * w + bx;
                for (col, v) in row.iter_mut().enumerate() {
                    match src.get(base + col) {
                        Some(&x) if x.is_finite() => *v = x,
                        _ => {
                            usable = false;
                            break 'load;
                        }
                    }
                }
            }
            if usable {
                dct8x8(&mut block, c);
                soften(&mut block, mode);
                idct8x8(&mut block, c);
                for (r, row) in block.iter().enumerate() {
                    let y = by + r;
                    if y < y0 || y >= y1 {
                        continue;
                    }
                    let local = (y - y0) * w + bx;
                    for (col, &v) in row.iter().enumerate() {
                        if let (Some(s), Some(n)) = (sum.get_mut(local + col), count.get_mut(local + col)) {
                            *s += v;
                            *n += 1.0;
                        }
                    }
                }
            }
            bx += REFINE_STRIDE;
        }
        by += REFINE_STRIDE;
    }
    for (i, o) in out.iter_mut().enumerate() {
        if let (Some(&s), Some(&n)) = (sum.get(i), count.get(i))
            && n > 0.0
        {
            *o = s / n;
        }
    }
}

/// 7-tap horizontal smoothing filter `[1, 2, 2, 2, 2, 2, 1] / 12` used as a directional guide for edge detection.
fn guide_7tap_h(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut dst = vec![0.0f32; w * h];
    par_rows(&mut dst, w, |y, row| {
        let base = y * w;
        let srow = &src[base..base + w];
        // Fast interior path (no reflection)
        if w >= 7 {
            for x in 3..w - 3 {
                row[x] = (srow[x - 3] + 2.0 * (srow[x - 2] + srow[x - 1] + srow[x] + srow[x + 1] + srow[x + 2]) + srow[x + 3]) * (1.0 / 12.0);
            }
            for x in (0..3).chain(w - 3..w) {
                let get = |dx: isize| srow[super::reflect(x as isize + dx, w)];
                row[x] = (get(-3) + 2.0 * (get(-2) + get(-1) + get(0) + get(1) + get(2)) + get(3)) * (1.0 / 12.0);
            }
        } else {
            for (x, o) in row.iter_mut().enumerate() {
                let get = |dx: isize| srow[super::reflect(x as isize + dx, w)];
                *o = (get(-3) + 2.0 * (get(-2) + get(-1) + get(0) + get(1) + get(2)) + get(3)) * (1.0 / 12.0);
            }
        }
    });
    dst
}

/// 7-tap vertical smoothing filter `[1, 2, 2, 2, 2, 2, 1] / 12` used as a directional guide for edge detection.
fn guide_7tap_v(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut dst = vec![0.0f32; w * h];
    par_rows(&mut dst, w, |y, row| {
        if h >= 7 && y >= 3 && y + 3 < h {
            let r_m3 = &src[(y - 3) * w..];
            let r_m2 = &src[(y - 2) * w..];
            let r_m1 = &src[(y - 1) * w..];
            let r_0 = &src[y * w..];
            let r_p1 = &src[(y + 1) * w..];
            let r_p2 = &src[(y + 2) * w..];
            let r_p3 = &src[(y + 3) * w..];
            for x in 0..w {
                row[x] = (r_m3[x] + 2.0 * (r_m2[x] + r_m1[x] + r_0[x] + r_p1[x] + r_p2[x]) + r_p3[x]) * (1.0 / 12.0);
            }
        } else {
            for (x, o) in row.iter_mut().enumerate() {
                let get = |dy: isize| {
                    let yy = super::reflect(y as isize + dy, h);
                    src[yy * w + x]
                };
                *o = (get(-3) + 2.0 * (get(-2) + get(-1) + get(0) + get(1) + get(2)) + get(3)) * (1.0 / 12.0);
            }
        }
    });
    dst
}

/// Remove isolated pixel spikes (SILKYPIX `RemoveIsolated`): an impulse whose value exceeds all 8
/// neighbours (or falls below all 8) is clamped to their `[min, max]`. A 1-pixel wide line or corner
/// has neighbours on the feature with similar values, so only isolated noise spikes are clipped.
fn remove_isolated(rgb: &mut Rgb32f) {
    let (w, h) = (rgb.width, rgb.height);
    if w < 3 || h < 3 {
        return;
    }
    const STRIP: usize = 64;
    let chunk = STRIP * w;
    let num_strips = rgb.data.len().div_ceil(chunk);

    // Save only the boundary rows needed by adjacent strips (~2 MB instead of 192 MB full clone).
    // Strip `si` needs row `y0 - 1` (bottom of strip `si - 1`) and row `y1` (top of strip `si + 1`).
    let mut top_boundary = Vec::with_capacity(num_strips);
    let mut bot_boundary = Vec::with_capacity(num_strips);
    for si in 0..num_strips {
        let y0 = si * STRIP;
        let y1 = (y0 + STRIP).min(h);
        let top = if y0 > 0 { rgb.data[(y0 - 1) * w..y0 * w].to_vec() } else { Vec::new() };
        let bot = if y1 < h { rgb.data[y1 * w..(y1 + 1) * w].to_vec() } else { Vec::new() };
        top_boundary.push(top);
        bot_boundary.push(bot);
    }

    rgb.data.par_chunks_mut(chunk).enumerate().for_each(|(si, strip)| {
        let y0 = si * STRIP;
        let rows = strip.len() / w;
        if rows == 0 {
            return;
        }
        let top_bound = &top_boundary[si];
        let bot_bound = &bot_boundary[si];

        let mut r_top = vec![[0.0f32; 3]; w];
        let mut r_mid = vec![[0.0f32; 3]; w];
        let mut r_bot = vec![[0.0f32; 3]; w];

        // Prime r_top and r_mid before the loop
        if !top_bound.is_empty() {
            r_top.copy_from_slice(top_bound);
        }
        r_mid.copy_from_slice(&strip[0..w]);

        for local_y in 0..rows {
            let y = y0 + local_y;

            // Load r_bot before modifying strip[local_y * w ..]
            if local_y + 1 < rows {
                r_bot.copy_from_slice(&strip[(local_y + 1) * w..(local_y + 2) * w]);
            } else if !bot_bound.is_empty() {
                r_bot.copy_from_slice(bot_bound);
            }

            if y > 0 && y + 1 < h {
                let out_row = &mut strip[local_y * w..(local_y + 1) * w];
                for x in 1..w - 1 {
                    let px = r_mid[x];
                    for c in 0..3 {
                        let v = px[c];
                        let mut lo = r_top[x - 1][c].min(r_top[x][c]).min(r_top[x + 1][c]);
                        let mut hi = r_top[x - 1][c].max(r_top[x][c]).max(r_top[x + 1][c]);

                        lo = lo.min(r_mid[x - 1][c]).min(r_mid[x + 1][c]);
                        hi = hi.max(r_mid[x - 1][c]).max(r_mid[x + 1][c]);

                        lo = lo.min(r_bot[x - 1][c]).min(r_bot[x][c]).min(r_bot[x + 1][c]);
                        hi = hi.max(r_bot[x - 1][c]).max(r_bot[x][c]).max(r_bot[x + 1][c]);

                        out_row[x][c] = v.clamp(lo, hi);
                    }
                }
            }

            // Shift for next row: r_top takes r_mid, r_mid takes r_bot
            std::mem::swap(&mut r_top, &mut r_mid);
            std::mem::swap(&mut r_mid, &mut r_bot);
        }
    });
}

pub(crate) fn directional(m: &Mosaic) -> Rgb32f {
    directional_with(m, true)
}

/// `refine` exists for the tests that pin the interpolation itself: it skips the X-Trans DCT pass, so the
/// precomputed fast path can be compared bit for bit with the straightforward per-pixel formulation.
fn directional_with(m: &Mosaic, refine: bool) -> Rgb32f {
    let (w, h) = (m.w, m.h);
    let off = Offsets::new(m);
    let xtrans = refine && is_xtrans(m.cfa);
    let raw_model = if xtrans { noise::estimate(m.data, w, h, m.cfa) } else { None };
    let g = if xtrans {
        match raw_model.as_ref().filter(|m| m.is_usable()) {
            Some(model) => {
                // Estimate the green plane in the Anscombe domain, where the noise has unit variance at
                // every level: the interpolation weights then compare differences in noise units and the
                // DCT threshold is a constant instead of a level-dependent one. Back to linear at the end.
                let y: Vec<f32> = m.data.iter().map(|&v| model.anscombe(v)).collect();
                let ym = Mosaic { w, h, data: &y, cfa: m.cfa };
                let gy = refine_green(&green(&ym, &off), w, h, RefineMode::Constant(1.0));
                gy.into_iter().map(|v| model.anscombe_inverse(v)).collect()
            }
            // Linear-domain refinement: with a fitted model (e.g. read-noise-only where Anscombe cannot
            // round-trip through f32), the threshold adapts to the noise at the block's level.
            // Tiny or uniform frames fall back to the block's own AC.
            None => {
                let mode = match &raw_model {
                    Some(m) => RefineMode::Model(m),
                    None => RefineMode::BlockAc,
                };
                refine_green(&green(m, &off), w, h, mode)
            }
        }
    } else {
        green(m, &off)
    };
    reconstruct_rgb(m, &off, &g, xtrans, raw_model.as_ref())
}

fn reconstruct_rgb(m: &Mosaic, off: &Offsets, g: &[f32], xtrans: bool, raw_model: Option<&NoiseModel>) -> Rgb32f {
    let (w, h) = (m.w, m.h);
    let (gh, gv) = if xtrans { (Some(guide_7tap_h(g, w, h)), Some(guide_7tap_v(g, w, h))) } else { (None, None) };
    let gg = |x: isize, y: isize| g[super::reflect(y, h) * w + super::reflect(x, w)];
    let mut out = Rgb32f::new(w, h);
    par_rows(&mut out.data, w, |y, row| {
        let interior_y = y >= 2 && y + 2 < h;
        let yi = y as isize;
        for (x, px) in row.iter_mut().enumerate() {
            if interior_y && x >= 2 && x + 2 < w {
                let i = y * w + x;
                let own = m.cfa.color_at(x, y) as usize;
                let gp = g[i];
                px[1] = gp;
                let (ghi, gvi, gdi) = match (&gh, &gv) {
                    (Some(gh), Some(gv)) => {
                        let h = gh[i];
                        let v = gv[i];
                        (h, v, (h + v) * 0.5)
                    }
                    _ => (0.0, 0.0, 0.0),
                };
                let lists = &off.rb[off.phase(x, y)];
                for (li, c) in [0usize, 2].into_iter().enumerate() {
                    if c == own {
                        px[c] = m.data[i];
                        continue;
                    }
                    let (mut s, mut ws) = (0.0f32, 0.0f32);
                    for nb in &lists[li] {
                        let j = (i as isize + nb.dy * w as isize + nb.dx) as usize;
                        let gq = g[j];
                        let diff = match (&gh, &gv) {
                            (Some(gh), Some(gv)) => match nb.kind {
                                0 => (gh[j] - ghi).abs(),
                                1 => (gv[j] - gvi).abs(),
                                _ => ((gh[j] + gv[j]) * 0.5 - gdi).abs(),
                            },
                            _ => (gq - gp).abs(),
                        };
                        let wt = 1.0 / (nb.d2 * (EPS + diff));
                        s += wt * (m.data[j] - gq);
                        ws += wt;
                    }
                    px[c] = if ws > 0.0 { gp + s / ws } else { gp };
                }
                continue;
            }
            let x = x as isize;
            let own = m.color(x, yi) as usize;
            let gp = gg(x, yi);
            let i = y * w + x as usize;
            px[1] = gp;
            for c in [0usize, 2] {
                if c == own {
                    px[c] = m.get(x, yi);
                    continue;
                }
                let (mut s, mut ws) = (0.0f32, 0.0f32);
                for dy in -2isize..=2 {
                    for dx in -2isize..=2 {
                        if m.color(x + dx, yi + dy) as usize != c {
                            continue;
                        }
                        let gq = gg(x + dx, yi + dy);
                        let d2 = (dx * dx + dy * dy) as f32;
                        let diff = match (&gh, &gv) {
                            (Some(gh), Some(gv)) => {
                                let (ax, ay) = (dx.abs(), dy.abs());
                                let j = super::reflect(yi + dy, h) * w + super::reflect(x + dx, w);
                                if ax > ay {
                                    (gh[j] - gh[i]).abs()
                                } else if ay > ax {
                                    (gv[j] - gv[i]).abs()
                                } else {
                                    ((gh[j] + gv[j]) * 0.5 - (gh[i] + gv[i]) * 0.5).abs()
                                }
                            }
                            _ => (gq - gp).abs(),
                        };
                        let wt = 1.0 / (d2 * (EPS + diff));
                        s += wt * (m.get(x + dx, yi + dy) - gq);
                        ws += wt;
                    }
                }
                px[c] = if ws > 0.0 { gp + s / ws } else { gp };
            }
        }
    });
    // Colour-difference interpolation is the fallback; with a noise profile the two colour channels are
    // re-estimated by a local regression on green, which is what removes chrome speckle at high ISO.
    if let Some(model) = raw_model.filter(|m| m.is_usable()) {
        chroma_guided_joint(m, g, model, &mut out);
    }
    if xtrans {
        remove_isolated(&mut out);
    }
    out
}

/// Radius of the chroma regression window: a 7×7 box, wide enough to hold several samples of each
/// colour on the 6×6 X-Trans cell and narrow enough to follow colour edges.
const CHROMA_RADIUS: usize = 3;
/// Output rows per parallel strip: keeps the per-strip ring buffer small.
const CHROMA_STRIP: usize = 128;
/// Guard added to the channel and to green before the ratio is taken, in normalised units
/// (`16 / 65535`). Without it the ratio is unbounded where green sits at black; the reference converter
/// adds the same 16 (of 65535) for the same reason.
const RATIO_GUARD: f32 = 16.0 / 65535.0;

fn chroma_guided_joint(m: &Mosaic, g: &[f32], model: &NoiseModel, out: &mut Rgb32f) {
    let (w, h) = (m.w, m.h);
    let r = CHROMA_RADIUS;
    if w < 2 * r + 1 || h < 2 * r + 1 || g.len() < w * h || out.data.len() < w * h {
        return;
    }
    let chunk = CHROMA_STRIP * w;
    out.data.par_chunks_mut(chunk).enumerate().for_each(|(si, rows)| {
        chroma_strip_joint(m, g, model, rows, w, h, si * CHROMA_STRIP);
    });
}

fn chroma_strip_joint(m: &Mosaic, g: &[f32], model: &NoiseModel, rows_out: &mut [[f32; 3]], w: usize, h: usize, y0: usize) {
    let r = CHROMA_RADIUS;
    let win = 2 * r + 1;
    let n = rows_out.len() / w;
    if n == 0 || w < win || h < win {
        return;
    }
    let y1 = (y0 + n).min(h);
    let ay0 = y0.saturating_sub(r);
    let ay1 = (y1 + r).min(h);
    let na = ay1 - ay0;
    if na == 0 {
        return;
    }
    let mut ab_r = vec![0.0f32; 2 * na * w];
    let mut ab_b = vec![0.0f32; 2 * na * w];
    let mut ring_r = vec![0.0f64; 5 * win * w];
    let mut ring_b = vec![0.0f64; 5 * win * w];
    let mut vsum_r = vec![0.0f64; 5 * w];
    let mut vsum_b = vec![0.0f64; 5 * w];
    let mut prefix_r = vec![[0.0f64; 5]; w + 1];
    let mut prefix_b = vec![[0.0f64; 5]; w + 1];

    for y in ay0.saturating_sub(r)..ay1 + r {
        if y >= win {
            let old = ((y - win) % win) * w;
            for p in 0..5usize {
                let base = p * win * w + old;
                let dst = p * w;
                for x in 0..w {
                    vsum_r[dst + x] -= ring_r[base + x];
                    vsum_b[dst + x] -= ring_b[base + x];
                }
            }
        }
        if y < h {
            prefix_r[0] = [0.0; 5];
            prefix_b[0] = [0.0; 5];
            let row_base = y * w;
            for x in 0..w {
                let i = row_base + x;
                let own = m.cfa.color_at(x, y) as usize;
                prefix_r[x + 1] = prefix_r[x];
                prefix_b[x + 1] = prefix_b[x];
                if own == 0 {
                    let green = f64::from(g[i]);
                    let rho = (f64::from(m.data[i]) + f64::from(RATIO_GUARD)) / (green + f64::from(RATIO_GUARD));
                    let vals = [1.0, if rho.is_finite() { rho } else { 0.0 }, green, green * green, if rho.is_finite() { rho * green } else { 0.0 }];
                    for p in 0..5 {
                        prefix_r[x + 1][p] += vals[p];
                    }
                } else if own == 2 {
                    let green = f64::from(g[i]);
                    let rho = (f64::from(m.data[i]) + f64::from(RATIO_GUARD)) / (green + f64::from(RATIO_GUARD));
                    let vals = [1.0, if rho.is_finite() { rho } else { 0.0 }, green, green * green, if rho.is_finite() { rho * green } else { 0.0 }];
                    for p in 0..5 {
                        prefix_b[x + 1][p] += vals[p];
                    }
                }
            }
            let slot = (y % win) * w;
            for x in 0..w {
                let lo = x.saturating_sub(r);
                let hi = (x + r + 1).min(w);
                for p in 0..5 {
                    let vr = prefix_r[hi][p] - prefix_r[lo][p];
                    ring_r[p * win * w + slot + x] = vr;
                    vsum_r[p * w + x] += vr;

                    let vb = prefix_b[hi][p] - prefix_b[lo][p];
                    ring_b[p * win * w + slot + x] = vb;
                    vsum_b[p * w + x] += vb;
                }
            }
        }
        if y < r {
            continue;
        }
        let oy = y - r;
        if oy < ay0 || oy >= ay1 {
            continue;
        }
        let row = oy - ay0;
        for x in 0..w {
            // Solve for Red
            let m0_r = vsum_r[x];
            if m0_r.is_finite() && m0_r > 0.5 {
                let inv = 1.0 / m0_r;
                let ratio_mean = vsum_r[w + x] * inv;
                let green_mean = vsum_r[2 * w + x] * inv;
                let green_var = (vsum_r[3 * w + x] * inv - green_mean * green_mean).max(0.0);
                let cov = vsum_r[4 * w + x] * inv - ratio_mean * green_mean;
                let lambda = f64::from(model.sigma(green_mean as f32)).powi(2) + 0.01 * green_mean * green_mean;
                let a = if green_var + lambda > 1e-12 { cov / (green_var + lambda) } else { 0.0 };
                let b = ratio_mean - a * green_mean;
                if a.is_finite() && b.is_finite() {
                    ab_r[row * w + x] = a as f32;
                    ab_r[(na + row) * w + x] = b as f32;
                }
            }
            // Solve for Blue
            let m0_b = vsum_b[x];
            if m0_b.is_finite() && m0_b > 0.5 {
                let inv = 1.0 / m0_b;
                let ratio_mean = vsum_b[w + x] * inv;
                let green_mean = vsum_b[2 * w + x] * inv;
                let green_var = (vsum_b[3 * w + x] * inv - green_mean * green_mean).max(0.0);
                let cov = vsum_b[4 * w + x] * inv - ratio_mean * green_mean;
                let lambda = f64::from(model.sigma(green_mean as f32)).powi(2) + 0.01 * green_mean * green_mean;
                let a = if green_var + lambda > 1e-12 { cov / (green_var + lambda) } else { 0.0 };
                let b = ratio_mean - a * green_mean;
                if a.is_finite() && b.is_finite() {
                    ab_b[row * w + x] = a as f32;
                    ab_b[(na + row) * w + x] = b as f32;
                }
            }
        }
    }
    // Window mean of the coefficients: horizontal sums, then a vertical sum per output row.
    let mut abh_r = vec![0.0f32; 2 * na * w];
    let mut abh_b = vec![0.0f32; 2 * na * w];
    let mut prefix_ab = vec![0.0f32; w + 1];
    for plane in 0..2usize {
        for row in 0..na {
            let base = (plane * na + row) * w;
            prefix_ab[0] = 0.0;
            for x in 0..w {
                prefix_ab[x + 1] = prefix_ab[x] + ab_r[base + x];
            }
            for x in 0..w {
                abh_r[base + x] = prefix_ab[(x + r + 1).min(w)] - prefix_ab[x.saturating_sub(r)];
            }
            prefix_ab[0] = 0.0;
            for x in 0..w {
                prefix_ab[x + 1] = prefix_ab[x] + ab_b[base + x];
            }
            for x in 0..w {
                abh_b[base + x] = prefix_ab[(x + r + 1).min(w)] - prefix_ab[x.saturating_sub(r)];
            }
        }
    }
    let mut sa_r = vec![0.0f32; w];
    let mut sb_r = vec![0.0f32; w];
    let mut sa_b = vec![0.0f32; w];
    let mut sb_b = vec![0.0f32; w];
    let mut cur: Option<(usize, usize)> = None;
    for oy in y0..y1 {
        let row = (oy - ay0) as isize;
        let lo = (row - r as isize).max(0) as usize;
        let hi = ((row + r as isize + 1).max(0) as usize).min(na);
        if hi <= lo {
            continue;
        }
        match cur {
            None => {
                for k in lo..hi {
                    let base = k * w;
                    for x in 0..w {
                        sa_r[x] += abh_r[base + x];
                        sb_r[x] += abh_r[(na + k) * w + x];
                        sa_b[x] += abh_b[base + x];
                        sb_b[x] += abh_b[(na + k) * w + x];
                    }
                }
            }
            Some((old_lo, old_hi)) => {
                for k in old_lo..lo {
                    let base = k * w;
                    for x in 0..w {
                        sa_r[x] -= abh_r[base + x];
                        sb_r[x] -= abh_r[(na + k) * w + x];
                        sa_b[x] -= abh_b[base + x];
                        sb_b[x] -= abh_b[(na + k) * w + x];
                    }
                }
                for k in old_hi..hi {
                    let base = k * w;
                    for x in 0..w {
                        sa_r[x] += abh_r[base + x];
                        sb_r[x] += abh_r[(na + k) * w + x];
                        sa_b[x] += abh_b[base + x];
                        sb_b[x] += abh_b[(na + k) * w + x];
                    }
                }
            }
        }
        cur = Some((lo, hi));
        let rows = (hi - lo) as f32;
        let out_row = &mut rows_out[(oy - y0) * w..(oy - y0 + 1) * w];
        for x in 0..w {
            let cols = ((x + r + 1).min(w) - x.saturating_sub(r)) as f32;
            let inv = 1.0 / (rows * cols);
            let i = oy * w + x;
            let gi = g[i];
            let delta = 3.0 * model.sigma(gi);

            // Red
            let ratio_r = (sa_r[x] * inv) * gi + (sb_r[x] * inv);
            let chan_r = (ratio_r * (gi + RATIO_GUARD) - RATIO_GUARD).max(0.0);
            let own_r = out_row[x][0];
            if chan_r.is_finite() && own_r.is_finite() {
                let d = chan_r - own_r;
                let out = own_r + if delta > 0.0 { d / (1.0 + d.abs() / delta) } else { 0.0 };
                out_row[x][0] = out.max(0.0);
            }

            // Blue
            let ratio_b = (sa_b[x] * inv) * gi + (sb_b[x] * inv);
            let chan_b = (ratio_b * (gi + RATIO_GUARD) - RATIO_GUARD).max(0.0);
            let own_b = out_row[x][2];
            if chan_b.is_finite() && own_b.is_finite() {
                let d = chan_b - own_b;
                let out = own_b + if delta > 0.0 { d / (1.0 + d.abs() / delta) } else { 0.0 };
                out_row[x][2] = out.max(0.0);
            }
        }
    }
}

/// Re-estimate one colour channel from a local linear regression of its level ratio on green.
///
/// At every pixel of that colour the mosaic holds one sample; around each pixel the level ratio
/// `ρ = (C + g₀) / (G + g₀)` is fitted to the green plane over a 7×7 window, `ρ ≈ a·G + b`, and the pixel's
/// own green level scales the fitted ratio back into a channel, `C = ρ̂ · (G + g₀) − g₀`. The ratio is the right
/// variable here: it is invariant when the illumination — and so both levels — scales, where the
/// difference `C − G` is not, so a colour survives a shadow boundary without the slope having to model it.
///
/// The fit is a ridge regression, `a = cov(G, ρ) / (var(G) + λ)`, with
/// `λ = σ²(Ḡ) + 0.01·Ḡ²`. The relative floor keeps CFA aliasing from becoming a fitted colour edge
/// even on a quiet sensor. Where the ratio does not co-vary with
/// green (flat area, near-black, noise) the slope collapses and the pixel takes the window's mean ratio
/// at its own green level — that is the denoise — while along a colour edge the slope survives and the
/// colour follows it. The coefficients themselves are then averaged over the same window
/// (`ρ̂ = ā·G + b̄`), which is what makes a guided filter free of the over/undershoot that per-pixel
/// coefficients produce.
///
/// The result is softly bounded to `±3σ` of the level the colour-difference interpolation already put in
/// `out`: a poorly conditioned fit across a colour boundary may improve the estimate but can never draw a
/// halo of its own.
///
/// The moments are accumulated by a separable box filter with a ring of horizontally summed rows, so the
/// cost is constant per pixel and no full-frame plane is allocated.
#[cfg(test)]
fn chroma_guided(m: &Mosaic, g: &[f32], model: &NoiseModel, out: &mut Rgb32f, ch: usize) {
    let (w, h) = (m.w, m.h);
    let r = CHROMA_RADIUS;
    if ch > 2 || w < 2 * r + 1 || h < 2 * r + 1 || g.len() < w * h || out.data.len() < w * h {
        return;
    }
    let chunk = CHROMA_STRIP * w;
    out.data.par_chunks_mut(chunk).enumerate().for_each(|(si, rows)| {
        chroma_strip(m, g, model, rows, w, h, si * CHROMA_STRIP, ch);
    });
}

#[cfg(test)]
fn chroma_strip(m: &Mosaic, g: &[f32], model: &NoiseModel, rows_out: &mut [[f32; 3]], w: usize, h: usize, y0: usize, ch: usize) {
    let r = CHROMA_RADIUS;
    let win = 2 * r + 1;
    let n = rows_out.len() / w;
    if n == 0 || w < win || h < win {
        return;
    }
    let y1 = (y0 + n).min(h);
    // Coefficients are needed one window around the rows we write, so that their own window mean is
    // complete there: that second averaging is what keeps the filter from drawing colour fringes.
    let ay0 = y0.saturating_sub(r);
    let ay1 = (y1 + r).min(h);
    let na = ay1 - ay0;
    if na == 0 {
        return;
    }
    let mut ab = vec![0.0f32; 2 * na * w];
    let mut ring = vec![0.0f64; 5 * win * w];
    // Running vertical sum of the five planes, so each pixel costs a subtraction and an addition
    // rather than a walk over the window's rows.
    let mut vsum = vec![0.0f64; 5 * w];
    let mut prefix_mom = vec![[0.0f64; 5]; w + 1];
    for y in ay0.saturating_sub(r)..ay1 + r {
        if y >= win {
            let old = ((y - win) % win) * w;
            for p in 0..5usize {
                let base = p * win * w + old;
                let dst = p * w;
                for x in 0..w {
                    vsum[dst + x] -= ring[base + x];
                }
            }
        }
        if y < h {
            prefix_mom[0] = [0.0; 5];
            let row_base = y * w;
            for x in 0..w {
                let i = row_base + x;
                let own = m.cfa.color_at(x, y) as usize;
                if own == ch {
                    let green = f64::from(g[i]);
                    let rho = (f64::from(m.data[i]) + f64::from(RATIO_GUARD)) / (green + f64::from(RATIO_GUARD));
                    let vals = [1.0, if rho.is_finite() { rho } else { 0.0 }, green, green * green, if rho.is_finite() { rho * green } else { 0.0 }];
                    for p in 0..5 {
                        prefix_mom[x + 1][p] = prefix_mom[x][p] + vals[p];
                    }
                } else {
                    prefix_mom[x + 1] = prefix_mom[x];
                }
            }
            let slot = (y % win) * w;
            for x in 0..w {
                let lo = x.saturating_sub(r);
                let hi = (x + r + 1).min(w);
                for p in 0..5 {
                    let v = prefix_mom[hi][p] - prefix_mom[lo][p];
                    ring[p * win * w + slot + x] = v;
                    vsum[p * w + x] += v;
                }
            }
        }
        if y < r {
            continue;
        }
        let oy = y - r;
        if oy < ay0 || oy >= ay1 {
            continue;
        }
        for x in 0..w {
            let mut mom = [0.0f64; 5];
            for (p, sum) in mom.iter_mut().enumerate() {
                *sum = vsum[p * w + x];
            }
            let m0 = mom[0];
            if !m0.is_finite() || m0 <= 0.5 {
                continue;
            }
            let inv = 1.0 / m0;
            let ratio_mean = mom[1] * inv;
            let green_mean = mom[2] * inv;
            let green_var = (mom[3] * inv - green_mean * green_mean).max(0.0);
            let cov = mom[4] * inv - ratio_mean * green_mean;
            let lambda = f64::from(model.sigma(green_mean as f32)).powi(2) + 0.01 * green_mean * green_mean;
            let a = if green_var + lambda > 1e-12 { cov / (green_var + lambda) } else { 0.0 };
            let b = ratio_mean - a * green_mean;
            if a.is_finite() && b.is_finite() {
                let row = oy - ay0;
                ab[row * w + x] = a as f32;
                ab[(na + row) * w + x] = b as f32;
            }
        }
    }
    // Window mean of the coefficients: horizontal sums, then a vertical sum per output row.
    let mut abh = vec![0.0f32; 2 * na * w];
    let mut prefix_ab = vec![0.0f32; w + 1];
    for plane in 0..2usize {
        for row in 0..na {
            let base = (plane * na + row) * w;
            prefix_ab[0] = 0.0;
            for x in 0..w {
                prefix_ab[x + 1] = prefix_ab[x] + ab[base + x];
            }
            for x in 0..w {
                abh[base + x] = prefix_ab[(x + r + 1).min(w)] - prefix_ab[x.saturating_sub(r)];
            }
        }
    }
    let mut sa = vec![0.0f32; w];
    let mut sb = vec![0.0f32; w];
    // Slide the window over the coefficient rows: one row enters and one leaves per output row.
    let mut cur: Option<(usize, usize)> = None;
    for oy in y0..y1 {
        let row = (oy - ay0) as isize;
        let lo = (row - r as isize).max(0) as usize;
        let hi = ((row + r as isize + 1).max(0) as usize).min(na);
        if hi <= lo {
            continue;
        }
        match cur {
            None => {
                for k in lo..hi {
                    let base = k * w;
                    for x in 0..w {
                        sa[x] += abh[base + x];
                        sb[x] += abh[(na + k) * w + x];
                    }
                }
            }
            Some((old_lo, old_hi)) => {
                for k in old_lo..lo {
                    let base = k * w;
                    for x in 0..w {
                        sa[x] -= abh[base + x];
                        sb[x] -= abh[(na + k) * w + x];
                    }
                }
                for k in old_hi..hi {
                    let base = k * w;
                    for x in 0..w {
                        sa[x] += abh[base + x];
                        sb[x] += abh[(na + k) * w + x];
                    }
                }
            }
        }
        cur = Some((lo, hi));
        let rows = (hi - lo) as f32;
        for x in 0..w {
            let cols = ((x + r + 1).min(w) - x.saturating_sub(r)) as f32;
            let inv = 1.0 / (rows * cols);
            let i = oy * w + x;
            // The fit predicts the colour *ratio*, which is what stays put across a luminance change; the
            // channel is that ratio times the pixel's own green level. The correction is bounded to the
            // local noise, so a badly-conditioned fit across a colour boundary cannot draw a halo of its
            // own.
            let ratio = (sa[x] * inv) * g[i] + (sb[x] * inv);
            let chan = (ratio * (g[i] + RATIO_GUARD) - RATIO_GUARD).max(0.0);
            let own = rows_out[(oy - y0) * w + x][ch];
            if chan.is_finite() && own.is_finite() {
                // Soft-saturate the correction at ±3σ: `d/(1 + |d|/Δ)` is `≈ d` while the fit agrees with
                // the interpolation and flattens towards `Δ` when it wants to move further, without the
                // piecewise corners of a hard clamp (which show up as high-frequency colour once the
                // pipeline's colour noise reduction re-normalises the plane).
                let delta = 3.0 * model.sigma(g[i]);
                let d = chan - own;
                let out = own + if delta > 0.0 { d / (1.0 + d.abs() / delta) } else { 0.0 };
                rows_out[(oy - y0) * w + x][ch] = out.max(0.0);
            }
        }
    }
}

#[cfg(test)]
#[path = "xtrans_guided_tests.rs"]
mod guided_green;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cfa;
    /// Mean of a block's samples (the DC of an orthonormal DCT is `8 × mean`).
    fn block_mean(b: &Block8) -> f32 {
        let mut sum = 0.0f32;
        for row in b {
            for &v in row {
                sum += v;
            }
        }
        sum / (REFINE_BLOCK * REFINE_BLOCK) as f32
    }

    /// The straightforward per-pixel formulation (reflection at every access): the precomputed
    /// interior path must match it bit for bit.
    fn reference(m: &Mosaic) -> Rgb32f {
        let (w, h) = (m.w, m.h);
        let off = Offsets::new(m);
        let mut g = vec![0f32; w * h];
        for y in 0..h as isize {
            for x in 0..w as isize {
                let o = &mut g[y as usize * w + x as usize];
                if m.color(x, y) == 1 {
                    *o = m.get(x, y);
                    continue;
                }
                if y >= 2 && y + 2 < h as isize && x >= 2 && x + 2 < w as isize {
                    let ph = off.phase(x as usize, y as usize);
                    if let Some(dir) = off.dir_green[ph] {
                        let hl = m.get(x + dir.hl_off, y);
                        let hr = m.get(x + dir.hr_off, y);
                        let gh = hl * dir.w_hl + hr * dir.w_hr;
                        let dh = (hl - hr).abs();

                        let vu = m.get(x, y + dir.vu_off / w as isize);
                        let vd = m.get(x, y + dir.vd_off / w as isize);
                        let gv = vu * dir.w_vu + vd * dir.w_vd;
                        let dv = (vu - vd).abs();

                        let wh = 1.0 / (EPS + dh);
                        let wv = 1.0 / (EPS + dv);
                        *o = (wh * gh + wv * gv) / (wh + wv);
                        continue;
                    }
                }
                let hl = [-1isize, -2].into_iter().find(|&dx| m.color(x + dx, y) == 1);
                let hr = [1isize, 2].into_iter().find(|&dx| m.color(x + dx, y) == 1);
                let vu = [-1isize, -2].into_iter().find(|&dy| m.color(x, y + dy) == 1);
                let vd = [1isize, 2].into_iter().find(|&dy| m.color(x, y + dy) == 1);
                if let (Some(hl), Some(hr), Some(vu), Some(vd)) = (hl, hr, vu, vd) {
                    let l = m.get(x + hl, y);
                    let r = m.get(x + hr, y);
                    let dl = hl.abs() as f32;
                    let dr = hr.abs() as f32;
                    let gh = (l * dr + r * dl) / (dl + dr);
                    let dh = (l - r).abs();

                    let u = m.get(x, y + vu);
                    let d = m.get(x, y + vd);
                    let du = vu.abs() as f32;
                    let dd = vd.abs() as f32;
                    let gv = (u * dd + d * du) / (du + dd);
                    let dv = (u - d).abs();

                    let wh = 1.0 / (EPS + dh);
                    let wv = 1.0 / (EPS + dv);
                    *o = (wh * gh + wv * gv) / (wh + wv);
                    continue;
                }
                let mut vals = Vec::new();
                for r in [1isize, 2] {
                    for dy in -r..=r {
                        for dx in -r..=r {
                            if (dx != 0 || dy != 0) && m.color(x + dx, y + dy) == 1 {
                                vals.push(m.get(x + dx, y + dy));
                            }
                        }
                    }
                    if !vals.is_empty() {
                        break;
                    }
                }
                *o = if vals.is_empty() { m.get(x, y) } else { robust_mean(&vals) };
            }
        }
        let gg = |x: isize, y: isize| g[super::super::reflect(y, h) * w + super::super::reflect(x, w)];
        Rgb32f::from_fn(w, h, |x, y| {
            let (x, y) = (x as isize, y as isize);
            let own = m.color(x, y) as usize;
            let gp = gg(x, y);
            let mut px = [0f32, gp, 0.0];
            for c in [0usize, 2] {
                if c == own {
                    px[c] = m.get(x, y);
                    continue;
                }
                let (mut s, mut ws) = (0.0f32, 0.0f32);
                for dy in -2isize..=2 {
                    for dx in -2isize..=2 {
                        if m.color(x + dx, y + dy) as usize == c {
                            let gq = gg(x + dx, y + dy);
                            let wt = 1.0 / ((dx * dx + dy * dy) as f32 * (EPS + (gq - gp).abs()));
                            s += wt * (m.get(x + dx, y + dy) - gq);
                            ws += wt;
                        }
                    }
                }
                px[c] = if ws > 0.0 { gp + s / ws } else { gp };
            }
            px
        })
    }

    #[test]
    fn fast_path_is_bit_exact() {
        let cfa = Cfa::xtrans().shifted(1, 2);
        let (w, h) = (37, 29);
        let mut seed = 12345u32;
        let data: Vec<f32> = (0..w * h)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (seed >> 8) as f32 / (1u32 << 24) as f32
            })
            .collect();
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let ga = green(&m, &Offsets::new(&m));
        let rgb = reference(&m);
        let gb = (0..w * h).map(|i| rgb.data[i][1]).collect::<Vec<_>>();
        for (i, (&x, &y)) in ga.iter().zip(&gb).enumerate() {
            if x.to_bits() != y.to_bits() {
                panic!("g mismatch at index {i} ({}, {}): ga={x} gb={y}", i % w, i / w);
            }
        }
        let (a, b) = (directional_with(&m, false), reference(&m));
        for (idx, (p, q)) in a.data.iter().zip(&b.data).enumerate() {
            let (x, y) = (idx % w, idx / w);
            if p[1].to_bits() != q[1].to_bits() {
                panic!("g mismatch at ({x}, {y}): got {} want {}", p[1], q[1]);
            }
            if p.map(f32::to_bits) != q.map(f32::to_bits) {
                panic!("mismatch at ({x}, {y}): got {:?} want {:?}", p, q);
            }
        }
        assert!(a.data.iter().zip(&b.data).all(|(p, q)| p.map(f32::to_bits) == q.map(f32::to_bits)));
    }

    /// The DCT pair is orthonormal, so it inverts itself to float precision.
    #[test]
    fn dct_round_trip_is_identity() {
        let c = dct_matrix();
        let mut seed = 7u32;
        let mut block: Block8 = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
        for row in block.iter_mut() {
            for v in row.iter_mut() {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *v = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            }
        }
        let original = block;
        dct8x8(&mut block, &c);
        idct8x8(&mut block, &c);
        for (a, b) in block.iter().zip(&original) {
            for (&x, &y) in a.iter().zip(b) {
                assert!((x - y).abs() < 1e-5, "{x} vs {y}");
            }
        }
    }

    /// DC is never thresholded, so a constant plane comes back unchanged.
    #[test]
    fn constant_plane_is_preserved() {
        let (w, h) = (37, 29);
        let src = vec![0.42f32; w * h];
        let out = refine_green(&src, w, h, RefineMode::BlockAc);
        for (i, &v) in out.iter().enumerate() {
            assert!((v - 0.42).abs() < 1e-6, "pixel {i}: {v}");
        }
    }

    /// On a flat field the refinement removes high-frequency noise but leaves the mean alone.
    #[test]
    fn flat_noise_is_reduced_without_a_level_shift() {
        let (w, h) = (64, 64);
        let mut seed = 99u32;
        let mut src = vec![0.0f32; w * h];
        for v in &mut src {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = 0.25 + ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.02;
        }
        let out = refine_green(&src, w, h, RefineMode::BlockAc);
        let variance = |v: &[f32]| {
            let n = v.len().max(1) as f32;
            let mean = v.iter().sum::<f32>() / n;
            v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / n
        };
        let (before, after) = (variance(&src), variance(&out));
        assert!(after < before * 0.8, "variance {before:.6} → {after:.6}");
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!((mean(&out) - mean(&src)).abs() < 1e-4);
    }

    /// A step edge keeps its position and its contrast: the refinement must not blur detail into mush.
    #[test]
    fn step_edge_stays_sharp() {
        let (w, h) = (32, 32);
        let src: Vec<f32> = (0..w * h).map(|i| if i % w < w / 2 { 0.2 } else { 0.8 }).collect();
        let out = refine_green(&src, w, h, RefineMode::BlockAc);
        for y in 0..h {
            for x in 0..w {
                let want = if x < w / 2 { 0.2 } else { 0.8 };
                let got = out[y * w + x];
                assert!((got - want).abs() < 0.02, "({x},{y}) {got} vs {want}");
            }
        }
    }

    /// The refinement is for X-Trans: another 6×6 layout (or Bayer) must be rejected.
    #[test]
    fn refinement_gate_accepts_only_xtrans() {
        assert!(is_xtrans(&Cfa::xtrans()));
        assert!(is_xtrans(&Cfa::xtrans().shifted(1, 2)));
        assert!(!is_xtrans(&Cfa::bayer("RGGB").unwrap()));
        let all_green = Cfa { width: 6, height: 6, pattern: vec![1; 36] };
        assert!(!is_xtrans(&all_green));
        let bayer6 = Cfa { width: 6, height: 6, pattern: [0, 1, 1, 2].repeat(9) };
        assert!(!is_xtrans(&bayer6));
    }

    /// Through [`directional`], the pass runs on X-Trans and only there.
    #[test]
    fn directional_refines_xtrans_and_nothing_else() {
        let (w, h) = (48, 40);
        let mut seed = 5u32;
        let data: Vec<f32> = (0..w * h)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                0.3 + ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.05
            })
            .collect();
        let xt = Cfa::xtrans();
        let m = Mosaic { w, h, data: &data, cfa: &xt };
        let (plain, refined) = (directional_with(&m, false), directional_with(&m, true));
        assert!(plain.data.iter().zip(&refined.data).any(|(a, b)| a[1] != b[1]), "X-Trans not refined");

        let other = Cfa { width: 6, height: 6, pattern: [0, 1, 1, 2].repeat(9) };
        let m = Mosaic { w, h, data: &data, cfa: &other };
        let (plain, refined) = (directional_with(&m, false), directional_with(&m, true));
        assert!(plain.data.iter().zip(&refined.data).all(|(a, b)| a[1] == b[1]), "non-X-Trans refined");
    }

    /// Texture is signal, not noise: a block whose own AC is large must not be thresholded away when the
    /// sensor's noise at that level is small. This is the whole point of passing a [`NoiseModel`] in.
    #[test]
    fn a_quiet_sensor_does_not_threshold_away_texture() {
        let c = dct_matrix();
        let mut seed = 4242u32;
        let mut texture: Block8 = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
        for row in texture.iter_mut() {
            for v in row.iter_mut() {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *v = 0.5 + ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.1;
            }
        }
        let energy = |b: &Block8| {
            let mean = block_mean(b);
            b.iter().flatten().map(|v| (v - mean).powi(2)).sum::<f32>()
        };
        let softened = |mode: RefineMode<'_>| {
            let mut b = texture;
            dct8x8(&mut b, &c);
            soften(&mut b, mode);
            idct8x8(&mut b, &c);
            energy(&b)
        };
        let (own, modelled) = (softened(RefineMode::BlockAc), softened(RefineMode::Constant(1e-3)));
        assert!(modelled > own * 2.0, "texture energy kept: model {modelled:.5}, block's own AC {own:.5}");
    }

    /// End to end: with a usable noise profile the green plane is estimated in the Anscombe domain, and the
    /// result is less noisy than the plain interpolation. Guards the wiring, not just the pieces.
    #[test]
    fn refined_green_is_quieter_than_plain_on_a_noisy_frame() {
        let (w, h) = (512, 512);
        let cfa = Cfa::xtrans();
        let (a, b) = (1e-3f32, 1e-6f32);
        let mut state = 31u32;
        let mut data = vec![0.0f32; w * h];
        for (i, v) in data.iter_mut().enumerate() {
            // Gentle lighting ramp across the frame so the profile is identifiable, plus sensor noise.
            let mu = 0.22 + 0.12 * (i % w) as f32 / w as f32;
            let sigma = (a * mu + b).sqrt();
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let u1 = ((state >> 8) as f32 / (1u32 << 24) as f32).max(1e-6);
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let u2 = (state >> 8) as f32 / (1u32 << 24) as f32;
            let gauss = (-2.0 * u1.ln()).sqrt() * (core::f32::consts::TAU * u2).cos();
            *v = (mu + gauss * sigma).max(0.0);
        }
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        assert!(noise::estimate(&data, w, h, &cfa).is_some(), "profile not identified");
        let lap = |rgb: &Rgb32f| {
            let (mut sq, mut n) = (0.0f64, 0.0f64);
            for y in 4..h - 4 {
                for x in 4..w - 4 {
                    let g = |x: usize, y: usize| rgb.data[y * w + x][1] as f64;
                    let l = 4.0 * g(x, y) - g(x - 1, y) - g(x + 1, y) - g(x, y - 1) - g(x, y + 1);
                    sq += l * l;
                    n += 1.0;
                }
            }
            (sq / n).sqrt()
        };
        let (plain, refined) = (directional_with(&m, false), directional_with(&m, true));
        assert!(lap(&refined) < lap(&plain) * 0.9, "laplacian {:.5} -> {:.5}", lap(&plain), lap(&refined));
    }

    /// A non-finite sample must not be smeared over its block.
    #[test]
    fn non_finite_samples_do_not_spread() {
        let (w, h) = (24, 24);
        let mut src = vec![0.3f32; w * h];
        src[10 * w + 10] = f32::NAN;
        let out = refine_green(&src, w, h, RefineMode::BlockAc);
        let nan_count = out.iter().filter(|v| v.is_nan()).count();
        assert_eq!(nan_count, 1, "NaN spread to {nan_count} pixels");
    }

    /// Gaussian sample from a small LCG (Box-Muller), deterministic across runs.
    pub(super) fn gauss(state: &mut u32) -> f32 {
        let mut next = || {
            *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((*state >> 8) as f32 + 0.5) / (1u32 << 24) as f32
        };
        let (u, v) = (next(), next());
        (-2.0 * u.ln()).sqrt() * (2.0 * core::f32::consts::PI * v).cos()
    }

    /// Debayer a synthetic scene: `scene(x, y)` gives the exact RGB before the CFA sampling and noise.
    fn shoot(scene: impl Fn(usize, usize) -> [f32; 3], w: usize, h: usize, sigma: f32, seed: u32) -> (Vec<f32>, Cfa) {
        let cfa = Cfa::xtrans();
        let mut state = seed;
        let mut data = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let c = cfa.color_at(x, y) as usize;
                data[y * w + x] = (scene(x, y)[c] + gauss(&mut state) * sigma).max(0.0);
            }
        }
        (data, cfa)
    }

    /// RMS of the Laplacian of one colour difference: the high-frequency colour speckle.
    fn chroma_lap(rgb: &crate::Rgb32f, c: usize) -> f32 {
        let (w, h) = (rgb.width, rgb.height);
        let mut acc = 0.0f64;
        let mut n = 0.0f64;
        for y in 2..h.saturating_sub(2) {
            for x in 2..w.saturating_sub(2) {
                let d = |i: usize| rgb.data[i][c] - rgb.data[i][1];
                let i = y * w + x;
                let lap = 4.0 * d(i) - d(i - 1) - d(i + 1) - d(i - w) - d(i + w);
                acc += (lap * lap) as f64;
                n += 1.0;
            }
        }
        (acc / n).sqrt() as f32
    }

    /// The guided chroma fit is a denoiser: in a flat field it must remove most of the colour speckle
    /// that per-site colour-difference interpolation leaves behind.
    #[test]
    fn guided_chroma_removes_colour_speckle() {
        let (w, h) = (640, 640);
        // A gentle ramp so the noise profile is identifiable (a perfectly uniform frame has none);
        // the scene stays neutral everywhere, so all colour variation in the output is noise.
        let (data, cfa) = shoot(
            |x, _| {
                let k = 2.0 * x as f32 / w as f32;
                [0.30 * k, 0.30 * k, 0.30 * k]
            },
            w,
            h,
            3e-3,
            9,
        );
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        assert!(noise::estimate(&data, w, h, &cfa).is_some(), "flat field must still yield a noise profile");
        let plain = directional_with(&m, false);
        let guided = directional_with(&m, true);
        for c in [0usize, 2] {
            let (a, b) = (chroma_lap(&plain, c), chroma_lap(&guided, c));
            assert!(b < a * 0.5, "channel {c}: chroma laplacian {a:.5} -> {b:.5}");
        }
    }

    #[test]
    fn guided_chroma_preserves_constant_colour_including_borders_and_strips() {
        let (w, h) = (43, CHROMA_STRIP * 2 + 11);
        let colour = [0.4, 0.2, 0.08];
        let (data, cfa) = shoot(|_, _| colour, w, h, 0.0, 1);
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let g = vec![colour[1]; w * h];
        let mut out = Rgb32f::new(w, h);
        out.data.fill(colour);
        let model = NoiseModel { a: 0.0, b: 1e-8 };
        for ch in [0, 2] {
            chroma_guided(&m, &g, &model, &mut out, ch);
        }
        for (i, pixel) in out.data.iter().enumerate() {
            for ch in [0, 2] {
                assert!((pixel[ch] - colour[ch]).abs() < 2e-6, "({},{}) channel {ch}: {:?}", i % w, i / w, pixel);
            }
        }
    }

    /// Strip boundaries must not create a seam: the parallel `CHROMA_STRIP`-row chunks must reproduce
    /// the result of a single full-height call, apart from floating-point reassociation.
    ///
    /// Every output row is a function of the mosaic and the green guide only (each strip builds its `ab`
    /// planes from `[y0 - r, y1 + r]`, and a row's window never leaves that band), so chunking cannot
    /// change the maths. The one difference is the order of the rolling f64 window sum: the first strip
    /// has the same accumulation history as the single call and stays bit-identical, while later strips
    /// restart the ring buffer, which can move the last f32 bit. Measured on this fixture: rows
    /// 0..`CHROMA_STRIP` bit-identical; 2502 of 30525 channel values (147 of 275 rows, all in
    /// strips >= 1) differ by at most 1.9e-9 absolute / 2.0e-7 relative (<= 2 ulp at these values). A real seam —
    /// a strip using coefficients built from the wrong band — is orders of magnitude larger, so the
    /// 1e-6 relative bound below still pins it.
    #[test]
    fn chroma_joint_strips_match_a_single_full_height_strip() {
        let (w, h) = (37, CHROMA_STRIP * 2 + 19);
        let (data, cfa) = shoot(
            |x, y| {
                let base = 0.12 + 0.5 * x as f32 / w as f32 + 0.2 * y as f32 / h as f32;
                [1.1 * base, base, 0.8 * base]
            },
            w,
            h,
            2e-3,
            17,
        );
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let g = green(&m, &Offsets::new(&m));
        let model = NoiseModel { a: 2e-5, b: 1e-7 };

        let mut chunked = Rgb32f::new(w, h);
        chroma_guided_joint(&m, &g, &model, &mut chunked);
        // One strip covering the whole frame: the same code with no chunk boundaries at all.
        let mut single = Rgb32f::new(w, h);
        chroma_strip_joint(&m, &g, &model, &mut single.data[..], w, h, 0);

        for (i, (a, b)) in chunked.data.iter().zip(&single.data).enumerate() {
            let y = i / w;
            for c in 0..3 {
                if y < CHROMA_STRIP {
                    assert_eq!(a[c].to_bits(), b[c].to_bits(), "first strip must be bit-identical: ({},{}) channel {c}: {a:?} != {b:?}", i % w, y);
                }
                let scale = a[c].abs().max(b[c].abs());
                assert!(
                    (a[c] - b[c]).abs() <= 1e-6 * scale,
                    "({},{}) channel {c}: {a:?} != {b:?} (relative {:e})",
                    i % w,
                    y,
                    (a[c] - b[c]).abs() / scale
                );
            }
        }
    }

    /// Straightforward clone-based `remove_isolated` (the pre-optimization implementation): every
    /// interior pixel is clamped to the min/max of its eight neighbours read from the unmodified
    /// image. The strip-parallel version saves only the boundary rows, so it must match this exactly.
    fn remove_isolated_serial(rgb: &Rgb32f) -> Vec<[f32; 3]> {
        let (w, h) = (rgb.width, rgb.height);
        let mut out = rgb.data.clone();
        if w < 3 || h < 3 {
            return out;
        }
        let src = &rgb.data;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let mut lo = [f32::INFINITY; 3];
                let mut hi = [f32::NEG_INFINITY; 3];
                for dy in [-1isize, 0, 1] {
                    for dx in [-1isize, 0, 1] {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let v = src[(y as isize + dy) as usize * w + (x as isize + dx) as usize];
                        for c in 0..3 {
                            lo[c] = lo[c].min(v[c]);
                            hi[c] = hi[c].max(v[c]);
                        }
                    }
                }
                for c in 0..3 {
                    out[y * w + x][c] = out[y * w + x][c].clamp(lo[c], hi[c]);
                }
            }
        }
        out
    }

    #[test]
    fn remove_isolated_strips_match_the_serial_pass() {
        let (w, h) = (41, 200); // crosses the strip boundaries at rows 63/64 and 127/128
        let mut rgb = Rgb32f::new(w, h);
        let mut seed = 0x5eed_1234u32;
        for (i, px) in rgb.data.iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let jitter = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            let base = 0.1 + 0.5 * x as f32 / w as f32 + 0.2 * y as f32 / h as f32;
            *px = [base + 0.01 * jitter, base, 0.9 * base];
        }
        // Impulses on and beside the strip boundaries, plus one in the middle.
        for &(x, y, v) in &[(7usize, 63usize, 0.99f32), (8, 64, 0.0), (33, 64, 0.98), (3, 127, 0.0), (4, 128, 0.99), (37, 128, 0.0), (20, 100, 0.97)]
        {
            rgb.data[y * w + x] = [v; 3];
        }
        let expect = remove_isolated_serial(&rgb);
        remove_isolated(&mut rgb);
        for (i, (a, b)) in rgb.data.iter().zip(&expect).enumerate() {
            for c in 0..3 {
                assert_eq!(a[c].to_bits(), b[c].to_bits(), "({},{}) channel {c}: {a:?} != {b:?}", i % w, i / w);
            }
        }
    }

    /// The feature's contract: an isolated spike above or below all eight neighbours is clamped to
    /// their range, while a one-pixel-wide line (its neighbours on the line carry the same value) and
    /// the border rows/columns are left alone.
    #[test]
    fn remove_isolated_clamps_spikes_and_keeps_one_pixel_lines() {
        let (w, h) = (15, 13);
        let flat = 0.25f32;
        let mut rgb = Rgb32f::new(w, h);
        rgb.data.fill([flat; 3]);
        for x in 2..w - 2 {
            rgb.data[3 * w + x] = [0.6; 3];
        }
        rgb.data[8 * w + 7] = [0.9; 3];
        rgb.data[9 * w + 9] = [0.02; 3];
        remove_isolated(&mut rgb);
        for x in 2..w - 2 {
            assert_eq!(rgb.data[3 * w + x], [0.6; 3], "one-pixel line at x={x} must survive");
        }
        assert_eq!(rgb.data[8 * w + 7], [flat; 3], "bright spike must clamp to its neighbours");
        assert_eq!(rgb.data[9 * w + 9], [flat; 3], "dark dip must clamp to its neighbours");
        for x in 0..w {
            assert_eq!(rgb.data[x], [flat; 3], "top border must not move");
            assert_eq!(rgb.data[(h - 1) * w + x], [flat; 3], "bottom border must not move");
        }
    }

    #[test]
    fn modelled_noise_threshold_also_removes_low_frequency_residue() {
        let model = NoiseModel { a: 0.0, b: 1e-4 };
        for mode in [RefineMode::Constant(0.01), RefineMode::Model(&model)] {
            let mut block = [[0.005; REFINE_BLOCK]; REFINE_BLOCK];
            block[0][0] = 2.0;
            block[0][2] = 0.1;
            soften(&mut block, mode);
            assert_eq!(block[0][0], 2.0);
            assert!((block[0][2] - 0.096).abs() < 1e-6);
            assert_eq!(block[0][1], 0.0, "low-frequency noise survived");
            assert_eq!(block[7][7], 0.0);
        }
    }

    #[test]
    fn modelled_refinement_preserves_strong_texture_across_scales_and_directions() {
        let (w, h) = (128, 128);
        for period in [4.0, 8.0, 16.0] {
            for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
                let mut state = 73;
                let wave = |x: usize, y: usize| (core::f32::consts::TAU * (dx * x + dy * y) as f32 / period).sin();
                let mut truth = Vec::new();
                let mut noisy = Vec::new();
                for y in 0..h {
                    for x in 0..w {
                        let value = 0.3 + 0.06 * wave(x, y);
                        truth.push(value);
                        noisy.push(value + 0.008 * gauss(&mut state));
                    }
                }
                let out = refine_green(&noisy, w, h, RefineMode::Constant(0.008));
                let (mut amplitude, mut reference, mut error, mut input_error) = (0.0, 0.0, 0.0, 0.0);
                for y in 8..h - 8 {
                    for x in 8..w - 8 {
                        let i = y * w + x;
                        amplitude += (out[i] - 0.3) * wave(x, y);
                        reference += (truth[i] - 0.3) * wave(x, y);
                        error += (out[i] - truth[i]).powi(2);
                        input_error += (noisy[i] - truth[i]).powi(2);
                    }
                }
                let contrast = amplitude / reference;
                assert!(contrast > 0.94, "period {period}, direction ({dx},{dy}): contrast {contrast}");
                assert!(error < 0.65 * input_error, "period {period}, direction ({dx},{dy}): error {error} / {input_error}");
            }
        }
    }

    #[test]
    fn modelled_refinement_reduces_error_without_erasing_known_texture() {
        let (w, h) = (128, 128);
        let mut state = 59;
        let mut truth = Vec::new();
        let mut noisy = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = 0.3 + if x >= w / 2 { 0.2 } else { 0.0 } + 0.04 * (core::f32::consts::TAU * y as f32 / 16.0).sin();
                truth.push(v);
                noisy.push(v + 0.008 * gauss(&mut state));
            }
        }
        let out = refine_green(&noisy, w, h, RefineMode::Constant(0.008));
        let error = |v: &[f32]| v.iter().zip(&truth).map(|(a, b)| (a - b).powi(2)).sum::<f32>();
        assert!(error(&out) < 0.65 * error(&noisy), "noise reduced by erasing structure: {} -> {}", error(&noisy), error(&out));
        let amplitude = |v: &[f32]| {
            let mut sum = 0.0;
            for y in 8..h - 8 {
                for x in 8..w / 2 - 8 {
                    sum += (v[y * w + x] - 0.3) * (core::f32::consts::TAU * y as f32 / 16.0).sin();
                }
            }
            sum
        };
        assert!(amplitude(&out) > 0.85 * amplitude(&truth), "known texture lost more than 15% of contrast");
        for y in 8..h - 8 {
            let contrast = out[y * w + w / 2 + 1] - out[y * w + w / 2 - 2];
            assert!(contrast > 0.17, "step softened at row {y}: {contrast}");
        }
    }

    #[test]
    fn guided_chroma_regresses_ratio_on_green_not_the_reverse() {
        let (w, h) = (64, 53);
        let scene = |x: usize, _: usize| {
            let g = 0.1 + 0.006 * x as f32;
            let r = (2.0 * g + 0.3) * (g + RATIO_GUARD) - RATIO_GUARD;
            [r, g, g]
        };
        let (data, cfa) = shoot(scene, w, h, 0.0, 1);
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let mut out = Rgb32f::new(w, h);
        for (i, p) in out.data.iter_mut().enumerate() {
            *p = scene(i % w, i / w);
        }
        let g: Vec<f32> = out.data.iter().map(|p| p[1]).collect();
        let model = NoiseModel { a: 0.0, b: 1e-8 };
        chroma_guided(&m, &g, &model, &mut out, 0);
        // Independent scalar ridge fit, including the second window over the coefficients.
        for (x, y) in [(8, 8), (19, 17), (32, 29), (47, 40)] {
            let (mut sum_a, mut sum_b) = (0.0f64, 0.0f64);
            for cy in y - 3..=y + 3 {
                for cx in x - 3..=x + 3 {
                    let mut samples = Vec::new();
                    for sy in cy - 3..=cy + 3 {
                        for sx in cx - 3..=cx + 3 {
                            if cfa.color_at(sx, sy) == 0 {
                                let p = scene(sx, sy);
                                let guide = f64::from(p[1]);
                                let ratio = (f64::from(p[0]) + f64::from(RATIO_GUARD)) / (guide + f64::from(RATIO_GUARD));
                                samples.push((guide, ratio));
                            }
                        }
                    }
                    let n = samples.len() as f64;
                    let mg = samples.iter().map(|s| s.0).sum::<f64>() / n;
                    let mr = samples.iter().map(|s| s.1).sum::<f64>() / n;
                    let var = samples.iter().map(|s| (s.0 - mg).powi(2)).sum::<f64>() / n;
                    let cov = samples.iter().map(|s| (s.0 - mg) * (s.1 - mr)).sum::<f64>() / n;
                    let a = cov / (var + f64::from(model.sigma(mg as f32)).powi(2) + 0.01 * mg * mg);
                    sum_a += a;
                    sum_b += mr - a * mg;
                }
            }
            let p = scene(x, y);
            let ratio = (sum_a * f64::from(p[1]) + sum_b) / 49.0;
            let chan = ratio * f64::from(p[1] + RATIO_GUARD) - f64::from(RATIO_GUARD);
            let diff = chan - f64::from(p[0]);
            let expected = f64::from(p[0]) + diff / (1.0 + diff.abs() / f64::from(3.0 * model.sigma(p[1])));
            let err = (f64::from(out.data[y * w + x][0]) - expected).abs();
            assert!(err < 2e-7, "ridge fit differs at ({x},{y}): {err}");
        }
    }

    /// The guided fit must suppress the 2-px colour residue that detail at the sensor's Nyquist limit
    /// leaves behind ("maze"). The chroma step runs on its own — same green plane and same starting
    /// interpolation either side — so the number belongs to the fit, not to the green refinement.
    #[test]
    fn guided_chroma_removes_the_two_pixel_colour_residue() {
        let (w, h) = (640, 640);
        // The checkerboard sits in a band, with gentle ramps either side: the noise profile is estimated
        // from the whole frame and needs blocks that the structure does not dominate.
        let (band0, band1) = (256, 384);
        let base = [0.44, 0.34, 0.24];
        let (data, cfa) = shoot(
            |x, y| {
                // One-pixel luminance checkerboard at a constant colour ratio: a neutral pattern for the
                // demosaic, so every 2-px colour variation in the band is residue.
                let k = 0.55 + 0.5 * y as f32 / h as f32 * if (band0..band1).contains(&y) && (x + y) % 2 == 0 { 1.20 } else { 1.0 };
                [base[0] * k, base[1] * k, base[2] * k]
            },
            w,
            h,
            1.5e-3,
            33,
        );
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let Some(model) = noise::estimate(&data, w, h, &cfa) else {
            panic!("the ramps must still yield a noise profile");
        };
        let plain = directional_with(&m, false);
        let g: Vec<f32> = plain.data.iter().map(|p| p[1]).collect();
        let mut guided = plain.clone();
        for c in [0usize, 2] {
            chroma_guided(&m, &g, &model, &mut guided, c);
        }
        // Residue over the band only: the Laplacian of the colour difference, which is where a 2-px
        // checkerboard residue sits.
        let residue = |rgb: &crate::Rgb32f, c: usize| {
            let mut acc = 0.0f64;
            let mut n = 0.0f64;
            for y in band0 + 4..band1 - 4 {
                for x in 4..w - 4 {
                    let d = |i: usize| rgb.data[i][c] - rgb.data[i][1];
                    let i = y * w + x;
                    let lap = 4.0 * d(i) - d(i - 1) - d(i + 1) - d(i - w) - d(i + w);
                    acc += (lap * lap) as f64;
                    n += 1.0;
                }
            }
            (acc / n).sqrt() as f32
        };
        for c in [0usize, 2] {
            let (a, b) = (residue(&plain, c), residue(&guided, c));
            assert!(b < a * 0.8, "channel {c}: colour residue {a:.5} -> {b:.5}");
        }
    }

    /// A level ratio divides by green, so a dark *and coloured* patch inside a bright field is the case
    /// that can blow the fit up: the ratio there is huge, and a window straddling both levels would turn
    /// that into a large intercept on the bright side. The estimate must stay bounded, close to the
    /// interpolation already in `out`.
    #[test]
    fn guided_chroma_survives_a_near_black_green_site() {
        let (w, h) = (640, 640);
        // A thin dark *and coloured* band in a bright field: green at black next to a live red and blue
        // is what makes the ratio explode, and a thin band keeps whole windows straddling both levels.
        let (dark0, dark1) = (300, 308);
        let base = [0.44, 0.34, 0.24];
        let (data, cfa) = shoot(
            |_x, y| {
                if (dark0..dark1).contains(&y) {
                    return [0.035, 0.0008, 0.030];
                }
                let k = 0.35 + 0.4 * y as f32 / h as f32;
                [base[0] * k, base[1] * k, base[2] * k]
            },
            w,
            h,
            1.5e-3,
            41,
        );
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let Some(model) = noise::estimate(&data, w, h, &cfa) else {
            panic!("the bright bands must still yield a noise profile");
        };
        let plain = directional_with(&m, false);
        let g: Vec<f32> = plain.data.iter().map(|p| p[1]).collect();
        let mut guided = plain.clone();
        for c in [0usize, 2] {
            chroma_guided(&m, &g, &model, &mut guided, c);
        }
        let peak = |rgb: &crate::Rgb32f| rgb.data.iter().fold(0.0f32, |m, p| m.max(p[0].max(p[2])));
        let (a, b) = (peak(&plain), peak(&guided));
        assert!(b <= a * 1.5 + 0.05, "channel peak {a:.4} -> {b:.4}: a black green site dragged the fit");
    }

    /// ...but not a bleed: across a colour boundary the guided output must not push the colour further
    /// from the truth than the colour-difference interpolation already does (no green/magenta fringe).
    #[test]
    fn guided_chroma_does_not_fringe_at_a_colour_edge() {
        let (w, h) = (320, 64);
        let warm = [0.50, 0.25, 0.12];
        let cool = [0.12, 0.25, 0.50];
        let (data, cfa) = shoot(
            |x, y| {
                let k = 0.7 + 0.6 * y as f32 / h as f32;
                let c = if x < w / 2 { warm } else { cool };
                [c[0] * k, c[1] * k, c[2] * k]
            },
            w,
            h,
            2e-3,
            21,
        );
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let plain = directional_with(&m, false);
        let guided = directional_with(&m, true);
        let err = |rgb: &crate::Rgb32f| {
            let mut worst = 0.0f32;
            for y in 8..h - 8 {
                for x in w / 2 - 6..w / 2 + 6 {
                    let base = if x < w / 2 { warm } else { cool };
                    let k = 0.7 + 0.6 * y as f32 / h as f32;
                    let truth = [base[0] * k, base[1] * k, base[2] * k];
                    for c in [0usize, 2] {
                        let got = rgb.data[y * w + x][c] - rgb.data[y * w + x][1];
                        worst = worst.max((got - (truth[c] - truth[1])).abs());
                    }
                }
            }
            worst
        };
        let (a, b) = (err(&plain), err(&guided));
        assert!(b <= a + 0.02, "edge chroma error {a:.4} -> {b:.4}");
    }

    /// Detail at the sensor's Nyquist limit is the worst case for the green refinement: the interpolated
    /// plane is a 1-px luminance checkerboard, and the stabilised domain must not amplify it. A frame with
    /// no shot noise fits `a = 0`, and the transform's read term then swamps `f32`; before the model was
    /// checked for that, the inverse brought back a green plane ranging 0…0.82 for a scene that never
    /// exceeds 0.36 — the checkerboard came back 2.3× over its own maximum.
    #[test]
    fn nyquist_checker_does_not_overshoot_its_own_range() {
        let (w, h) = (512, 512);
        let level = |y: usize| 0.06 + 0.30 * y as f32 / h as f32;
        let (data, cfa) = shoot(
            |x, y| {
                let k = if (x + y) % 2 == 0 { 1.0 } else { 0.5 };
                let l = level(y) * k;
                [l, l, l]
            },
            w,
            h,
            0.0,
            7,
        );
        let m = Mosaic { w, h, data: &data, cfa: &cfa };
        let top = level(h - 1);
        for refine in [false, true] {
            let rgb = directional_with(&m, refine);
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for y in 8..h - 8 {
                for x in 8..w - 8 {
                    let v = rgb.data[y * w + x][1];
                    assert!(v.is_finite(), "green is not finite at ({x}, {y})");
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            assert!(lo >= 0.0, "refine={refine}: green goes below black ({lo:.4})");
            assert!(hi <= top * 1.05, "refine={refine}: green reaches {hi:.4} for a scene topping at {top:.4}");
        }
    }

    /// Ground-truth synthetic CFA evaluation required for XTR-T1:
    /// Measures contrast retention and error before/after DCT refinement across:
    /// 1. High-frequency crosshatch fabric pattern
    /// 2. Intermediate diagonal stripes (45 deg)
    /// 3. Step edge sharpness & ringing bounds
    /// 4. Flat noise reduction
    ///
    /// Baseline only: this does not exercise a re-injection or guided reconstruction candidate.
    /// Conclusions about those alternatives require a separate reproducible A/B experiment.
    #[test]
    fn synthetic_cfa_ground_truth_benchmark_xtr_t1() {
        let (w, h) = (128, 128);
        let cfa = Cfa::xtrans();

        // 1. Crosshatch fabric (period 4px)
        let fabric_scene = |x: usize, y: usize| {
            let wx = (core::f32::consts::TAU * x as f32 / 4.0).cos();
            let wy = (core::f32::consts::TAU * y as f32 / 4.0).cos();
            let val = 0.35 + 0.05 * wx * wy;
            [val, val, val]
        };
        let (data_fabric, _) = shoot(fabric_scene, w, h, 0.003, 11);
        let m_fabric = Mosaic { w, h, data: &data_fabric, cfa: &cfa };
        let off = Offsets::new(&m_fabric);

        let g_raw = green(&m_fabric, &off);
        let g_dct = refine_green(&g_raw, w, h, RefineMode::Constant(0.003));

        // Evaluate contrast and MSE on fabric
        let mut ref_amp = 0.0f64;
        let mut raw_amp = 0.0f64;
        let mut dct_amp = 0.0f64;
        let mut raw_mse = 0.0f64;
        let mut dct_mse = 0.0f64;
        let mut count = 0.0f64;

        for y in 16..h - 16 {
            for x in 16..w - 16 {
                let target = fabric_scene(x, y)[1];
                let carrier = (core::f32::consts::TAU * x as f32 / 4.0).cos() * (core::f32::consts::TAU * y as f32 / 4.0).cos();
                let i = y * w + x;

                ref_amp += ((target - 0.35) * carrier) as f64;
                raw_amp += ((g_raw[i] - 0.35) * carrier) as f64;
                dct_amp += ((g_dct[i] - 0.35) * carrier) as f64;
                raw_mse += (g_raw[i] - target).powi(2) as f64;
                dct_mse += (g_dct[i] - target).powi(2) as f64;
                count += 1.0;
            }
        }
        let _raw_contrast = (raw_amp / ref_amp) as f32;
        let dct_contrast = (dct_amp / ref_amp) as f32;
        let raw_rmse = (raw_mse / count).sqrt() as f32;
        let dct_rmse = (dct_mse / count).sqrt() as f32;

        assert!(dct_contrast > 0.50, "DCT fabric contrast {dct_contrast} must be > 0.50");
        assert!(dct_rmse < raw_rmse, "DCT RMSE {dct_rmse} must improve on raw {raw_rmse}");

        // 2. Diagonal stripes (period 5px, 45 deg)
        let diag_scene = |x: usize, y: usize| {
            let val = 0.35 + 0.05 * (core::f32::consts::TAU * (x + y) as f32 / 5.0).sin();
            [val, val, val]
        };
        let (data_diag, _) = shoot(diag_scene, w, h, 0.003, 23);
        let m_diag = Mosaic { w, h, data: &data_diag, cfa: &cfa };
        let g_raw_diag = green(&m_diag, &off);
        let g_dct_diag = refine_green(&g_raw_diag, w, h, RefineMode::Constant(0.003));

        let mut diag_ref_amp = 0.0f64;
        let mut diag_dct_amp = 0.0f64;
        for y in 16..h - 16 {
            for x in 16..w - 16 {
                let target = diag_scene(x, y)[1];
                let carrier = (core::f32::consts::TAU * (x + y) as f32 / 5.0).sin();
                let i = y * w + x;
                diag_ref_amp += ((target - 0.35) * carrier) as f64;
                diag_dct_amp += ((g_dct_diag[i] - 0.35) * carrier) as f64;
            }
        }
        let diag_dct_contrast = (diag_dct_amp / diag_ref_amp) as f32;
        assert!(diag_dct_contrast > 0.55, "DCT diagonal contrast {diag_dct_contrast} must be > 0.55");

        // 3. Flat field test: DCT must suppress high-frequency noise by at least 3x
        let flat_scene = |_x: usize, _y: usize| [0.35f32, 0.35, 0.35];
        let (data_flat, _) = shoot(flat_scene, w, h, 0.003, 37);
        let m_flat = Mosaic { w, h, data: &data_flat, cfa: &cfa };
        let g_raw_flat = green(&m_flat, &off);
        let g_dct_flat = refine_green(&g_raw_flat, w, h, RefineMode::Constant(0.003));

        let std_dev = |v: &[f32]| {
            let mut acc = 0.0f64;
            let mut n = 0.0f64;
            for y in 16..h - 16 {
                for x in 16..w - 16 {
                    let d = v[y * w + x] - 0.35;
                    acc += (d * d) as f64;
                    n += 1.0;
                }
            }
            (acc / n).sqrt() as f32
        };
        let flat_raw_noise = std_dev(&g_raw_flat);
        let flat_dct_noise = std_dev(&g_dct_flat);
        assert!(flat_dct_noise < flat_raw_noise * 0.35, "DCT must suppress flat noise: {flat_dct_noise} vs {flat_raw_noise}");

        // 4. Step edge overshoot / undershoot check
        let step_scene = |x: usize, _y: usize| {
            let v = if x < w / 2 { 0.20f32 } else { 0.60f32 };
            [v, v, v]
        };
        let (data_step, _) = shoot(step_scene, w, h, 0.0, 43);
        let m_step = Mosaic { w, h, data: &data_step, cfa: &cfa };
        let g_raw_step = green(&m_step, &off);
        let g_dct_step = refine_green(&g_raw_step, w, h, RefineMode::Constant(0.001));

        let (mut min_val, mut max_val) = (f32::INFINITY, f32::NEG_INFINITY);
        for y in 16..h - 16 {
            for x in 16..w - 16 {
                let v = g_dct_step[y * w + x];
                min_val = min_val.min(v);
                max_val = max_val.max(v);
            }
        }
        assert!(min_val >= 0.19 && max_val <= 0.61, "Step edge had ringing: [{min_val}, {max_val}]");

        // 5. Peak harmonic DCT retention test
        let mut b_test: Block8 = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
        b_test[0][0] = 0.35 * 8.0;
        b_test[2][2] = 0.025; // fabric carrier harmonic
        let mut b_noisy = b_test;
        let mut state = 91;
        for i in 0..8 {
            for j in 0..8 {
                if (i | j) != 0 {
                    b_noisy[i][j] += 0.003 * gauss(&mut state);
                }
            }
        }
        let mut b_softened = b_noisy;
        soften(&mut b_softened, RefineMode::Constant(0.003));
        assert!(b_softened[2][2] > 0.015, "Peak harmonic retention must exceed 60%: got {}", b_softened[2][2]);
    }
}
