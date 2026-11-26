//! Scene-backed Liquid Glass controls. Form values and asynchronous actions belong to the app.
use crate::prelude::*;
use std::ops::RangeInclusive;

const fn glass_spring(omega: f64, damping_ratio: f64, precision: f64) -> SpringOptions {
    SpringOptions {
        duration: std::time::Duration::ZERO,
        bounce: 0.,
        mass: 1.,
        stiffness: omega * omega,
        damping: 2. * omega * damping_ratio,
        precision,
        velocity: 0.,
    }
}

pub fn card<'u, 'e>(cx: &'u mut UiContext<'e>) -> crate::Layout<'u, 'e> {
    column(cx)
        .padding(20)
        .gap(12)
        .radius(24.)
        .material(LiquidGlass::regular())
}

/// A clear frosted card that samples the current scene behind it.
pub fn frosted_card<R>(
    cx: &mut UiContext,
    width: f32,
    height: f32,
    padding: f32,
    contents: impl FnOnce(&mut UiContext<'_>) -> R,
) -> R {
    column(cx)
        .width(width)
        .height(height)
        .position(Position::Relative)
        .radius(28.)
        .corner_smoothing(0.)
        .elevation(crate::Elevation::GLASS_CARD)
        .children(|cx| {
            LiquidGlass::regular().show_face(cx, 28., crate::materials::GlassFace::Card);
            column(cx)
                .width_percent(100.)
                .height_percent(100.)
                .padding(padding)
                .justify(Justify::SpaceBetween)
                .children(contents)
                .inner
        })
        .inner
}

pub fn button<'u, 'e, 's>(cx: &'u mut UiContext<'e>, label: &'s str) -> crate::Button<'u, 'e, 's> {
    crate::widgets::button(cx)
        .text(label)
        .height(40.)
        .radius(20.)
        .material(LiquidGlass::regular())
}

