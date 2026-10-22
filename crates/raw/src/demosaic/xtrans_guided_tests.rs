//! XTR-T1 candidates G1/G2. Test-only, with known sensor noise rather than a fitted noise floor.
//! G2 is a masked spectral reconstruction, specified alongside `sparse_cfa_green` below.
//! G1:
//! Fit the residual (measured CFA green minus DCT green) against DCT green in a 5x5 window.
//! With means G/R, guide variance V and covariance C:
//! slope = clamp(C / (V + sigma(G)^2 + 1e-12), -0.5, 1);
//! gate = max(V - sigma(G)^2, 0) / (V + sigma(G)^2 + 1e-12);
//! output = DCT + strength * gate * (R + slope * (DCT - G)).
//! Only actual green observations enter the fit; both CFA site groups receive the prediction.
//! This is an original hypothesis, not a recovered converter algorithm or calibrated SNR gate.

use super::tests::gauss;
use super::*;
use std::f32::consts::TAU;

const W: usize = 96;
const H: usize = 96;
const BORDER: usize = 16;

#[derive(Clone, Copy)]
enum Candidate {
    G1,
    G2,
}

fn sparse_cfa_step(block: &mut Block8, observed: &Block8, mask: &[[bool; REFINE_BLOCK]; REFINE_BLOCK], lambda: f32, matrix: &Block8) {
    for (dy, row) in block.iter_mut().enumerate() {
        for (dx, value) in row.iter_mut().enumerate() {
            if mask[dy][dx] {
                *value = observed[dy][dx];
            }
        }
    }
    dct8x8(block, matrix);
    for (dy, row) in block.iter_mut().enumerate() {
        for (dx, value) in row.iter_mut().enumerate() {
            if (dx | dy) != 0 {
                *value = value.signum() * (value.abs() - lambda).max(0.0);
            }
        }
    }
    idct8x8(block, matrix);
}

/// G2 targets 0.5*||M*IDCT(c)-y||^2 + lambda*sum(|c_AC|) per 8x8 block.
/// M contains only measured CFA green sites. Unit-step proximal gradient: replace observed
/// sites, DCT, soft-threshold AC, inverse DCT; repeat 32 times, then average overlapping blocks.
/// Lambda is 0.75 or 1.5 sensor sigmas (one in stabilized space). DC is never thresholded.
fn sparse_cfa_green(m: &Mosaic, initial: &[f32], lambda: f32) -> Vec<f32> {
    let mut out = initial.to_vec();
    let mut sum = vec![0.0; initial.len()];
    let mut count = vec![0u32; initial.len()];
    let matrix = dct_matrix();
    if m.w < REFINE_BLOCK || m.h < REFINE_BLOCK {
        return out;
    }
    for by in (0..=m.h - REFINE_BLOCK).step_by(REFINE_STRIDE) {
        for bx in (0..=m.w - REFINE_BLOCK).step_by(REFINE_STRIDE) {
            let mut block = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
            let mut observed = block;
            let mut mask = [[false; REFINE_BLOCK]; REFINE_BLOCK];
            let (mut dc, mut n) = (0.0f64, 0u32);
            for (dy, row) in observed.iter_mut().enumerate() {
                for (dx, value) in row.iter_mut().enumerate() {
                    if m.cfa.color_at(bx + dx, by + dy) == 1 {
                        *value = m.data[(by + dy) * m.w + bx + dx];
                        mask[dy][dx] = true;
                        dc += f64::from(*value);
                        n += 1;
                    }
                }
            }
            assert!(n > 0);
            // Remove DC before repeated f32 transforms; otherwise even a constant drifts.
            let dc = (dc / f64::from(n)) as f32;
            for (dy, row) in block.iter_mut().enumerate() {
                for (dx, value) in row.iter_mut().enumerate() {
                    *value = initial[(by + dy) * m.w + bx + dx] - dc;
                    observed[dy][dx] -= dc;
                }
            }
            for _ in 0..32 {
                sparse_cfa_step(&mut block, &observed, &mask, lambda, &matrix);
            }
            for (dy, row) in block.iter().enumerate() {
                for (dx, &value) in row.iter().enumerate() {
                    let i = (by + dy) * m.w + bx + dx;
                    sum[i] += value + dc;
                    count[i] += 1;
                }
            }
        }
    }
    for ((value, total), n) in out.iter_mut().zip(sum).zip(count) {
        if n > 0 {
            *value = total / n as f32;
        }
    }
    out
}

