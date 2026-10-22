//! Demosaic diagnostics for X-Trans raws, plus optional 100 % crops.
//!
//! ```text
//! cargo run --release -p lightcraft-raw --example xtrans_quality -- photo.raf [--crops DIR] [--at X,Y]
//! ```
//!
//! For an A/B between two builds, dump the full camera-RGB frame from each and compare:
//!
//! ```text
//! # build A
//! cargo run --release -p lightcraft-raw --example xtrans_quality -- photo.raf --dump a.f32
//! # build B (after switching commits)
//! cargo run --release -p lightcraft-raw --example xtrans_quality -- photo.raf --dump b.f32 --compare a.f32
//! ```
//!
//! `--dump` writes `LCXTRANS1 <w> <h>\n` followed by `w*h*3` little-endian f32 samples (no white
//! balance, no colour matrix, no tone curve), so the comparison is bit-for-bit on the demosaic
//! output. `--compare` reports how many samples differ, the median and maximum |delta| and how many
//! 8-bit codes of an sRGB proxy change. Build each commit with its own `CARGO_TARGET_DIR`: a shared
//! one can hand back the other worktree's artifacts (Cargo compares mtimes), which silently turns
//! the A/B into a self-comparison.
//!
//! - **Flat noise**: standard deviation of the 3×3 Laplacian of green, at the 10th percentile over all
//!   mid-tone 256² windows (a percentile rather than the minimum, which moves between builds).
//!   Demosaic noise lives here; sharpening and texture must not move it much.
//! - **High-frequency residual**: RMS of green minus its four adjacent neighbours' mean,
//!   relative to green RMS. Detects period-two patterns but is not a selective Nyquist measure.
//! - **Chroma**: R-G and B-G energies are measured separately and combined in quadrature.
//! - **Edge energy**: mean gradient magnitude of the strongest 1 % of pixels. If a change buys
//!   noise reduction by blurring detail, this drops.
//!
//! The raw is developed to camera RGB (no white balance, no colour matrix, no tone curve), so the
//! numbers compare like with like across builds; the crops are auto-exposed and gamma-encoded for
//! looking at, not for measuring.
//! Metrics version 2 is shared with `noise_sweep`; old chroma/HF values are not comparable.
//! Missing qualifying windows print `NA`, never a zero-noise score.

#[path = "support/xtrans_metrics.rs"]
mod metrics;

use lightcraft_raw::{Method, Rgb32f, demosaic};
use metrics::luma;

const CROP: usize = 512;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: xtrans_quality FILE.RAF [--crops DIR] [--dump FILE] [--compare FILE]")?;
    let mut crops: Option<String> = None;
    let mut dump: Option<String> = None;
    let mut compare: Option<String> = None;
    let mut at: Vec<(usize, usize)> = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--crops" => crops = Some(args.next().ok_or("--crops needs a directory")?),
            "--dump" => dump = Some(args.next().ok_or("--dump needs a file")?),
            "--compare" => compare = Some(args.next().ok_or("--compare needs a file")?),
            // Fixed top-left corners, so two builds can be compared at the same place: the windows
            // the harness picks on its own move when the demosaic changes.
            "--at" => {
                let spec = args.next().ok_or("--at needs X,Y")?;
                let (x, y) = spec.split_once(',').ok_or("--at needs X,Y")?;
                at.push((x.parse()?, y.parse()?));
            }
            other => return Err(format!("unknown argument `{other}`").into()),
        }
    }

    let bytes = std::fs::read(&path)?;
    let raw = lightcraft_raw::decode(&bytes)?;
    if raw.cpp != 1 || !raw.cfa.as_ref().is_some_and(|c| c.width == 6 && c.height == 6) {
        return Err("not a single-channel X-Trans raw".into());
    }
    let started = std::time::Instant::now();
    let norm = raw.normalized()?;
    let normalize_ms = started.elapsed().as_secs_f64() * 1e3;
    let started = std::time::Instant::now();
    let rgb = demosaic(&norm, Method::Ahd);
    let demosaic_ms = started.elapsed().as_secs_f64() * 1e3;

    println!(
        "{}  {}x{}  ISO {}  normalize {normalize_ms:.2} ms  demosaic {:.2} ms",
        path.rsplit('/').next().unwrap_or(&path),
        rgb.width,
        rgb.height,
        raw.metadata.iso.map_or("?".to_string(), |i| i.to_string()),
        demosaic_ms
    );

    let m = metrics::measure(&rgb)?;
    println!("metrics version {}", metrics::VERSION);
    println!("flat noise (green Laplacian std, 10th pct of 256² windows): {}", metrics::value(m.flat.map(|v| v.0)));
    println!("green HF residual (adjacent-neighbour, relative): {}", metrics::value(m.green_hf));
    println!("edge energy (top 1 % gradient): {:.4e}", m.edge);
    println!("texture energy (median-90 % gradient): {:.4e}", m.texture);
    println!("chroma noise (R-G/B-G Laplacian RMS, 10th pct of 128² windows): {}", metrics::value(m.chroma));
    println!("chroma HF residual (R-G/B-G quadrature, relative): {}", metrics::value(m.chroma_hf));

    if let Some(file) = dump {
        write_dump(&rgb, &file)?;
        println!("dump: {file} ({}x{}, f32 LE)", rgb.width, rgb.height);
    }
    if let Some(file) = compare {
        let (w, h, other) = read_dump(&file)?;
        compare_dumps(&rgb, w, h, &other)?;
    }

    if let Some(dir) = crops {
        std::fs::create_dir_all(&dir)?;
        let busiest = busiest_window(&rgb);
        let mut windows = vec![("detail".to_string(), busiest)];
        if let Some((_, at)) = m.flat {
            windows.push(("flat".to_string(), at));
        }
        for (i, (x, y)) in at.iter().enumerate() {
            windows.push((format!("at{i}"), (*x, *y)));
        }
        for (name, (x, y)) in windows {
            let file = format!("{dir}/{name}_{x}_{y}.ppm");
            write_crop(&rgb, x, y, &file)?;
            println!("crop {name}: {file}");
        }
    }
    Ok(())
}

