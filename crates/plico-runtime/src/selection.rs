//! Read-only paragraph selection uses the exact shaped layouts used for painting.
use super::*;
use plico_renderer::{ClipboardRequest, Point, PointerId, PointerType};
use std::{ops::Range, time::Duration};
use unicode_segmentation::UnicodeSegmentation;

struct Paragraph {
    entity: Entity,
    group: Entity,
    text: Arc<str>,
    layout: plico_renderer::paragraph::ParagraphLayout,
}
#[derive(Clone, Copy)]
struct Endpoint {
    entity: Entity,
    byte: usize,
}
struct Press {
    entity: Entity,
    pointer: PointerId,
    pointer_type: PointerType,
    position: Point,
    time: Duration,
    count: u8,
}
#[derive(Default)]
pub(super) struct TextSelection {
    paragraphs: Vec<Paragraph>,
    anchor: Option<Endpoint>,
    head: Option<Endpoint>,
    dragging: bool,
    last_press: Option<Press>,
    click_settings: plico_renderer::MultiClickSettings,
    unit: u8,
    start: Option<(Endpoint, Endpoint)>,
    pub(super) hovered: bool,
}
fn grapheme_floor(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .take_while(|(i, _)| *i <= byte)
        .last()
        .map_or(0, |(i, _)| i)
}
// Unicode segmentation keeps combining text and emoji sequences intact. This
// is UAX #29 word segmentation, not dictionary-based CJK word inference.
fn unit_range(text: &str, byte: usize, unit: u8) -> Range<usize> {
    let byte = grapheme_floor(text, byte);
    let range = if unit == 3 {
        let start = text[..byte]
            .char_indices()
            .rev()
            .find(|(_, c)| matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let end = text[byte..]
            .char_indices()
            .find(|(_, c)| matches!(c, '\n' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'))
            .map_or(text.len(), |(i, _)| byte + i);
        start..end
    } else {
        text.split_word_bound_indices()
            .find(|(start, word)| byte < start + word.len())
            .or_else(|| text.split_word_bound_indices().next_back())
            .map_or(0..0, |(start, word)| start..start + word.len())
    };
    let start = text
        .grapheme_indices(true)
        .take_while(|(i, _)| *i <= range.start)
        .last()
        .map_or(0, |(i, _)| i);
    let end = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .find(|i| *i >= range.end)
        .unwrap_or(text.len());
    start..end
}
/// Shared empty selection rects: `Arc::from([])` allocates a header on every
/// construction, and the no-selection path builds one per placed node per
/// refresh.
fn empty_rects() -> Arc<[[f32; 4]]> {
    static EMPTY: std::sync::OnceLock<Arc<[[f32; 4]]>> = std::sync::OnceLock::new();
    EMPTY.get_or_init(|| Vec::new().into()).clone()
}
impl Adapter {
    pub(super) fn set_multi_click_settings(
        &mut self,
        settings: plico_renderer::MultiClickSettings,
    ) {
        let settings = settings.normalized();
        if self.selection.click_settings != settings {
            self.selection.click_settings = settings;
            self.selection.last_press = None;
        }
    }

    fn selection_group(&self, engine: &Engine, id: NodeId) -> Option<Entity> {
        for placed in self.placed_chain(id) {
            let entity = self.reverse[&placed.node_id];
            let n = engine.node(entity)?;
            if n.style.disabled || n.kind.interactive() {
                return None;
            }
            if n.style.selectable {
                return Some(entity);
            }
        }
        None
    }
    pub(super) fn selection_work_needed(&self, engine: &Engine, geometry_changed: bool) -> bool {
        if self.selection_dirty {
            return true;
        }
        if self.selectable_nodes.is_empty() && self.selection.paragraphs.is_empty() {
            return false;
        }
        geometry_changed
            || self.paint_dirty.iter().any(|id| {
                self.reverse
                    .get(id)
                    .and_then(|e| engine.node(*e))
                    .is_none_or(|node| {
                        let caps = node.kind.capabilities();
                        // Leaf editing/visual controls cannot change a read-only
                        // selection elsewhere. Containers and selectable text remain
                        // conservative because they can affect eligibility/geometry.
                        !node.children.is_empty()
                            || (caps.contains(Capabilities::TEXT_SELECT)
                                && !caps.contains(Capabilities::TEXT_EDIT))
                    })
            })
    }
    pub(super) fn refresh_selection(&mut self, engine: &Engine, tcx: &mut TextContext) {
        #[cfg(feature = "profiling")]
        let _span = tracing::trace_span!(target: "plico::layout", "backend.selection").entered();
        // No selectable owner and no previous selection to invalidate. Avoid
        // walking/shaping every label after unrelated paint-only changes.
        if self.selectable_nodes.is_empty()
            && self.selection.paragraphs.is_empty()
            && self.selection.anchor.is_none()
            && self.selection.head.is_none()
        {
            return;
        }
        let mut paragraphs = Vec::new();
        for p in &self.placed {
            self.stats.selection_visits += 1;
            let entity = self.reverse[&p.node_id];
            if engine.node(entity).is_none_or(|n| {
                let caps = n.kind.capabilities();
                !caps.contains(Capabilities::TEXT_SELECT) || caps.contains(Capabilities::TEXT_EDIT)
            }) || !self.modal_allows(engine, entity)
            {
                continue;
            }
            if let Some(group) = self.selection_group(engine, p.node_id)
                && let Some(layout) = plico_renderer::scene::layout_node_paragraph(tcx, p)
                && let Some(text) = &p.paint.text
            {
                paragraphs.push(Paragraph {
                    entity,
                    group,
                    text: text.clone(),
                    layout,
                });
            }
        }
        let valid = [self.selection.anchor, self.selection.head]
            .into_iter()
            .flatten()
            .all(|end| {
                paragraphs
                    .iter()
                    .find(|p| p.entity == end.entity)
                    .is_some_and(|p| {
                        p.text.is_char_boundary(end.byte)
                            && grapheme_floor(&p.text, end.byte) == end.byte
                            && self
                                .selection
                                .paragraphs
                                .iter()
                                .find(|old| old.entity == p.entity)
                                .is_none_or(|old| p.text.starts_with(old.text.as_ref()))
                    })
            });
        if !valid {
            self.selection.anchor = None;
            self.selection.head = None;
            self.selection.dragging = false;
            self.selection.last_press = None;
            self.selection.start = None;
        }
        self.selection.paragraphs = paragraphs;
        let ranges = self.selected_ranges();
        if ranges.is_empty() {
            // No selection anywhere: only stale rects need clearing, and the
            // common case is that there are none to clear.
            for p in &mut self.placed {
                if p.paint.selection_rects.is_empty() {
                    continue;
                }
                p.paint.selection_rects = empty_rects();
                self.tree
                    .get_node_context_mut(p.node_id)
                    .unwrap()
                    .selection_rects = empty_rects();
                self.scene_dirty_nodes.insert(p.node_id);
                self.stats.paint_writes += 1;
            }
        } else {
            for p in &mut self.placed {
                let entity = self.reverse[&p.node_id];
                let rects: Arc<[[f32; 4]]> = if let Some((_, start, end)) =
                    ranges.iter().find(|(e, _, _)| *e == entity)
                    && let Some(para) = self
                        .selection
                        .paragraphs
                        .iter()
                        .find(|n| n.entity == entity)
                {
                    para.layout.selection_rects(*start..*end).into()
                } else {
                    empty_rects()
                };
                if p.paint.selection_rects != rects {
                    self.tree
                        .get_node_context_mut(p.node_id)
                        .unwrap()
                        .selection_rects = rects.clone();
                    p.paint.selection_rects = rects;
                    self.scene_dirty_nodes.insert(p.node_id);
                    self.stats.paint_writes += 1;
                }
            }
        }
    }
    fn selected_ranges(&self) -> Vec<(Entity, usize, usize)> {
        let Some(a) = self.selection.anchor else {
            return Vec::new();
        };
        let Some(b) = self.selection.head else {
            return Vec::new();
        };
        let paras = &self.selection.paragraphs;
        let Some(ai) = paras.iter().position(|p| p.entity == a.entity) else {
            return Vec::new();
        };
        let Some(bi) = paras.iter().position(|p| p.entity == b.entity) else {
            return Vec::new();
        };
        let ((ai, a), (bi, b)) = if (ai, a.byte) <= (bi, b.byte) {
            ((ai, a), (bi, b))
        } else {
            ((bi, b), (ai, a))
        };
        paras
            .iter()
            .enumerate()
            .skip(ai)
            .take(bi - ai + 1)
            .filter(|(_, p)| p.group == paras[ai].group)
            .map(|(i, p)| {
                (
                    p.entity,
                    if i == ai { a.byte } else { 0 },
                    if i == bi { b.byte } else { p.text.len() },
                )
            })
            .collect()
    }
    /// Plain displayed text, with paragraph separators; never Markdown syntax.
    pub fn selected_text(&self) -> Option<String> {
        let parts: Vec<_> = self
            .selected_ranges()
            .iter()
            .filter_map(|(e, a, b)| {
                self.selection
                    .paragraphs
                    .iter()
                    .find(|p| p.entity == *e)
                    .map(|p| p.text[*a..*b].to_string())
            })
            .collect();
        let text = parts.join("\n\n");
        (!text.is_empty()).then_some(text)
    }
    fn selection_point(&self, engine: &Engine, point: Point, dragging: bool) -> Option<Endpoint> {
        let group = self.selection.anchor.and_then(|a| {
            self.selection
                .paragraphs
                .iter()
                .find(|p| p.entity == a.entity)
                .map(|p| p.group)
        });
        let mut best = None;
        let mut distance = f64::INFINITY;
        for para in self.selection.paragraphs.iter().rev() {
            if !self.modal_allows(engine, para.entity) || dragging && Some(para.group) != group {
                continue;
            }
            let node = &self.placed[self.placed_indices[&self.nodes[&para.entity].id]];
            let Some(bounds) = self.visible_bounds(node.node_id) else {
                continue;
            };
            let inside = self.visual_contains(node.node_id, point.x, point.y);
            if !dragging && !inside {
                continue;
            }
            let d = if inside {
                0.
            } else {
                (point.y - f64::from(bounds[1])).min(0.).abs()
                    + (point.y - f64::from(bounds[3])).max(0.)
                    + (point.x - f64::from(bounds[0])).min(0.).abs()
                    + (point.x - f64::from(bounds[2])).max(0.)
            };
            if d >= distance {
                continue;
            }
            let local = self.layout_point(node.node_id, point.x, point.y)?;
            let byte = para.layout.hit_test(
                local.x as f32 - node.content_origin[0],
                local.y as f32 - node.content_origin[1],
            );
            best = Some(Endpoint {
                entity: para.entity,
                byte,
            });
            distance = d;
        }
        best.map(|mut end| {
            if let Some(para) = self
                .selection
                .paragraphs
                .iter()
                .find(|p| p.entity == end.entity)
            {
                end.byte = grapheme_floor(&para.text, end.byte);
            }
            end
        })
    }
    fn selection_unit(&self, point: Endpoint, unit: u8) -> (Endpoint, Endpoint) {
        let range = self
            .selection
            .paragraphs
            .iter()
            .find(|p| p.entity == point.entity)
            .map(|p| unit_range(&p.text, point.byte, unit))
            .unwrap_or(point.byte..point.byte);
        (
            Endpoint {
                byte: range.start,
                ..point
            },
            Endpoint {
                byte: range.end,
                ..point
            },
        )
    }
    fn endpoint_order(&self, point: Endpoint) -> (usize, usize) {
        (
            self.selection
                .paragraphs
                .iter()
                .position(|p| p.entity == point.entity)
                .unwrap_or(0),
            point.byte,
        )
    }
    fn drag_selection(&mut self, point: Endpoint) {
        if self.selection.unit > 1
            && let Some((start, end)) = self.selection.start
        {
            let (a, b) = self.selection_unit(point, self.selection.unit);
            if self.endpoint_order(point) < self.endpoint_order(start) {
                self.selection.anchor = Some(end);
                self.selection.head = Some(a);
            } else {
                self.selection.anchor = Some(start);
                self.selection.head = Some(if self.endpoint_order(b) < self.endpoint_order(end) {
                    end
                } else {
                    b
                });
            }
        } else {
            self.selection.head = Some(point);
        }
    }
    pub(super) fn selection_event(
        &mut self,
        engine: &Engine,
        event: &UiEvent,
    ) -> Option<EventResponse> {
        let mut handled = false;
        let mut clipboard = None;
        match event {
            UiEvent::Pointer(p) => {
                let hit = self.hit(engine, p.position.x, p.position.y);
                let point = self.selection_point(engine, p.position, self.selection.dragging);
                self.selection.hovered = point.is_some() && hit.is_none();
                if self.selection.dragging
                    && matches!(p.phase, PointerPhase::Move | PointerPhase::Up)
                    && self.selection.last_press.as_ref().is_some_and(|last| {
                        (last.position.x - p.position.x).powi(2)
                            + (last.position.y - p.position.y).powi(2)
                            > 16.
                    })
                {
                    self.selection.last_press = None;
                }
                match p.phase {
                    PointerPhase::Down if p.button == Some(PointerButton::Primary) => {
                        if let Some(point) = point.filter(|_| hit.is_none()) {
                            let same_group = self.selection.anchor.is_some_and(|a| {
                                let group = |entity| {
                                    self.selection
                                        .paragraphs
                                        .iter()
                                        .find(|p| p.entity == entity)
                                        .map(|p| p.group)
                                };
                                group(a.entity) == group(point.entity)
                            });
                            if !p.modifiers.shift() || !same_group {
                                self.selection.anchor = Some(point);
                            }
                            self.selection.head = Some(point);
                            let count = if p.modifiers.shift() {
                                1
                            } else {
                                self.selection
                                    .last_press
                                    .as_ref()
                                    .filter(|last| {
                                        last.entity == point.entity
                                            && last.pointer == p.properties.id
                                            && last.pointer_type == p.properties.pointer_type
                                            && engine.time().now.checked_sub(last.time).is_some_and(
                                                |elapsed| {
                                                    self.selection.click_settings.accepts(
                                                        last.position,
                                                        p.position,
                                                        elapsed,
                                                    )
                                                },
                                            )
                                    })
                                    .map_or(1, |last| (last.count + 1).min(3))
                            };
                            self.selection.last_press = (!p.modifiers.shift()).then_some(Press {
                                entity: point.entity,
                                pointer: p.properties.id,
                                pointer_type: p.properties.pointer_type,
                                position: p.position,
                                time: engine.time().now,
                                count,
                            });
                            self.selection.unit = count;
                            if count > 1 {
                                let (a, b) = self.selection_unit(point, count);
                                self.selection.anchor = Some(a);
                                self.selection.head = Some(b);
                            }
                            self.selection.start = self.selection.anchor.zip(self.selection.head);
                            self.selection.dragging = true;
                            if let Some((buffer, _)) = self.focused.and_then(|e| engine.input(e)) {
                                buffer.edit(plico::InputEdit::ClearPreedit);
                            }
                            self.focused = None;
                            self.pressed = None;
                            handled = true;
                        } else {
                            // Let the pointer event continue to its target, but
                            // invalidate highlights even if no backend node changes.
                            self.selection_dirty |=
                                self.selection.anchor.is_some() || self.selection.head.is_some();
                            self.selection.anchor = None;
                            self.selection.head = None;
                            self.selection.dragging = false;
                            self.selection.last_press = None;
                            self.selection.start = None;
                        }
                    }
                    PointerPhase::Move if self.selection.dragging => {
                        if let Some(point) = point {
                            self.drag_selection(point);
                        }
                        handled = true;
                    }
                    PointerPhase::Up
                        if self.selection.dragging && p.button == Some(PointerButton::Primary) =>
                    {
                        if let Some(point) = point {
                            self.drag_selection(point);
                        }
                        self.selection.dragging = false;
                        handled = true;
                    }
                    PointerPhase::Cancel => {
                        self.selection.dragging = false;
                        self.selection.last_press = None;
                        self.selection.start = None;
                    }
                    PointerPhase::Leave => {
                        self.selection.hovered = false;
                    }
                    _ => {}
                }
            }
            UiEvent::Key(k) if k.phase == KeyPhase::Down && self.selection.anchor.is_some() => {
                if k.modifiers.primary_shortcut() && matches!(k.key.as_str(), "c" | "C") {
                    clipboard = self.selected_text().map(ClipboardRequest::Write);
                    handled = true;
                } else if k.modifiers.primary_shortcut() && matches!(k.key.as_str(), "a" | "A") {
                    let group = self.selection.anchor.and_then(|a| {
                        self.selection
                            .paragraphs
                            .iter()
                            .find(|p| p.entity == a.entity)
                            .map(|p| p.group)
                    });
                    let mut paras = self
                        .selection
                        .paragraphs
                        .iter()
                        .filter(|p| Some(p.group) == group);
                    if let Some(first) = paras.next() {
                        let last = paras.next_back().unwrap_or(first);
                        self.selection.anchor = Some(Endpoint {
                            entity: first.entity,
                            byte: 0,
                        });
                        self.selection.head = Some(Endpoint {
                            entity: last.entity,
                            byte: last.text.len(),
                        });
                    }
                    handled = true;
                } else if k.key == "Escape" {
                    self.selection.anchor = None;
                    self.selection.head = None;
                    handled = true;
                }
            }
            UiEvent::Focus(false) => {
                self.selection.dragging = false;
                self.selection.last_press = None;
                self.selection.start = None;
            }
            _ => {}
        }
        if handled {
            self.selection_dirty = true;
        }
        handled.then_some(EventResponse {
            handled: true,
            request_redraw: true,
            consume_key_text: true,
            text_input: Some(false),
            clipboard,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico::prelude::*;
    use plico::{VirtualClock, widgets::text};
    use plico_renderer::{FrameSource, Modifiers, PointerEvent, PointerProperties};
    struct Demo;
    impl HostView for Demo {
        #[component]
        fn show(&mut self, cx: &mut UiContext<'_>) {
            column(cx).selectable(true).children(|cx| {
                text(cx, "one two three\nsecond line").font_size(20);
            });
        }
    }
    fn setup() -> (Source<Demo>, VirtualClock, TextContext) {
        let clock = VirtualClock::default();
        let mut source = Source::with_clock(Demo, clock.clone());
        let mut text = TextContext::new();
        source.build_frame(&mut text, 400, 160);
        assert_eq!(source.adapter.selection.paragraphs.len(), 1);
        (source, clock, text)
    }
    fn point(source: &Source<Demo>, byte: usize) -> Point {
        let para = &source.adapter.selection.paragraphs[0];
        let node = &source.adapter.placed
            [source.adapter.placed_indices[&source.adapter.nodes[&para.entity].id]];
        let rect = para.layout.caret_rect(byte).unwrap();
        Point {
            x: f64::from(node.content_origin[0] + rect[0]) + 0.1,
            y: f64::from(node.content_origin[1] + (rect[1] + rect[3]) * 0.5),
        }
    }
    fn pointer(source: &mut Source<Demo>, phase: PointerPhase, position: Point) {
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
    fn click(source: &mut Source<Demo>, at: Point) {
        pointer(source, PointerPhase::Down, at);
        pointer(source, PointerPhase::Up, at);
    }
    #[test]
    fn host_interval_uses_virtual_time_and_updates_reset_only_the_click_streak() {
        let (mut source, clock, _) = setup();
        let at = point(&source, 5);
        click(&mut source, at);
        clock.advance(Duration::from_millis(750)).unwrap();
        click(&mut source, at);
        assert!(
            source.adapter.selected_text().is_none(),
            "portable threshold is 500ms"
        );
        source.set_multi_click_settings(plico_renderer::MultiClickSettings {
            interval: Duration::from_secs(1),
            ..Default::default()
        });
        click(&mut source, at);
        clock.advance(Duration::from_millis(750)).unwrap();
        click(&mut source, at);
        assert_eq!(source.adapter.selected_text().as_deref(), Some("two"));
        source.set_multi_click_settings(plico_renderer::MultiClickSettings {
            interval: Duration::from_millis(100),
            ..Default::default()
        });
        assert_eq!(
            source.adapter.selected_text().as_deref(),
            Some("two"),
            "settings do not clear visible selection"
        );
        click(&mut source, at);
        clock.advance(Duration::from_millis(150)).unwrap();
        click(&mut source, at);
        assert!(
            source.adapter.selected_text().is_none(),
            "slow pair no longer selects a word"
        );
    }

    #[test]
    fn repeated_clicks_select_words_then_paragraphs_and_use_virtual_time() {
        let (mut source, clock, mut text) = setup();
        let at = point(&source, 5);
        click(&mut source, at);
        assert!(source.adapter.selected_text().is_none());
        clock.advance(Duration::from_millis(100)).unwrap();
        click(&mut source, at);
        assert_eq!(source.adapter.selected_text().as_deref(), Some("two"));
        source.build_frame(&mut text, 400, 160);
        assert!(
            source
                .adapter
                .placed
                .iter()
                .any(|n| !n.paint.selection_rects.is_empty())
        );
        clock.advance(Duration::from_millis(100)).unwrap();
        click(&mut source, at);
        assert_eq!(
            source.adapter.selected_text().as_deref(),
            Some("one two three")
        );
        clock.advance(Duration::from_millis(600)).unwrap();
        click(&mut source, at);
        assert!(source.adapter.selected_text().is_none());
    }
    #[test]
    fn double_click_drag_keeps_the_initial_word_when_reversing_direction() {
        let (mut source, clock, _) = setup();
        let at = point(&source, 5);
        click(&mut source, at);
        clock.advance(Duration::from_millis(100)).unwrap();
        pointer(&mut source, PointerPhase::Down, at);
        let end = point(&source, 10);
        pointer(&mut source, PointerPhase::Move, end);
        assert_eq!(source.adapter.selected_text().as_deref(), Some("two three"));
        let start = point(&source, 1);
        pointer(&mut source, PointerPhase::Move, start);
        pointer(&mut source, PointerPhase::Up, start);
        assert_eq!(source.adapter.selected_text().as_deref(), Some("one two"));
    }
    #[test]
    fn moving_to_another_word_and_cancelling_reset_the_click_sequence() {
        let (mut source, clock, _) = setup();
        let two = point(&source, 5);
        click(&mut source, two);
        clock.advance(Duration::from_millis(100)).unwrap();
        let one = point(&source, 1);
        click(&mut source, one);
        assert!(source.adapter.selected_text().is_none());
        clock.advance(Duration::from_millis(100)).unwrap();
        click(&mut source, one);
        assert_eq!(source.adapter.selected_text().as_deref(), Some("one"));
        pointer(&mut source, PointerPhase::Cancel, one);
        clock.advance(Duration::from_millis(100)).unwrap();
        click(&mut source, one);
        assert!(source.adapter.selected_text().is_none());
    }
    #[test]
    fn unicode_units_do_not_split_combining_sequences_emoji_or_crlf() {
        let text = "e\u{301} 👩‍👩‍👧‍👧 next";
        assert_eq!(&text[unit_range(text, 1, 2)], "e\u{301}");
        let emoji = text.find('👩').unwrap();
        let range = unit_range(text, emoji + 4, 2);
        assert_eq!(&text[range], "👩‍👩‍👧‍👧");
        assert_eq!(grapheme_floor(text, emoji + 4), emoji);
        assert_eq!(
            &"first\r\nsecond"[unit_range("first\r\nsecond", 8, 3)],
            "second"
        );
        assert_eq!(
            &"first\r\nsecond"[unit_range("first\r\nsecond", 6, 3)],
            "first"
        );
    }
}
