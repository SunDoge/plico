use plico::{Engine, VisibleRegion, prelude::*};

#[test]
fn clipping_handles_empty_touching_and_unknown_bounds_conservatively() {
    let viewport = VisibleRegion::from_rect([10., 20., 110., 220.]);
    assert!(viewport.intersects([0., 0., 11., 21.]));
    assert!(!viewport.intersects([0., 0., 10., 20.]));
    assert!(!viewport.intersects([30., 30., 30., 40.]));
    assert!(viewport.intersects([f32::NAN, 0., 1., 1.]));
    assert_eq!(viewport.intersect(VisibleRegion::UNBOUNDED), viewport);
    assert_eq!(
        viewport.intersect(VisibleRegion::EMPTY),
        VisibleRegion::EMPTY
    );
    assert_eq!(
        viewport
            .intersect(VisibleRegion::from_rect([20., 30., 50., 60.]))
            .bounds(),
        Some([20., 30., 50., 60.])
    );
    assert!(!VisibleRegion::EMPTY.intersects([f32::NAN; 4]));
    assert_eq!(
        VisibleRegion::from_rect([0., 0., f32::INFINITY, 1.]),
        VisibleRegion::UNBOUNDED
    );
    assert_eq!(VisibleRegion::from_size([0., 100.]), VisibleRegion::EMPTY);
    assert_eq!(
        viewport.overscan([5., 10.]).bounds(),
        Some([5., 10., 115., 230.])
    );
}

#[test]
fn transformed_viewports_support_pan_zoom_rotation_and_singular_fallback() {
    let viewport = VisibleRegion::from_size([100., 200.]);
    assert_eq!(
        viewport.to_local([2., 0., 0., 2., -20., -40.]).bounds(),
        Some([10., 20., 60., 120.])
    );
    assert_eq!(
        viewport.to_local([0., 1., -1., 0., 0., 0.]).bounds(),
        Some([0., -100., 200., 0.])
    );
    assert_eq!(viewport.to_local([0.; 6]), VisibleRegion::UNBOUNDED);
    assert_eq!(viewport.to_local([f32::NAN; 6]), VisibleRegion::UNBOUNDED);
    assert_eq!(VisibleRegion::EMPTY.to_local([0.; 6]), VisibleRegion::EMPTY);
    // Rotated AABBs may include extra content but must contain all inverse corners.
    for [a, b, c, d, tx, ty] in [
        [0.8_f32, 0.6, -0.6, 0.8, 12.5, -7.25],
        [1., 0.25, 0.5, 1., -3., 2.],
    ] {
        let local = viewport
            .to_local([a, b, c, d, tx, ty])
            .bounds()
            .unwrap()
            .map(f64::from);
        let [a, b, c, d, tx, ty] = [a, b, c, d, tx, ty].map(f64::from);
        for [x, y] in [[0., 0.], [100., 0.], [0., 200.], [100., 200.]] {
            let u = (d * (x - tx) - c * (y - ty)) / (a * d - b * c);
            let v = (a * (y - ty) - b * (x - tx)) / (a * d - b * c);
            assert!(local[0] <= u && local[2] >= u && local[1] <= v && local[3] >= v);
        }
    }
}

#[test]
fn fixed_rows_and_columns_are_bounded_without_scanning_the_collection() {
    let viewport = VisibleRegion::from_rect([200., 1000., 500., 1100.]);
    assert_eq!(viewport.rows(1_000_000, 20., 2), 48..57);
    assert_eq!(viewport.columns(1_000_000, 100., 1), 1..6);
    assert_eq!(
        VisibleRegion::from_rect([0., 20., 100., 40.]).rows(100, 20., 0),
        1..2
    );
    assert_eq!(
        VisibleRegion::from_rect([0., -100., 100., -1.]).rows(100, 20., 2),
        0..0
    );
    assert_eq!(viewport.rows(10, 20., 2), 0..0);
    assert_eq!(viewport.rows(0, 20., 2), 0..0);
    assert_eq!(viewport.rows(100, f32::NAN, 2), 0..100);
    assert_eq!(VisibleRegion::UNBOUNDED.rows(100, 20., 2), 0..100);
    assert_eq!(viewport.rows(100, 20., usize::MAX), 0..100);
}

#[test]
fn million_cell_grid_declares_only_visible_cells_and_preserves_business_keys() {
    let mut engine = Engine::default();
    let mut original = None;
    for offset in [1000., 1020.] {
        let region = VisibleRegion::from_rect([200., offset, 500., offset + 100.]);
        let mut built = 0;
        let stats = engine
            .frame_declarations(|cx| {
                for y in region.rows(1000, 20., 0) {
                    for x in region.columns(1000, 100., 0) {
                        text(cx, "cell").key((y * 1000 + x) as u64);
                        built += 1;
                    }
                }
            })
            .unwrap();
        assert_eq!(built, 15);
        assert_eq!(stats.visited, 15);
        let kept = engine.find(engine.root(), 51_002_u64).unwrap();
        if let Some(original) = original {
            assert_eq!(kept, original);
            assert!(
                engine.find(engine.root(), 50_002_u64).is_none(),
                "rows leaving visibility must unmount normally"
            );
        }
        original = Some(kept);
    }
}

#[test]
fn virtual_list_uses_scroll_feedback_and_shared_visibility_ranges() {
    let mut engine = Engine::default();
    let mut rendered = 0..0;
    engine
        .frame_declarations(|cx| {
            rendered = virtual_list(cx, 1_000_000, 20.)
                .key("list")
                .height(100.)
                .overscan(1)
                .rows(
                    |i| i as u64,
                    |cx, _| {
                        text(cx, "row");
                    },
                )
                .rendered;
        })
        .unwrap();
    assert_eq!(rendered, 0..6);
    let list = engine.find(engine.root(), "list").unwrap();
    assert!(engine.observes_scroll(list));
    engine.set_scroll_viewport(list, 1000., 100.);
    engine
        .frame_declarations(|cx| {
            rendered = virtual_list(cx, 1_000_000, 20.)
                .key("list")
                .height(100.)
                .overscan(1)
                .rows(
                    |i| i as u64,
                    |cx, _| {
                        text(cx, "row");
                    },
                )
                .rendered;
        })
        .unwrap();
    assert_eq!(rendered, 49..56);
}