/// Busiest [`CROP`]² window by Laplacian energy: where detail retention shows.
fn busiest_window(rgb: &Rgb32f) -> (usize, usize) {
    let (w, h) = (rgb.width, rgb.height);
    let mut best = ((0usize, 0usize), 0.0f64);
    for y in (64..h.saturating_sub(CROP + 64)).step_by(64) {
        for x in (64..w.saturating_sub(CROP + 64)).step_by(64) {
            let (mut e, mut n) = (0.0f64, 0f64);
            for yy in (y..y + CROP).step_by(2) {
                for xx in (x..x + CROP).step_by(2) {
                    let l = 4.0 * green(rgb, xx, yy) as f64
                        - green(rgb, xx - 1, yy) as f64
                        - green(rgb, xx + 1, yy) as f64
                        - green(rgb, xx, yy - 1) as f64
                        - green(rgb, xx, yy + 1) as f64;
                    e += l * l;
                    n += 1.0;
                }
            }
            let e = e / n;
            if e > best.1 {
                best = ((x, y), e);
            }
        }
    }
    best.0
}

fn write_crop(rgb: &Rgb32f, x: usize, y: usize, path: &str) -> std::io::Result<()> {
    let (w, h) = (rgb.width, rgb.height);
    if w == 0 || h == 0 || w.checked_mul(h) != Some(rgb.data.len()) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "crop needs a complete non-empty RGB frame"));
    }
    let (cw, ch) = (w.min(CROP), h.min(CROP));
    let (x, y) = (x.min(w - cw), y.min(h - ch));
    let mut sample: Vec<f32> = (y..y + ch).flat_map(|yy| (x..x + cw).map(move |xx| luma(rgb, xx, yy))).collect();
    sample.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
    let white = sample.get(sample.len() * 995 / 1000).copied().unwrap_or(1.0).max(1e-6);
    let mut out = format!("P6\n{cw} {ch}\n255\n").into_bytes();
    for yy in y..y + ch {
        for xx in x..x + cw {
            for c in 0..3 {
                let v = (rgb.data[yy * w + xx][c] / white).clamp(0.0, 1.0).powf(1.0 / 2.2);
                out.push((v * 255.0 + 0.5) as u8);
            }
        }
    }
    std::fs::write(path, out)
}

fn green(rgb: &Rgb32f, x: usize, y: usize) -> f32 {
    rgb.data[y * rgb.width + x][1]
}

