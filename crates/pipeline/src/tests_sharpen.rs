//! Known-signal qualification for sharpening: edge overshoot, halo control, and flat noise.

use std::sync::Arc;

use lightcraft_develop::{DevelopSettings, Process};
use lightcraft_raster::Rgb32f;

use crate::{Quality, RenderRequest, SourceInfo, render};

fn luma_srgb(c: [u8; 4]) -> f32 {
    let to_linear = |v: u8| {
        let x = v as f32 / 255.0;
        if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * to_linear(c[0]) + 0.7152 * to_linear(c[1]) + 0.0722 * to_linear(c[2])
}

fn gaussian_noise(w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let mut seed = 101u64;
    let mut out = Vec::with_capacity(w * h);
    for _ in 0..w * h {
        let mut uniform = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ((seed >> 40) as f32 + 0.5) / (1u64 << 24) as f32
        };
        let n = (-2.0 * uniform().ln()).sqrt() * (std::f32::consts::TAU * uniform()).cos();
        out.push(n * sigma);
    }
    out
}

#[test]
fn edge_overshoot_and_halo_profile() {
    let (w, h) = (512, 128);
    // Step edge from 0.20 to 0.60 at x = w / 2
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, _y| {
        let v = if x < w / 2 { 0.20 } else { 0.60 };
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    // Baseline: unsharpened
    let mut s0 = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s0.detail.sharpen_amount = 0.0;
    let r0 = render(&src, &info, &s0, &req);

    // Sharpened Amount 40 (standard raw default)
    let mut s40 = s0.clone();
    s40.detail.sharpen_amount = 40.0;
    s40.detail.sharpen_masking = 0.0;
    let r40 = render(&src, &info, &s40, &req);

    // Sharpened Amount 100 (strong)
    let mut s100 = s0.clone();
    s100.detail.sharpen_amount = 100.0;
    s100.detail.sharpen_masking = 0.0;
    let r100 = render(&src, &info, &s100, &req);

    // Inspect row profile at y = h / 2
    let y = h / 2;
    let p0: Vec<f32> = (0..w).map(|x| luma_srgb(r0.image.data[y * w + x])).collect();
    let p40: Vec<f32> = (0..w).map(|x| luma_srgb(r40.image.data[y * w + x])).collect();
    let p100: Vec<f32> = (0..w).map(|x| luma_srgb(r100.image.data[y * w + x])).collect();

    let low_plateau = p0[w / 4];
    let high_plateau = p0[3 * w / 4];

    // Measure undershoot and overshoot for Amount 40
    let min_40 = p40[..w / 2].iter().copied().fold(f32::INFINITY, f32::min);
    let max_40 = p40[w / 2..].iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let undershoot_40 = (low_plateau - min_40).max(0.0);
    let overshoot_40 = (max_40 - high_plateau).max(0.0);

    // Measure undershoot and overshoot for Amount 100
    let min_100 = p100[..w / 2].iter().copied().fold(f32::INFINITY, f32::min);
    let max_100 = p100[w / 2..].iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let undershoot_100 = (low_plateau - min_100).max(0.0);
    let overshoot_100 = (max_100 - high_plateau).max(0.0);

    eprintln!("Step edge plateaus: low={low_plateau:.4}, high={high_plateau:.4}");
    eprintln!("Amount 40: undershoot={undershoot_40:.4}, overshoot={overshoot_40:.4}");
    eprintln!("Amount 100: undershoot={undershoot_100:.4}, overshoot={overshoot_100:.4}");

    // Sharpening must produce positive overshoot / undershoot at the edge
    assert!(undershoot_40 > 0.001, "Amount 40 must undershoot at edge: {undershoot_40}");
    assert!(overshoot_40 > 0.001, "Amount 40 must overshoot at edge: {overshoot_40}");
    assert!(undershoot_100 > undershoot_40, "Amount 100 must have larger undershoot than 40");
    assert!(overshoot_100 > overshoot_40, "Amount 100 must have larger overshoot than 40");

    // Halo measurement: overshoot documents current unconstrained fringe magnitude
    assert!(overshoot_40 < 0.15, "Amount 40 overshoot excessive: {overshoot_40}");
    assert!(overshoot_100 < 0.30, "Amount 100 overshoot excessive: {overshoot_100}");
}

#[test]
fn flat_noise_amplification_vs_masking() {
    let (w, h) = (256, 256);
    let noise = gaussian_noise(w, h, 0.02);
    // Flat field at 0.35 + noise
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let v = (0.35 + noise[y * w + x]).max(0.01);
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    // Baseline: unsharpened
    let mut s0 = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s0.detail.sharpen_amount = 0.0;
    let r0 = render(&src, &info, &s0, &req);

    // Unmasked sharpening: Amount 40, Masking 0
    let mut s_unmasked = s0.clone();
    s_unmasked.detail.sharpen_amount = 40.0;
    s_unmasked.detail.sharpen_masking = 0.0;
    let r_unmasked = render(&src, &info, &s_unmasked, &req);

    // Masked sharpening: Amount 40, Masking 60 (typical X-Trans / high-ISO setting)
    let mut s_masked = s0.clone();
    s_masked.detail.sharpen_amount = 40.0;
    s_masked.detail.sharpen_masking = 60.0;
    let r_masked = render(&src, &info, &s_masked, &req);

    // Compute variance in interior (16..w-16)
    let variance = |img: &lightcraft_raster::Rgba8| {
        let mut sum = 0.0f64;
        let mut sum_sq = 0.0f64;
        let mut count = 0.0f64;
        for y in 16..h - 16 {
            for x in 16..w - 16 {
                let v = luma_srgb(img.data[y * w + x]) as f64;
                sum += v;
                sum_sq += v * v;
                count += 1.0;
            }
        }
        let mean = sum / count;
        (sum_sq / count - mean * mean) as f32
    };

    let var_0 = variance(&r0.image);
    let var_unmasked = variance(&r_unmasked.image);
    let var_masked = variance(&r_masked.image);

    let amp_unmasked = var_unmasked / var_0;
    let amp_masked = var_masked / var_0;

    eprintln!(
        "Flat noise variance: baseline={var_0:.6}, unmasked={var_unmasked:.6} (ratio {amp_unmasked:.3}), masked={var_masked:.6} (ratio {amp_masked:.3})"
    );

    // Unmasked sharpening amplifies flat noise
    assert!(amp_unmasked > 1.05, "Unmasked sharpening should amplify flat noise: {amp_unmasked}");
    // Masking 60 must suppress flat noise amplification back close to 1.0 (<= 1.05)
    assert!(amp_masked <= 1.05, "Masking 60 must protect flat noise from amplification: {amp_masked}");
}

#[test]
fn texture_contrast_enhancement() {
    let (w, h) = (256, 256);
    // Sine wave texture of period 8 px
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, _y| {
        let carrier = (std::f32::consts::TAU * x as f32 / 8.0).cos();
        let v = 0.35 + 0.05 * carrier;
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    let mut s0 = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s0.detail.sharpen_amount = 0.0;
    let r0 = render(&src, &info, &s0, &req);

    let mut s40 = s0.clone();
    s40.detail.sharpen_amount = 40.0;
    s40.detail.sharpen_masking = 0.0;
    let r40 = render(&src, &info, &s40, &req);

    let mut s80 = s0.clone();
    s80.detail.sharpen_amount = 80.0;
    s80.detail.sharpen_masking = 0.0;
    let r80 = render(&src, &info, &s80, &req);

    let contrast = |img: &lightcraft_raster::Rgba8| {
        let mut amp = 0.0f64;
        let mut count = 0.0f64;
        for y in 16..h - 16 {
            for x in 16..w - 16 {
                let carrier = (std::f32::consts::TAU * x as f32 / 8.0).cos() as f64;
                let v = luma_srgb(img.data[y * w + x]) as f64;
                amp += (v - 0.35) * carrier;
                count += 1.0;
            }
        }
        (amp / count) as f32
    };

    let c0 = contrast(&r0.image);
    let c40 = contrast(&r40.image);
    let c80 = contrast(&r80.image);

    let boost_40 = (c40 - c0) / c0;
    let boost_80 = (c80 - c0) / c0;

    eprintln!("Texture contrast: baseline={c0:.5}, amount40={c40:.5} (+{:.1}%), amount80={c80:.5} (+{:.1}%)", boost_40 * 100.0, boost_80 * 100.0);

    assert!(c40 > c0 * 1.01, "Amount 40 must boost texture contrast: {c40} vs {c0}");
    assert!(c80 > c40 * 1.05, "Amount 80 must boost texture contrast more than Amount 40: {c80} vs {c40}");
}

#[test]
fn sharpen_preview_scale_consistency() {
    let (w, h) = (1024, 768);
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, _y| {
        let xf = x as f32;
        let carrier = (std::f32::consts::TAU * xf / 16.0).cos();
        let edge = if x > 512 { 0.4 } else { 0.0 };
        let v = (0.3 + 0.1 * carrier + edge).clamp(0.05, 0.95);
        [v; 3]
    }));
    let info = SourceInfo::default();

    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.detail.sharpen_amount = 50.0;
    s.detail.sharpen_masking = 20.0;

    let req_full = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };
    let r_full = render(&src, &info, &s, &req_full);

    // Downscale full render to 512x384
    let (pw, ph) = (w / 2, h / 2);
    let full_f32 = r_full.image.map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]);
    let r_scaled = lightcraft_raster::resample::resize(&full_f32, pw, ph, lightcraft_raster::resample::Filter::Mitchell);

    let req_preview = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(pw, ph) };
    let r_preview = render(&src, &info, &s, &req_preview);
    let prev_f32 = r_preview.image.map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]);

    let mut sum_diff_sq = 0.0f64;
    let n = (pw * ph) as f64;
    for i in 0..pw * ph {
        let d = r_scaled.data[i][0] - prev_f32.data[i][0];
        sum_diff_sq += (d * d) as f64;
    }
    let rms = (sum_diff_sq / n).sqrt() as f32;
    eprintln!("Sharpen preview consistency at 0.5x: RMS difference = {rms:.5}");
    assert!(rms < 0.02, "Preview must be consistent with downscaled full-size render: rms={rms}");
}

