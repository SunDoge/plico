//! Isolated state-style experiment. No public API and no reactive subscriptions.
//! Run: cargo run -p plico-desktop --example state_style
//! Tests: cargo test -p plico-desktop --example state_style
use plico::{Clock, VirtualClock, prelude::*};
use plico_desktop::{HostView, Source};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::Duration;

// Paint-only values: RGB, scale, border width. Layout dimensions stay fixed.
// Colors here are deliberately simple RGB interpolation, not a proposed color API.
type Paint = [f32; 5];
const SNAPPY: f32 = 0.045;
const FLUID: f32 = 0.060;
const EPSILON: f32 = 0.001;

#[derive(Clone, Copy, Default)]
struct Interaction {
    hovered: bool,
    selected: bool,
    focused: bool,
    pressed: bool,
    disabled: bool,
}
impl Interaction {
    fn targets(self) -> [f32; 5] {
        if self.disabled {
            return [0., 0., 0., 0., 1.];
        }
        [
            self.hovered,
            self.selected,
            self.focused,
            self.pressed,
            false,
        ]
        .map(|on| if on { 1. } else { 0. })
    }
}

#[derive(Clone, Copy)]
struct StateStyle<const N: usize> {
    idle: [f32; N],
    // Explicit precedence: hover -> selected -> focus -> press -> disabled.
    states: [Option<[f32; N]>; 5],
}
impl<const N: usize> StateStyle<N> {
    fn resolve(self, weights: [f32; 5]) -> [f32; N] {
        let mut value = self.idle;
        for (target, weight) in self.states.into_iter().zip(weights) {
            if let Some(target) = target {
                for (value, target) in value.iter_mut().zip(target) {
                    *value += (target - *value) * weight;
                }
            }
        }
        value
    }
}

#[derive(Default)]
struct Animator {
    weights: [f32; 5],
}
impl Animator {
    // Host-owned, sampled once per frame, no clock access in the style resolver.
    fn advance(&mut self, interaction: Interaction, dt: Duration) -> bool {
        let mut active = false;
        for (index, (value, target)) in self
            .weights
            .iter_mut()
            .zip(interaction.targets())
            .enumerate()
        {
            let half_life = if index == 1 { FLUID } else { SNAPPY };
            *value = target
                + (*value - target)
                    * (-dt.as_secs_f32() * std::f32::consts::LN_2 / half_life).exp();
            if (*value - target).abs() <= EPSILON {
                *value = target;
            } else {
                active = true;
            }
        }
        active
    }
}

fn palette(idle: Paint, hover: Paint, selected: Paint, pressed: Paint) -> StateStyle<5> {
    StateStyle {
        idle,
        states: [
            Some(hover),
            Some(selected),
            Some([0.45, 0.50, 0.95, 1., 2.]),
            Some(pressed),
            Some([0.25, 0.27, 0.32, 1., 0.]),
        ],
    }
}
fn styles() -> [StateStyle<5>; 3] {
    [
        palette(
            [0.25, 0.33, 0.80, 1., 0.],
            [0.35, 0.44, 0.95, 1.025, 0.],
            [0.20, 0.55, 0.65, 1., 0.],
            [0.18, 0.25, 0.65, 0.97, 0.],
        ),
        palette(
            [0.16, 0.18, 0.24, 1., 0.],
            [0.25, 0.28, 0.38, 1., 0.],
            [0.32, 0.40, 0.85, 1., 0.],
            [0.22, 0.28, 0.60, 0.98, 0.],
        ),
        palette(
            [0.28, 0.30, 0.36, 1., 0.],
            [0.48, 0.51, 0.61, 1., 0.],
            [0.40, 0.48, 0.85, 1., 0.],
            [0.60, 0.64, 0.76, 1., 0.],
        ),
    ]
}
fn color(paint: Paint) -> Color {
    let [r, g, b] = [paint[0], paint[1], paint[2]].map(|v| (v.clamp(0., 1.) * 255.).round() as u32);
    Color::Literal((r << 16) | (g << 8) | b)
}
struct Sample {
    label: &'static str,
    paints: [Paint; 3],
}
struct Experiment {
    samples: Vec<Sample>,
}
impl HostView for Experiment {
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width(740).height(720).padding(24).gap(14).background(Color::Literal(0x13161e)) + |cx| {
            text(cx, "State styles / virtual-clock experiment").font_size(24).color(Color::Literal(0xe9edf8));
            text(
                cx,
                "Simulated input; one resolver drives button, tab and scrollbar thumb",
            ).font_size(16).color(Color::Literal(0xaeb8cc));
            for (index, sample) in self.samples.iter().enumerate() {
                row(cx).key(index).height(58).gap(24).align(Align::Center) + |cx| {
                    text(cx, sample.label).width(200).font_size(16).color(Color::Literal(0xd9e0ef));
                    let [button_paint, tab_paint, thumb_paint] = sample.paints;
                    button(cx).text("Save").color(Color::Literal(0xffffff)).width(120).height(40).radius(8.)
                        .background(color(button_paint)).scale(button_paint[3], button_paint[3])
                        .border(button_paint[4], Color::Literal(0xa0acff)).motion(Motion::None);
                    button(cx).text("Overview").color(Color::Literal(0xffffff)).width(130).height(40).radius(8.)
                        .background(color(tab_paint)).scale(tab_paint[3], tab_paint[3])
                        .border(tab_paint[4], Color::Literal(0xa0acff)).motion(Motion::None);
                    column(cx).width(100).height(12).radius(6.).background(Color::Literal(0x222632)) + |cx| {
                        column(cx).width(52).height(12).radius(6.).background(color(thumb_paint))
                            .children(|_| {});
                    };
                };
            }
        };
    }
}

