//! Camera-RGB diagnostics, version 2. Shared by both X-Trans harnesses.
//! High-frequency residuals are broadband Laplacian proxies, not selective Nyquist or fidelity scores.

use lightcraft_raw::Rgb32f;

pub const VERSION: u32 = 2;
const FLAT: usize = 256;
const CHROMA: usize = 128;

pub struct Metrics {
    pub flat: Option<(f64, (usize, usize))>,
    pub green_hf: Option<f64>,
    pub edge: f64,
    pub texture: f64,
    pub chroma: Option<f64>,
    pub chroma_hf: Option<f64>,
}

pub fn measure(rgb: &Rgb32f) -> Result<Metrics, &'static str> {
    let (w, h) = (rgb.width, rgb.height);
    if w.checked_mul(h) != Some(rgb.data.len()) || w <= 16 || h <= 16 {
        return Err("metrics need a complete RGB frame larger than 16x16");
    }
    if rgb.data.iter().flatten().any(|v| !v.is_finite()) {
        return Err("metrics cannot measure non-finite RGB samples");
    }
    let (edge, texture) = gradient_bands(rgb);
    let (green_hf, chroma_hf) = high_frequency(rgb);
    Ok(Metrics { flat: flat_noise(rgb), green_hf, edge, texture, chroma: chroma_noise(rgb), chroma_hf })
}

pub fn value(v: Option<f64>) -> String {
    v.map_or_else(|| "NA".to_string(), |v| format!("{v:.4e}"))
}

pub fn luma(rgb: &Rgb32f, x: usize, y: usize) -> f32 {
    let p = rgb.data[y * rgb.width + x];
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

fn green(rgb: &Rgb32f, x: usize, y: usize) -> f64 {
    f64::from(rgb.data[y * rgb.width + x][1])
}

fn chroma(rgb: &Rgb32f, x: usize, y: usize, channel: usize) -> f64 {
    let p = rgb.data[y * rgb.width + x];
    f64::from(p[channel]) - f64::from(p[1])
}

fn laplacian(at: impl Fn(usize, usize) -> f64, x: usize, y: usize) -> f64 {
    4.0 * at(x, y) - at(x - 1, y) - at(x + 1, y) - at(x, y - 1) - at(x, y + 1)
}

fn flat_noise(rgb: &Rgb32f) -> Option<(f64, (usize, usize))> {
    let (w, h) = (rgb.width, rgb.height);
    let mut values = Vec::new();
    for y in (64..h.saturating_sub(FLAT + 64)).step_by(64) {
        for x in (64..w.saturating_sub(FLAT + 64)).step_by(64) {
            let mut sum = 0.0f64;
            let mut n = 0;
            for yy in (y..y + FLAT).step_by(4) {
                for xx in (x..x + FLAT).step_by(4) {
                    sum += f64::from(luma(rgb, xx, yy));
                    n += 1;
                }
            }
            if !(0.08..0.6).contains(&(sum / f64::from(n))) {
                continue;
            }
            let (mut sum, mut sq, mut n) = (0.0, 0.0, 0);
            for yy in y + 2..y + FLAT - 2 {
                for xx in x + 2..x + FLAT - 2 {
                    let v = laplacian(|x, y| green(rgb, x, y), xx, yy);
                    sum += v;
                    sq += v * v;
                    n += 1;
                }
            }
            let mean = sum / f64::from(n);
            values.push(((sq / f64::from(n) - mean * mean).max(0.0).sqrt(), (x, y)));
        }
    }
    values.sort_by(|a, b| a.0.total_cmp(&b.0));
    values.get(values.len() / 10).copied()
}

fn chroma_noise(rgb: &Rgb32f) -> Option<f64> {
    let (w, h) = (rgb.width, rgb.height);
    let mut values = Vec::new();
    for y in (32..h.saturating_sub(CHROMA + 32)).step_by(32) {
        for x in (32..w.saturating_sub(CHROMA + 32)).step_by(32) {
            let (mut sq, mut n) = (0.0, 0);
            for yy in y..y + CHROMA {
                for xx in x..x + CHROMA {
                    for channel in [0, 2] {
                        sq += laplacian(|x, y| chroma(rgb, x, y, channel), xx, yy).powi(2);
                        n += 1;
                    }
                }
            }
            values.push((sq / f64::from(n)).sqrt());
        }
    }
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 10).copied()
}

fn high_frequency(rgb: &Rgb32f) -> (Option<f64>, Option<f64>) {
    let (mut g, mut c, mut signal) = (0.0, 0.0, 0.0);
    for y in 8..rgb.height - 8 {
        for x in 8..rgb.width - 8 {
            g += (0.25 * laplacian(|x, y| green(rgb, x, y), x, y)).powi(2);
            for channel in [0, 2] {
                c += (0.25 * laplacian(|x, y| chroma(rgb, x, y, channel), x, y)).powi(2);
            }
            signal += green(rgb, x, y).powi(2);
        }
    }
    if signal == 0.0 {
        return (None, None);
    }
    (Some((g / signal).sqrt()), Some((c / (2.0 * signal)).sqrt()))
}

fn gradient_bands(rgb: &Rgb32f) -> (f64, f64) {
    let mut gradients = Vec::new();
    for y in (8..rgb.height - 8).step_by(2) {
        for x in (8..rgb.width - 8).step_by(2) {
            let gx = green(rgb, x + 1, y) - green(rgb, x - 1, y);
            let gy = green(rgb, x, y + 1) - green(rgb, x, y - 1);
            gradients.push(gx.hypot(gy));
        }
    }
    gradients.sort_by(f64::total_cmp);
    let mean = |values: &[f64]| values.iter().sum::<f64>() / values.len().max(1) as f64;
    let n = gradients.len();
    (mean(&gradients[n * 99 / 100..]), mean(&gradients[n / 2..n * 9 / 10]))
}
