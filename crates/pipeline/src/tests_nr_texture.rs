//! Known-signal qualification for spatial NR, independent of camera colour and sharpening.

use crate::local::{denoise, guided, guided_nr, nr_params};
use crate::{Quality, RenderRequest, SourceInfo, plan, render};
use lightcraft_develop::{DevelopSettings, Process};
use lightcraft_raster::{Plane, Rgb32f, blur::gaussian};
use std::sync::Arc;

fn noise(w: usize, h: usize) -> Plane {
    let mut seed = 73u64;
    let mut out = Plane::new(w, h);
    for v in &mut out.data {
        let mut uniform = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ((seed >> 40) as f32 + 0.5) / (1u64 << 24) as f32
        };
        *v = (-2.0 * uniform().ln()).sqrt() * (std::f32::consts::TAU * uniform()).cos();
    }
    out
}

fn filter(p: &Plane) -> Plane {
    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.detail.nr_luminance = 61.0;
    s.detail.nr_detail = 87.0;
    let nr = nr_params(&s, 1.0).0.unwrap();
    let f = guided_nr(p, nr);
    Plane::from_fn(p.width, p.height, |x, y| {
        let v = p.get(x, y);
        v + nr.k * (f.get(x, y) - v)
    })
}

fn interior(p: &Plane) -> impl Iterator<Item = (usize, usize)> {
    (16..p.height - 16).flat_map(move |y| (16..p.width - 16).map(move |x| (x, y)))
}

#[test]
fn spatial_nr_preserves_coherent_texture_at_fixed_noise_reduction() {
    let n = noise(128, 128);
    for period in [4.0, 8.0, 16.0] {
        for (dx, dy) in [(1.0, 0.0), (0.0, 1.0), (1.0, 1.0), (0.9659258, 0.25881904), (0.8660254, 0.5), (0.5, 0.8660254), (0.25881904, 0.9659258)] {
            let clean = Plane::from_fn(128, 128, |x, y| 0.3 * (std::f32::consts::TAU * (dx * x as f32 + dy * y as f32) / period).sin());
            let noisy = Plane::from_fn(128, 128, |x, y| clean.get(x, y) + 0.03 * n.get(x, y));
            let out = filter(&noisy);
            let contrast = interior(&clean).map(|(x, y)| out.get(x, y) * clean.get(x, y)).sum::<f32>()
                / interior(&clean).map(|(x, y)| clean.get(x, y).powi(2)).sum::<f32>();
            let error = interior(&clean).map(|(x, y)| (out.get(x, y) - clean.get(x, y)).powi(2)).sum::<f32>()
                / interior(&clean).map(|(x, y)| (noisy.get(x, y) - clean.get(x, y)).powi(2)).sum::<f32>();
            eprintln!("period={period} direction=({dx},{dy}) contrast={contrast:.4} mse/noisy={error:.4}");
            assert!(contrast > 0.95, "coherent texture attenuated: {contrast}");
            assert!(error < 0.5, "noise/error not reduced: {error}");
        }
    }
}

#[test]
fn spatial_nr_flat_noise_qualification() {
    for correlation in [0.0, 1.0, 2.0] {
        let n = gaussian(&noise(128, 128), correlation);
        let rms = (n.data.iter().map(|x| x * x).sum::<f32>() / n.data.len() as f32).sqrt();
        let input = n.map(|x| x * (0.03 / rms));
        let output = filter(&input);
        let mut s = DevelopSettings::default();
        s.detail.nr_luminance = 61.0;
        s.detail.nr_detail = 87.0;
        let nr = nr_params(&s, 1.0).0.unwrap();
        let old = guided(&input, nr.sigma, nr.eps);
        let old = Plane::from_fn(input.width, input.height, |x, y| input.get(x, y) + nr.k * (old.get(x, y) - input.get(x, y)));
        let ratio = interior(&input).map(|(x, y)| output.get(x, y).powi(2)).sum::<f32>()
            / interior(&input).map(|(x, y)| input.get(x, y).powi(2)).sum::<f32>();
        let versus_old =
            interior(&input).map(|(x, y)| output.get(x, y).powi(2)).sum::<f32>() / interior(&input).map(|(x, y)| old.get(x, y).powi(2)).sum::<f32>();
        eprintln!("correlation={correlation} residual variance={ratio:.5}");
        assert!(ratio < 0.65, "flat noise not suppressed: {ratio}");
        assert!(versus_old <= 1.05, "flat noise variance increased: {versus_old}");
    }
}