/// The XL clear button used by the reference Play demo.
pub fn clear_button(cx: &mut UiContext, label: &str, icon: &crate::SvgAsset) -> bool {
    clear_button_with_pulse(cx, label, icon, None).clicked
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClearButtonResponse {
    pub clicked: bool,
    /// The scripted press released during this declaration.
    pub pulse_finished: bool,
}

/// Changing the pulse token plays one visual press using the same springs as
/// pointer input. A pulse preserves focus and never activates the button.
pub fn clear_button_with_pulse(
    cx: &mut UiContext,
    label: &str,
    icon: &crate::SvgAsset,
    pulse: Option<u64>,
) -> ClearButtonResponse {
    const HEIGHT: f32 = 64.;
    let button = button_base(cx, label)
        .position(Position::Relative)
        .height(HEIGHT)
        .radius(32.)
        .padding_x(32)
        .align(Align::Center)
        .justify(Justify::Center)
        .children(|cx| {
            let pulse = cx.pulse(
                "glass button pulse",
                pulse,
                std::time::Duration::from_millis(160),
            );
            // GlassLayer's three optical springs and its uniform host scale.
            // The shader takes the final half-extent growth in DIP, so the
            // logical button box can stay fixed throughout the gesture.
            let [width, height] = cx.layout_size().unwrap_or([128., HEIGHT]);
            let max_growth = 12. / width.max(1.);
            let reduced = cx.display_preferences().reduce_motion;
            let hover = !reduced && (cx.hovered() || pulse.active);
            let down = !reduced && (cx.pressed() || pulse.active);
            let spring = |cx: &mut UiContext, key, target, omega, damping| {
                if reduced {
                    cx.jump(key, 0.)
                } else {
                    cx.spring(key, target, glass_spring(omega, damping, 0.0005))
                }
            };
            let press = spring(cx, "glass press", f32::from(down), 16., 0.6);
            let x = spring(cx, "glass width", f32::from(hover), 17., 0.42);
            let y = spring(cx, "glass height", f32::from(hover), 12., 0.34);
            let scale_target = if down { 0.08 } else { 0.04 * f32::from(hover) };
            let scale = 1. + spring(cx, "glass scale", scale_target.min(max_growth), 16., 0.6);
            let press_growth = 0.5 * 0.08_f32.min(max_growth) * press.clamp(-0.12, 1.15);
            let hover_growth = 0.08 * height;
            LiquidGlass::regular().show_face(
                cx,
                32.,
                crate::materials::GlassFace::ClearButton {
                    growth: [
                        width * press_growth + hover_growth * x,
                        height * press_growth + hover_growth * y,
                    ],
                    press,
                },
            );
            row(cx)
                .gap(10)
                .align(Align::Center)
                .scale(scale, scale)
                .children(|cx| {
                    svg(cx, icon).size(20., 20.);
                    text(cx, label).font_size(18).weight(600).no_wrap();
                });
            pulse.finished
        });
    ClearButtonResponse {
        clicked: button.response.clicked(),
        pulse_finished: button.inner,
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum ActionState {
    #[default]
    Idle,
    Pending,
    Success,
}

static ACTION_ICON: crate::ShaderAsset =
    crate::shader_asset!("../../assets/materials/action_icon.wgsl");

// Invert the time coordinate of the upstream CSS cubic-bezier timing curve.
fn action_ease(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if t <= 0. || t >= 1. {
        return t;
    }
    let cubic = |u: f32, a: f32, b: f32| {
        3. * (1. - u) * (1. - u) * u * a + 3. * (1. - u) * u * u * b + u * u * u
    };
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..14 {
        let u = (lo + hi) * 0.5;
        if cubic(u, x1, x2) < t {
            lo = u;
        } else {
            hi = u;
        }
    }
    cubic((lo + hi) * 0.5, y1, y2)
}
#[derive(Clone, Copy, Default, PartialEq)]
struct ActionPulse {
    x: crate::motion::OpticalSpring,
    y: crate::motion::OpticalSpring,
    press: crate::motion::OpticalSpring,
    scale: crate::motion::OpticalSpring,
    bend: f32,
    baseline: f32,
    nod: f32,
    closing: f32,
}

/// Returns one activation. The caller sets Pending while work is outstanding and
/// sets Success on completion or Idle on failure/retry.
pub fn action_button(cx: &mut UiContext, label: &str, state: ActionState) -> bool {
    let material = LiquidGlass {
        tint: Color::Foreground,
        tint_opacity: 1.,
        ..LiquidGlass::lens()
    };
    button_base(cx, label)
        .height(48.)
        .radius(24.)
        .padding(0)
        .gap(0)
        .disabled(state != ActionState::Idle)
        .children(|cx| {
            let apart = state == ActionState::Idle && !cx.pressed();
            let reduced = cx.display_preferences().reduce_motion;
            let (omega, zeta) = if apart { (5., 0.72) } else { (13., 0.7) };
            let separation = if reduced {
                f32::from(apart)
            } else {
                cx.spring("drop", f32::from(apart), glass_spring(omega, zeta, 0.0005))
                    .clamp(0., 1.06)
            };
            let now = cx.now();
            let last = cx.state(|| (state, None::<std::time::Duration>));
            let (old, mut start) = last.get();
            if old != state {
                start = Some(now);
                last.commit((state, start));
            }
            let elapsed = if reduced {
                1.
            } else {
                start.map_or(1., |t| now.saturating_sub(t).as_secs_f32())
            };
            let swap = action_ease((elapsed / 0.38).min(1.), 0.2, 0.8, 0.2, 1.);
            let letters_end = 0.4 + 0.01 * label.chars().count().saturating_sub(1) as f32;
            let hold = letters_end.max(0.54) + 0.6;
            let swelling = state == ActionState::Success && start.is_some() && elapsed < hold;
            let hovered = cx.hovered_within();
            let width = cx.layout_size().map_or(200., |size| size[0].max(1.));
            let motion = cx.state(|| (ActionPulse::default(), now));
            let (mut pulse, previous) = motion.get();
            let dt = now.saturating_sub(previous).as_secs_f32().min(1. / 30.);
            let mut content_scale = [1.; 2];
            let mut grow = [0.; 2];
            let mut k = 1.;
            let mut host_scale = 1.;
            if reduced {
                pulse = ActionPulse::default();
            } else {
                if old != state {
                    if hovered {
                        pulse.x.velocity += 24.;
                        pulse.y.velocity += 20.;
                    } else if state != ActionState::Success {
                        pulse.nod = 0.16;
                    }
                }
                let hover = f32::from(hovered || swelling || pulse.nod > 0.);
                pulse.nod = (pulse.nod - dt).max(0.);
                let down = cx.pressed() || swelling;
                let max_scale = 12. / width;
                let target = if down { 0.08 } else { 0.04 * hover };
                let mut moving = pulse.scale.step(target.min(max_scale), 16., 0.6, dt);
                moving |= pulse.press.step(f32::from(down), 16., 0.6, dt);
                moving |= pulse.x.step(hover, 17., 0.42, dt);
                moving |= pulse.y.step(hover, 12., 0.34, dt);
                pulse.bend += (f32::from(swelling) - pulse.bend)
                    * (dt * if swelling { 30. } else { 5. }).min(1.);
                if !swelling && pulse.bend < 0.004 {
                    pulse.bend = 0.;
                }
                pulse.baseline += (hover - pulse.baseline)
                    * (dt * if hover > pulse.baseline { 4. } else { 12. }).min(1.);
                pulse.closing = (pulse.closing + if apart { -12. } else { 12. } * dt).clamp(0., 1.);
                host_scale += pulse.scale.x;
                k = (1. + 0.08_f32.min(max_scale) * pulse.press.x.clamp(-0.12, 1.15)) / host_scale;
                grow = [
                    width * 0.5 * (k - 1.) + 3.84 * pulse.x.x / host_scale,
                    24. * (k - 1.) + 3.84 * pulse.y.x / host_scale,
                ];
                content_scale = [
                    1. + pulse.bend * 0.08 * (pulse.x.x - pulse.baseline) * 48. / width,
                    1. + pulse.bend * 0.08 * (pulse.y.x - pulse.baseline),
                ];
                if moving || pulse.nod > 0. || pulse.bend > 0. {
                    cx.request_animation_frame();
                }
            }
            motion.commit((pulse, now));
            if !reduced
                && start.is_some()
                && (elapsed < letters_end.max(0.54) || swelling || state == ActionState::Pending)
            {
                cx.request_animation_frame();
            }
            let spacer = if reduced {
                f32::from(state == ActionState::Idle)
            } else {
                let from = if state == ActionState::Idle { 0. } else { 1. };
                let to = f32::from(state == ActionState::Idle);
                if state == ActionState::Success {
                    0.
                } else {
                    from + (to - from) * action_ease((elapsed / 0.30).min(1.), 0.2, 0.8, 0.2, 1.)
                }
            };
            row(cx)
                .position(Position::Relative)
                .height(48.)
                .align(Align::Center)
                .padding_left(24.)
                .padding_right(42. + 36. * spacer)
                .scale(host_scale, host_scale)
                .children(|cx| {
                    material.show_droplet(cx, separation, grow, [k, pulse.closing, pulse.press.x]);
                    // Measure the new label independently of its animated clipping box,
                    // like the upstream ResizeObserver-backed label sizer.
                    let natural = row(cx)
                        .absolute(0., 0.)
                        .opacity(0.)
                        .children(|cx| {
                            let width = cx.layout_size().map(|v| v[0]);
                            text(cx, label).font_size(15).weight(600);
                            width
                        })
                        .inner;
                    let natural = natural.filter(|w| *w > 0.).map(|width| {
                        let measured = cx.state(|| (width, width, now));
                        let (from, to, began) = measured.get();
                        let t = (now.saturating_sub(began).as_secs_f32() / 0.3).min(1.);
                        let current = from + (to - from) * action_ease(t, 0.2, 0.8, 0.2, 1.);
                        if width != to {
                            measured.commit((current, width, now));
                        }
                        if !reduced && (width != to || t < 1. && from != to) {
                            cx.request_animation_frame();
                        }
                        if reduced { width } else { current }
                    });
                    let mut label_box = row(cx).gap(0).scale(content_scale[0], content_scale[1]);
                    if let Some(width) = natural.filter(|w| *w > 0.) {
                        label_box = label_box.width(width);
                    }
                    label_box.children(|cx| {
                        for (index, ch) in label.chars().enumerate() {
                            let progress = if reduced || start.is_none() {
                                1.
                            } else {
                                action_ease(
                                    ((elapsed - index as f32 * 0.01) / 0.4).clamp(0., 1.),
                                    0.,
                                    0.,
                                    0.58,
                                    1.,
                                )
                            };
                            cx.keyed(index as u64, |cx| {
                                let mut buf = [0; 4];
                                let tone = cx.theme().resolve(Color::Surface);
                                let channel = |shift: u32| {
                                    ((((tone >> shift) & 255) as f32 * progress).round() as u32)
                                        << shift
                                };
                                let color = channel(16) | channel(8) | channel(0);
                                text(cx, ch.encode_utf8(&mut buf) as &str)
                                    .shrink(0.)
                                    .color(Color::Literal(color))
                                    .font_size(15)
                                    .weight(600)
                                    .opacity(progress)
                                    .blur(10. * (1. - progress));
                            });
                        }
                    });
                    column(cx)
                        .position(Position::Absolute)
                        .inset_right(3. * (1. - spacer))
                        .inset_top(0.)
                        .width(48.)
                        .height(48.)
                        .align(Align::Center)
                        .justify(Justify::Center)
                        .scale(content_scale[0], content_scale[1])
                        .blur(3. * (1. - swap))
                        .opacity(swap)
                        .translate(0., 4. * (1. - swap))
                        .children(|cx| {
                            let tone = cx.theme().resolve(Color::Surface);
                            let phase = match state {
                                ActionState::Idle => 0.,
                                ActionState::Pending if reduced => 0.,
                                ActionState::Pending => elapsed.rem_euclid(0.8) / 0.8,
                                ActionState::Success => action_ease(
                                    ((elapsed - 0.12) / 0.42).clamp(0., 1.),
                                    0.65,
                                    0.,
                                    0.35,
                                    1.,
                                ),
                            };
                            shader(cx, &ACTION_ICON).width(18.).height(18.).values(&[
                                state as u8 as f32,
                                phase,
                                ((tone >> 16) & 255) as f32 / 255.,
                                ((tone >> 8) & 255) as f32 / 255.,
                                (tone & 255) as f32 / 255.,
                            ]);
                        });
                });
        })
        .response
        .clicked()
}

/// A clipped ribbon of text behind the refracting surface.
pub fn badge(cx: &mut UiContext, label: &str, width: f32) {
    badge_with_speed(cx, label, width, 0.4);
}

/// Horizontal loops per second. A zero rate gives a static badge.
pub fn badge_with_speed(cx: &mut UiContext, label: &str, width: f32, speed: f32) {
    badge_ribbon(cx, label, width, speed, None, false);
}

/// A coloured ribbon. Hover is supplied by the containing surface.
pub fn tinted_badge(cx: &mut UiContext, label: &str, width: f32, tint: Color, hovered: bool) {
    badge_ribbon(cx, label, width, 0.4, Some(tint), hovered);
}

fn badge_ribbon(
    cx: &mut UiContext,
    label: &str,
    width: f32,
    speed: f32,
    tint: Option<Color>,
    parent_hovered: bool,
) {
    let w = width.max(24.);
    let height = if tint.is_some() { 22. } else { 26. };
    let ink = tint.map_or(Color::Foreground, |_| Color::Literal(0xffffff));
    let mut badge = row(cx)
        .width(w)
        .height(height)
        .radius(height * 0.5)
        .clip(true)
        .position(Position::Relative)
        .align(Align::Center)
        .justify(Justify::Center);
    if let Some(tint) = tint {
        badge = badge.background(tint);
    }
    badge.children(|cx| {
        let preferences = cx.display_preferences();
        if speed <= 0.
            || !speed.is_finite()
            || preferences.reduce_motion
            || preferences.reduce_transparency
        {
            text(cx, label).font_size(12).weight(600).color(ink);
        } else {
            let advance = row(cx)
                .absolute(0., 0.)
                .opacity(0.)
                .children(|cx| {
                    let width = cx.layout_size().map(|size| size[0]);
                    text(cx, label).font_size(12).weight(600);
                    width
                })
                .inner
                .unwrap_or(24.)
                + 12.;
            let now = cx.now();
            let phase = cx.state(|| (now, 0.0_f64, 1.0_f64));
            let (previous, offset, rate) = phase.get();
            let dt = now.saturating_sub(previous).as_secs_f64();
            let goal = if parent_hovered || cx.hovered_within() {
                3.5
            } else {
                1.0
            };
            let next_rate = goal + (rate - goal) * (-dt * 1.6).exp();
            let offset = (offset + dt * f64::from(speed) * (rate + next_rate) * 0.5).fract();
            phase.commit((now, offset, next_rate));
            let count = (w / advance).ceil() as usize + 2;
            for index in 0..count {
                cx.keyed(index as u64, |cx| {
                    let x = (index as f32 - offset as f32) * advance;
                    row(cx)
                        .absolute(x - 2., 0.)
                        .height(height)
                        .align(Align::Center)
                        .gap(4)
                        .children(|cx| {
                            column(cx).width(4.).height(4.).background(ink).end();
                            text(cx, label).font_size(12).weight(600).color(ink);
                        });
                });
            }
            cx.request_animation_frame();
        }
        if !preferences.reduce_transparency {
            column(cx)
                .absolute(0., 0.)
                .width(w)
                .height(height)
                .radius(height * 0.5)
                .material(LiquidGlass {
                    bevel: 5.,
                    refraction: 0.42,
                    rim: 0.7,
                    ..LiquidGlass::lens()
                })
                .end();
        }
    });
}

pub fn button_group<'u, 'e>(cx: &'u mut UiContext<'e>, vertical: bool) -> crate::Layout<'u, 'e> {
    let layout = if vertical { column(cx) } else { row(cx) };
    layout
        .semantic_role(crate::WidgetRole::Group)
        .align(Align::Center)
        .padding(4)
        .gap(0)
        .radius(20.)
        .corner_smoothing(0.)
        .glass_group()
        .material(LiquidGlass {
            tint: Color::Literal(0xffffff),
            tint_opacity: 1.,
            ..LiquidGlass::clear()
        })
}

pub fn group_item(cx: &mut UiContext, label: &str) -> bool {
    crate::widgets::button(cx)
        .text(label)
        .ghost()
        .radius(16.)
        .clicked()
}

/// Noninteractive inline label for a segmented group (units, counts, or captions).
pub fn group_text(cx: &mut UiContext, label: &str) {
    text(cx, label)
        .padding_x(8.)
        .weight(500)
        .color(Color::Muted);
}

pub fn group_separator(cx: &mut UiContext, vertical_group: bool) {
    let line = column(cx)
        .semantic_role(crate::WidgetRole::Separator)
        .background(Color::Border);
    if vertical_group {
        line.width_percent(100.).height(1.).end();
    } else {
        line.width(1.).height(24.).end();
    }
}

/// A positionable lens for compositions with their own selection model.
pub fn lens<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> crate::Layout<'u, 'e> {
    column(cx)
        .absolute(x, y)
        .width(width)
        .height(height)
        .radius(height / 2.)
        .material(LiquidGlass::lens())
}

pub fn tabs(cx: &mut UiContext, selected: &mut usize, labels: &[&str], tab_width: f32) -> bool {
    let before = *selected;
    if labels.is_empty() {
        return false;
    }
    *selected = (*selected).min(labels.len() - 1);
    let w = tab_width.max(32.);
    let stride = w + 4.;
    let width = stride * labels.len() as f32 + 4.;
    let track_alpha = if cx.theme().resolve(Color::Surface) & 255 < 128 {
        0.08
    } else {
        0.05
    };
    row(cx)
        .width(width)
        .height(48.)
        .padding(4)
        .gap(4)
        .position(Position::Relative)
        .radius(24.)
        .background(Color::Foreground)
        .background_alpha(track_alpha)
        .semantic_role(crate::WidgetRole::TabList)
        .selection_follows_focus(true)
        .children(|cx| {
            let pointer = cx.pointer_fraction().map(|fraction| fraction[0] * width);
            let drag = cx.state(|| None::<(f32, bool)>);
            let old_drag = drag.get();
            let preferences = cx.display_preferences();
            let lens_open = cx.state(|| 0.);
            let previous_press = lens_open.get();
            if !preferences.reduce_transparency {
                column(cx)
                    .absolute(*selected as f32 * stride + 4., 4.)
                    .width(w)
                    .height(40.)
                    .radius(20.)
                    .background(if track_alpha == 0.08 {
                        Color::Panel
                    } else {
                        Color::Literal(0xffffff)
                    })
                    .opacity(1. - previous_press)
                    .end();
            }
            let mut held = false;
            let mut clicked = None;
            for (i, label) in labels.iter().enumerate() {
                cx.keyed(i as u64, |cx| {
                    let item = button_base(cx, *label)
                        .width(w)
                        .height(40.)
                        .shrink(0.)
                        .justify(Justify::Center)
                        .selected(*selected == i)
                        .semantic_role(crate::WidgetRole::Tab)
                        .children(|cx| {
                            text(cx, *label).font_size(14).weight(600).color(
                                if *selected == i && previous_press < 0.2 {
                                    Color::Foreground
                                } else {
                                    Color::Muted
                                },
                            );
                            cx.pressed()
                        });
                    held |= item.inner;
                    if item.response.clicked() {
                        clicked = Some(i);
                    }
                });
            }
            let mut drag_target = None;
            let mut dragging = false;
            if let Some(pointer) = pointer {
                let (origin, moved) = old_drag.unwrap_or((pointer, false));
                dragging = moved || (pointer - origin).abs() >= 4.;
                let target = (pointer - w * 0.5).clamp(4., width - w - 4.);
                if dragging && held {
                    drag_target = Some(target);
                } else if dragging && old_drag.is_some() {
                    clicked = Some(((target - 4.) / stride).round() as usize);
                }
                drag.commit(held.then_some((origin, dragging)));
            } else {
                drag.commit(None);
            }
            if let Some(index) = clicked.filter(|i| *i != *selected) {
                *selected = index.min(labels.len() - 1);
                // Tab labels were already declared above. Refresh their selected
                // state now, including a drag release that generates no click.
                cx.request_rebuild();
            }
            let presentation = |cx: &mut UiContext, key, target, omega, damping| {
                if preferences.reduce_motion {
                    target
                } else {
                    cx.spring(key, target, glass_spring(omega, damping, 0.001))
                }
            };
            let following = held && dragging;
            let x = drag_target.unwrap_or(*selected as f32 * stride + 4.);
            let shown = presentation(
                cx,
                "selection",
                x,
                if following { 40. } else { 14. },
                if following { 0.999 } else { 0.75 },
            );
            let travelling = (shown - x).abs() > w * 0.06;
            let press =
                presentation(cx, "press", f32::from(held || travelling), 16., 0.6).clamp(0., 1.);
            lens_open.commit(press);
            if preferences.reduce_transparency {
                // A solid underline keeps the selection visible with reduced transparency.
                column(cx)
                    .absolute(shown + 10., 40.)
                    .width(w - 20.)
                    .height(2.)
                    .radius(1.)
                    .background(Color::Accent)
                    .end();
            } else if press > 0.001 || travelling {
                let grow = 8. * press;
                lens(cx, shown - grow, 4. - grow * 0.5, w + 2. * grow, 40. + grow)
                    .opacity(press)
                    .material(LiquidGlass {
                        bevel: 7.,
                        refraction: 0.1 + 0.72 * press,
                        rim: 0.55,
                        ..LiquidGlass::lens()
                    })
                    .end();
            }
        });
    before != *selected
}

pub fn popover<'u, 'e, 's>(
    cx: &'u mut UiContext<'e>,
    state: &'s mut PopoverState,
    label: &'s str,
) -> crate::Popover<'u, 'e, 's> {
    cx.popover(state, label).material(LiquidGlass::regular())
}
pub fn dropdown_menu<'u, 'e, 's>(
    cx: &'u mut UiContext<'e>,
    state: &'s mut PopoverState,
    label: &'s str,
) -> crate::Popover<'u, 'e, 's> {
    crate::widgets::menu(cx, state, label)
        .width(176.)
        .material(LiquidGlass::regular())
}
/// Divider between glass menu commands. It arrives in sequence in stretched menus.
#[component]
pub fn menu_separator(cx: &mut UiContext) {
    column(cx)
        .semantic_role(crate::WidgetRole::Separator)
        .padding_x(14.)
        .padding_y(4.)
        .children(separator);
}

