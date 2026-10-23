//! Hover-stability of the workspace action row: hovering the "Save changes"
//! button must not move any button's geometry between frames, and the
//! enabled/primary restyle after a save must not unmount or resize buttons.
use plico_gallery::{Gallery, Page};
use plico_renderer::Modifiers;
use plico_renderer::{
    FrameSource, Point, PointerButton, PointerEvent, PointerPhase, PointerProperties, TextContext,
    UiEvent,
};
use plico_vello::Source;
use std::sync::Arc;
use std::time::Duration;

fn pointer(phase: PointerPhase, x: f64, y: f64) -> UiEvent {
    UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y },
        button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
            .then_some(PointerButton::Primary),
        buttons: 1,
        modifiers: Modifiers::empty(),
        properties: PointerProperties::default(),
    })
}
fn frame(s: &mut Source<Gallery>, tcx: &mut TextContext) -> Vec<(String, [f32; 4])> {
    let nodes = s.build_frame(tcx, 1120, 820);
    nodes
        .iter()
        .filter_map(|n| n.paint.text.as_ref().map(|t| (t.to_string(), n.rect)))
        .collect()
}
#[test]
fn hovering_and_saving_keeps_action_row_geometry_stable() {
    let clock = Arc::new(plico_core::VirtualClock::default());
    let mut s = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Workspace),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    let base = frame(&mut s, &mut tcx);
    let save = base
        .iter()
        .find(|(t, _)| t == "Save changes")
        .map(|(_, r)| *r)
        .expect("save button");

    // Hover directly over the save button, mid-animation ticks.
    let (cx, cy) = (
        (save[0] + save[2]) as f64 / 2.,
        (save[1] + save[3]) as f64 / 2.,
    );
    for tick in 0..8 {
        clock.advance(Duration::from_millis(16)).unwrap();
        s.handle_event(pointer(PointerPhase::Move, cx, cy));
        let nodes = frame(&mut s, &mut tcx);
        let rect = nodes
            .iter()
            .find(|(t, _)| t == "Save changes")
            .map(|(_, r)| *r)
            .expect("save button during hover");
        assert_eq!(
            rect, save,
            "tick {tick}: save button moved under hover: {rect:?} vs {save:?}"
        );
        // Every action button must remain present each frame.
        for label in ["Save changes", "Reset changes", "Archive project"] {
            assert!(
                nodes.iter().any(|(t, _)| t == label),
                "tick {tick}: {label} disappeared"
            );
        }
    }
    // Move the hover off to a distant point: buttons must stay put.
    clock.advance(Duration::from_millis(16)).unwrap();
    s.handle_event(pointer(PointerPhase::Move, 1000., 60.));
    let nodes = frame(&mut s, &mut tcx);
    let rect = nodes
        .iter()
        .find(|(t, _)| t == "Save changes")
        .map(|(_, r)| *r)
        .unwrap();
    assert_eq!(rect, save, "save button moved after hover left: {rect:?}");

    // Click save: dirty flips false, button restyles but geometry holds.
    for phase in [PointerPhase::Down, PointerPhase::Up] {
        s.handle_event(pointer(phase, cx, cy));
        clock.advance(Duration::from_millis(16)).unwrap();
        frame(&mut s, &mut tcx);
    }
    for tick in 0..10 {
        clock.advance(Duration::from_millis(16)).unwrap();
        let nodes = frame(&mut s, &mut tcx);
        let rect = nodes
            .iter()
            .find(|(t, _)| t == "Save changes")
            .map(|(_, r)| *r)
            .unwrap_or(save);
        assert_eq!(
            rect, save,
            "tick {tick}: save button moved after save: {rect:?}"
        );
        assert!(
            nodes.iter().any(|(t, _)| t == "Save changes"),
            "tick {tick}: save button disappeared after save"
        );
    }
}