fn guided_residual(m: &Mosaic, guide: &[f32], model: NoiseModel, strength: f32) -> Vec<f32> {
    let mut out = guide.to_vec();
    for y in 0..m.h {
        for x in 0..m.w {
            let mut moments = [0.0f64; 5];
            for yy in y.saturating_sub(2)..(y + 3).min(m.h) {
                for xx in x.saturating_sub(2)..(x + 3).min(m.w) {
                    if m.cfa.color_at(xx, yy) != 1 {
                        continue;
                    }
                    let i = yy * m.w + xx;
                    let g = f64::from(guide[i]);
                    let residual = f64::from(m.data[i]) - g;
                    for (sum, value) in moments.iter_mut().zip([1.0, g, residual, g * g, g * residual]) {
                        *sum += value;
                    }
                }
            }
            let [n, sg, sr, sgg, sgr] = moments;
            if n < 3.0 {
                continue;
            }
            let (g, r) = (sg / n, sr / n);
            let variance = (sgg / n - g * g).max(0.0);
            let noise = f64::from(model.sigma(g as f32)).powi(2);
            let denom = variance + noise + 1e-12;
            let slope = ((sgr / n - g * r) / denom).clamp(-0.5, 1.0);
            let gate = (variance - noise).max(0.0) / denom;
            let i = y * m.w + x;
            out[i] += (f64::from(strength) * gate * (r + slope * (f64::from(guide[i]) - g))) as f32;
        }
    }
    out
}

#[derive(Clone, Copy, Debug)]
enum Scene {
    Stripe,
    Weave,
    Flat,
    Edge,
    ColourEdge,
}

fn scene(kind: Scene, period: f32, angle: f32, amplitude: f32, phase: f32, x: usize, y: usize) -> [f32; 3] {
    let (sin, cos) = angle.to_radians().sin_cos();
    let u = (x as f32 - W as f32 / 2.0) * cos + (y as f32 - H as f32 / 2.0) * sin;
    let v = -(x as f32) * sin + y as f32 * cos;
    let carrier = match kind {
        Scene::Stripe => (TAU * u / period + phase).cos(),
        Scene::Weave => (TAU * u / period + phase).cos() * (TAU * v / period + phase * 0.7).cos(),
        Scene::Flat => 0.0,
        Scene::Edge | Scene::ColourEdge => {
            if u < 0.0 {
                -1.0
            } else {
                1.0
            }
        }
    };
    let g = 0.35 + amplitude * carrier;
    if matches!(kind, Scene::ColourEdge) { [0.35 - amplitude * carrier, g, 0.35 + 0.5 * amplitude * carrier] } else { [g; 3] }
}

#[derive(Default)]
struct Metrics {
    count: [usize; 2],
    error: [f64; 2],
    contrast: [f64; 2],
    reference: [f64; 2],
    chroma: f64,
    min: f32,
    max: f32,
}