#[test]
fn legacy_and_zero_detail_use_the_frozen_filter() {
    let p = noise(64, 64);
    for process in Process::ALL {
        for detail in [0.0, 50.0, 87.0, 100.0] {
            let mut s = DevelopSettings { process: process.version(), ..Default::default() };
            s.detail.nr_luminance = 61.0;
            s.detail.nr_detail = detail;
            let nr = nr_params(&s, 1.0).0.unwrap();
            if process != Process::V3 || detail == 0.0 {
                assert_eq!(nr.protection, 0.0);
                assert_eq!(guided_nr(&p, nr).data, guided(&p, nr.sigma, nr.eps).data);
            }
        }
    }
}

#[test]
fn coherent_nr_constants_tiny_planes_and_disabled_nr() {
    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.detail.nr_luminance = 100.0;
    s.detail.nr_detail = 100.0;
    let nr = nr_params(&s, 1.0).0.unwrap();
    for (w, h) in [(0, 0), (1, 1), (1, 17), (17, 1), (31, 29)] {
        for v in [-20.0, 0.0, 4.0] {
            let p = Plane::filled(w, h, v);
            assert!(guided_nr(&p, nr).data.iter().all(|&out| (out - v).abs() < 1e-5));
        }
    }
    let mut img = lightcraft_raster::Rgb32f::from_fn(64, 64, |x, y| [0.1 + ((x + y) % 7) as f32 * 0.01; 3]);
    let original = img.data.clone();
    s.detail.nr_luminance = 0.0;
    s.detail.nr_color = 0.0;
    denoise(&mut img, &s, 1.0);
    assert_eq!(img.data, original);
}

#[test]
fn weak_and_crossed_textures_do_not_increase_error_over_v2() {
    let n = noise(128, 128);
    for amplitude in [0.02, 0.06, 0.15] {
        for crossed in [false, true] {
            let clean = Plane::from_fn(128, 128, |x, y| {
                let a = amplitude * (std::f32::consts::TAU * x as f32 / 8.0).sin();
                a + if crossed { amplitude * (std::f32::consts::TAU * y as f32 / 8.0).sin() } else { 0.0 }
            });
            let input = clean.zip_map(&n, |c, n| c + 0.03 * n);
            let output = filter(&input);
            let mut s = DevelopSettings { process: Process::V2.version(), ..Default::default() };
            s.detail.nr_luminance = 61.0;
            s.detail.nr_detail = 87.0;
            let nr = nr_params(&s, 1.0).0.unwrap();
            let old = guided_nr(&input, nr);
            let old = input.zip_map(&old, |p, f| p + nr.k * (f - p));
            let error = |p: &Plane| interior(p).map(|(x, y)| (p.get(x, y) - clean.get(x, y)).powi(2)).sum::<f32>();
            assert!(error(&output) <= 1.03 * error(&old), "amplitude={amplitude} crossed={crossed}");
        }
    }
}

#[test]
fn process_switch_invalidates_nr_and_draft_planes() {
    use crate::{Quality, RenderRequest, SourceInfo, StageCache, plan, render, render_cached};
    use std::sync::Arc;
    let n = noise(128, 128);
    let src = Arc::new(n.map(|v| [0.18 * (0.2 * v).exp2(); 3]));
    let info = SourceInfo::default();
    let cache = StageCache::default();
    let mut s = DevelopSettings::default();
    s.detail.nr_luminance = 61.0;
    s.detail.nr_detail = 87.0;
    s.effects.clarity = 10.0;
    for quality in [Quality::Full, Quality::Draft] {
        let req = RenderRequest { quality, ..RenderRequest::fit(128, 128) };
        let mut previous_key = None;
        for process in [Process::V2, Process::V3, Process::V2] {
            s.process = process.version();
            let key = plan(&src, &info, &s, &req).pre_nr_key;
            assert_ne!(previous_key, Some(key));
            previous_key = Some(key);
            assert_eq!(render_cached(&src, &info, &s, &req, &cache).image, render(&src, &info, &s, &req).image);
            if process == Process::V3 {
                for value in [10.0, 45.0, 80.0] {
                    s.detail.nr_luminance = value;
                    assert_eq!(render_cached(&src, &info, &s, &req, &cache).image, render(&src, &info, &s, &req).image);
                }
            }
        }
    }
}

