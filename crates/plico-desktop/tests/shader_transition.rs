use plico::prelude::*;
use plico_desktop::{HostView, Source};
use plico_renderer::{FrameSource, PaintCommand, Scene, TextContext};
use std::time::Duration;
static EFFECT: ShaderAsset = ShaderAsset::new(
    "fn plico_effect(_uv:vec2f)->vec4f {return vec4f(plico.values[0].x,0.,0.,1.);}",
);
struct Demo {
    target: f32,
    shows: usize,
    duration: u16,
    outset: [f32; 2],
}
impl HostView for Demo {
    #[component]
    fn show(&mut self, cx: &mut UiContext) {
        self.shows += 1;
        shader(cx, &EFFECT)
            .width(32.)
            .height(32.)
            .values(&[self.target])
            .transition_value(0, self.duration)
            .paint_outset(self.outset[0], self.outset[1]);
    }
}
fn sample(source: &mut Source<Demo>, text: &mut TextContext) -> (f32, [f32; 2], [f64; 6]) {
    source
        .render_retained_frame(&mut Scene::new(), text, (64, 64), 1.)
        .unwrap();
    source
        .adapter
        .placed
        .iter()
        .filter_map(|n| n.paint.widget.as_ref())
        .flat_map(|scene| scene.commands.iter())
        .find_map(|command| match command {
            PaintCommand::Shader { effect, transform } => Some((
                effect.values()[0],
                effect.logical_size(),
                transform.as_coeffs(),
            )),
            _ => None,
        })
        .unwrap()
}
fn setup() -> (Source<Demo>, plico::VirtualClock, TextContext) {
    let clock = plico::VirtualClock::default();
    (
        Source::with_clock(
            Demo {
                target: 0.,
                shows: 0,
                duration: 100,
                outset: [0.; 2],
            },
            clock.clone(),
        ),
        clock,
        TextContext::new(),
    )
}
#[test]
fn shader_values_interpolate_and_retarget_without_view_or_layout_work_then_idle() {
    let (mut source, clock, mut text) = setup();
    assert_eq!(sample(&mut source, &mut text).0, 0.);
    assert!(!source.has_anim());
    source.view.target = 1.;
    source.request_rebuild();
    assert_eq!(sample(&mut source, &mut text).0, 0.);
    let shows = source.view.shows;
    assert!(source.has_anim());
    clock.advance(Duration::from_millis(50)).unwrap();
    assert!((sample(&mut source, &mut text).0 - 0.5).abs() < 1e-6);
    assert_eq!(source.view.shows, shows);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    source.view.target = 0.;
    source.request_rebuild();
    assert!((sample(&mut source, &mut text).0 - 0.5).abs() < 1e-6);
    let shows = source.view.shows;
    clock.advance(Duration::from_millis(50)).unwrap();
    assert!((sample(&mut source, &mut text).0 - 0.25).abs() < 1e-6);
    clock.advance(Duration::from_millis(50)).unwrap();
    assert_eq!(sample(&mut source, &mut text).0, 0.);
    assert_eq!(source.view.shows, shows);
    assert_eq!(source.adapter.stats.layout_runs, 0);
    assert!(!source.has_anim());
}
#[test]
fn reduced_motion_snaps_uniform_targets_and_zero_duration_is_immediate() {
    let (mut source, clock, mut text) = setup();
    sample(&mut source, &mut text);
    source.view.target = 1.;
    source.request_rebuild();
    sample(&mut source, &mut text);
    clock.advance(Duration::from_millis(20)).unwrap();
    sample(&mut source, &mut text);
    source.set_reduced_motion(true);
    assert_eq!(sample(&mut source, &mut text).0, 1.);
    assert!(!source.has_anim());
    source.set_reduced_motion(false);
    source.view.duration = 0;
    source.view.target = 0.;
    source.request_rebuild();
    assert_eq!(sample(&mut source, &mut text).0, 0.);
    assert!(!source.has_anim());
}
#[test]
fn shader_paint_overflow_expands_only_the_canvas_not_the_layout() {
    let (mut source, _, mut text) = setup();
    sample(&mut source, &mut text);
    let before: Vec<_> = source.adapter.placed.iter().map(|n| n.rect).collect();
    source.view.outset = [1.1, 0.45];
    source.request_rebuild();
    let (_, size, transform) = sample(&mut source, &mut text);
    assert_eq!(size, [34.2, 32.9]);
    assert!((transform[4] + 1.1).abs() < 1e-6 && (transform[5] + 0.45).abs() < 1e-6);
    assert_eq!(
        source
            .adapter
            .placed
            .iter()
            .map(|n| n.rect)
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(source.adapter.stats.layout_runs, 0);
}