fn measure(rgb: &Rgb32f, truth: &[[f32; 3]], cfa: &Cfa) -> Metrics {
    let mut result = Metrics { min: f32::INFINITY, max: f32::NEG_INFINITY, ..Metrics::default() };
    // Centre per CFA group so DC bias cannot masquerade as signal gain.
    let mut means = [[0.0f64; 2]; 2];
    for y in BORDER..H - BORDER {
        for x in BORDER..W - BORDER {
            let i = y * W + x;
            let group = usize::from(cfa.color_at(x, y) != 1);
            result.count[group] += 1;
            means[group][0] += f64::from(truth[i][1]);
            means[group][1] += f64::from(rgb.data[i][1]);
        }
    }
    for (group, means) in means.iter_mut().enumerate() {
        for mean in means {
            *mean /= result.count[group] as f64;
        }
    }
    for y in BORDER..H - BORDER {
        for x in BORDER..W - BORDER {
            let i = y * W + x;
            let group = usize::from(cfa.color_at(x, y) != 1);
            let (p, t) = (rgb.data[i], truth[i]);
            assert!(p.iter().all(|v| v.is_finite()));
            let signal = f64::from(t[1]) - means[group][0];
            result.reference[group] += signal * signal;
            result.contrast[group] += (f64::from(p[1]) - means[group][1]) * signal;
            result.error[group] += f64::from(p[1] - t[1]).powi(2);
            for c in [0, 2] {
                result.chroma += f64::from((p[c] - p[1]) - (t[c] - t[1])).powi(2);
            }
            result.min = result.min.min(p[1]);
            result.max = result.max.max(p[1]);
        }
    }
    result
}

impl Metrics {
    fn rmse(&self, group: usize) -> f64 {
        (self.error[group] / self.count[group] as f64).sqrt()
    }

    fn contrast(&self, group: usize) -> f64 {
        if self.reference[group] > 1e-12 { self.contrast[group] / self.reference[group] } else { 0.0 }
    }

    fn chroma_rmse(&self) -> f64 {
        (self.chroma / (2 * self.count.iter().sum::<usize>()) as f64).sqrt()
    }
}

fn trial(kind: Scene, period: f32, angle: f32, amplitude: f32, sigma: f32, seed: u32) -> [Metrics; 3] {
    trial_with(kind, period, angle, amplitude, sigma, seed, Candidate::G1)
}

fn trial_with(kind: Scene, period: f32, angle: f32, amplitude: f32, sigma: f32, seed: u32, candidate: Candidate) -> [Metrics; 3] {
    let cfa = Cfa::xtrans();
    let model = NoiseModel { a: sigma * sigma / 0.36, b: sigma * sigma / 36.0 };
    let mut state = seed;
    let phase = (seed % 17) as f32 * 0.19;
    let truth: Vec<_> = (0..W * H).map(|i| scene(kind, period, angle, amplitude, phase, i % W, i / W)).collect();
    let data: Vec<_> = truth
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let value = p[cfa.color_at(i % W, i / W) as usize];
            let noise = if sigma == 0.0 { 0.0 } else { model.sigma(value) * gauss(&mut state) };
            value + noise
        })
        .collect();
    let m = Mosaic { w: W, h: H, data: &data, cfa: &cfa };
    let off = Offsets::new(&m);
    let guide = if sigma == 0.0 {
        refine_green(&green(&m, &off), W, H, RefineMode::Constant(0.0))
    } else {
        let stabilized: Vec<_> = data.iter().map(|&v| model.anscombe(v)).collect();
        let sm = Mosaic { w: W, h: H, data: &stabilized, cfa: &cfa };
        refine_green(&green(&sm, &off), W, H, RefineMode::Constant(1.0)).into_iter().map(|v| model.anscombe_inverse(v)).collect()
    };
    let parameters = match candidate {
        Candidate::G1 => [0.0, 0.5, 1.0],
        Candidate::G2 => [0.0, 0.75, 1.5],
    };
    parameters.map(|parameter| {
        let g = if parameter == 0.0 {
            guide.clone()
        } else {
            match candidate {
                Candidate::G1 => guided_residual(&m, &guide, model, parameter),
                Candidate::G2 if sigma == 0.0 => sparse_cfa_green(&m, &guide, 0.0),
                Candidate::G2 => {
                    let stabilized: Vec<_> = data.iter().map(|&v| model.anscombe(v)).collect();
                    let initial: Vec<_> = guide.iter().map(|&v| model.anscombe(v)).collect();
                    let sm = Mosaic { data: &stabilized, ..m };
                    sparse_cfa_green(&sm, &initial, parameter).into_iter().map(|v| model.anscombe_inverse(v)).collect()
                }
            }
        };
        let rgb = reconstruct_rgb(&m, &off, &g, true, (sigma > 0.0).then_some(&model));
        measure(&rgb, &truth, &cfa)
    })
}

