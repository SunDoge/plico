#![cfg(all(target_os = "linux", feature = "linux-text"))]
use plico_native::text::{Error, SystemText, TextRequest};
use std::sync::Arc;

#[test]
fn cached_layout_metrics_and_pixels_have_one_source_of_truth() {
    let mut system = SystemText::new(1024 * 1024).unwrap();
    assert!(!system.font_families().is_empty());
    let mut request = TextRequest::new("Sphinx ffi 0123456789");
    let a = system.rasterize(&request).unwrap();
    assert!(Arc::ptr_eq(&a, &system.rasterize(&request).unwrap()));
    assert_eq!(
        a.rgba.len(),
        a.pixels[0] as usize * a.pixels[1] as usize * 4
    );
    assert!(a.baseline > 0. && a.baseline <= a.logical_size[1]);
    assert_eq!(a.lines, 1);
    assert_eq!(a.missing_glyphs, 0);
    request.scale = 2.;
    let b = system.rasterize(&request).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
    assert!(b.pixels[1] > a.pixels[1]);
    assert!((b.logical_size[1] - a.logical_size[1]).abs() <= 2.);
    request.width = Some(60.);
    let wrapped = system.rasterize(&request).unwrap();
    assert!(wrapped.lines > 1);
    assert!(wrapped.logical_size[0] <= 60.);
    system.clear();
    assert_eq!(system.cached_bytes(), 0);
    assert!(a.rgba.iter().any(|&b| b != 0));
}

#[test]
fn transparent_colored_text_is_straight_alpha_and_empty_text_keeps_metrics() {
    let mut system = SystemText::new(0).unwrap();
    let mut request = TextRequest::new("SSS");
    request.color = [255, 0, 0, 128];
    let bitmap = system.rasterize(&request).unwrap();
    let ink: Vec<_> = bitmap
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[3] > 0)
        .collect();
    assert!(!ink.is_empty());
    for p in ink {
        assert_eq!(p[0], 255);
        assert_eq!(p[1], 0);
        assert_eq!(p[2], 0);
        assert!(p[3] <= 128);
    }
    for text in ["", "   "] {
        let bitmap = system.rasterize(&TextRequest::new(text)).unwrap();
        assert!(bitmap.logical_size[1] > 0.);
        assert!(bitmap.rgba.iter().all(|&b| b == 0));
    }
    assert_eq!(system.cached_bytes(), 0);
}

#[test]
fn unicode_overhangs_and_cache_eviction_preserve_caller_owned_outputs() {
    let mut system = SystemText::new(32768).unwrap();
    let request = TextRequest::new("j\u{301} 中文 العربية 👩‍💻");
    let a = system.rasterize(&request).unwrap();
    for size in [12., 20., 40., 60.] {
        let mut request = request.clone();
        request.italic = true;
        request.size = size;
        let b = system.rasterize(&request).unwrap();
        assert!(b.rgba.iter().any(|&b| b != 0));
        assert!(system.cached_bytes() <= 32768);
    }
    assert!(a.rgba.iter().any(|&b| b != 0));
    // Only owned pixels cross threads, never a Pango/Cairo context.
    std::thread::spawn(move || assert!(!a.rgba.is_empty()))
        .join()
        .unwrap();
}

#[test]
fn rejects_bad_sizes_and_inputs_before_native_calls() {
    let mut system = SystemText::new(0).unwrap();
    for size in [f64::NAN, f64::INFINITY, -1., 0., 100000.] {
        let mut r = TextRequest::new("x");
        r.size = size;
        assert!(matches!(system.rasterize(&r), Err(Error::InvalidRequest)));
    }
    for text in ["a\0b", &"x".repeat(65537)] {
        assert!(matches!(
            system.rasterize(&TextRequest::new(text)),
            Err(Error::InvalidRequest)
        ));
    }
    let mut r = TextRequest::new("x");
    r.width = Some(f64::NAN);
    assert!(matches!(system.rasterize(&r), Err(Error::InvalidRequest)));
}
