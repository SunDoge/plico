use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{
    FrameSource, Modifiers, PaintCommand, Point, PointerButton, PointerEvent, PointerPhase, Scene,
    TextContext, UiEvent,
};
use std::time::Duration;
struct Demo {
    shows: usize,
    clicks: usize,
    clip: bool,
}
impl HostView for Demo {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        column(cx).width(200).height(100).background(Color::Literal(0x36556b)) + |cx| {
            column(cx).absolute(40., 30.).width(120).height(40).clip(self.clip) + |cx| {
                if button(cx).text("Continue").width(120).height(40).radius(20.)
                    .material(LiquidGlass::regular()).clicked()
                {
                    self.clicks += 1;
                }
            };
        };
    }
}
fn frame(source: &mut Source<Demo>, text: &mut TextContext) -> Scene {
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, text, (200, 100), 1.)
        .unwrap();
    assert!(source.last_error.is_none());
    assert_eq!(source.adapter.shader_diagnostics().count(), 0);
    scene
}
fn values(source: &Source<Demo>) -> Vec<f32> {
    source
        .adapter
        .placed
        .iter()
        .filter_map(|n| n.paint.widget.as_ref())
        .flat_map(|s| s.commands.iter())
        .find_map(|c| match c {
            PaintCommand::Shader { effect, .. } => Some(effect.values().to_vec()),
            _ => None,
        })
        .unwrap()
}
fn pointer(source: &mut Source<Demo>, text: &mut TextContext, phase: PointerPhase, x: f64) {
    source.prepare_for_event(text);
    source.handle_event(UiEvent::Pointer(PointerEvent {
        phase,
        position: Point { x, y: 50. },
        button: Some(PointerButton::Primary),
        buttons: u32::from(phase == PointerPhase::Down),
        modifiers: Modifiers::empty(),
        properties: Default::default(),
    }));
}
fn layout(source: &Source<Demo>) -> Vec<[f32; 4]> {
    source.adapter.placed.iter().map(|n| n.rect).collect()
}
#[test]
fn glass_hover_and_press_sample_the_clock_without_redeclaring_or_moving_content() {
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            shows: 0,
            clicks: 0,
            clip: false,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    let bounds = layout(&source);
    pointer(&mut source, &mut text, PointerPhase::Move, 100.);
    frame(&mut source, &mut text);
    let shows = source.view.shows;
    assert_eq!(values(&source)[16], 0.);
    clock.advance(Duration::from_millis(60)).unwrap();
    frame(&mut source, &mut text);
    assert!((values(&source)[16] - 0.5).abs() < 1e-5);
    clock.advance(Duration::from_millis(60)).unwrap();
    frame(&mut source, &mut text);
    assert_eq!(values(&source)[16], 1.);
    assert_eq!(source.view.shows, shows);
    assert!(!source.has_anim());
    pointer(&mut source, &mut text, PointerPhase::Down, 100.);
    frame(&mut source, &mut text);
    let shows = source.view.shows;
    clock.advance(Duration::from_millis(75)).unwrap();
    frame(&mut source, &mut text);
    assert!((values(&source)[17] - 0.5).abs() < 1e-5);
    clock.advance(Duration::from_millis(75)).unwrap();
    frame(&mut source, &mut text);
    assert_eq!(values(&source)[17], 1.);
    assert_eq!(source.view.shows, shows);
    assert_eq!(layout(&source), bounds);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert!(!source.has_anim());
    pointer(&mut source, &mut text, PointerPhase::Up, 100.);
    frame(&mut source, &mut text);
    assert_eq!(source.view.clicks, 1);
    source.set_reduced_motion(true);
    frame(&mut source, &mut text);
    assert_eq!(values(&source)[17], 0.);
    assert!(!source.has_anim());
    pointer(&mut source, &mut text, PointerPhase::Down, 39.4);
    frame(&mut source, &mut text);
    pointer(&mut source, &mut text, PointerPhase::Up, 39.4);
    frame(&mut source, &mut text);
    assert_eq!(
        source.view.clicks, 1,
        "the painted margin must not enlarge hit testing"
    );
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn pressed_glass_expands_pixels_beyond_the_normal_box_without_expanding_layout() {
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            shows: 0,
            clicks: 0,
            clip: false,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    let baseline = renderer
        .render_scene(&frame(&mut source, &mut text), [200, 100], [0.; 4], false)
        .unwrap();
    let bounds = layout(&source);
    pointer(&mut source, &mut text, PointerPhase::Down, 100.);
    frame(&mut source, &mut text);
    clock.advance(Duration::from_millis(150)).unwrap();
    let pressed = renderer
        .render_scene(&frame(&mut source, &mut text), [200, 100], [0.; 4], false)
        .unwrap();
    let pixel = |bytes: &[u8], x: usize, y: usize| {
        let i = (y * 200 + x) * 4;
        bytes[i..i + 4].to_vec()
    };
    assert_ne!(
        pixel(&baseline, 39, 50),
        pixel(&pressed, 39, 50),
        "press should visibly grow left by about 1.1 DIPs"
    );
    assert_eq!(
        pixel(&baseline, 37, 50),
        pixel(&pressed, 37, 50),
        "growth must stay bounded"
    );
    assert_eq!(layout(&source), bounds);
}

#[test]
fn hover_keeps_its_deadline_when_press_begins_halfway_through_it() {
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            shows: 0,
            clicks: 0,
            clip: false,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    frame(&mut source, &mut text);
    pointer(&mut source, &mut text, PointerPhase::Move, 100.);
    frame(&mut source, &mut text);
    clock.advance(Duration::from_millis(60)).unwrap();
    frame(&mut source, &mut text);
    pointer(&mut source, &mut text, PointerPhase::Down, 100.);
    frame(&mut source, &mut text);
    clock.advance(Duration::from_millis(60)).unwrap();
    frame(&mut source, &mut text);
    assert_eq!(values(&source)[16], 1.);
    assert!(values(&source)[17] > 0. && values(&source)[17] < 1.);
}
#[test]
#[ignore = "requires a working wgpu adapter"]
fn parent_clip_still_contains_the_expanded_material_and_no_opaque_button_fill_is_inferred() {
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Demo {
            shows: 0,
            clicks: 0,
            clip: true,
        },
        clock.clone(),
    );
    let mut text = TextContext::new();
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new()).unwrap();
    let baseline = renderer
        .render_scene(&frame(&mut source, &mut text), [200, 100], [0.; 4], false)
        .unwrap();
    let i = (50 * 200 + 54) * 4;
    assert!(
        baseline[i + 2] < 230,
        "a native opaque fill hid the scene behind the glass"
    );
    pointer(&mut source, &mut text, PointerPhase::Down, 100.);
    frame(&mut source, &mut text);
    clock.advance(Duration::from_millis(150)).unwrap();
    let pressed = renderer
        .render_scene(&frame(&mut source, &mut text), [200, 100], [0.; 4], false)
        .unwrap();
    for y in 0..100 {
        for x in 0..200 {
            if !(40..160).contains(&x) || !(30..70).contains(&y) {
                let i = (y * 200 + x) * 4;
                assert_eq!(
                    &baseline[i..i + 4],
                    &pressed[i..i + 4],
                    "escaped parent clip at ({x},{y})"
                );
            }
        }
    }
}

#[test]
fn material_background_keeps_keyboard_focus_chrome() {
    use plico_desktop::testing::{Command, Driver};
    let mut driver = Driver::new(
        Demo {
            shows: 0,
            clicks: 0,
            clip: false,
        },
        200,
        100,
    )
    .unwrap();
    driver.execute(Command::Key { key: "Tab".into() }).unwrap();
    driver.advance(Duration::from_millis(160)).unwrap();
    assert!(
        driver
            .placed_nodes()
            .iter()
            .any(|node| node.paint.outline_width > 0.),
        "glass must preserve keyboard focus indication"
    );
}