pub fn select<'u, 'e, 's, T: Clone + PartialEq>(
    cx: &'u mut UiContext<'e>,
    state: &'s mut PopoverState,
    label: &'s str,
    value: &'s mut T,
    options: &'s [SelectOption<'s, T>],
) -> crate::widgets::Select<'u, 'e, 's, T> {
    crate::widgets::select(cx, state, label, value, options)
        .width(256.)
        .max_height(300.)
        .material(LiquidGlass::regular())
}

#[derive(Clone, Copy)]
enum GlassToggle {
    Switch,
    Checkbox,
    Circle,
}

fn paint_toggle_lens(
    cx: &mut UiContext,
    [x, y, width, height]: [f32; 4],
    radius: f32,
    press: f32,
    optics_scale: f32,
    settings: LiquidGlass,
    bend: f32,
) {
    let clear = 1. - settings.tint_opacity;
    column(cx)
        .absolute(x, y)
        .width(width)
        .height(height)
        .radius(radius)
        .corner_smoothing(0.)
        .shadow(crate::BoxShadow {
            offset_x: 0.,
            offset_y: (11. + 11. * press) * optics_scale,
            blur: (20.4 + 13.6 * press) * optics_scale,
            spread: 0.,
            opacity: 0.12 * (0.55 + 0.45 * press) * clear,
            color: Color::Literal(0x000000),
        })
        .children(|cx| {
            settings.show_face(cx, radius, crate::materials::GlassFace::ControlLens(bend));
        });
}

