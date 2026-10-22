//! Sweep X-Trans raws using the same versioned metrics as `xtrans_quality`.

#[path = "support/xtrans_metrics.rs"]
mod metrics;

use lightcraft_raw::Method;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return Err("usage: noise_sweep FILE1.RAF [FILE2.RAF ...]".into());
    }
    println!("metrics version {}", metrics::VERSION);
    println!("File ISO Shot(a) Read(b) Sig(0.5) Anscombe NormalizeMs DemosaicMs FlatNoise GreenHF Edge Texture ChromaNoise ChromaHF");
    let mut failures = 0usize;
    for path in &args {
        if let Err(e) = measure_file(path) {
            eprintln!("{}: {e}", path);
            failures += 1;
        }
    }
    if failures > 0 {
        return Err(format!("{failures} input(s) failed").into());
    }
    Ok(())
}

fn measure_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let raw = lightcraft_raw::decode(&std::fs::read(path)?)?;
    if raw.cpp != 1 || !raw.cfa.as_ref().is_some_and(|c| c.width == 6 && c.height == 6) {
        return Err("not a single-channel X-Trans raw".into());
    }
    let started = std::time::Instant::now();
    let norm = raw.normalized()?;
    let normalize_ms = started.elapsed().as_secs_f64() * 1e3;
    let cfa = norm.cfa.as_ref().ok_or("no CFA")?;
    let model = lightcraft_raw::noise::estimate(&norm.data, norm.width, norm.height, cfa);
    let (a, b, sigma, usable) = model.map_or_else(
        || ("NA".to_string(), "NA".to_string(), "NA".to_string(), "NO"),
        |m| (format!("{:.2e}", m.a), format!("{:.2e}", m.b), format!("{:.2e}", m.sigma(0.5)), if m.is_usable() { "YES" } else { "NO" }),
    );
    let started = std::time::Instant::now();
    let rgb = lightcraft_raw::demosaic(&norm, Method::Ahd);
    let demosaic_ms = started.elapsed().as_secs_f64() * 1e3;
    let m = metrics::measure(&rgb)?;
    println!(
        "{} {} {a} {b} {sigma} {usable} {normalize_ms:.2} {demosaic_ms:.2} {} {} {:.4e} {:.4e} {} {}",
        Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path),
        raw.metadata.iso.map_or_else(|| "NA".to_string(), |iso| iso.to_string()),
        metrics::value(m.flat.map(|v| v.0)),
        metrics::value(m.green_hf),
        m.edge,
        m.texture,
        metrics::value(m.chroma),
        metrics::value(m.chroma_hf),
    );
    Ok(())
}
