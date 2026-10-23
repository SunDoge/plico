//! Native single-line editing and masked-only password shaping.
use super::*;
use parley::{Affinity, Cursor, Selection};
use plico_core::InputEdit;
use plico_renderer::{ClipboardRequest, ImeEvent, ImeState, PaintScene, Point, WidgetImePurpose};
use vello_common::{
    kurbo::{Affine, Rect},
    peniko::{BlendMode, Fill},
};
use zeroize::Zeroizing;

pub(super) struct Geometry {
    layout: Arc<parley::Layout<[u8; 4]>>,
    offset: f32,
    y: f32,
    caret: [f64; 4],
}
impl Adapter {
    pub(super) fn align_input(&mut self, index: usize, tcx: &mut TextContext, engine: &Engine) {
        let node = &self.placed[index];
        let entity = self.reverse[&node.node_id];
        let Some((buffer, placeholder)) = engine.input(entity) else {
            return;
        };
        let input = buffer.presentation();
        let empty = input.text.is_empty();
        let text: Arc<str> = Arc::from(if empty {
            if placeholder.is_empty() {
                " "
            } else {
                placeholder
            }
        } else {
            &input.text
        });
        let color = if empty {
            rgb(engine.theme().muted)
        } else {
            node.paint.text_color
        };
        let rgba = color.to_rgba8();
        let layout = plico_renderer::text::layout_text_styled(
            tcx,
            text,
            node.paint.font_size,
            node.paint.font_weight,
            false,
            None,
            plico_renderer::style::TextAlign::Start,
            [rgba.r, rgba.g, rgba.b, rgba.a],
            Arc::from([]),
            node.paint.font_family.as_ref(),
            None,
        );
        let head = Cursor::from_byte_index(&layout, input.head, Affinity::Downstream);
        let caret = head.geometry(&layout, 1.);
        let focused = self.focused == Some(entity) && !engine.node(entity).unwrap().style.disabled;
        let width = node.content_size[0].max(1.);
        let height = node.content_size[1].max(1.);
        let mut offset = self.input_layouts.get(&entity).map_or(0., |g| g.offset);
        if focused {
            if caret.x0 < offset as f64 {
                offset = caret.x0 as f32;
            }
            if caret.x1 > (offset + width) as f64 {
                offset = (caret.x1 as f32 - width).max(0.);
            }
        } else {
            offset = 0.;
        }
        offset = offset.clamp(0., (layout.width() - width + 1.).max(0.));
        let y = ((height - layout.height()) / 2.).max(0.);
        let mut scene = Scene::default();
        scene.push_layer(
            BlendMode::default(),
            1.,
            Affine::IDENTITY,
            &Rect::new(0., 0., width as f64, height as f64),
            None,
            None,
        );
        let transform = Affine::translate((-(offset as f64), y as f64));
        if focused && !empty {
            let anchor = Cursor::from_byte_index(&layout, input.anchor, Affinity::Downstream);
            for (rect, _) in Selection::new(anchor, head).geometry(&layout) {
                scene.fill(
                    Fill::NonZero,
                    transform,
                    Color::from_rgba8(59, 130, 246, 90),
                    None,
                    &Rect::new(rect.x0, rect.y0, rect.x1, rect.y1),
                );
            }
        }
        plico_renderer::scene::draw_text_layout_into(
            &mut scene,
            tcx,
            &layout,
            Affine::IDENTITY,
            transform,
            1.,
        );
        if focused {
            scene.fill(
                Fill::NonZero,
                transform,
                rgb(engine.theme().foreground),
                None,
                &Rect::new(caret.x0, caret.y0, caret.x1, caret.y1),
            );
        }
        scene.pop_layer();
        let geometry = Geometry {
            layout,
            offset,
            y,
            caret: [
                caret.x0 - offset as f64,
                caret.y0 + y as f64,
                caret.x1 - offset as f64,
                caret.y1 + y as f64,
            ],
        };
        self.input_layouts.insert(entity, geometry);
        let fragment = Arc::new(scene);
        self.placed[index].paint.widget = Some(fragment.clone());
        self.tree
            .get_node_context_mut(self.placed[index].node_id)
            .unwrap()
            .widget = Some(fragment);
    }
    pub(super) fn input_pointer(
        &mut self,
        engine: &Engine,
        entity: Option<Entity>,
        point: Point,
        shift: bool,
    ) {
        let Some(entity) = entity else {
            return;
        };
        let Some((buffer, _)) = engine.input(entity) else {
            return;
        };
        if let Some(geometry) = self.input_layouts.get(&entity) {
            let p = &self.placed[self.placed_indices[&self.nodes[&entity].id]];
            let Some(point) = self.layout_point(p.node_id, point.x, point.y) else {
                return;
            };
            let cursor = Cursor::from_point(
                &geometry.layout,
                point.x as f32 - p.content_origin[0] + geometry.offset,
                point.y as f32 - p.content_origin[1] - geometry.y,
            );
            buffer.edit(InputEdit::Select(cursor.index(), shift));
        }
    }
    pub(super) fn input_event(
        &mut self,
        engine: &Engine,
        event: &mut UiEvent,
    ) -> Option<EventResponse> {
        let entity = self.focused?;
        let (buffer, _) = engine.input(entity)?;
        if engine.node(entity)?.style.disabled {
            return None;
        }
        let mut response = EventResponse {
            handled: true,
            request_redraw: true,
            text_input: Some(true),
            ..EventResponse::IGNORED
        };
        match event {
            UiEvent::TextInput(text) | UiEvent::Paste(text) => {
                buffer.edit(InputEdit::Insert(Zeroizing::new(std::mem::take(text))))
            }
            UiEvent::Ime(ImeEvent::Commit(text)) => {
                buffer.edit(InputEdit::Insert(Zeroizing::new(std::mem::take(text))))
            }
            UiEvent::Ime(ImeEvent::Preedit { text, .. }) => {
                buffer.edit(InputEdit::Preedit(Zeroizing::new(std::mem::take(text))))
            }
            UiEvent::Ime(ImeEvent::Disabled) => buffer.edit(InputEdit::ClearPreedit),
            UiEvent::Ime(ImeEvent::Enabled) => {}
            UiEvent::Ime(ImeEvent::DeleteSurrounding {
                before_bytes,
                after_bytes,
            }) => buffer.edit(InputEdit::DeleteSurrounding(*before_bytes, *after_bytes)),
            UiEvent::Key(key) if key.phase == KeyPhase::Down => {
                if buffer.is_composing() && !matches!(key.key.as_str(), "Tab" | "Escape") {
                    return Some(EventResponse {
                        text_input: Some(true),
                        ..EventResponse::IGNORED
                    });
                }
                if engine.node(entity)?.style.input_navigation && key.modifiers.is_empty() {
                    let navigation = match key.key.as_str() {
                        "ArrowUp" => Some(plico_core::InputNavigation::Previous),
                        "ArrowDown" => Some(plico_core::InputNavigation::Next),
                        "PageUp" => Some(plico_core::InputNavigation::PageUp),
                        "PageDown" => Some(plico_core::InputNavigation::PageDown),
                        "Escape" if !buffer.is_composing() && !key.repeat => {
                            Some(plico_core::InputNavigation::Cancel)
                        }
                        _ => None,
                    };
                    if let Some(action) = navigation {
                        buffer.edit(InputEdit::Navigate(action));
                        response.consume_key_text = true;
                        return Some(response);
                    }
                }
                let shift = key.modifiers.shift();
                let command = key.modifiers.primary_shortcut();
                response.consume_key_text = true;
                match key.key.as_str() {
                    "a" | "A" if command => buffer.edit(InputEdit::SelectAll),
                    "v" | "V" if command => response.clipboard = Some(ClipboardRequest::Read),
                    "c" | "C" if command => {
                        response.clipboard = buffer.selected_text().map(ClipboardRequest::Write)
                    }
                    "x" | "X" if command => {
                        if let Some(text) = buffer.selected_text() {
                            response.clipboard = Some(ClipboardRequest::Write(text));
                            buffer.edit(InputEdit::Insert(Zeroizing::new(String::new())));
                        }
                    }
                    "Backspace" => buffer.edit(InputEdit::Backspace),
                    "Delete" => buffer.edit(InputEdit::Delete),
                    "ArrowLeft" => buffer.edit(InputEdit::Left(shift)),
                    "ArrowRight" => buffer.edit(InputEdit::Right(shift)),
                    "Home" => buffer.edit(InputEdit::Home(shift)),
                    "End" => buffer.edit(InputEdit::End(shift)),
                    "Enter" if !key.repeat => buffer.edit(InputEdit::Submit),
                    "Escape" => buffer.edit(InputEdit::ClearPreedit),
                    "Tab" => return None,
                    _ => {
                        return Some(EventResponse {
                            text_input: Some(true),
                            ..EventResponse::IGNORED
                        });
                    }
                }
            }
            _ => return None,
        }
        Some(response)
    }
    pub(super) fn input_ime_state(&self, engine: &Engine) -> Option<ImeState> {
        let entity = self.focused?;
        let (buffer, _) = engine.input(entity)?;
        if engine.node(entity)?.style.disabled {
            return None;
        }
        let geometry = self.input_layouts.get(&entity)?;
        let placed = &self.placed[self.placed_indices[&self.nodes[&entity].id]];
        let (head, anchor) = buffer.selection_bytes();
        let text = buffer.text().unwrap_or_default();
        let secret = buffer.is_secret();
        Some(ImeState {
            client_id: super::semantic_id(entity),
            cursor_area: self.visual_rect(
                placed.node_id,
                [
                    geometry.caret[0] + placed.content_origin[0] as f64,
                    geometry.caret[1] + placed.content_origin[1] as f64,
                    geometry.caret[2] + placed.content_origin[0] as f64,
                    geometry.caret[3] + placed.content_origin[1] as f64,
                ],
            )?,
            surrounding_cursor: if secret { 0 } else { head },
            surrounding_anchor: if secret { 0 } else { anchor },
            surrounding_text: text,
            selection_utf16: buffer.selection_utf16(),
            selection_reversed: head < anchor,
            marked_range_utf16: None,
            purpose: if secret {
                WidgetImePurpose::Password
            } else {
                WidgetImePurpose::Normal
            },
            multiline: false,
            completion: false,
            spellcheck: false,
        })
    }
}