#[test]
fn sharpen_radius_controls_spatial_profile() {
    let (w, h) = (2048, 128);
    // Step edge at x = 1024
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, _y| {
        let v = if x < 1024 { 0.20 } else { 0.60 };
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    let mut s_tight = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s_tight.detail.sharpen_amount = 60.0;
    s_tight.detail.sharpen_radius = 0.5;

    let mut s_wide = s_tight.clone();
    s_wide.detail.sharpen_radius = 2.5;

    let r_tight = render(&src, &info, &s_tight, &req);
    let r_wide = render(&src, &info, &s_wide, &req);

    // Profile at y = 64
    let y = 64;
    let p_tight: Vec<f32> = (0..w).map(|x| luma_srgb(r_tight.image.data[y * w + x])).collect();
    let p_wide: Vec<f32> = (0..w).map(|x| luma_srgb(r_wide.image.data[y * w + x])).collect();

    let plateau = p_tight[w - 10];
    for dx in 1..=8 {
        eprintln!(
            "dx={dx}: tight={:.5} (dist={:.5}), wide={:.5} (dist={:.5})",
            p_tight[1024 + dx],
            (p_tight[1024 + dx] - plateau).abs(),
            p_wide[1024 + dx],
            (p_wide[1024 + dx] - plateau).abs()
        );
    }
    let dist_tight = (p_tight[1024 + 3] - plateau).abs();
    let dist_wide = (p_wide[1024 + 3] - plateau).abs();

    eprintln!("Halo width at +3px from edge: radius 0.5 dist={dist_tight:.5}, radius 2.5 dist={dist_wide:.5}");
    assert!(dist_wide > dist_tight, "Radius 2.5 must create a broader halo profile than radius 0.5");
}

#[test]
fn sharpen_detail_controls_halo_and_micro_contrast() {
    let (w, h) = (512, 128);
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, _y| {
        let v = if x < w / 2 { 0.20 } else { 0.60 };
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    // Detail = 0: strong halo suppression
    let mut s0 = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s0.detail.sharpen_amount = 80.0;
    s0.detail.sharpen_detail = 0.0;
    let r0 = render(&src, &info, &s0, &req);

    // Detail = 100: relaxed halo suppression
    let mut s100 = s0.clone();
    s100.detail.sharpen_detail = 100.0;
    let r100 = render(&src, &info, &s100, &req);

    let y = h / 2;
    let p0: Vec<f32> = (0..w).map(|x| luma_srgb(r0.image.data[y * w + x])).collect();
    let p100: Vec<f32> = (0..w).map(|x| luma_srgb(r100.image.data[y * w + x])).collect();
    let high_plateau = p0[3 * w / 4];

    let max0 = p0[w / 2..].iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let max100 = p100[w / 2..].iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let overshoot0 = (max0 - high_plateau).max(0.0);
    let overshoot100 = (max100 - high_plateau).max(0.0);

    eprintln!("Detail 0 overshoot={overshoot0:.4}, Detail 100 overshoot={overshoot100:.4}");
    assert!(overshoot100 > overshoot0, "Detail 100 must allow greater edge overshoot than Detail 0: {overshoot100} vs {overshoot0}");
}

#[test]
fn sharpen_process_v1_and_v2_freeze() {
    let (w, h) = (256, 64);
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let v = (x as f32 / w as f32 * 0.5 + y as f32 / h as f32 * 0.2).clamp(0.1, 0.9);
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest { quality: Quality::Full, ..RenderRequest::fit(w, h) };

    for p in [Process::V1, Process::V2] {
        let mut s1 = DevelopSettings { process: p.version(), ..Default::default() };
        s1.detail.sharpen_amount = 60.0;
        s1.detail.sharpen_radius = 0.5;
        s1.detail.sharpen_detail = 0.0;

        let mut s2 = s1.clone();
        s2.detail.sharpen_radius = 2.5;
        s2.detail.sharpen_detail = 100.0;

        let r1 = render(&src, &info, &s1, &req);
        let r2 = render(&src, &info, &s2, &req);

        assert_eq!(r1.image.data, r2.image.data, "Process {p:?} must be frozen bit-for-bit regardless of sharpen_radius and sharpen_detail");
    }
}

#[test]
fn local_sharpening_does_not_change_texture_outside_its_mask() {
    use lightcraft_develop::{LocalAdjustments, Mask, MaskComponent, MaskOp, MaskShape};
    use lightcraft_geom::Point;

    let (w, h) = (4096, 128);
    let src = Arc::new(Rgb32f::from_fn(w, h, |x, y| {
        let v = 0.3 + 0.25 * (std::f32::consts::TAU * (x + y) as f32 / 12.0).sin();
        [v; 3]
    }));
    let info = SourceInfo::default();
    let req = RenderRequest::fit(w, h);
    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.effects.texture = 10.0;
    s.detail.sharpen_amount = 0.0;
    let before = render(&src, &info, &s, &req);
    s.masks.push(Mask {
        components: vec![MaskComponent {
            name: None,
            op: MaskOp::Add,
            invert: false,
            shape: MaskShape::Radial { center: Point::new(0.25, 0.5), rx: 0.12, ry: 0.08, angle: 0.0, feather: 0.0, invert: false },
        }],
        adjust: LocalAdjustments { sharpness: 80.0, ..Default::default() },
        ..Default::default()
    });
    let cache = crate::StageCache::default();
    for radius in [0.5, 1.0, 3.0, 0.5] {
        s.detail.sharpen_radius = radius;
        let after = crate::render_cached(&src, &info, &s, &req, &cache);
        assert_eq!(after.image.data, render(&src, &info, &s, &req).image.data, "stale sharpening plane at radius {radius}");
        let mut changed_inside = false;
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if x >= w / 2 {
                    assert_eq!(before.image.data[i], after.image.data[i], "local sharpen leaked at ({x}, {y}), radius {radius}");
                } else {
                    changed_inside |= before.image.data[i] != after.image.data[i];
                }
            }
        }
        assert!(changed_inside, "mask must actually sharpen some pixels");
    }
}

#[test]
fn sharpening_does_not_change_other_detail_plane_radii() {
    use crate::local::plane_sigmas;
    use lightcraft_develop::{LocalAdjustments, Mask};

    let mut s = DevelopSettings { process: Process::V3.version(), ..Default::default() };
    s.detail.sharpen_amount = 0.0;
    for adjust in [
        LocalAdjustments { texture: 30.0, ..Default::default() },
        LocalAdjustments { noise: 50.0, ..Default::default() },
        LocalAdjustments { defringe: 40.0, ..Default::default() },
    ] {
        s.masks = vec![Mask { adjust, ..Default::default() }];
        let base = plane_sigmas(&s, 4896.0, Quality::Full).texture;
        for amount in [0.0, 1.0, 80.0] {
            for radius in [0.5, 1.0, 3.0] {
                s.detail.sharpen_amount = amount;
                s.detail.sharpen_radius = radius;
                assert_eq!(plane_sigmas(&s, 4896.0, Quality::Full).texture, base);
            }
        }
        s.detail.sharpen_amount = 0.0;
    }
}