#[derive(Clone, Copy, PartialEq)]
struct ToggleGrab {
    origin: f64,
    initial: f32,
    on_indicator: bool,
    moved: bool,
}

fn toggle(
    cx: &mut UiContext,
    label: &str,
    description: Option<&str>,
    value: &mut bool,
    shape: GlassToggle,
) -> bool {
    let is_switch = matches!(shape, GlassToggle::Switch);
    let height = if is_switch { 28. } else { 22. };
    let width = if is_switch {
        137. * height / 60.
    } else {
        height
    };
    let travel = if is_switch { 47. * height / 60. } else { 0. };
    let radius = if matches!(shape, GlassToggle::Checkbox) {
        6.6
    } else {
        height * 0.5
    };
    let response = button_base(cx, label)
        .semantic_role(if is_switch {
            crate::WidgetRole::Switch
        } else {
            crate::WidgetRole::CheckBox
        })
        .checked(*value)
        .gap(12)
        .align(Align::Center)
        .children(|cx| {
            let down = cx.pressed();
            let pointer = cx.pointer_position();
            let grab = cx.state(|| None::<ToggleGrab>);
            let mut active = grab.get();
            let mut position = f32::from(*value);
            let mut dragged = false;
            let within_indicator = cx
                .pointer_fraction()
                .zip(cx.layout_size())
                .is_some_and(|(pos, size)| (0. ..=width).contains(&(pos[0] * size[0])));
            if let Some((x, _)) = pointer {
                if down && active.is_none() {
                    active = Some(ToggleGrab {
                        origin: x,
                        initial: position,
                        on_indicator: within_indicator,
                        moved: false,
                    });
                }
                if is_switch && let Some(contact) = active.as_mut().filter(|p| p.on_indicator) {
                    let delta = x - contact.origin;
                    contact.moved |= delta.abs() >= 3.;
                    dragged = contact.moved;
                    if dragged {
                        position = (contact.initial + delta as f32 / travel).clamp(0., 1.);
                    }
                }
            } else {
                active = None;
            }
            grab.commit(if down { active } else { None });
            let indicator_pressed = down
                && active.is_some_and(|press| press.on_indicator)
                && (is_switch || within_indicator);
            let reduced = cx.display_preferences().reduce_motion;
            let spring = |cx: &mut UiContext, key, target, omega, damping| {
                if reduced {
                    target
                } else {
                    cx.spring(key, target, glass_spring(omega, damping, 0.0005))
                }
            };
            let on = if is_switch {
                spring(
                    cx,
                    "position",
                    position,
                    if dragged { 40. } else { 15. },
                    if dragged { 1. } else { 0.7 },
                )
            } else {
                spring(cx, "fill", position, 18., 1.)
            };
            let press = spring(
                cx,
                "press",
                f32::from(
                    indicator_pressed || is_switch && !dragged && (on - position).abs() > 0.08,
                ),
                16.,
                0.6,
            );
            let dark = cx.theme().resolve(Color::Surface) & 255 < 128;
            let track = Color::Literal(if dark { 0x34353b } else { 0xc6c5c9 });
            let well = Color::Literal(if dark { 0xffffff } else { 0x000000 });
            let phase = press.clamp(-0.12, 1.15);
            let press = press.clamp(0., 1.);
            let reduced_transparency = cx.display_preferences().reduce_transparency;
            column(cx)
                .position(Position::Relative)
                .width(width)
                .height(height)
                .radius(radius)
                .corner_smoothing(0.)
                .background(if is_switch { track } else { well })
                .background_alpha(if is_switch {
                    1.
                } else if dark {
                    0.06
                } else {
                    0.055
                })
                .children(|cx| {
                    column(cx)
                        .absolute(0., 0.)
                        .width(width)
                        .height(height)
                        .radius(radius)
                        .corner_smoothing(0.)
                        .background(Color::Literal(0x3291ff))
                        .opacity(on.clamp(0., 1.))
                        .end();
                    if is_switch {
                        let unit = height / 60.;
                        let rest_h = 51. * unit;
                        let rest_w = 81. * unit;
                        let pad = (height - rest_h) * 0.5;
                        let zoom = 1. + 0.55 * phase;
                        let w = rest_w * zoom;
                        let h = rest_h * zoom;
                        let x = pad + on * travel + (rest_w - w) * 0.5;
                        let scale = 51. * 1.55 / 120.;
                        let bend = (6. * scale + (26. - 6. * scale) * press)
                            * (0.06 + 0.94 * press)
                            * unit;
                        paint_toggle_lens(
                            cx,
                            [x, (height - h) * 0.5, w, h],
                            (h * 0.5 - unit).max(0.),
                            press,
                            scale * unit,
                            LiquidGlass {
                                tint_opacity: 1. - press,
                                bevel: (51. * 0.42 + (62. * scale - 51. * 0.42) * press) * unit,
                                rim: 0.55 * (0.04 + 0.96 * press),
                                fallback: Color::Literal(0xffffff),
                                ..LiquidGlass::lens()
                            },
                            bend,
                        );
                    } else {
                        shader(cx, &ACTION_ICON)
                            .absolute(0., 0.)
                            .width(width)
                            .height(height)
                            .values(&[3., on.clamp(0., 1.), 1., 1., 1.]);
                        if !reduced_transparency {
                            let cap = height * 1.2 * (1. + 0.18 * phase);
                            let offset = (height - cap) * 0.5;
                            let unit = height / 100.;
                            let scale = 1.18;
                            let bend = 37. * scale * (0.55 + 0.45 * press) * unit;
                            paint_toggle_lens(
                                cx,
                                [offset, offset, cap, cap],
                                radius * (cap / height),
                                press,
                                scale * unit,
                                LiquidGlass {
                                    tint_opacity: 0.,
                                    bevel: (120. * 0.42 + (62. * scale - 120. * 0.42) * press)
                                        * unit,
                                    rim: 0.55,
                                    ..LiquidGlass::lens()
                                },
                                bend,
                            );
                        }
                    }
                });
            column(cx).gap(0).children(|cx| {
                text(cx, label).font_size(13).weight(500);
                if let Some(description) = description {
                    text(cx, description).font_size(12).color(Color::Muted);
                }
            });
            (!down && dragged).then_some(position > 0.5)
        });
    let next = response
        .inner
        .or_else(|| response.response.clicked().then_some(!*value));
    let changed = next.is_some_and(|v| v != *value);
    if let Some(v) = next {
        *value = v;
    }
    changed
}
pub fn glass_checkbox(cx: &mut UiContext, label: &str, value: &mut bool) -> bool {
    toggle(cx, label, None, value, GlassToggle::Checkbox)
}
pub fn glass_switch(cx: &mut UiContext, label: &str, value: &mut bool) -> bool {
    toggle(cx, label, None, value, GlassToggle::Switch)
}

