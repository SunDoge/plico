use super::*;
use plico::Theme;
use std::time::Duration;
struct Counting {
    gallery: gallery::Gallery,
    calls: usize,
}
impl HostView for Counting {
    fn show(&mut self, ui: &mut plico::UiContext<'_>) {
        self.calls += 1;
        plico_dev::App::show(&mut self.gallery, ui)
    }
    fn theme(&self) -> Theme {
        plico_dev::App::theme(&self.gallery)
    }
}
#[test]
fn animated_theme_reuses_scene_and_accepts_events_without_stale_geometry() {
    let mut s = Source::new(Counting {
        gallery: gallery::Gallery {
            router: plico::Router::new(gallery::Page::Controls),
            ..Default::default()
        },
        calls: 0,
    });
    let mut tcx = TextContext::new();
    let now = Instant::now();
    let mut scene = Scene::new();
    let nodes = s.build_frame(&mut tcx, 920, 780);
    s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
    s.transition
        .compose(&mut scene, s.engine.theme(), (920, 780, 1.), now);
    s.view.gallery.dark = true;
    let nodes = s.build_frame(&mut tcx, 920, 780);
    let rects: Vec<_> = nodes.iter().map(|n| n.rect).collect();
    s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
    s.transition
        .compose(&mut scene, s.engine.theme(), (920, 780, 1.), now);
    assert!(s.has_anim());
    let calls = s.view.calls;
    let mut frames = Vec::new();
    for ms in [0, 100, 200] {
        let nodes = s.build_frame(&mut tcx, 920, 780);
        assert_eq!(s.view.calls, calls, "animation must not re-declare app");
        assert_eq!(nodes.iter().map(|n| n.rect).collect::<Vec<_>>(), rects);
        s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
        s.transition.compose(
            &mut scene,
            s.engine.theme(),
            (920, 780, 1.),
            now + Duration::from_millis(ms),
        );
        frames.push(scene.clone());
    }
    assert!(!s.has_anim());
    assert_ne!(frames[0], frames[1]);
    assert_ne!(frames[1], frames[2]);
    if let Ok(directory) = std::env::var("PLICO_THEME_REPORT_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        for (ms, scene) in [0, 100, 200].into_iter().zip(&frames) {
            plico_shell::renderer::render_to_png(
                scene,
                920,
                780,
                s.base_color(),
                &format!("{directory}/{ms}.png"),
            )
            .unwrap();
        }
    }
    // Start another transition, then commit input before its next vsync.
    s.view.gallery.dark = false;
    let nodes = s.build_frame(&mut tcx, 920, 780);
    s.paint_scene(&mut scene, &nodes, &mut tcx, (920, 780), 1.);
    s.transition.compose(
        &mut scene,
        s.engine.theme(),
        (920, 780, 1.),
        now + Duration::from_millis(201),
    );
    let target = s
        .adapter
        .semantics(&s.engine, 0)
        .node_by_role(plico_renderer::SemanticRole::Button, "Increment", None)
        .unwrap()
        .id;
    assert!(s.handle_semantic_action(plico_renderer::SemanticAction::Click { target }));
    s.prepare_for_event(&mut tcx);
    let nodes = s.build_frame(&mut tcx, 920, 780);
    assert!(!s.declarations_reused);
    assert_eq!(s.view.gallery.count, 1);
    let expected = gallery::i18n::count_message(s.view.gallery.language, "clicked-count", 1);
    assert!(
        nodes
            .iter()
            .any(|n| n.paint.text.as_deref() == Some(expected.as_str())),
        "updated caption missing: expected {expected:?}"
    );
}

#[test]
fn declaration_errors_can_be_observed_or_transferred_without_exposing_storage() {
    struct DuplicateKeys;

    impl HostView for DuplicateKeys {
        fn show(&mut self, ui: &mut plico::UiContext<'_>) {
            ui.text("First").key("duplicate");
            ui.text("Second").key("duplicate");
        }
    }

    let mut source = Source::new(DuplicateKeys);
    let mut text = TextContext::new();
    source.build_frame(&mut text, 320, 200);

    assert!(source.last_error().is_some());
    assert!(source.take_error().is_some());
    assert!(source.last_error().is_none());
}