#[test]
fn guided_green_constant_and_zero_strength_are_identity() {
    let cfa = Cfa::xtrans();
    let data = vec![0.35; W * H];
    let m = Mosaic { w: W, h: H, data: &data, cfa: &cfa };
    let model = NoiseModel { a: 1e-4, b: 1e-6 };
    assert_eq!(guided_residual(&m, &data, model, 1.0), data);
    let guide: Vec<_> = (0..W * H).map(|i| 0.3 + 0.05 * (i as f32 * 0.3).sin()).collect();
    assert_eq!(guided_residual(&m, &guide, model, 0.0), guide);
    let other_colours: Vec<_> = data.iter().enumerate().map(|(i, &g)| if cfa.color_at(i % W, i / W) == 1 { g } else { 0.9 }).collect();
    let changed = Mosaic { data: &other_colours, ..m };
    assert_eq!(guided_residual(&m, &guide, model, 1.0), guided_residual(&changed, &guide, model, 1.0));
}

#[test]
fn guided_green_candidate_improves_coarse_weave_but_amplifies_alias_and_edge_extremes() {
    let coarse = trial(Scene::Weave, 10.0, 30.0, 0.02, 0.003, 11);
    for candidate in &coarse[1..] {
        for group in 0..2 {
            assert!(candidate.rmse(group) < coarse[0].rmse(group) * 0.95);
            assert!((candidate.contrast(group) - 1.0).abs() < (coarse[0].contrast(group) - 1.0).abs());
        }
        assert!(candidate.chroma_rmse() < coarse[0].chroma_rmse());
    }
    let fine = trial(Scene::Weave, 4.0, 0.0, 0.06, 0.012, 91);
    assert!(fine[2].rmse(0) < fine[0].rmse(0) * 0.95);
    assert!(fine[2].rmse(1) > fine[0].rmse(1) * 1.02);
    assert!(fine[2].chroma_rmse() > fine[0].chroma_rmse() * 1.05);
    assert!(fine[0].contrast(1) < 0.0);
    assert!(fine[2].contrast(1) < fine[0].contrast(1));

    let edge = trial(Scene::Edge, 1.0, 45.0, 0.15, 0.012, 91);
    for candidate in &edge[1..] {
        assert!(candidate.max > edge[0].max + 0.001);
    }
    let flat = trial(Scene::Flat, 1.0, 0.0, 0.15, 0.012, 91);
    for candidate in &flat[1..] {
        for group in 0..2 {
            assert!((candidate.rmse(group) - flat[0].rmse(group)).abs() < 1e-7);
        }
    }
}

#[test]
fn sparse_cfa_green_preserves_constants_and_uses_actual_cfa_phases() {
    let (w, h) = (19, 17);
    for dy in 0..6 {
        for dx in 0..6 {
            let cfa = Cfa::xtrans().shifted(dx, dy);
            let data: Vec<_> = (0..w * h).map(|i| if cfa.color_at(i % w, i / w) == 1 { 0.35 } else { 0.9 }).collect();
            let initial = vec![0.35; w * h];
            let m = Mosaic { w, h, data: &data, cfa: &cfa };
            for lambda in [0.0, 0.003] {
                let out = sparse_cfa_green(&m, &initial, lambda);
                assert!(out.iter().all(|v| (*v - 0.35).abs() < 1e-7), "phase {dx},{dy}, lambda {lambda}");
            }
            let tiny = Mosaic { w: 3, h: 2, data: &data[..6], cfa: &cfa };
            assert_eq!(sparse_cfa_green(&tiny, &initial[..6], 0.003), initial[..6]);
        }
    }
}

