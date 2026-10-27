use super::*;
use kurbo::{Affine, Circle, Rect};
use peniko::Fill;
use plico_renderer::PaintScene;
impl Adapter {
    pub(super) fn align_sliders(&mut self, engine: &Engine) {
        for node in &mut self.placed {
            let entity = self.reverse[&node.node_id];
            let n = engine.node(entity).unwrap();
            if n.kind != Kind::Slider || node.paint.widget.is_some() {
                continue;
            }
            let Some(range) = n.numeric else {
                continue;
            };
            let [w, h] = node.content_size;
            let vertical = range.vertical;
            let length = if vertical { h } else { w };
            let cross = if vertical { w } else { h };
            let margin = 8_f32.min(length / 2.);
            let available = (length - margin * 2.).max(0.);
            let at = margin
                + available
                    * (if vertical {
                        1. - range.fraction() as f32
                    } else {
                        range.fraction() as f32
                    });
            let center = cross / 2.;
            let thickness = 4_f32.min(cross);
            let pressed = self.motions.checked(entity, false) as f64;
            let knob = (8_f64 + pressed * 2.)
                .min(cross as f64 / 2.)
                .min(length as f64 / 2.);
            let rect = |start: f32, end: f32| {
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
            let mut scene = Scene::default();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                rgb(engine.theme().border),
                None,
                &rect(margin, length - margin).to_rounded_rect(2.),
            );
            let from_max = range.reversed ^ vertical;
            let fill = if from_max {
                rect(at, length - margin)
            } else {
                rect(margin, at)
            };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                rgb(engine.theme().accent),
                None,
                &fill.to_rounded_rect(2.),
            );
            let (x, y) = if vertical { (center, at) } else { (at, center) };
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                rgb(engine.theme().accent),
                None,
                &Circle::new((x as f64, y as f64), knob),
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                rgb(engine.theme().panel),
                None,
                &Circle::new((x as f64, y as f64), knob * 0.625),
            );
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
                && engine
                    .node(e)
                    .is_some_and(|n| n.kind == Kind::Slider && !n.style.disabled)
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
                        .filter(|e| enabled(self, *e))
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
                        buffer.edit(plico_core::InputEdit::ClearPreedit);
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
                    let n = engine.node(entity)?;
                    let range = n.numeric?;
                    let placed = &self.placed[self.placed_indices[&self.nodes[&entity].id]];
                    let vertical = range.vertical;
                    let length = if vertical {
                        placed.content_size[1]
                    } else {
                        placed.content_size[0]
                    };
                    let margin = 8_f64.min(length as f64 / 2.);
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
                    engine.queue_value(
                        entity,
                        range.normalize(range.min + t.clamp(0., 1.) * (range.max - range.min)),
                    );
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
                let value = engine.pending_value(entity).unwrap_or(range.value);
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
                engine.queue_value(entity, range.normalize(value));
                Some(response())
            }
            _ => None,
        }
    }
}
