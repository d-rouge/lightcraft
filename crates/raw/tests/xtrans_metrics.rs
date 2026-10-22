#[path = "../examples/support/xtrans_metrics.rs"]
mod metrics;

use lightcraft_raw::Rgb32f;

#[test]
fn detects_period_two_stripes_and_checkerboards_in_every_phase() {
    for checker in [false, true] {
        for phase in [0, 1] {
            let rgb = Rgb32f::from_fn(64, 64, |x, y| {
                let sign = if (x + if checker { y } else { 0 } + phase) % 2 == 0 { 1.0 } else { -1.0 };
                [0.35 + 0.05 * sign; 3]
            });
            let measured = metrics::measure(&rgb).unwrap();
            let expected = if checker { 0.1 / (0.35f64.powi(2) + 0.05f64.powi(2)).sqrt() } else { 0.05 / (0.35f64.powi(2) + 0.05f64.powi(2)).sqrt() };
            assert!((measured.green_hf.unwrap() - expected).abs() < 1e-6);
        }
    }
}

#[test]
fn opposite_chroma_errors_do_not_cancel() {
    let rgb = Rgb32f::from_fn(400, 400, |x, y| {
        let error = if (x + y) % 2 == 0 { 0.05 } else { -0.05 };
        [0.35 + error, 0.35, 0.35 - error]
    });
    let measured = metrics::measure(&rgb).unwrap();
    assert!((measured.chroma.unwrap() - 0.4).abs() < 1e-6);
    assert!((measured.chroma_hf.unwrap() - 0.1 / 0.35).abs() < 1e-6);
    assert!(measured.green_hf.unwrap() < 1e-6);
}

#[test]
fn constants_are_quiet_and_missing_windows_are_not_zero_scores() {
    assert_eq!(metrics::VERSION, 2);
    for level in [0.0, 0.35] {
        let measured = metrics::measure(&Rgb32f::filled(400, 400, [level; 3])).unwrap();
        assert_eq!(measured.green_hf, (level > 0.0).then_some(0.0));
        assert_eq!(measured.chroma_hf, (level > 0.0).then_some(0.0));
        assert_eq!(measured.edge, 0.0);
        assert_eq!(measured.texture, 0.0);
        assert_eq!(measured.chroma, Some(0.0));
        assert_eq!(measured.flat.is_some(), level > 0.0);
    }
    let small = metrics::measure(&Rgb32f::filled(32, 32, [0.35; 3])).unwrap();
    assert!(small.flat.is_none() && small.chroma.is_none());
    assert_eq!(metrics::value(None), "NA");
}

#[test]
fn malformed_or_non_finite_frames_are_errors() {
    for rgb in [
        Rgb32f::new(0, 0),
        Rgb32f { width: usize::MAX, height: 64, data: vec![] },
        Rgb32f { width: 64, height: 64, data: vec![] },
        Rgb32f::filled(64, 64, [f32::NAN; 3]),
    ] {
        assert!(metrics::measure(&rgb).is_err());
    }
}