#[test]
fn sparse_cfa_green_proximal_steps_reduce_the_stated_objective() {
    let matrix = dct_matrix();
    let lambda = 0.003;
    for dy in 0..6 {
        for dx in 0..6 {
            let cfa = Cfa::xtrans().shifted(dx, dy);
            let mut observed = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
            let mut mask = [[false; REFINE_BLOCK]; REFINE_BLOCK];
            for (y, row) in observed.iter_mut().enumerate() {
                for (x, value) in row.iter_mut().enumerate() {
                    *value = 0.02 * (TAU * x as f32 / 4.0).cos() * (TAU * y as f32 / 4.0).cos();
                    mask[y][x] = cfa.color_at(x, y) == 1;
                }
            }
            let objective = |block: &Block8| {
                let mut loss = 0.0f64;
                for y in 0..REFINE_BLOCK {
                    for x in 0..REFINE_BLOCK {
                        if mask[y][x] {
                            loss += 0.5 * f64::from(block[y][x] - observed[y][x]).powi(2);
                        }
                    }
                }
                let mut coefficients = *block;
                dct8x8(&mut coefficients, &matrix);
                for (y, row) in coefficients.iter().enumerate() {
                    for (x, &value) in row.iter().enumerate() {
                        if (x | y) != 0 {
                            loss += f64::from(lambda * value.abs());
                        }
                    }
                }
                loss
            };
            let mut block = [[0.0; REFINE_BLOCK]; REFINE_BLOCK];
            let initial = objective(&block);
            let mut previous = initial;
            for _ in 0..32 {
                sparse_cfa_step(&mut block, &observed, &mask, lambda, &matrix);
                let loss = objective(&block);
                assert!(loss <= previous + 1e-9, "phase {dx},{dy}: {previous} -> {loss}");
                previous = loss;
            }
            assert!(previous < initial * 0.8);
        }
    }
}

#[test]
fn sparse_cfa_green_recovers_alias_but_regresses_diagonals_and_noise() {
    let fine = trial_with(Scene::Weave, 4.0, 0.0, 0.06, 0.012, 91, Candidate::G2);
    for candidate in &fine[1..] {
        assert!(candidate.rmse(0) < fine[0].rmse(0) * 0.7);
        assert!(candidate.rmse(1) < fine[0].rmse(1) * 0.3);
        assert!(fine[0].contrast(1) < 0.0 && candidate.contrast(1) > 0.75);
        assert!(candidate.chroma_rmse() < fine[0].chroma_rmse() * 0.4);
    }
    let diagonal = trial_with(Scene::Stripe, 6.0, 30.0, 0.02, 0.003, 11, Candidate::G2);
    for candidate in &diagonal[1..] {
        assert!(candidate.rmse(1) > diagonal[0].rmse(1) * 1.5);
        assert!(candidate.chroma_rmse() > diagonal[0].chroma_rmse() * 1.5);
    }
    let flat = trial_with(Scene::Flat, 1.0, 0.0, 0.15, 0.003, 11, Candidate::G2);
    assert!(flat[1].rmse(0) > flat[0].rmse(0) * 2.0);
    assert!(flat[1].rmse(1) > flat[0].rmse(1) * 1.5);
    assert!(flat[2].rmse(0) < flat[0].rmse(0) * 1.05);
    let edge = trial_with(Scene::Edge, 1.0, 75.0, 0.15, 0.003, 91, Candidate::G2);
    assert!(edge[1].max > edge[0].max + 0.03);
}

/// Qualification gate: <=5% error/false-colour/flat-noise cost in every case and both CFA groups;
/// at least 5% RMSE improvement and closer-to-one contrast on half the noisy texture cases.
/// A failing candidate stays test-only. Thresholds are fixed before inspecting the campaign.
#[test]
#[ignore = "XTR-T1 reproducible multi-seed research campaign; run explicitly with --ignored --nocapture"]
fn guided_green_candidate_campaign() {
    assert_eq!(campaign(Candidate::G1), [false, false], "G1 decision changed; inspect and update evidence");
}