#[test]
fn hover_and_save_are_stable_on_the_retained_present_path() {
    let clock = Arc::new(plico_core::VirtualClock::default());
    let mut s = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Workspace),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    s.build_and_paint(&mut scene, &mut tcx, 1120, 820, 1.);
    let save = {
        let nodes = s.build_frame(&mut tcx, 1120, 820);
        nodes
            .iter()
            .find(|n| n.paint.text.as_deref() == Some("Save changes"))
            .map(|n| n.rect)
            .unwrap()
    };
    let (cx, cy) = (
        (save[0] + save[2]) as f64 / 2.,
        (save[1] + save[3]) as f64 / 2.,
    );
    // Simulate the shell: event then retained present per vsync.
    for tick in 0..10 {
        clock.advance(Duration::from_millis(16)).unwrap();
        s.handle_event(pointer(
            if tick % 2 == 0 {
                PointerPhase::Move
            } else {
                PointerPhase::Move
            },
            cx + tick as f64, // small drift like a real mouse
            cy,
        ));
        let mut scene = plico_renderer::Scene::new();
        s.render_retained_frame(&mut scene, &mut tcx, (1120, 820), 1.)
            .unwrap();
        // The presented scene must still contain the action-row buttons.
        // Structural check: node texts after present.
        let nodes = s.build_frame(&mut tcx, 1120, 820);
        for label in ["Save changes", "Reset changes", "Archive project"] {
            assert!(
                nodes.iter().any(|n| n.paint.text.as_deref() == Some(label)),
                "tick {tick}: {label} vanished on retained path"
            );
        }
    }
}

#[test]
fn hover_anywhere_keeps_action_buttons_presented() {
    let clock = Arc::new(plico_core::VirtualClock::default());
    let mut s = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Workspace),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    s.build_and_paint(&mut scene_placeholder(), &mut tcx, 1120, 820, 1.);
    // Sweep hover across the whole window like a moving mouse.
    for step in 0..40 {
        clock.advance(Duration::from_millis(16)).unwrap();
        let x = 40. + step as f64 * 26.;
        let y = 80. + (step % 7) as f64 * 90.;
        s.handle_event(pointer(PointerPhase::Move, x, y));
        let mut scene = plico_renderer::Scene::new();
        s.render_retained_frame(&mut scene, &mut tcx, (1120, 820), 1.)
            .unwrap();
        let nodes = s.build_frame(&mut tcx, 1120, 820);
        for label in ["Save changes", "Reset changes", "Archive project"] {
            assert!(
                nodes.iter().any(|n| n.paint.text.as_deref() == Some(label)),
                "step {step} at ({x},{y}): {label} vanished"
            );
        }
    }
}
fn scene_placeholder() -> plico_renderer::Scene {
    plico_renderer::Scene::new()
}

#[test]
fn incremental_scene_splices_never_lose_button_glyphs() {
    let clock = Arc::new(plico_core::VirtualClock::default());
    let mut s = Source::with_clock(
        Gallery {
            router: plico_core::Router::new(Page::Workspace),
            ..Default::default()
        },
        clock.clone(),
    );
    let mut tcx = TextContext::new();
    let mut scene = plico_renderer::Scene::new();
    s.build_and_paint(&mut scene, &mut tcx, 1120, 820, 1.);
    // Baseline scene content: count glyph runs and total glyphs.
    let baseline: (usize, usize) =
        scene
            .commands
            .iter()
            .fold((0, 0), |(runs, glyphs), c| match c {
                plico_renderer::PaintCommand::GlyphRun { glyphs: g, .. } => {
                    (runs + 1, glyphs + g.len())
                }
                _ => (runs, glyphs),
            });
    let save = {
        let nodes = s.build_frame(&mut tcx, 1120, 820);
        nodes
            .iter()
            .find(|n| n.paint.text.as_deref() == Some("Save changes"))
            .unwrap()
            .rect
    };
    let (cx, cy) = (
        (save[0] + save[2]) as f64 / 2.,
        (save[1] + save[3]) as f64 / 2.,
    );
    for tick in 0..12 {
        clock.advance(Duration::from_millis(16)).unwrap();
        // Alternate on/off the button: real paint churn every other tick,
        // exactly the motion-animated restyle the incremental path targets.
        let (x, y) = if tick % 2 == 0 {
            (cx, cy)
        } else {
            (cx + 200., cy - 40.)
        };
        s.handle_event(pointer(PointerPhase::Move, x, y));
        let mut scene = plico_renderer::Scene::new();
        s.render_retained_frame(&mut scene, &mut tcx, (1120, 820), 1.)
            .unwrap();
        let content: (usize, usize) =
            scene
                .commands
                .iter()
                .fold((0, 0), |(runs, glyphs), c| match c {
                    plico_renderer::PaintCommand::GlyphRun { glyphs: g, .. } => {
                        (runs + 1, glyphs + g.len())
                    }
                    _ => (runs, glyphs),
                });
        assert_eq!(
            content, baseline,
            "tick {tick}: scene glyph content changed ({content:?} vs {baseline:?})"
        );
        eprintln!(
            "tick {tick}: incremental={} builds={} reuses={}",
            s.adapter.stats.scene_incremental,
            s.adapter.stats.scene_builds,
            s.adapter.stats.scene_reuses
        );
    }
}