/// Circular checkbox with the same interaction and accessible state.
pub fn glass_checkbox_circle(cx: &mut UiContext, label: &str, value: &mut bool) -> bool {
    toggle(cx, label, None, value, GlassToggle::Circle)
}

/// Switch with a description below its accessible label.
pub fn glass_switch_with_description(
    cx: &mut UiContext,
    label: &str,
    description: &str,
    value: &mut bool,
) -> bool {
    toggle(cx, label, Some(description), value, GlassToggle::Switch)
}

pub fn glass_slider(
    cx: &mut UiContext,
    label: &str,
    value: &mut f64,
    range: RangeInclusive<f64>,
    step: f64,
    width: f32,
) -> bool {
    const UNIT: f32 = 12. / 32.;
    const REST_WIDTH: f32 = 132. * UNIT;
    const REST_HEIGHT: f32 = 68. * UNIT;
    const HEIGHT: f32 = REST_HEIGHT + 8.;
    let w = width.max(32.);
    let start = *range.start();
    let span = range.end() - start;
    let track = if cx.theme().resolve(Color::Surface) & 255 < 128 {
        Color::Literal(0x34353b)
    } else {
        Color::Literal(0xc6c5c9)
    };
    column(cx)
        .width(w)
        .height(HEIGHT)
        .position(Position::Relative)
        .radius(HEIGHT / 2.)
        .children(|cx| {
            let response = crate::widgets::slider(cx, label, value, range)
                .step(step)
                .unstyled()
                .grab_width(REST_WIDTH + 16. * UNIT)
                .absolute(0., 0.)
                .width(w)
                .height(HEIGHT)
                .min_height(0.)
                .response();
            let fraction = if span > 0. {
                ((response.value() - start) / span).clamp(0., 1.) as f32
            } else {
                0.
            };
            let reduced = cx.display_preferences().reduce_motion;
            let spring = |cx: &mut UiContext, key, target, omega, damping| {
                if reduced {
                    target
                } else {
                    cx.spring(key, target, glass_spring(omega, damping, 0.0005))
                }
            };
            let shown = spring(cx, "brightness", fraction, 30., 1.).clamp(0., 1.);
            let pressed = spring(
                cx,
                "brightness press",
                f32::from(response.pressed()),
                16.,
                0.6,
            );
            let clear = pressed.clamp(0., 1.);
            let now = cx.now();
            let motion = cx.state(|| (now, shown, 0.));
            let (previous_time, previous_value, previous_stretch) = motion.get();
            let dt = now.saturating_sub(previous_time).as_secs_f32();
            let stretch = if reduced {
                0.
            } else if dt > 0. {
                let velocity = (shown - previous_value) / dt * w / UNIT;
                let target = (velocity * 9e-5).clamp(-0.04, 0.04);
                previous_stretch + (target - previous_stretch) * (dt * 18.).min(1.)
            } else {
                previous_stretch
            };
            if stretch.abs() > 0.0001 {
                cx.request_animation_frame();
            }
            motion.commit((now, shown, stretch));

            let track_y = (HEIGHT - 12.) * 0.5;
            column(cx)
                .absolute(0., track_y)
                .width(w)
                .height(12.)
                .radius(6.)
                .clip(true)
                .background(track)
                .children(|cx| {
                    if shown > 0. {
                        column(cx)
                            .absolute(0., 0.)
                            .width((w * shown).max(12.))
                            .height(12.)
                            .radius(6.)
                            .background(Color::Literal(0x3291ff))
                            .end();
                    }
                });
            let phase = pressed.clamp(-0.12, 1.15);
            let width = (REST_WIDTH + (245. * UNIT - REST_WIDTH) * phase) * (1. + stretch.abs());
            let height =
                (REST_HEIGHT + (120. * UNIT - REST_HEIGHT) * phase) * (1. - 0.45 * stretch.abs());
            let inset = (width * 0.5 * (1. - 0.44 * clear)).min(w * 0.5);
            let center =
                (shown * w - 22. * UNIT * clear).clamp(inset, w - inset) - 60. * UNIT * stretch;
            let radius = width.min(height) * 0.5;
            column(cx)
                .absolute(center - width * 0.5, (HEIGHT - height) * 0.5)
                .width(width)
                .height(height)
                .radius(radius)
                .shadow(crate::BoxShadow {
                    offset_x: 0.,
                    offset_y: 2. + 7. * clear,
                    blur: 4. + 8. * clear,
                    spread: 0.,
                    opacity: 0.04 + 0.13 * clear,
                    color: Color::Literal(0x000000),
                })
                .children(|cx| {
                    LiquidGlass {
                        tint: Color::Literal(0xffffff),
                        tint_opacity: 1. - clear,
                        bevel: (68. * 0.42 + (62. - 68. * 0.42) * clear) * UNIT,
                        rim: 0.55 * (0.04 + 0.96 * clear),
                        fallback: Color::Literal(0xffffff),
                        ..LiquidGlass::lens()
                    }
                    .show_face(
                        cx,
                        radius,
                        crate::materials::GlassFace::ControlLens(
                            (6. + 31. * clear) * (0.06 + 0.94 * clear) * UNIT,
                        ),
                    );
                });
            response.changed()
        })
        .inner
}