#[test]
fn coherent_texture_previews_match_downsampled_full_nr() {
    use crate::local::log_lum;
    use lightcraft_raster::{
        Rgb32f,
        resample::{Filter, resize},
    };
    let n = noise(128, 128);
    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.detail.nr_luminance = 61.0;
    s.detail.nr_detail = 87.0;
    s.detail.nr_color = 0.0;
    for period in [4.0, 8.0, 16.0] {
        let source =
            Rgb32f::from_fn(128, 128, |x, y| [0.18 * (0.3 * (std::f32::consts::TAU * x as f32 / period).sin() + 0.03 * n.get(x, y)).exp2(); 3]);
        let mut full = source.clone();
        denoise(&mut full, &s, 1.0);
        for (edge, scale) in [(64, 0.5), (32, 0.25), (16, 0.125)] {
            let reference = resize(&full, edge, edge, Filter::Mitchell);
            let mut preview = resize(&source, edge, edge, Filter::Mitchell);
            denoise(&mut preview, &s, scale);
            let error = ((4..edge - 4)
                .flat_map(|y| (4..edge - 4).map(move |x| (x, y)))
                .map(|(x, y)| (log_lum(preview.get(x, y)) - log_lum(reference.get(x, y))).powi(2))
                .sum::<f32>()
                / ((edge - 8) * (edge - 8)) as f32)
                .sqrt();
            eprintln!("coherent preview period={period} scale={scale}: {error}");
            assert!(error < 0.01, "coherent preview differs by {error} stops");
        }
    }
}

