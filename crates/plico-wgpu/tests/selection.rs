#![cfg(all(feature = "system-text", target_os = "linux"))]
use plico::VirtualClock;
use plico::prelude::*;
use plico_renderer::{
    ClipboardRequest, FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, Point,
    PointerButton, PointerEvent, PointerPhase, PointerProperties, Scene, TextContext, UiEvent,
};
use plico_runtime::{HostView, Source};
use std::time::Duration;
struct Demo {
    shows: usize,
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        self.shows += 1;
        column(cx).padding(20).selectable(true).children(|cx| {
            text(cx, "one two three\n中文与日本語 · e\u{301} · 👩‍💻").font_size(20);
        });
    }
}
#[test]
#[ignore = "requires a working wgpu adapter and Pango system fonts"]
fn native_word_selection_highlights_and_copies_at_fractional_dpi() {
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    for scale in [1., 1.25, 2.] {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(Demo { shows: 0 }, clock.clone());
        let mut text = TextContext::new();
        renderer.configure_text(&mut text, scale).unwrap();
        let mut scene = Scene::new();
        let size = [(400. * scale) as u32, (180. * scale) as u32];
        source
            .render_retained_frame(&mut scene, &mut text, (size[0], size[1]), scale)
            .unwrap();
        let before = renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        let nodes = source.build_frame(&mut text, size[0], size[1]);
        let node = nodes
            .iter()
            .find(|n| {
                n.paint
                    .text
                    .as_deref()
                    .is_some_and(|s| s.starts_with("one two"))
            })
            .unwrap();
        let layout = plico_renderer::scene::layout_node_paragraph(&mut text, node).unwrap();
        assert!(
            !layout.selection_rects(4..7).is_empty(),
            "native paragraph returned no selection geometry"
        );
        assert!(
            layout
                .selection_rects(4..7)
                .iter()
                .all(|r| r[2] > r[0] && r[3] > r[1]),
            "selection rectangle used width/height as right/bottom"
        );
        let caret = layout.caret_rect(5).unwrap();
        let position = Point {
            x: f64::from(node.content_origin[0] + caret[0]) + 0.1,
            y: f64::from(node.content_origin[1] + (caret[1] + caret[3]) * 0.5),
        };
        let shows = source.view().shows;
        for _ in 0..2 {
            for phase in [PointerPhase::Down, PointerPhase::Up] {
                source.handle_event(UiEvent::Pointer(PointerEvent {
                    phase,
                    position,
                    button: Some(PointerButton::Primary),
                    buttons: u32::from(phase == PointerPhase::Down),
                    modifiers: Modifiers::empty(),
                    properties: PointerProperties::default(),
                }));
            }
            clock.advance(Duration::from_millis(100)).unwrap();
        }
        assert_eq!(
            source.selected_text().as_deref(),
            Some("two"),
            "scale={scale}"
        );
        source
            .render_retained_frame(&mut scene, &mut text, (size[0], size[1]), scale)
            .unwrap();
        let after = renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        let backend = source.frame_diagnostics().backend;
        assert!(
            before != after,
            "selection did not paint at scale={scale}; visits={} writes={} builds={} reuses={} commands={}",
            backend.selection_visits,
            backend.paint_writes,
            backend.scene_builds,
            backend.scene_reuses,
            scene.flattened().commands.len()
        );
        assert_eq!(
            source.view().shows,
            shows,
            "selection reran application declarations"
        );
        let response = source.handle_event(UiEvent::Key(KeyEvent {
            phase: KeyPhase::Down,
            key: "c".into(),
            code: "KeyC".into(),
            key_without_modifiers: "c".into(),
            text: None,
            text_with_all_modifiers: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::CONTROL,
            repeat: false,
            synthetic: false,
        }));
        assert!(matches!(response.clipboard, Some(ClipboardRequest::Write(s)) if s == "two"));
        source.handle_event(UiEvent::Key(KeyEvent {
            phase: KeyPhase::Down,
            key: "Escape".into(),
            code: "Escape".into(),
            key_without_modifiers: "Escape".into(),
            text: None,
            text_with_all_modifiers: None,
            location: KeyLocation::Standard,
            modifiers: Modifiers::empty(),
            repeat: false,
            synthetic: false,
        }));
        source
            .render_retained_frame(&mut scene, &mut text, (size[0], size[1]), scale)
            .unwrap();
        let cleared = renderer.render_scene(&scene, size, [1.; 4], true).unwrap();
        assert!(
            before == cleared,
            "clearing selection left stale pixels at scale={scale}"
        );
        assert_eq!(source.view().shows, shows);
    }
}