/// Drag direction, completion distance and fill for a confirmation track.
#[derive(Debug, Clone, Copy)]
pub struct SlideOptions {
    pub threshold: f32,
    pub mirror: bool,
    pub fill: bool,
}
impl Default for SlideOptions {
    fn default() -> Self {
        Self {
            threshold: 0.9,
            mirror: false,
            fill: true,
        }
    }
}

/// A pointer must start on the thumb and release beyond 90% of its travel.
/// Keyboard and accessibility button activations are equivalent to confirmation.
pub fn slide_to_confirm(cx: &mut UiContext, label: &str, state: ActionState, width: f32) -> bool {
    slide_to_confirm_with(cx, label, state, width, SlideOptions::default())
}

pub fn slide_to_confirm_with(
    cx: &mut UiContext,
    label: &str,
    state: ActionState,
    width: f32,
    options: SlideOptions,
) -> bool {
    slide(cx, label, label, state, width, options, false)
}

/// The large, unfilled alarm slider, with a solid stop glyph.
pub fn slide_to_confirm_xl(
    cx: &mut UiContext,
    label: &str,
    success_label: &str,
    state: ActionState,
    width: f32,
) -> bool {
    slide(
        cx,
        label,
        success_label,
        state,
        width,
        SlideOptions {
            fill: false,
            ..SlideOptions::default()
        },
        true,
    )
}

