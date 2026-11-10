//! Native single/multiline editing and masked-only password shaping.
use super::*;
use kurbo::{Affine, Rect};
use peniko::Fill;
use plico::{InputEdit, StandardShortcut};
use plico_renderer::paragraph::ParagraphLayout;
use plico_renderer::{ClipboardRequest, ImeEvent, ImeState, PaintScene, Point, WidgetImePurpose};
use zeroize::Zeroizing;

#[derive(Default)]
pub(super) struct InputPointers {
    last: Option<InputPress>,
    drag: Option<InputDrag>,
}
impl InputPointers {
    pub(super) fn clear_streak(&mut self) {
        self.last = None;
    }
}
struct InputPress {
    entity: Entity,
    pointer: plico_renderer::PointerId,
    pointer_type: plico_renderer::PointerType,
    point: Point,
    time: std::time::Duration,
    count: u8,
    value_revision: u64,
}
#[derive(Clone, Copy)]
struct InputDrag {
    entity: Entity,
    pointer: plico_renderer::PointerId,
    pointer_type: plico_renderer::PointerType,
    unit: u8,
    initial: (usize, usize),
    value_revision: u64,
}
#[derive(PartialEq)]
struct GeometryKey {
    revision: u64,
    multiline: bool,
    epoch: usize,
    size: [f32; 2],
    font_family: Option<Arc<str>>,
    font_size: f32,
    font_weight: f32,
    theme: Theme,
    focused: bool,
    text_color: peniko::Color,
}
pub(super) struct Geometry {
    key: GeometryKey,
    placeholder: Arc<str>,
    layout: ParagraphLayout,
    offset: f32,
    y: f32,
    caret: [f64; 4],
}
impl Adapter {
    pub(super) fn align_input(&mut self, index: usize, tcx: &mut TextContext, engine: &Engine) {
        self.stats.input_align_checks += 1;
        let node = &self.placed[index];
        let entity = self.reverse[&node.node_id];
        let Some((buffer, placeholder)) = engine.input(entity) else {
            return;
        };
        let style = &engine.node(entity).unwrap().style;
        let focused = self.focused == Some(entity) && !style.disabled;
        let key = GeometryKey {
            revision: buffer.revision(),
            multiline: buffer.is_multiline(),
            epoch: tcx.cache_generation(),
            size: node.content_size,
            font_family: node.paint.font_family.clone(),
            font_size: node.paint.font_size,
            font_weight: node.paint.font_weight,
            theme: engine.theme(),
            focused,
            text_color: node.paint.text_color,
        };
        if node.paint.widget.is_some()
            && self
                .input_layouts
                .get(&entity)
                .is_some_and(|g| g.key == key && g.placeholder.as_ref() == placeholder)
        {
            self.stats.input_reuses += 1;
            return;
        }
        self.stats.input_rebuilds += 1;
        let reveal = buffer.is_secret() && style.input_password_visible;
        let input = buffer.presentation_with(reveal);
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
        let multiline = buffer.is_multiline();
        let wrap_width = multiline.then_some(node.content_size[0].max(1.));
        let request = tcx.has_native_editing().then(|| {
            Arc::new(
                tcx.paragraph_request(
                    &plico_renderer::style::Paint {
                        text: Some(text.clone()),
                        font_size: node.paint.font_size,
                        font_weight: node.paint.font_weight,
                        font_family: node.paint.font_family.clone(),
                        text_color: color,
                        ..Default::default()
                    },
                    wrap_width,
                )
                .expect("input always has presentation text"),
            )
        });
        let native = if let Some(request) = &request {
            let Some(paragraph) = tcx.native_paragraph(request.clone()) else {
                return;
            };
            paragraph
                .caret_rect(input.head)
                .is_some()
                .then_some(ParagraphLayout::Native(paragraph))
        } else {
            None
        };
        let layout = native.unwrap_or_else(|| {
            ParagraphLayout::Parley(plico_renderer::text::layout_text_styled(
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
                wrap_width,
            ))
        });
        let [x0, y0, x1, y1] = layout
            .caret_rect(input.head)
            .expect("editing provider has caret geometry");
        let caret = Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64);
        let metrics = layout.metrics();
        let width = node.content_size[0].max(1.);
        let height = node.content_size[1].max(1.);
        let mut offset = self.input_layouts.get(&entity).map_or(0., |g| g.offset);
        if focused && !multiline {
            if caret.x0 < offset as f64 {
                offset = caret.x0 as f32;
            }
            if caret.x1 > (offset + width) as f64 {
                offset = (caret.x1 as f32 - width).max(0.);
            }
        } else {
            offset = 0.;
        }
        offset = offset.clamp(0., (metrics.width - width + 1.).max(0.));
        let y = if multiline {
            let mut scroll = if focused {
                self.input_layouts
                    .get(&entity)
                    .map_or(0., |g| (-g.y).max(0.))
            } else {
                0.
            };
            if focused {
                if caret.y0 < f64::from(scroll) {
                    scroll = caret.y0 as f32;
                }
                if caret.y1 > f64::from(scroll + height) {
                    scroll = caret.y1 as f32 - height;
                }
            }
            -scroll.clamp(0., (metrics.height - height).max(0.))
        } else {
            ((height - metrics.height) / 2.).max(0.)
        };
        let mut scene = Scene::default();
        scene.push_clip_layer(
            Affine::IDENTITY,
            &Rect::new(0., 0., width as f64, height as f64),
        );
        let transform = Affine::translate((-(offset as f64), y as f64));
        if focused && !empty {
            for [x0, y0, x1, y1] in
                layout.selection_rects(input.anchor.min(input.head)..input.anchor.max(input.head))
            {
                scene.fill(
                    Fill::NonZero,
                    transform,
                    Color::from_rgba8(59, 130, 246, 90),
                    None,
                    &Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64),
                );
            }
        }
        match &layout {
            ParagraphLayout::Native(_) => {
                scene.draw_paragraph(request.expect("native editing request"), transform)
            }
            ParagraphLayout::Parley(layout) => plico_renderer::scene::draw_text_layout_into(
                &mut scene,
                tcx,
                layout,
                Affine::IDENTITY,
                transform,
                1.,
            ),
        }
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
            key,
            placeholder: Arc::from(placeholder),
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
    fn input_position(
        &self,
        entity: Entity,
        point: Point,
    ) -> Option<(usize, Option<(usize, usize)>)> {
        let geometry = self.input_layouts.get(&entity)?;
        let placed = &self.placed[*self.placed_indices.get(&self.nodes.get(&entity)?.id)?];
        let point = self.layout_point(placed.node_id, point.x, point.y)?;
        let x = point.x as f32 - placed.content_origin[0] + geometry.offset;
        let y = point.y as f32 - placed.content_origin[1] - geometry.y;
        Some((
            geometry.layout.hit_test(x, y),
            geometry.layout.line_range_at(y).map(|r| (r.start, r.end)),
        ))
    }
    pub(super) fn input_pointer(
        &mut self,
        engine: &Engine,
        entity: Option<Entity>,
        p: plico_renderer::PointerEvent,
    ) {
        let Some(entity) = entity else {
            self.input_pointers.last = None;
            return;
        };
        let Some((buffer, _)) = engine.input(entity) else {
            self.input_pointers.last = None;
            return;
        };
        let Some((byte, visual_range)) = self.input_position(entity, p.position) else {
            return;
        };
        let now = engine.time().now;
        let value_revision = buffer.value_revision();
        let count = if p.modifiers.shift() {
            1
        } else {
            self.input_pointers
                .last
                .as_ref()
                .filter(|last| {
                    last.entity == entity
                        && last.pointer == p.properties.id
                        && last.pointer_type == p.properties.pointer_type
                        && last.value_revision == value_revision
                        && now.checked_sub(last.time).is_some_and(|elapsed| {
                            self.multi_click_settings()
                                .accepts(last.point, p.position, elapsed)
                        })
                })
                .map_or(1, |last| (last.count + 1).min(3))
        };
        self.input_pointers.last = (!p.modifiers.shift()).then_some(InputPress {
            entity,
            pointer: p.properties.id,
            pointer_type: p.properties.pointer_type,
            point: p.position,
            time: now,
            count,
            value_revision,
        });
        match count {
            2 => buffer.edit(InputEdit::SelectWord {
                at: byte,
                extend_from: None,
            }),
            3 => buffer.edit(InputEdit::SelectLine {
                at: byte,
                visual_range,
                extend_from: None,
            }),
            _ => buffer.edit(InputEdit::Select(byte, p.modifiers.shift())),
        }
        let (head, anchor) = buffer.selection_display_bytes();
        self.input_pointers.drag = Some(InputDrag {
            entity,
            pointer: p.properties.id,
            pointer_type: p.properties.pointer_type,
            unit: count,
            initial: (head.min(anchor), head.max(anchor)),
            value_revision,
        });
    }
    pub(super) fn input_pointer_event(
        &mut self,
        engine: &Engine,
        event: &UiEvent,
    ) -> Option<EventResponse> {
        use plico_renderer::{PointerButton, PointerPhase};
        match event {
            UiEvent::Focus(false)
            | UiEvent::TextInput(_)
            | UiEvent::Paste(_)
            | UiEvent::Ime(ImeEvent::Commit(_) | ImeEvent::Preedit { .. }) => {
                self.input_pointers = Default::default();
            }
            UiEvent::Key(key) if key.phase == KeyPhase::Down => {
                self.input_pointers = Default::default()
            }
            UiEvent::Pointer(p) => {
                if p.phase == PointerPhase::Down {
                    self.input_pointers.drag = None;
                    if p.button != Some(PointerButton::Primary)
                        || self
                            .hit(engine, p.position.x, p.position.y)
                            .and_then(|entity| engine.input(entity))
                            .is_none()
                    {
                        self.input_pointers.last = None;
                    }
                    return None;
                }
                let drag = self.input_pointers.drag?;
                if drag.pointer != p.properties.id || drag.pointer_type != p.properties.pointer_type
                {
                    return None;
                }
                let Some((buffer, _)) = engine.input(drag.entity) else {
                    self.input_pointers = Default::default();
                    return None;
                };
                if self.focused != Some(drag.entity)
                    || engine.node(drag.entity)?.style.disabled
                    || buffer.value_revision() != drag.value_revision
                {
                    self.input_pointers = Default::default();
                    return None;
                }
                if p.phase == PointerPhase::Move && p.buttons & 1 != 0 {
                    if self.input_pointers.last.as_ref().is_some_and(|last| {
                        !self.multi_click_settings().accepts(
                            last.point,
                            p.position,
                            std::time::Duration::ZERO,
                        )
                    }) {
                        self.input_pointers.last = None;
                    }
                    if let Some((byte, visual_range)) = self.input_position(drag.entity, p.position)
                    {
                        match drag.unit {
                            2 => buffer.edit(InputEdit::SelectWord {
                                at: byte,
                                extend_from: Some(drag.initial),
                            }),
                            3 => buffer.edit(InputEdit::SelectLine {
                                at: byte,
                                visual_range,
                                extend_from: Some(drag.initial),
                            }),
                            _ => buffer.edit(InputEdit::Select(byte, true)),
                        }
                        self.pointer_position = Some((p.position.x, p.position.y));
                        return Some(EventResponse {
                            handled: true,
                            request_redraw: true,
                            text_input: Some(true),
                            ..EventResponse::IGNORED
                        });
                    }
                }
                if matches!(p.phase, PointerPhase::Up | PointerPhase::Cancel)
                    || matches!(p.phase, PointerPhase::Move | PointerPhase::Leave)
                        && p.buttons & 1 == 0
                {
                    self.input_pointers.drag = None;
                    if p.phase != PointerPhase::Up {
                        self.input_pointers.last = None;
                    }
                }
            }
            _ => {}
        }
        None
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
        // Caret math matches what is painted: a revealed secret navigates by
        // its real bytes, a masked one by bullet offsets.
        let reveal = buffer.is_secret()
            && engine
                .node(entity)
                .is_some_and(|node| node.style.input_password_visible);
        let presentation = buffer.presentation_with(reveal);
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
                if buffer.is_composing()
                    && !matches!(key.key.as_str(), "Tab" | "Escape")
                    && !key.matches_standard_shortcut(StandardShortcut::Paste)
                {
                    return Some(EventResponse {
                        text_input: Some(true),
                        ..EventResponse::IGNORED
                    });
                }
                if engine.node(entity)?.style.input_navigation && key.modifiers.is_empty() {
                    let navigation = match key.key.as_str() {
                        "ArrowUp" => Some(plico::InputNavigation::Previous),
                        "ArrowDown" => Some(plico::InputNavigation::Next),
                        "PageUp" => Some(plico::InputNavigation::PageUp),
                        "PageDown" => Some(plico::InputNavigation::PageDown),
                        "Escape" if !buffer.is_composing() && !key.repeat => {
                            Some(plico::InputNavigation::Cancel)
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
                // Modifier-applied logical text may be a control character or
                // differ under an input method/layout. Use the event's standard
                // shortcut identity before matching ordinary navigation keys.
                let editing_key = if key.matches_standard_shortcut(StandardShortcut::Paste) {
                    "v"
                } else if key.matches_standard_shortcut(StandardShortcut::Copy) {
                    "c"
                } else if key.matches_standard_shortcut(StandardShortcut::Cut) {
                    "x"
                } else if key.matches_standard_shortcut(StandardShortcut::SelectAll) {
                    "a"
                } else {
                    key.key.as_str()
                };
                match editing_key {
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
                    "Home" | "End" if buffer.is_multiline() && !command => {
                        if let Some(geometry) = self.input_layouts.get(&entity)
                            && let Some([_, y0, _, y1]) =
                                geometry.layout.caret_rect(presentation.head)
                        {
                            let x = if editing_key == "Home" {
                                -1.
                            } else {
                                geometry.layout.metrics().width + 1.
                            };
                            let at = geometry.layout.hit_test(x, (y0 + y1) * 0.5);
                            buffer.edit(InputEdit::Select(at, shift));
                        }
                    }
                    "Home" => buffer.edit(InputEdit::Home(shift)),
                    "End" => buffer.edit(InputEdit::End(shift)),
                    "Enter" if buffer.is_multiline() && !command => {
                        buffer.edit(InputEdit::Insert(Zeroizing::new("\n".into())));
                    }
                    "ArrowUp" | "ArrowDown" if buffer.is_multiline() => {
                        if let Some(geometry) = self.input_layouts.get(&entity) {
                            let head = presentation.head;
                            if let Some([x0, y0, _, y1]) = geometry.layout.caret_rect(head) {
                                let direction = if editing_key == "ArrowUp" { -1. } else { 1. };
                                let line = geometry.layout.metrics().first_line_height;
                                let at = geometry
                                    .layout
                                    .hit_test(x0, (y0 + y1) * 0.5 + direction * line);
                                buffer.edit(InputEdit::Select(at, shift));
                            }
                        }
                    }
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
        // IME keeps the secret protections (surrounding text withheld,
        // Password purpose) even while the painted presentation reveals the
        // value: reveal is a display concern, not an input-method one.
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

#[cfg(test)]
mod shortcut_tests {
    use super::*;
    #[test]
    fn password_paste_uses_unmodified_key_even_with_ime_preedit() {
        let mut engine = Engine::default();
        let buffer = plico::TextBuffer::secret();
        engine
            .frame_declarations(|cx| {
                plico::widgets::text_input(cx, "key", &buffer).key("key");
            })
            .unwrap();
        let id = engine.find(engine.root(), "key").unwrap();
        let mut adapter = Adapter {
            focused: Some(id),
            ..Default::default()
        };
        buffer.edit(InputEdit::Preedit(Zeroizing::new("候选".into())));
        let mut event = UiEvent::Key(plico_renderer::KeyEvent {
            phase: KeyPhase::Down,
            key: "\u{16}".into(),
            key_without_modifiers: "v".into(),
            code: "KeyV".into(),
            text: None,
            text_with_all_modifiers: None,
            location: plico_renderer::KeyLocation::Standard,
            modifiers: if cfg!(target_os = "macos") {
                plico::Modifiers::META
            } else {
                plico::Modifiers::CONTROL
            },
            repeat: false,
            synthetic: false,
        });
        let response = adapter.input_event(&engine, &mut event).unwrap();
        assert!(matches!(response.clipboard, Some(ClipboardRequest::Read)));
        adapter
            .input_event(&engine, &mut UiEvent::Paste("test-key".into()))
            .unwrap();
        assert_eq!(buffer.with_value(|v| v.to_owned()), "test-key");
        assert!(!buffer.is_composing());
        assert!(buffer.text().is_none());
    }
}

#[cfg(test)]
mod multiline_tests {
    use super::*;
    use plico_renderer::FrameSource;
    struct View(plico::TextBuffer);
    impl crate::HostView for View {
        fn show(&mut self, cx: &mut plico::UiContext) {
            plico::widgets::text_input(cx, "Editor", &self.0)
                .key("editor")
                .width(120.)
                .height(100.);
        }
    }
    fn pointer(
        source: &mut crate::Source<View>,
        phase: plico_renderer::PointerPhase,
        position: Point,
    ) {
        use plico_renderer::*;
        source.handle_event(UiEvent::Pointer(PointerEvent {
            phase,
            position,
            button: matches!(phase, PointerPhase::Down | PointerPhase::Up)
                .then_some(PointerButton::Primary),
            buttons: u32::from(phase != PointerPhase::Up),
            modifiers: Modifiers::empty(),
            properties: PointerProperties::default(),
        }));
    }
    fn point(source: &crate::Source<View>, entity: Entity, byte: usize) -> Point {
        let g = &source.adapter.input_layouts[&entity];
        let node = &source.adapter.placed
            [source.adapter.placed_indices[&source.adapter.nodes[&entity].id]];
        let rect = g.layout.caret_rect(byte).unwrap();
        Point {
            x: f64::from(node.content_origin[0] + rect[0] - g.offset) + 0.1,
            y: f64::from(node.content_origin[1] + (rect[1] + rect[3]) * 0.5 + g.y),
        }
    }
    #[test]
    fn editor_double_click_drag_reverses_and_survives_pointer_leave() {
        use plico_renderer::PointerPhase::*;
        let buffer = plico::TextBuffer::new();
        buffer.set("one two three".to_owned().into());
        let clock = plico::VirtualClock::default();
        let mut source = crate::Source::with_clock(View(buffer.clone()), clock.clone());
        source.build_frame(&mut TextContext::new(), 240, 180);
        let entity = source.engine.find(source.engine.root(), "editor").unwrap();
        let at = point(&source, entity, 5);
        pointer(&mut source, Down, at);
        pointer(&mut source, Up, at);
        clock
            .advance(std::time::Duration::from_millis(100))
            .unwrap();
        pointer(&mut source, Down, at);
        assert_eq!(buffer.selected_text().as_deref(), Some("two"));
        pointer(&mut source, Leave, at);
        let right = point(&source, entity, 11);
        pointer(&mut source, Move, right);
        assert_eq!(buffer.selected_text().as_deref(), Some("two three"));
        let left = point(&source, entity, 1);
        pointer(&mut source, Move, left);
        assert_eq!(buffer.selected_text().as_deref(), Some("one two"));
        pointer(&mut source, Up, left);
        pointer(&mut source, Move, right);
        assert_eq!(buffer.selected_text().as_deref(), Some("one two"));
    }
    #[test]
    fn editing_or_focus_loss_cancels_pointer_selection() {
        use plico_renderer::PointerPhase::*;
        let buffer = plico::TextBuffer::new();
        buffer.set("one two three".to_owned().into());
        let mut source = crate::Source::new(View(buffer.clone()));
        source.build_frame(&mut TextContext::new(), 240, 180);
        let entity = source.engine.find(source.engine.root(), "editor").unwrap();
        let left = point(&source, entity, 1);
        let right = point(&source, entity, 11);
        pointer(&mut source, Down, left);
        buffer.set("new content".to_owned().into());
        let selection = buffer.selection_display_bytes();
        pointer(&mut source, Move, right);
        assert_eq!(buffer.selection_display_bytes(), selection);
        assert!(source.adapter.input_pointers.drag.is_none());
        pointer(&mut source, Down, left);
        source.handle_event(UiEvent::Focus(false));
        pointer(&mut source, Move, right);
        assert!(source.adapter.input_pointers.drag.is_none());
        assert!(source.adapter.input_pointers.last.is_none());
    }
    #[test]
    fn editor_triple_click_uses_wrapped_visual_line() {
        use plico_renderer::PointerPhase::*;
        let buffer = plico::TextBuffer::multiline();
        buffer.set("alpha beta gamma delta epsilon zeta".to_owned().into());
        let clock = plico::VirtualClock::default();
        let mut source = crate::Source::with_clock(View(buffer.clone()), clock.clone());
        source.build_frame(&mut TextContext::new(), 240, 180);
        let entity = source.engine.find(source.engine.root(), "editor").unwrap();
        let at = point(&source, entity, 2);
        let (_, range) = source.adapter.input_position(entity, at).unwrap();
        let (start, end) = range.unwrap();
        assert!(end < buffer.text().unwrap().len());
        for _ in 0..3 {
            pointer(&mut source, Down, at);
            pointer(&mut source, Up, at);
            clock
                .advance(std::time::Duration::from_millis(100))
                .unwrap();
        }
        assert_eq!(
            buffer.selected_text().as_deref(),
            Some(&buffer.text().unwrap()[start..end])
        );
    }
    #[test]
    fn multiline_wraps_at_top_and_enter_inserts_without_submitting() {
        let buffer = plico::TextBuffer::multiline();
        buffer.set("first".to_owned().into());
        let mut source = crate::Source::new(View(buffer.clone()));
        let mut text = TextContext::new();
        source.build_frame(&mut text, 240, 180);
        let entity = source.engine.find(source.engine.root(), "editor").unwrap();
        assert_eq!(source.adapter.input_layouts[&entity].y, 0.);
        source.adapter.focused = Some(entity);
        let mut enter = UiEvent::Key(plico_renderer::KeyEvent {
            key: "Enter".into(),
            code: "Enter".into(),
            phase: KeyPhase::Down,
            key_without_modifiers: "Enter".into(),
            text: None,
            text_with_all_modifiers: None,
            location: plico_renderer::KeyLocation::Standard,
            modifiers: Default::default(),
            repeat: false,
            synthetic: false,
        });
        source
            .adapter
            .input_event(&source.engine, &mut enter)
            .unwrap();
        source
            .adapter
            .input_event(&source.engine, &mut UiEvent::Paste("second\nthird".into()))
            .unwrap();
        assert_eq!(buffer.text().as_deref(), Some("first\nsecond\nthird"));
        assert_eq!(buffer.submitted_count(), 0);
        buffer.set("word ".repeat(60).into());
        source.build_frame(&mut text, 240, 180);
        let geometry = &source.adapter.input_layouts[&entity];
        assert!(geometry.layout.metrics().height > geometry.key.size[1]);
        assert!(geometry.y < 0., "caret should scroll into view");
        assert!(
            geometry.caret[1] >= 0. && geometry.caret[3] <= f64::from(geometry.key.size[1]) + 1.
        );
        source.adapter.focused = None;
        source.build_frame(&mut text, 240, 180);
        assert_eq!(source.adapter.input_layouts[&entity].y, 0.);
    }
    #[test]
    fn single_line_retains_vertical_centering() {
        let buffer = plico::TextBuffer::new();
        buffer.set("first".to_owned().into());
        let mut source = crate::Source::new(View(buffer));
        source.build_frame(&mut TextContext::new(), 240, 180);
        let entity = source.engine.find(source.engine.root(), "editor").unwrap();
        assert!(source.adapter.input_layouts[&entity].y > 20.);
    }
}