fn interaction(frame: usize) -> Interaction {
    Interaction {
        hovered: (1..36).contains(&frame),
        selected: (28..44).contains(&frame),
        focused: (36..44).contains(&frame),
        pressed: (24..28).contains(&frame),
        disabled: (44..60).contains(&frame),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clock = VirtualClock::default();
    let mut previous = clock.now();
    let mut animator = Animator::default();
    let mut samples = Vec::new();
    let captures = [
        (0, "Idle"),
        (16, "Hover"),
        (26, "Press interrupted hover"),
        (32, "Release + select"),
        (40, "Focus + selected"),
        (52, "Disabled"),
        (66, "Return interrupted"),
        (180, "Settled idle"),
    ];
    println!("frame,time_ms,hover,selected,focus,press,disabled,active");
    let mut settled = false;
    for frame in 0..=180 {
        if frame > 0 {
            clock.advance(Duration::from_nanos(16_666_667))?;
        }
        let now = clock.now();
        let active = animator.advance(interaction(frame), now - previous);
        previous = now;
        if let Some((_, label)) = captures.iter().find(|(at, _)| *at == frame) {
            samples.push(Sample {
                label,
                paints: styles().map(|style| style.resolve(animator.weights)),
            });
        }
        println!(
            "{frame},{:.3},{:.5},{:.5},{:.5},{:.5},{:.5},{active}",
            now.as_secs_f64() * 1000.,
            animator.weights[0],
            animator.weights[1],
            animator.weights[2],
            animator.weights[3],
            animator.weights[4]
        );
        settled = !active;
    }
    assert!(
        settled,
        "host must stop animation wakeups after convergence"
    );
    assert_eq!(animator.weights, [0.; 5]);
    let mut source = Source::with_clock(Experiment { samples }, clock);
    let mut text = TextContext::new();
    let mut scene = Scene::new();
    source
        .render_retained_frame(&mut scene, &mut text, (740, 720), 1.)
        .expect("retained frame");
    std::fs::create_dir_all("reports/state-style")?;
    plico_shell::renderer::render_to_image(&scene, 740, 720, source.base_color())?
        .save("reports/state-style/samples.png")?;
    eprintln!("snapshot: reports/state-style/samples.png");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layered_precedence_and_missing_states() {
        let style = StateStyle {
            idle: [0.],
            states: [Some([1.]), None, Some([3.]), Some([4.]), Some([5.])],
        };
        assert_eq!(style.resolve([1., 1., 0., 0., 0.]), [1.]);
        assert_eq!(style.resolve([1.; 5]), [5.]);
    }
    #[test]
    fn frame_rate_independent_before_snap_threshold() {
        let target = Interaction {
            hovered: true,
            ..Default::default()
        };
        let mut coarse = Animator::default();
        let mut fine = Animator::default();
        coarse.advance(target, Duration::from_millis(160));
        for _ in 0..10 {
            fine.advance(target, Duration::from_millis(16));
        }
        assert!((coarse.weights[0] - fine.weights[0]).abs() < 0.00001);
    }
    #[test]
    fn interrupted_transition_is_continuous_and_eventually_sleeps() {
        let mut animator = Animator::default();
        animator.advance(
            Interaction {
                hovered: true,
                ..Default::default()
            },
            Duration::from_millis(40),
        );
        let before = animator.weights;
        animator.advance(
            Interaction {
                pressed: true,
                ..Default::default()
            },
            Duration::ZERO,
        );
        assert_eq!(before, animator.weights);
        assert!(animator.advance(Interaction::default(), Duration::from_millis(16)));
        for _ in 0..120 {
            animator.advance(Interaction::default(), Duration::from_millis(16));
        }
        assert!(!animator.advance(Interaction::default(), Duration::from_millis(16)));
        assert_eq!(animator.weights, [0.; 5]);
    }
    #[test]
    fn disabled_suppresses_interaction_targets() {
        assert_eq!(
            Interaction {
                hovered: true,
                pressed: true,
                selected: true,
                focused: true,
                disabled: true
            }
            .targets(),
            [0., 0., 0., 0., 1.]
        );
    }
    #[test]
    fn paint_state_changes_reuse_native_layout() {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(
            Experiment {
                samples: vec![Sample {
                    label: "Probe",
                    paints: styles().map(|style| style.idle),
                }],
            },
            clock.clone(),
        );
        let mut text = TextContext::new();
        source.build_frame(&mut text, 740, 720);
        let mut animator = Animator::default();
        for _ in 0..30 {
            let dt = Duration::from_millis(16);
            clock.advance(dt).unwrap();
            animator.advance(
                Interaction {
                    hovered: true,
                    ..Default::default()
                },
                dt,
            );
            source.view.samples[0].paints = styles().map(|style| style.resolve(animator.weights));
            source.request_rebuild();
            source.build_frame(&mut text, 740, 720);
            assert_eq!(
                source.adapter.stats.layout_runs, 0,
                "paint-only transition must reuse layout"
            );
            assert_eq!(
                source.adapter.stats.created, 0,
                "widget identity must stay stable"
            );
        }
    }
}