#[test]
#[ignore = "XTR-T1/G2 research campaign; run explicitly with --ignored --nocapture"]
fn sparse_cfa_green_candidate_campaign() {
    assert_eq!(campaign(Candidate::G2), [false, false], "G2 decision changed; inspect and update evidence");
}

fn campaign(candidate: Candidate) -> [bool; 2] {
    let (tag, parameters) = match candidate {
        Candidate::G1 => ("XTR_T1", [0.0, 0.5, 1.0]),
        Candidate::G2 => ("XTR_T1_G2", [0.0, 0.75, 1.5]),
    };
    let mut cases = 0;
    let mut rejected = [0usize; 2];
    let mut improved = [0usize; 2];
    let mut textures = 0;
    println!(
        "{tag}_HEADER,scene,period,angle,amplitude,sigma,seed,parameter,green_rmse,other_rmse,green_contrast,other_contrast,chroma_rmse,min,max"
    );
    for sigma in [0.0, 0.003, 0.012] {
        for seed in [11, 37, 91] {
            for kind in [Scene::Stripe, Scene::Weave, Scene::Flat, Scene::Edge, Scene::ColourEdge] {
                let periods: &[f32] = if matches!(kind, Scene::Stripe | Scene::Weave) { &[4.0, 6.0, 10.0] } else { &[1.0] };
                let amplitudes: &[f32] = if matches!(kind, Scene::Stripe | Scene::Weave) { &[0.02, 0.06] } else { &[0.15] };
                for &period in periods {
                    for &angle in if matches!(kind, Scene::Flat) { &[0.0][..] } else { &[0.0, 30.0, 45.0, 75.0][..] } {
                        for &amplitude in amplitudes {
                            let metrics = trial_with(kind, period, angle, amplitude, sigma, seed, candidate);
                            let base = &metrics[0];
                            let texture = matches!(kind, Scene::Stripe | Scene::Weave) && sigma > 0.0;
                            textures += usize::from(texture);
                            cases += 1;
                            for (v, m) in metrics.iter().enumerate() {
                                println!(
                                    "{tag},{kind:?},{period},{angle},{amplitude},{sigma},{seed},{:.2},{:.9},{:.9},{:.6},{:.6},{:.9},{:.6},{:.6}",
                                    parameters[v],
                                    m.rmse(0),
                                    m.rmse(1),
                                    m.contrast(0),
                                    m.contrast(1),
                                    m.chroma_rmse(),
                                    m.min,
                                    m.max
                                );
                                if v == 0 {
                                    continue;
                                }
                                let edge_safe =
                                    !matches!(kind, Scene::Edge | Scene::ColourEdge) || (m.min >= base.min - 0.001 && m.max <= base.max + 0.001);
                                let safe = (0..2).all(|g| m.rmse(g) <= base.rmse(g) * 1.05 + 1e-6)
                                    && m.chroma_rmse() <= base.chroma_rmse() * 1.05 + 1e-6
                                    && edge_safe;
                                rejected[v - 1] += usize::from(!safe);
                                let gain =
                                    (0..2).all(|g| m.rmse(g) < base.rmse(g) * 0.95 && (m.contrast(g) - 1.0).abs() < (base.contrast(g) - 1.0).abs());
                                improved[v - 1] += usize::from(texture && gain);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut decisions = [false; 2];
    for v in 0..2 {
        let accepted = rejected[v] == 0 && improved[v] * 2 >= textures;
        decisions[v] = accepted;
        println!(
            "{tag}_SUMMARY,parameter={},cases={cases},budget_failures={},texture_gains={}/{textures},accepted={accepted}",
            parameters[v + 1],
            rejected[v],
            improved[v]
        );
    }
    assert_eq!(cases, 513);
    assert_eq!(textures, 288);
    decisions
}
