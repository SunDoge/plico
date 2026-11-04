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

#[test]
fn glyph_layout_geometry_preserves_utf8_utf16_and_dpi_boundaries() {
    use plico_native::text::atlas::Atlas;
    let atlas = Atlas::new().unwrap();
    for scale in [1., 1.25, 1.5, 2.] {
        let mut request = TextRequest::new("A👩‍💻中文 e\u{301}\nمرحبا بالعالم");
        request.size = 16.;
        request.scale = scale;
        let layout = atlas.layout(&request).unwrap();
        assert_eq!(layout.text(), request.text);
        for byte in (0..=request.text.len()).filter(|b| request.text.is_char_boundary(*b)) {
            let utf16 = layout.utf16_from_byte(byte).unwrap();
            assert_eq!(layout.byte_from_utf16(utf16), Some(byte));
            let caret = layout.caret_rect(byte).unwrap();
            assert!(caret.iter().all(|v| v.is_finite()));
            assert!(caret[3] > 0.);
            let hit = layout.hit_test(caret[0], caret[1] + caret[3] / 2.).unwrap();
            assert!(request.text.is_char_boundary(hit));
        }
        assert_eq!(
            layout.byte_from_utf16(2),
            None,
            "half of an emoji surrogate was accepted"
        );
        assert!(layout.utf16_from_byte(2).is_none());
        assert!(layout.caret_rect(2).is_none());
        assert!(layout.hit_test(f64::NAN, 0.).is_none());
        let chinese = request.text.find('中').unwrap();
        let end = chinese + "中文".len();
        let rects = layout.selection_rects(chinese..end).unwrap();
        assert!(!rects.is_empty());
        for r in rects {
            assert!(r[2] > 0. && r[3] > 0.);
        }
        assert!(layout.selection_rects(2..end).is_none());
        assert!(layout.selection_rects(0..0).unwrap().is_empty());
        assert!(layout.selection_rects(0..request.text.len()).unwrap().len() >= 2);
        let end = request.text.len();
        let caret = layout.caret_rect(end).unwrap();
        assert_eq!(layout.ime_candidate_rect(end, None), Some(caret));
        let candidate = layout.ime_candidate_rect(end, Some(0..end)).unwrap();
        assert_eq!(
            candidate[1], caret[1],
            "candidate moved to another visual line"
        );
        assert!(layout.ime_candidate_rect(end, Some(2..end)).is_none());
    }
}

#[test]
fn native_layout_options_align_space_and_ellipsize_at_the_target_scale() {
    use plico_native::text::atlas::{Alignment, LayoutContext, LayoutOptions};
    let context = LayoutContext::new().unwrap();
    for scale in [1., 1.25, 2.] {
        let mut request = TextRequest::new("Small aligned text");
        request.size = 16.;
        request.scale = scale;
        request.width = Some(240.);
        let start = context.layout(&request).unwrap();
        let center = context
            .layout_styled(
                &request,
                LayoutOptions {
                    alignment: Alignment::Center,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(center.glyphs[0].x > start.glyphs[0].x + 20. * scale);
        let spaced = context
            .layout_styled(
                &request,
                LayoutOptions {
                    letter_spacing: 2.,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(spaced.intrinsic_widths()[1] > start.intrinsic_widths()[1] + 10.);
        request.text = "one\ntwo";
        let tall = context
            .layout_styled(
                &request,
                LayoutOptions {
                    line_height: Some(32.),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(
            (tall.logical_size[1] - 64.).abs() < 1.1,
            "line height at {scale}: {:?}",
            tall.logical_size
        );
        request.text = "A long label that needs truncation";
        request.width = Some(60.);
        let ellipsis = context
            .layout_styled(
                &request,
                LayoutOptions {
                    ellipsis: true,
                    max_lines: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(ellipsis.line_metrics().len(), 1);
        assert!(ellipsis.intrinsic_widths()[1] > ellipsis.logical_size[0]);
        request.width = Some(0.);
        assert!(context.layout(&request).is_ok());
        assert!(
            context
                .layout_styled(
                    &request,
                    LayoutOptions {
                        letter_spacing: f64::NAN,
                        ..Default::default()
                    }
                )
                .is_err()
        );
    }
}