/// Retained XTR-T2 prototype: measures one heuristic gate's tradeoff, not noise calibration.
#[test]
fn xtr_t2_correlated_noise_vs_crossed_texture() {
    let w = 128;
    let h = 128;
    let n = noise(w, h);

    // 1. Correlated noise field: noise smoothed at correlation length 1.5
    let corr_noise = gaussian(&n, 1.5);
    let rms = (corr_noise.data.iter().map(|x| x * x).sum::<f32>() / corr_noise.data.len() as f32).sqrt();
    let noise_input = corr_noise.map(|x| x * (0.03 / rms));
    let noise_output = filter(&noise_input);
    let noise_residual = interior(&noise_input).map(|(x, y)| noise_output.get(x, y).powi(2)).sum::<f32>()
        / interior(&noise_input).map(|(x, y)| noise_input.get(x, y).powi(2)).sum::<f32>();

    // 2. Crossed woven texture (period 8px in both X and Y) with additive noise
    let fabric = Plane::from_fn(w, h, |x, y| {
        let wx = (std::f32::consts::TAU * x as f32 / 8.0).cos();
        let wy = (std::f32::consts::TAU * y as f32 / 8.0).cos();
        0.05 * wx * wy
    });
    let fabric_input = Plane::from_fn(w, h, |x, y| fabric.get(x, y) + 0.015 * n.get(x, y));
    let fabric_output = filter(&fabric_input);

    // Diagnostic measurement of tensor eigenvalues on noise vs fabric:
    let measure_tensor = |p: &Plane| {
        let mut products = lightcraft_raster::Rgb32f::new(p.width, p.height);
        for y in 0..p.height {
            for x in 0..p.width {
                let (xi, yi) = (x as isize, y as isize);
                let at = |x, y| p.get_clamped(x, y);
                let gx = (3.0 * (at(xi + 1, yi - 1) - at(xi - 1, yi - 1))
                    + 10.0 * (at(xi + 1, yi) - at(xi - 1, yi))
                    + 3.0 * (at(xi + 1, yi + 1) - at(xi - 1, yi + 1)))
                    / 32.0;
                let gy = (3.0 * (at(xi - 1, yi + 1) - at(xi - 1, yi - 1))
                    + 10.0 * (at(xi, yi + 1) - at(xi, yi - 1))
                    + 3.0 * (at(xi + 1, yi + 1) - at(xi + 1, yi - 1)))
                    / 32.0;
                products.data[y * p.width + x] = [gx * gx, gx * gy, gy * gy];
            }
        }
        let t = lightcraft_raster::blur::gaussian(&products, 2.5);
        let mut max_large = 0.0f32;
        let mut mean_large = 0.0f32;
        let mut mean_small = 0.0f32;
        let mut count = 0.0f32;
        for y in 16..p.height - 16 {
            for x in 16..p.width - 16 {
                let [xx, xy, yy] = t.get(x, y);
                let disc = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
                let small = ((xx + yy - disc) * 0.5).max(0.0);
                let large = (xx + yy + disc) * 0.5;
                max_large = max_large.max(large);
                mean_large += large;
                mean_small += small;
                count += 1.0;
            }
        }
        (mean_large / count, mean_small / count, max_large)
    };
    let (n_mean_large, n_mean_small, n_max_large) = measure_tensor(&noise_input);
    let (f_mean_large, f_mean_small, f_max_large) = measure_tensor(&fabric_input);
    let white_noise = n.map(|x| x * 0.03);
    let (w_mean_large, w_mean_small, w_max_large) = measure_tensor(&white_noise);
    eprintln!("White noise tensor: mean_large={w_mean_large:.6}, mean_small={w_mean_small:.6}, max_large={w_max_large:.6}");
    eprintln!("Correlated noise tensor: mean_large={n_mean_large:.6}, mean_small={n_mean_small:.6}, max_large={n_max_large:.6}");
    eprintln!("Fabric texture tensor: mean_large={f_mean_large:.6}, mean_small={f_mean_small:.6}, max_large={f_max_large:.6}");

    let _fabric_contrast = interior(&fabric).map(|(x, y)| fabric_output.get(x, y) * fabric.get(x, y)).sum::<f32>()
        / interior(&fabric).map(|(x, y)| fabric.get(x, y).powi(2)).sum::<f32>();
    let _fabric_error = interior(&fabric).map(|(x, y)| (fabric_output.get(x, y) - fabric.get(x, y)).powi(2)).sum::<f32>()
        / interior(&fabric).map(|(x, y)| (fabric_input.get(x, y) - fabric.get(x, y)).powi(2)).sum::<f32>();

    // This threshold is derived from the slider's regularizer, not measured sensor noise.
    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.detail.nr_luminance = 61.0;
    s.detail.nr_detail = 87.0;
    let nr = nr_params(&s, 1.0).0.unwrap();
    let tau_noise = 0.005 * nr.eps;

    let filter_t2 = |p: &Plane| {
        let mut basic = guided(p, nr.sigma, nr.eps);
        let mut products = lightcraft_raster::Rgb32f::new(p.width, p.height);
        for y in 0..p.height {
            for x in 0..p.width {
                let (xi, yi) = (x as isize, y as isize);
                let at = |x, y| p.get_clamped(x, y);
                let gx = (3.0 * (at(xi + 1, yi - 1) - at(xi - 1, yi - 1))
                    + 10.0 * (at(xi + 1, yi) - at(xi - 1, yi))
                    + 3.0 * (at(xi + 1, yi + 1) - at(xi - 1, yi + 1)))
                    / 32.0;
                let gy = (3.0 * (at(xi - 1, yi + 1) - at(xi - 1, yi - 1))
                    + 10.0 * (at(xi, yi + 1) - at(xi, yi - 1))
                    + 3.0 * (at(xi + 1, yi + 1) - at(xi + 1, yi - 1)))
                    / 32.0;
                products.data[y * p.width + x] = [gx * gx, gx * gy, gy * gy];
            }
        }
        let tensor = lightcraft_raster::blur::gaussian(&products, nr.sigma);
        let weights: [f32; 9] = std::array::from_fn(|i| (-0.5 * ((i as f32 - 4.0) / nr.sigma).powi(2)).exp());
        let total: f32 = weights.iter().sum();

        for y in 0..p.height {
            for x in 0..p.width {
                let i = y * p.width + x;
                let [xx, xy, yy] = tensor.get(x, y);
                let disc = ((xx - yy).powi(2) + 4.0 * xy * xy).sqrt();
                let small = ((xx + yy - disc) * 0.5).max(0.0);
                let large = (xx + yy + disc) * 0.5;

                // Heuristic energy gate, not a calibrated SNR estimate.
                let snr_gate = ((large - tau_noise) / (large + tau_noise).max(1e-12)).clamp(0.0, 1.0);
                // Anisotropy and crossed texture protection:
                // Rather than hard-clamping to 0 when small > 0.25 * large, we evaluate directional coherence
                // The measurements below quantify how much noise this admits.
                let coherence = ((large - small) / (large + small).max(1e-12)).clamp(0.0, 1.0);
                // Texture confidence: combines edge coherence and crossed texture energy above noise
                let tex_coherence = (coherence + 0.5 * (small / large.max(1e-12))).clamp(0.0, 1.0);
                let confidence = nr.protection * snr_gate * tex_coherence;

                if confidence <= 0.0 {
                    continue;
                }
                let (tx, ty) = if xx >= yy { (-xy, xx - small) } else { (yy - small, -xy) };
                let norm = tx.hypot(ty).max(1e-12);
                let (tx, ty) = (tx / norm, ty / norm);
                let mut sum = 0.0;
                for (step, weight) in (-4..=4).zip(weights) {
                    sum += weight * crate::local::tangent_sample(p, x as isize, y as isize, tx, ty, step as f32);
                }
                basic.data[i] += confidence * (sum / total - basic.data[i]);
            }
        }
        Plane::from_fn(p.width, p.height, |x, y| {
            let v = p.get(x, y);
            v + nr.k * (basic.get(x, y) - v)
        })
    };

    let noise_output_t2 = filter_t2(&noise_input);
    let noise_residual_t2 = interior(&noise_input).map(|(x, y)| noise_output_t2.get(x, y).powi(2)).sum::<f32>()
        / interior(&noise_input).map(|(x, y)| noise_input.get(x, y).powi(2)).sum::<f32>();
    eprintln!("Correlated noise residual: baseline={noise_residual:.5}, t2={noise_residual_t2:.5}");

    for amp in [0.03, 0.05, 0.08, 0.12] {
        let fabric = Plane::from_fn(w, h, |x, y| {
            let wx = (std::f32::consts::TAU * x as f32 / 8.0).cos();
            let wy = (std::f32::consts::TAU * y as f32 / 8.0).cos();
            amp * wx * wy
        });
        let fabric_input = Plane::from_fn(w, h, |x, y| fabric.get(x, y) + 0.015 * n.get(x, y));
        let fabric_output = filter(&fabric_input);
        let fabric_output_t2 = filter_t2(&fabric_input);

        let c_orig = interior(&fabric).map(|(x, y)| fabric_output.get(x, y) * fabric.get(x, y)).sum::<f32>()
            / interior(&fabric).map(|(x, y)| fabric.get(x, y).powi(2)).sum::<f32>();
        let c_t2 = interior(&fabric).map(|(x, y)| fabric_output_t2.get(x, y) * fabric.get(x, y)).sum::<f32>()
            / interior(&fabric).map(|(x, y)| fabric.get(x, y).powi(2)).sum::<f32>();
        let err_orig = interior(&fabric).map(|(x, y)| (fabric_output.get(x, y) - fabric.get(x, y)).powi(2)).sum::<f32>()
            / interior(&fabric).map(|(x, y)| (fabric_input.get(x, y) - fabric.get(x, y)).powi(2)).sum::<f32>();
        let err_t2 = interior(&fabric).map(|(x, y)| (fabric_output_t2.get(x, y) - fabric.get(x, y)).powi(2)).sum::<f32>()
            / interior(&fabric).map(|(x, y)| (fabric_input.get(x, y) - fabric.get(x, y)).powi(2)).sum::<f32>();
        assert!(c_t2 >= c_orig && err_t2 <= err_orig, "prototype lost its measured texture benefit at amplitude {amp}");
        eprintln!(
            "Fabric amp={amp:.2}: contrast {c_orig:.4} -> {c_t2:.4} (+{:.1}%), error {err_orig:.4} -> {err_t2:.4} (-{:.1}%)",
            100.0 * (c_t2 - c_orig) / c_orig.max(1e-6),
            100.0 * (err_orig - err_t2) / err_orig.max(1e-6)
        );
    }
    assert!(noise_residual < 0.25, "correlated noise was preserved as texture: {noise_residual}");
    assert!(noise_residual_t2 > noise_residual * 1.05, "prototype no longer violates the 5% residual-noise budget; reassess rejection");
    assert!(noise_residual_t2 < noise_residual * 1.20, "prototype changed materially: baseline {noise_residual}, candidate {noise_residual_t2}");
}

