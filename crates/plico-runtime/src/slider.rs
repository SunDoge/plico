use super::recipe;
use super::*;
use kurbo::{Affine, Rect, RoundedRect, Stroke};
use peniko::{Color as PaintColor, Fill};
use plico_renderer::PaintScene;

/// MyGO's slider proportions: a 5x4-unit capsule knob on a 1.5-unit track.
/// The 5-unit axis follows the track, so the knob's centre stops half that
/// length from each end.
fn metrics(theme: Theme) -> (f32, f32, f32) {
    let unit = theme.metrics.spacing.max(1) as f32;
    let (knob_w, knob_h) = (5. * unit, 4. * unit);
    (knob_w / 2., knob_w, knob_h)
}

fn paint_color(rgb: u32, alpha: f32) -> PaintColor {
    PaintColor::from_rgba8(
        ((rgb >> 16) & 255) as u8,
        ((rgb >> 8) & 255) as u8,
        (rgb & 255) as u8,
        (alpha.clamp(0., 1.) * 255.).round() as u8,
    )
}

impl Adapter {
    pub(super) fn align_sliders(&mut self, engine: &Engine) {
        for &entity in &self.slider_nodes {
            let Some(index) = self
                .nodes
                .get(&entity)
                .and_then(|n| self.placed_indices.get(&n.id))
                .copied()
            else {
                continue;
            };
            self.stats.slider_visits += 1;
            let node = &mut self.placed[index];
            let n = engine.node(entity).unwrap();
            if n.kind != Kind::Slider || !n.style.control_chrome || node.paint.widget.is_some() {
                continue;
            }
            let Some(range) = n.numeric else {
                continue;
            };
            let theme = engine.theme();
            let [w, h] = node.content_size;
            let vertical = range.vertical;
            let length = if vertical { h } else { w };
            let cross = if vertical { w } else { h };
            let (margin, knob_w, knob_h) = metrics(theme);
            let margin = margin.min(length / 2.);
            let available = (length - margin * 2.).max(0.);
            let position = |value: f64| {
                margin
                    + available
                        * (if vertical {
                            1. - range.fraction_of(value) as f32
                        } else {
                            range.fraction_of(value) as f32
                        })
            };
            let at = position(range.value);
            let high_at = range.high.map(position);
            let center = cross / 2.;
            let thickness = (1.5 * theme.metrics.spacing.max(1) as f32).min(cross);
            let radius = thickness / 2.;
            let held = self.motions.checked(entity, false).clamp(0., 1.);
            let grow = 1. + 0.35 * held;
            let span = |start: f32, end: f32| {
                if vertical {
                    Rect::new(
                        (center - thickness / 2.) as f64,
                        start as f64,
                        (center + thickness / 2.) as f64,
                        end as f64,
                    )
                } else {
                    Rect::new(
                        start as f64,
                        (center - thickness / 2.) as f64,
                        end as f64,
                        (center + thickness / 2.) as f64,
                    )
                }
            };
            let knob_rect = |at: f32| {
                let (nw, nh) = if vertical {
                    (knob_h * grow, knob_w * grow)
                } else {
                    (knob_w * grow, knob_h * grow)
                };
                let (cx, cy) = if vertical { (center, at) } else { (at, center) };
                Rect::new(
                    (cx - nw / 2.) as f64,
                    (cy - nh / 2.) as f64,
                    (cx + nw / 2.) as f64,
                    (cy + nh / 2.) as f64,
                )
            };
            let mut scene = Scene::default();
            // Held, the knob becomes a lens: the track shows through it, so
            // the lightening layer sits under the track (MyGO paintKnob).
            if held > 0. {
                for knob_at in std::iter::once(at).chain(high_at) {
                    let k = knob_rect(knob_at);
                    let lens =
                        PaintColor::from_rgba8(255, 255, 255, (0.1 * held * 255.).round() as u8);
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        lens,
                        None,
                        &k.to_rounded_rect(k.width().min(k.height()) / 2.),
                    );
                }
            }
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                rgb(theme.border),
                None,
                &span(0., length).to_rounded_rect(radius as f64),
            );
            let fill = if let Some(high_at) = high_at {
                span(at.min(high_at), at.max(high_at))
            } else {
                let from_max = range.reversed ^ vertical;
                if from_max {
                    span(at, length)
                } else {
                    span(0., at)
                }
            };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                rgb(theme.accent),
                None,
                &fill.to_rounded_rect(radius as f64),
            );
            let face = recipe::thumb_face(theme);
            let rim = paint_color(0, recipe::knob_rim_alpha(theme) * held);
            for knob_at in std::iter::once(at).chain(high_at) {
                let k = knob_rect(knob_at);
                let rad = k.width().min(k.height()) / 2.;
                if held < 1. {
                    scene.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        paint_color(face, 1. - held),
                        None,
                        &k.to_rounded_rect(rad),
                    );
                }
                if held > 0. {
                    scene.stroke(
                        &Stroke::new(1.),
                        Affine::IDENTITY,
                        rim,
                        None,
                        &k.to_rounded_rect(rad),
                    );
                    scene.stroke(
                        &Stroke::new(1.),
                        Affine::IDENTITY,
                        paint_color(0xffffff, 0.25 * held),
                        None,
                        &RoundedRect::new(
                            k.x0 + 1.,
                            k.y0 + 1.,
                            k.x1 - 1.,
                            k.y1 - 1.,
                            (rad - 1.).max(0.),
                        ),
                    );
                }
            }
            let fragment = Arc::new(scene);
            node.paint.widget = Some(fragment.clone());
            self.tree.get_node_context_mut(node.node_id).unwrap().widget = Some(fragment);
        }
    }
    pub(super) fn slider_event(
        &mut self,
        engine: &mut Engine,
        event: &UiEvent,
    ) -> Option<EventResponse> {
        let response = || EventResponse {
            handled: true,
            request_redraw: true,
            consume_key_text: true,
            text_input: Some(false),
            ..EventResponse::IGNORED
        };
        let enabled = |adapter: &Self, e| {
            adapter.modal_allows(engine, e)
                && engine.node(e).is_some_and(|n| {
                    matches!(n.kind, Kind::Slider | Kind::ValueControl) && !n.style.disabled
                })
        };
        match event {
            UiEvent::Focus(false) => {
                self.slider_drag = None;
                None
            }
            UiEvent::Pointer(pointer) => {
                if !pointer.position.x.is_finite() || !pointer.position.y.is_finite() {
                    return Some(EventResponse::IGNORED);
                }
                let start = pointer.phase == PointerPhase::Down
                    && pointer.button == Some(PointerButton::Primary);
                let entity = if start {
                    self.hit(engine, pointer.position.x, pointer.position.y)
                        // Composed value controls own keyboard/range semantics;
                        // their descendants define pointer hit regions.
                        .filter(|e| {
                            enabled(self, *e)
                                && engine.node(*e).is_some_and(|n| n.kind == Kind::Slider)
                        })
                } else {
                    self.slider_drag.filter(|e| enabled(self, *e))
                }?;
                if matches!(pointer.phase, PointerPhase::Down | PointerPhase::Up)
                    && pointer.button != Some(PointerButton::Primary)
                {
                    return Some(response());
                }
                if start {
                    if let Some((buffer, _)) = self
                        .focused
                        .filter(|old| *old != entity)
                        .and_then(|old| engine.input(old))
                        && buffer.is_composing()
                    {
                        buffer.edit(plico::InputEdit::ClearPreedit);
                    }
                    self.slider_drag = Some(entity);
                    self.pressed = Some(entity);
                    self.focused = Some(entity);
                    self.focus_visible = false;
                }
                if matches!(pointer.phase, PointerPhase::Cancel | PointerPhase::Leave) {
                    self.slider_drag = None;
                    self.pressed = None;
                    return Some(response());
                }
                if matches!(
                    pointer.phase,
                    PointerPhase::Down | PointerPhase::Move | PointerPhase::Up
                ) {
                    let range = engine.node(entity)?.numeric?;
                    let placed = &self.placed[self.placed_indices[&self.nodes[&entity].id]];
                    let vertical = range.vertical;
                    let length = if vertical {
                        placed.content_size[1]
                    } else {
                        placed.content_size[0]
                    };
                    let (margin, _, _) = metrics(engine.theme());
                    let margin = (margin as f64).min(length as f64 / 2.);
                    let available = (length as f64 - margin * 2.).max(0.);
                    let point =
                        self.layout_point(placed.node_id, pointer.position.x, pointer.position.y)?;
                    let point = if vertical {
                        point.y - placed.content_origin[1] as f64
                    } else {
                        point.x - placed.content_origin[0] as f64
                    };
                    let mut t = if available > 0. {
                        (point - margin) / available
                    } else {
                        0.
                    };
                    if vertical {
                        t = 1. - t;
                    }
                    let value = range.min + t.clamp(0., 1.) * (range.max - range.min);
                    if let Some(high) = range.high {
                        let low = engine.pending_value(entity).unwrap_or(range.value);
                        let high = engine.pending_high_value(entity).unwrap_or(high);
                        if start {
                            let d_low = (t - range.fraction_of(low)).abs();
                            let d_high = (t - range.fraction_of(high)).abs();
                            self.slider_thumbs
                                .insert(entity, usize::from(d_high < d_low));
                        }
                        if self.slider_thumbs.get(&entity).copied().unwrap_or(0) == 0 {
                            engine
                                .queue_value(entity, range.normalize(value).clamp(range.min, high));
                        } else {
                            engine.queue_high_value(
                                entity,
                                range.normalize(value).max(low).clamp(range.min, range.max),
                            );
                        }
                    } else {
                        engine.queue_value(entity, range.normalize(value));
                    }
                }
                if pointer.phase == PointerPhase::Up {
                    self.slider_drag = None;
                    self.pressed = None;
                }
                Some(response())
            }
            UiEvent::Key(key) if key.phase == KeyPhase::Down => {
                let entity = self.focused.filter(|e| enabled(self, *e))?;
                let range = engine.node(entity)?.numeric?;
                let low = engine.pending_value(entity).unwrap_or(range.value);
                let high = engine.pending_high_value(entity).or(range.high);
                let thumb = high.map_or(0, |_| {
                    self.slider_thumbs.get(&entity).copied().unwrap_or(0).min(1)
                });
                let value = if thumb == 0 { low } else { high.unwrap_or(low) };
                let delta = range.increment();
                let value = match key.key.as_str() {
                    "ArrowRight" | "ArrowUp" => value + delta,
                    "ArrowLeft" | "ArrowDown" => value - delta,
                    "PageUp" => value + delta * 10.,
                    "PageDown" => value - delta * 10.,
                    "Home" => range.min,
                    "End" => range.max,
                    " " | "Enter" => return Some(response()),
                    _ => return None,
                };
                self.focus_visible = true;
                if let Some(high) = high {
                    if thumb == 0 {
                        engine.queue_value(entity, range.normalize(value).clamp(range.min, high));
                    } else {
                        engine.queue_high_value(
                            entity,
                            range.normalize(value).max(low).clamp(range.min, range.max),
                        );
                    }
                } else {
                    engine.queue_value(entity, range.normalize(value));
                }
                Some(response())
            }
            _ => None,
        }
    }
}