fn slide(
    cx: &mut UiContext,
    label: &str,
    success_label: &str,
    state: ActionState,
    width: f32,
    options: SlideOptions,
    xl: bool,
) -> bool {
    let height = if xl { 96. } else { 44. };
    let inset = if xl { 4. } else { 2. };
    let thumb = height - 2. * inset;
    let w = width.max(height + 36.);
    let travel = w - height;
    let threshold = if options.threshold.is_finite() {
        options.threshold.clamp(0., 1.)
    } else {
        0.9
    };
    let mut control = button_base(cx, label)
        .position(Position::Relative)
        .width(w)
        .height(height)
        .radius(height * 0.5)
        .padding(0)
        .align(Align::Center)
        .justify(Justify::Center)
        .disabled(state != ActionState::Idle);
    if xl {
        control = control.background(Color::Foreground).background_alpha(0.05);
    } else {
        control = control.material(LiquidGlass::regular());
    }
    let response = control.children(|cx| {
        // Retain where the thumb was grabbed so it doesn't jump to the pointer's centre.
        let tracking = cx.state(|| None::<(f32, bool)>);
        let previous = tracking.get();
        let pointer = cx.pointer_fraction().map(|p| {
            if options.mirror {
                (1. - p[0]) * w
            } else {
                p[0] * w
            }
        });
        let position = if state == ActionState::Idle {
            0.
        } else {
            travel
        };
        let (grab, armed) = previous.unwrap_or_else(|| {
            let x = pointer.unwrap_or(0.);
            (x, x <= height)
        });
        let down = cx.pressed();
        tracking.commit(down.then_some((grab, armed)));
        let delta = pointer.map_or(0., |p| p - grab);
        let dragging = down && armed;
        let target = if dragging {
            let beyond = delta - travel;
            if beyond > 0. {
                travel + 24. * beyond / (beyond + thumb * 0.5)
            } else {
                delta.max(0.)
            }
        } else {
            position
        };
        let preferences = cx.display_preferences();
        let spring = |cx: &mut UiContext, key, target, omega, damping| {
            if preferences.reduce_motion {
                target
            } else {
                cx.spring(key, target, glass_spring(omega, damping, 0.01))
            }
        };
        let shown = spring(
            cx,
            "thumb",
            target,
            if dragging {
                40.
            } else if state == ActionState::Idle {
                22.
            } else {
                14.
            },
            if dragging { 0.999 } else { 0.75 },
        );
        let moving = (shown - target).abs() > thumb * 0.06;
        let press =
            spring(cx, "thumb press", f32::from(dragging || moving), 16., 0.6).clamp(0., 1.);
        if options.fill && shown > 0. {
            let fill_width = height + shown.clamp(0., travel);
            column(cx)
                .absolute(if options.mirror { w - fill_width } else { 0. }, 0.)
                .width(fill_width)
                .height(height)
                .radius(height * 0.5)
                .background(Color::Selection)
                .end();
        }
        let display = if state == ActionState::Success {
            success_label
        } else {
            label
        };
        let mut caption = row(cx)
            .absolute(0., 0.)
            .width(w)
            .height(height)
            .align(Align::Center)
            .justify(Justify::Center);
        if xl {
            let progress = (shown / travel).clamp(0., 1.);
            caption = caption
                .padding_left(height * (1. - progress))
                .padding_right(height * progress);
        }
        caption.children(|cx| {
            text(cx, display)
                .font_size(if xl { 20 } else { 14 })
                .weight(600);
        });
        let logical_x = if options.mirror {
            travel - shown
        } else {
            shown
        };
        let now = cx.now();
        let last = cx.state(|| (now, shown, 0.));
        let (previous_time, previous_x, previous_speed) = last.get();
        let dt = now.saturating_sub(previous_time).as_secs_f32();
        let speed = if dt > 0. {
            ((shown - previous_x).abs() / (dt * 22. * thumb)).min(1.)
        } else {
            previous_speed
        };
        last.commit((now, shown, speed));
        let stretch = if preferences.reduce_motion { 0. } else { speed };
        let grow = 1. + 0.12 * press;
        let lens_w = thumb * grow * (1. + 0.20 * stretch);
        let lens_h = thumb * grow * (1. - 0.12 * stretch);
        let x = inset + logical_x + (thumb - lens_w) * 0.5;
        let y = inset + (thumb - lens_h) * 0.5;
        let resting_tint = if state == ActionState::Success {
            Color::Foreground
        } else {
            Color::Literal(0xffffff)
        };
        let material = LiquidGlass {
            tint: resting_tint,
            tint_opacity: 1. - press,
            bevel: if xl { 18. } else { 8. },
            refraction: 0.15 + 0.65 * press,
            fallback: resting_tint,
            ..LiquidGlass::lens()
        };
        column(cx)
            .absolute(x, y)
            .width(lens_w)
            .height(lens_h)
            .align(Align::Center)
            .justify(Justify::Center)
            .children(|cx| {
                let radius = lens_w.min(lens_h) * 0.5;
                if xl {
                    material.show_face(cx, radius, crate::materials::GlassFace::SlideThumb(press));
                } else {
                    material.show(cx, radius, 0.);
                }
                let ink = if state == ActionState::Success {
                    Color::Surface
                } else if press > 0.5 {
                    Color::Foreground
                } else {
                    Color::Literal(0x171717)
                };
                if state == ActionState::Success {
                    svg(cx, &crate::icons::CHECK)
                        .size(if xl { 32. } else { 20. }, if xl { 32. } else { 20. })
                        .color(ink);
                } else if xl {
                    column(cx)
                        .width(28.)
                        .height(28.)
                        .radius(6.)
                        .background(ink)
                        .end();
                } else {
                    text(cx, if options.mirror { "←" } else { "→" }).color(ink);
                }
            });
        previous.is_none() || (armed && pointer.is_some() && delta >= travel * threshold)
    });
    response.response.clicked() && response.inner
}