#[test]
fn nr_contrast_semantics_and_qualification() {
    let (w, h) = (256, 256);
    let n1 = noise(w, h);
    let n2 = noise(w, h);

    // 1. Flat noise scene: 0.18 mean with Gaussian white noise
    let flat_noisy = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let n = 0.03 * n1.get(x, y);
        let v = (0.18 + n).clamp(0.01, 0.99);
        [v; 3]
    }));

    // 2. High-contrast step edge scene: 0.15 on left, 0.75 on right, with small noise
    let step_edge = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let base = if x >= 128 { 0.75 } else { 0.15 };
        let n = 0.015 * n2.get(x, y);
        let v = (base + n).clamp(0.01, 0.99);
        [v; 3]
    }));

    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    // Baseline settings: Process V3, NR 50, Detail 50, Contrast 0
    let mut s_base = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s_base.detail.nr_luminance = 50.0;
    s_base.detail.nr_detail = 50.0;
    s_base.detail.nr_contrast = 0.0;

    let mut s_contrast = s_base.clone();
    s_contrast.detail.nr_contrast = 100.0;

    // A. Flat noise test: Contrast 100 must NOT disrupt noise smoothing in flat patches
    let r_flat_0 = render(&flat_noisy, &info, &s_base, &req);
    let r_flat_100 = render(&flat_noisy, &info, &s_contrast, &req);

    let variance = |img: &lightcraft_raster::Rgba8| {
        let mut sum = 0.0f64;
        let mut sum_sq = 0.0f64;
        let mut count = 0.0f64;
        for y in 16..h - 16 {
            for x in 16..w - 16 {
                let v = img.data[y * w + x][0] as f64 / 255.0;
                sum += v;
                sum_sq += v * v;
                count += 1.0;
            }
        }
        let mean = sum / count;
        (sum_sq / count - mean * mean) as f32
    };

    let var_0 = variance(&r_flat_0.image);
    let var_100 = variance(&r_flat_100.image);
    let ratio = var_100 / var_0;
    eprintln!("Flat noise variance: contrast=0 -> {var_0:.6}, contrast=100 -> {var_100:.6} (ratio {ratio:.3})");
    // Adobe Lightroom manual: "Higher Contrast values preserve contrast, but can produce noisier patches."
    // Contrast 100 allows at most 30% noise variance increase at the extreme slider position.
    assert!(ratio <= 1.30, "Flat noise variance at extreme contrast 100: ratio={ratio}");

    // B. High-contrast edge test: Contrast 100 must preserve greater edge step height
    let r_step_0 = render(&step_edge, &info, &s_base, &req);
    let r_step_100 = render(&step_edge, &info, &s_contrast, &req);

    // Measure step height across the edge transition (x=127 vs x=129)
    let step_height = |img: &lightcraft_raster::Rgba8| {
        let mut low = 0.0f64;
        let mut high = 0.0f64;
        for y in 32..h - 32 {
            low += img.data[y * w + 127][0] as f64 / 255.0;
            high += img.data[y * w + 129][0] as f64 / 255.0;
        }
        let n = (h - 64) as f64;
        (high / n - low / n) as f32
    };

    let h_0 = step_height(&r_step_0.image);
    let h_100 = step_height(&r_step_100.image);
    eprintln!("Step edge transition (127 vs 129): contrast=0 -> {h_0:.4}, contrast=100 -> {h_100:.4}");
    assert!(h_100 > h_0, "Contrast 100 must preserve greater edge height: {h_100} vs {h_0}");

    // C. Process V1 and V2: nr_contrast must have ZERO effect (bit-for-bit identical)
    for p in [Process::V1, Process::V2] {
        let mut s1 = s_base.clone();
        s1.process = p.version();
        s1.detail.nr_contrast = 0.0;
        let mut s2 = s1.clone();
        s2.detail.nr_contrast = 100.0;

        let r1 = render(&step_edge, &info, &s1, &req);
        let r2 = render(&step_edge, &info, &s2, &req);
        assert_eq!(r1.image.data, r2.image.data, "Process {p:?} must ignore nr_contrast completely");
    }

    // D. Cache key invalidation: changing nr_contrast must change lin_key
    let key_0 = plan(&step_edge, &info, &s_base, &req).lin_key;
    let key_100 = plan(&step_edge, &info, &s_contrast, &req).lin_key;
    assert_ne!(key_0, key_100, "Changing nr_contrast must invalidate lin_key");
}