/// Full-frame camera RGB for an A/B between builds: `LCXTRANS1 <w> <h>\n` then `w*h*3` little-endian
/// f32. No white balance, matrix or tone curve is applied, so two builds of the same file differ
/// only where the demosaic does.
fn write_dump(rgb: &Rgb32f, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "LCXTRANS1 {} {}", rgb.width, rgb.height)?;
    let mut bytes = Vec::with_capacity(rgb.data.len() * 12);
    for px in &rgb.data {
        for v in px {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(())
}

fn read_dump(path: &str) -> Result<(usize, usize, Vec<[f32; 3]>), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let newline = bytes.iter().position(|&b| b == b'\n').ok_or("dump: missing header")?;
    let header = std::str::from_utf8(&bytes[..newline]).map_err(|_| "dump: header is not UTF-8")?;
    let mut fields = header.split_whitespace();
    if fields.next() != Some("LCXTRANS1") {
        return Err("dump: not an LCXTRANS1 file".into());
    }
    let width: usize = fields.next().ok_or("dump: missing width")?.parse()?;
    let height: usize = fields.next().ok_or("dump: missing height")?.parse()?;
    let body = bytes.get(newline + 1..).ok_or("dump: truncated")?;
    let expected = width.checked_mul(height).and_then(|n| n.checked_mul(12)).ok_or("dump: dimensions overflow")?;
    if body.len() != expected {
        return Err(format!("dump: {} bytes, expected {expected}", body.len()).into());
    }
    let mut data = Vec::with_capacity(width * height);
    for px in body.as_chunks::<12>().0 {
        let mut out = [0f32; 3];
        for (v, ch) in out.iter_mut().zip(px.as_chunks::<4>().0) {
            *v = f32::from_le_bytes(*ch);
        }
        data.push(out);
    }
    Ok((width, height, data))
}

/// The A/B report: bit-level difference, |delta| distribution and the visual proxy (8-bit sRGB codes).
fn compare_dumps(rgb: &Rgb32f, width: usize, height: usize, other: &[[f32; 3]]) -> Result<(), Box<dyn std::error::Error>> {
    if (rgb.width, rgb.height) != (width, height) {
        return Err(format!("compare: dump is {width}x{height}, this run is {}x{}", rgb.width, rgb.height).into());
    }
    let mut deltas = Vec::with_capacity(rgb.data.len() * 3);
    let mut different = 0usize;
    let mut max = 0f32;
    let mut max_at = (0usize, 0usize, 0usize);
    let mut codes_changed = 0usize;
    for (i, (a, b)) in rgb.data.iter().zip(other).enumerate() {
        for c in 0..3 {
            let (va, vb) = (a[c], b[c]);
            let delta = (va - vb).abs();
            if va.to_bits() != vb.to_bits() {
                different += 1;
            }
            if delta > max {
                max = delta;
                max_at = (i % width, i / width, c);
            }
            if srgb8(va) != srgb8(vb) {
                codes_changed += 1;
            }
            deltas.push(delta);
        }
    }
    let total = deltas.len();
    if total == 0 {
        return Err("compare: empty frame".into());
    }
    let middle = total / 2;
    let median = *deltas.select_nth_unstable_by(middle, f32::total_cmp).1;
    let at = rgb.data[max_at.1 * width + max_at.0];
    let relative = max / at[max_at.2].abs().max(1e-12);
    println!(
        "compare: {total} samples, {different} differ ({:.2} %), median |delta| {median:.2e}, max {max:.2e} at ({},{}) channel {} (relative {relative:.1e})",
        different as f64 * 100.0 / total as f64,
        max_at.0,
        max_at.1,
        max_at.2
    );
    println!("compare: sRGB-proxy 8-bit codes changed {codes_changed} ({:.3} %)", codes_changed as f64 * 100.0 / total as f64);
    Ok(())
}

/// sRGB transfer function on the linear sample, quantised to 8 bits: a display proxy, not a measure
/// of the final render (no white balance, matrix or tone curve).
fn srgb8(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_smaller_than_the_requested_window_do_not_read_past_the_frame() {
        let file = std::env::temp_dir().join(format!("lightcraft-xtrans-crop-{}.ppm", std::process::id()));
        write_crop(&Rgb32f::filled(32, 24, [0.3; 3]), usize::MAX, usize::MAX, file.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(&file);
        std::fs::remove_file(&file).unwrap();
        let bytes = bytes.unwrap();
        let header = b"P6\n32 24\n255\n";
        assert!(bytes.starts_with(header));
        assert_eq!(bytes.len(), header.len() + 32 * 24 * 3);
    }

    #[test]
    fn malformed_crops_return_an_error_before_writing() {
        assert!(write_crop(&Rgb32f::new(0, 0), 0, 0, "").is_err());
        let invalid = Rgb32f { width: usize::MAX, height: 24, data: vec![] };
        assert!(write_crop(&invalid, 0, 0, "").is_err());
    }
}
