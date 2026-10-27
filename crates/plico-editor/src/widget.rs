use crate::{CodeEditorState, document::Document};
use floem_editor_core::{buffer::rope_text::RopeText, editor::EditType};
use kurbo::{Affine, Rect};
use parley::{Affinity, Cursor, Layout, Selection};
use peniko::{Color, Fill};
use plico_renderer::{
    ImeEvent, KeyPhase, PaintContext, PaintScene, Point, PointerButton, PointerPhase, TextContext,
    UiEvent, Widget, WidgetChanges, WidgetEventResult, WidgetGeometry,
    style::TextAlign,
    text::{TextRun, layout_text_styled},
};
use std::sync::{Arc, LazyLock};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

const FONT: f32 = 14.;
const LINE: f32 = 23.;
const GUTTER: f32 = 60.;
const TOP: f32 = 8.;
// Initial highlighter is deliberately bounded; text editing remains available above this size.
const HIGHLIGHT_LIMIT: usize = 256 * 1024;
static SYNTAX: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

struct ShapedLine {
    number: usize,
    start: usize,
    end: usize,
    text: Arc<str>,
    layout: Arc<Layout<[u8; 4]>>,
    number_layout: Arc<Layout<[u8; 4]>>,
}
/// Native code surface. Attach to an explicitly sized slot with `SurfaceSource`.
/// Single cursor, no soft wrapping; Parley owns all visual hit testing.
pub struct CodeEditor {
    state: CodeEditorState,
    extension: String,
    family: Arc<str>,
    size: [f32; 2],
    scroll: [f32; 2],
    lines: Vec<ShapedLine>,
    styles: Vec<Arc<[TextRun]>>,
    highlighted_revision: Option<u64>,
    cache: Option<(u64, usize, String, usize, usize)>,
    preedit: String,
    preedit_cursor: usize,
    focused: bool,
    dragging: bool,
    reveal: bool,
    preferred_x: Option<f32>,
    pending_vertical: Option<(usize, f32, bool)>,
    caret: [f32; 4],
}
impl CodeEditor {
    pub fn new(state: CodeEditorState) -> Self {
        Self {
            state,
            extension: "rs".into(),
            family: Arc::from("monospace"),
            size: [0.; 2],
            scroll: [0.; 2],
            lines: Vec::new(),
            styles: Vec::new(),
            highlighted_revision: None,
            cache: None,
            preedit: String::new(),
            preedit_cursor: 0,
            focused: false,
            dragging: false,
            reveal: true,
            preferred_x: None,
            pending_vertical: None,
            caret: [0.; 4],
        }
    }
    pub fn language(mut self, extension: &str) -> Self {
        self.extension = extension.into();
        self
    }
    pub fn state(&self) -> CodeEditorState {
        self.state.clone()
    }
    pub fn scroll_offset(&self) -> [f32; 2] {
        self.scroll
    }
    fn shape(
        tcx: &mut TextContext,
        text: Arc<str>,
        runs: Arc<[TextRun]>,
        family: &Arc<str>,
        color: [u8; 4],
    ) -> Arc<Layout<[u8; 4]>> {
        layout_text_styled(
            tcx,
            text,
            FONT,
            400.,
            false,
            Some((LINE, false)),
            TextAlign::Start,
            color,
            runs,
            Some(family),
            None,
        )
    }
    fn highlight(&mut self, d: &Document) {
        if self.highlighted_revision == Some(d.revision) {
            return;
        }
        self.styles.clear();
        if d.buffer.len() <= HIGHLIGHT_LIMIT {
            let syntax = SYNTAX
                .find_syntax_by_extension(&self.extension)
                .unwrap_or_else(|| SYNTAX.find_syntax_plain_text());
            let mut highlighter = HighlightLines::new(syntax, &THEMES.themes["base16-ocean.dark"]);
            for number in 0..=d.buffer.last_line() {
                let line = d.buffer.line_content(number);
                let mut start = 0;
                let runs = highlighter
                    .highlight_line(&line, &SYNTAX)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(style, part)| {
                        let end = start + part.len();
                        let limit = line.trim_end_matches(['\r', '\n']).len();
                        let range = start.min(limit)..end.min(limit);
                        start = end;
                        TextRun {
                            range,
                            font_size: FONT,
                            font_weight: 400.,
                            font_italic: false,
                            line_height: Some((LINE, false)),
                            color: [
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                                255,
                            ],
                        }
                    })
                    .collect::<Vec<_>>();
                self.styles.push(runs.into());
            }
        }
        self.highlighted_revision = Some(d.revision);
    }
    fn refresh(&mut self, tcx: &mut TextContext) {
        let state = self.state.clone();
        let mut d = state.0.lock().unwrap();
        if self.highlighted_revision != Some(d.revision) {
            self.reveal = true;
        }
        self.highlight(&d);
        // Composition is a temporary projection; committed text and undo stay untouched.
        let preview = if self.preedit.is_empty() {
            None
        } else {
            let mut rope = d.buffer.text().clone();
            rope.edit(d.range(), self.preedit.as_str());
            Some(floem_editor_core::buffer::Buffer::new(rope))
        };
        let buffer = preview.as_ref().unwrap_or(&d.buffer);
        let mut head = if preview.is_some() {
            d.range().start + self.preedit_cursor
        } else {
            d.head
        };
        let rows = ((self.size[1] - TOP * 2.) / LINE).floor().max(1.) as usize;
        if self.reveal {
            let row = buffer.line_of_offset(head);
            let first = (self.scroll[1] / LINE).floor() as usize;
            if row < first {
                self.scroll[1] = row as f32 * LINE;
            } else if row >= first + rows {
                self.scroll[1] = (row + 1 - rows) as f32 * LINE;
            }
        }
        self.scroll[1] = self.scroll[1].clamp(
            0.,
            ((buffer.last_line() + 1) as f32 * LINE + TOP * 2. - self.size[1]).max(0.),
        );
        let first = (self.scroll[1] / LINE).floor() as usize;
        let (anchor_key, head_key) = if self.preedit.is_empty() {
            (0, 0)
        } else {
            (d.anchor, d.head)
        };
        let key = (
            d.revision,
            first,
            self.preedit.clone(),
            anchor_key,
            head_key,
        );
        let count = (rows + 2).min(buffer.last_line().saturating_sub(first) + 1);
        if self.cache.as_ref() != Some(&key) || self.lines.len() != count {
            self.lines.clear();
            for number in first..first + count {
                let start = buffer.offset_of_line(number);
                let content = buffer.line_content(number);
                let content = content.trim_end_matches(['\r', '\n']);
                let end = start + content.len();
                let composing = !self.preedit.is_empty();
                let value = content.to_owned();
                let text: Arc<str> = Arc::from(value);
                let runs = if composing {
                    Arc::from([])
                } else {
                    self.styles
                        .get(number)
                        .cloned()
                        .unwrap_or_else(|| Arc::from([]))
                };
                let layout =
                    Self::shape(tcx, text.clone(), runs, &self.family, [216, 222, 233, 255]);
                let number_layout = Self::shape(
                    tcx,
                    Arc::from((number + 1).to_string()),
                    Arc::from([]),
                    &self.family,
                    [110, 124, 145, 255],
                );
                self.lines.push(ShapedLine {
                    number,
                    start,
                    end,
                    text,
                    layout,
                    number_layout,
                });
            }
            self.cache = Some(key);
        }
        if let Some((target, x, shift)) = self.pending_vertical.take()
            && let Some(line) = self.lines.iter().find(|line| line.number == target)
        {
            let offset = Cursor::from_point(&line.layout, x, LINE / 2.)
                .index()
                .min(line.end - line.start);
            head = line.start + offset;
            let anchor = if shift { d.anchor } else { head };
            d.select(anchor, head);
            self.reveal = true;
        }
        if let Some(line) = self.lines.iter().find(|l| head >= l.start && head <= l.end) {
            let index = head - line.start;
            let c = Cursor::from_byte_index(&line.layout, index, Affinity::Downstream)
                .geometry(&line.layout, 1.);
            if self.reveal {
                let width = (self.size[0] - GUTTER - 12.).max(1.);
                if c.x0 < self.scroll[0] as f64 {
                    self.scroll[0] = c.x0.max(0.) as f32;
                }
                if c.x1 > (self.scroll[0] + width) as f64 {
                    self.scroll[0] = (c.x1 as f32 - width).max(0.);
                }
            }
            let x = GUTTER - self.scroll[0];
            let y = TOP + line.number as f32 * LINE - self.scroll[1];
            self.caret = [
                c.x0 as f32 + x,
                c.y0 as f32 + y,
                c.x1 as f32 + x,
                c.y1 as f32 + y,
            ];
        }
        self.reveal = false;
    }
    fn hit(&self, point: Point) -> Option<usize> {
        let row = ((point.y as f32 - TOP + self.scroll[1]).max(0.) / LINE).floor() as usize;
        let line = self.lines.iter().min_by_key(|l| l.number.abs_diff(row))?;
        let cursor = Cursor::from_point(
            &line.layout,
            point.x as f32 - GUTTER + self.scroll[0],
            LINE / 2.,
        );
        let offset = cursor.index().min(line.text.len());
        let projected = (line.start + offset).min(line.end);
        if self.preedit.is_empty() {
            Some(projected)
        } else {
            let d = self.state.0.lock().unwrap();
            let range = d.range();
            Some(if projected <= range.start {
                projected
            } else if projected < range.start + self.preedit.len() {
                range.start
            } else {
                projected - self.preedit.len() + range.len()
            })
        }
    }
    fn changed(&mut self) {
        self.reveal = true;
        self.preferred_x = None;
    }
    fn move_to(&mut self, position: usize, shift: bool) {
        let mut d = self.state.0.lock().unwrap();
        let anchor = if shift { d.anchor } else { position };
        d.select(anchor, position);
        drop(d);
        self.reveal = true;
    }
    fn key(&mut self, key: &str, shift: bool, primary: bool) -> WidgetEventResult {
        if !self.preedit.is_empty() {
            return WidgetEventResult::IGNORED;
        }
        if primary {
            match key.to_ascii_lowercase().as_str() {
                "a" => {
                    let mut d = self.state.0.lock().unwrap();
                    let end = d.buffer.len();
                    d.select(0, end);
                }
                "z" => {
                    if shift {
                        self.state.redo();
                    } else {
                        self.state.undo();
                    }
                    self.changed();
                }
                "y" => {
                    self.state.redo();
                    self.changed();
                }
                "c" => return WidgetEventResult::copy(self.state.selected_text()),
                "x" => {
                    let text = self.state.selected_text();
                    self.state.0.lock().unwrap().insert("", EditType::Cut);
                    self.changed();
                    return WidgetEventResult::copy_with_value_change(text);
                }
                "v" => return WidgetEventResult::paste(),
                _ => return WidgetEventResult::IGNORED,
            }
            return WidgetEventResult::handled_consuming_key_text();
        }
        match key {
            "Backspace" | "Delete" => {
                self.state.0.lock().unwrap().delete(key == "Backspace");
                self.changed();
            }
            "Enter" => {
                self.state
                    .0
                    .lock()
                    .unwrap()
                    .insert("\n", EditType::InsertNewline);
                self.changed();
            }
            "Tab" if !shift => {
                self.state
                    .0
                    .lock()
                    .unwrap()
                    .insert("    ", EditType::Indent);
                self.changed();
            }
            "ArrowLeft" | "ArrowRight" | "Home" | "End" => {
                let d = self.state.0.lock().unwrap();
                let pos = match key {
                    "Home" => d.buffer.offset_of_line(d.buffer.line_of_offset(d.head)),
                    "End" => d
                        .buffer
                        .line_end_offset(d.buffer.line_of_offset(d.head), true),
                    "ArrowLeft" if !shift && !d.range().is_empty() => d.range().start,
                    "ArrowRight" if !shift && !d.range().is_empty() => d.range().end,
                    "ArrowLeft" => d.buffer.prev_grapheme_offset(d.head, 1, 0),
                    _ => d.buffer.next_grapheme_offset(d.head, 1, d.buffer.len()),
                };
                drop(d);
                self.move_to(pos, shift);
                self.preferred_x = None;
            }
            "ArrowUp" | "ArrowDown" | "PageUp" | "PageDown" => {
                let d = self.state.0.lock().unwrap();
                let row = d.buffer.line_of_offset(d.head);
                drop(d);
                let amount = if key.starts_with("Page") {
                    (self.size[1] / LINE).floor().max(1.) as usize
                } else {
                    1
                };
                let down = matches!(key, "ArrowDown" | "PageDown");
                let d = self.state.0.lock().unwrap();
                let target = if down {
                    (row + amount).min(d.buffer.last_line())
                } else {
                    row.saturating_sub(amount)
                };
                drop(d);
                let x = *self
                    .preferred_x
                    .get_or_insert(self.caret[0] - GUTTER + self.scroll[0]);
                // Offscreen vertical movement is completed after the target line is shaped.
                if let Some(line) = self.lines.iter().find(|l| l.number == target) {
                    let offset = Cursor::from_point(&line.layout, x, LINE / 2.)
                        .index()
                        .min(line.end - line.start);
                    self.move_to(line.start + offset, shift);
                } else {
                    self.pending_vertical = Some((target, x, shift));
                    self.scroll[1] = target as f32 * LINE;
                    self.reveal = false;
                    self.cache = None;
                }
            }
            _ => return WidgetEventResult::IGNORED,
        }
        WidgetEventResult::handled_consuming_key_text()
    }
}
impl Widget for CodeEditor {
    fn layout_changed(&mut self, geometry: WidgetGeometry) {
        if self.size != geometry.content_size {
            self.size = geometry.content_size;
            self.cache = None;
        }
    }
    fn prepare_for_event(&mut self, tcx: &mut TextContext) -> WidgetChanges {
        self.refresh(tcx);
        WidgetChanges::empty()
    }
    fn focus_changed(&mut self, focused: bool) -> WidgetChanges {
        self.focused = focused;
        if !focused {
            self.preedit.clear();
            self.preedit_cursor = 0;
            self.dragging = false;
        }
        WidgetChanges::REDRAW
    }
    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        self.size = cx.size();
        self.refresh(cx.text());
        let rect = Rect::new(0., 0., self.size[0] as f64, self.size[1] as f64);
        cx.scene_mut().fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(24, 29, 39),
            None,
            &rect,
        );
        let state = self.state.clone();
        let d = state.0.lock().unwrap();
        let range = d.range();
        // Keep code, selections and horizontal scrolling out of the fixed gutter.
        cx.scene_mut().push_clip_layer(
            Affine::IDENTITY,
            &Rect::new(GUTTER as f64, 0., rect.x1, rect.y1),
        );
        for line in &self.lines {
            let y = TOP + line.number as f32 * LINE - self.scroll[1];
            let origin = [(GUTTER - self.scroll[0]) as f64, y as f64];
            let start = range.start.max(line.start);
            let end = range.end.min(line.end);
            if start < end && self.preedit.is_empty() {
                let a =
                    Cursor::from_byte_index(&line.layout, start - line.start, Affinity::Downstream);
                let b =
                    Cursor::from_byte_index(&line.layout, end - line.start, Affinity::Downstream);
                for (r, _) in Selection::new(a, b).geometry(&line.layout) {
                    cx.scene_mut().fill(
                        Fill::NonZero,
                        Affine::translate((origin[0], origin[1])),
                        Color::from_rgb8(53, 74, 108),
                        None,
                        &Rect::new(r.x0, r.y0, r.x1, r.y1),
                    );
                }
            }
            cx.draw_text_layout(&line.layout, origin);
            if !self.preedit.is_empty()
                && d.range().start >= line.start
                && d.range().start <= line.end
            {
                let a = Cursor::from_byte_index(
                    &line.layout,
                    d.range().start - line.start,
                    Affinity::Downstream,
                )
                .geometry(&line.layout, 1.);
                let b = Cursor::from_byte_index(
                    &line.layout,
                    d.range().start - line.start + self.preedit.len(),
                    Affinity::Downstream,
                )
                .geometry(&line.layout, 1.);
                cx.scene_mut().fill(
                    Fill::NonZero,
                    Affine::translate((origin[0], origin[1])),
                    Color::from_rgb8(144, 190, 255),
                    None,
                    &Rect::new(a.x0, a.y1 - 2., b.x0.max(a.x0 + 1.), a.y1 - 1.),
                );
            }
        }
        if self.focused {
            let [x0, y0, x1, y1] = self.caret;
            cx.scene_mut().fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(208, 226, 255),
                None,
                &Rect::new(x0 as f64, y0 as f64, x1 as f64, y1 as f64),
            );
        }
        cx.scene_mut().pop_layer();
        for line in &self.lines {
            let y = TOP + line.number as f32 * LINE - self.scroll[1];
            cx.draw_text_layout(
                &line.number_layout,
                [
                    GUTTER as f64 - 12. - line.number_layout.width() as f64,
                    y as f64,
                ],
            );
        }
    }
    fn handle_event(&mut self, event: &UiEvent) -> WidgetEventResult {
        match event {
            UiEvent::Key(k) if k.phase == KeyPhase::Down => {
                self.key(&k.key, k.modifiers.shift(), k.modifiers.primary_shortcut())
            }
            UiEvent::TextInput(text) if self.preedit.is_empty() => {
                self.state.insert(text);
                self.changed();
                WidgetEventResult::VALUE_CHANGED
            }
            UiEvent::Paste(text) => {
                self.preedit.clear();
                self.preedit_cursor = 0;
                self.state.0.lock().unwrap().insert(text, EditType::Paste);
                self.changed();
                WidgetEventResult::VALUE_CHANGED
            }
            UiEvent::Ime(ImeEvent::Preedit { text, cursor }) => {
                if text.contains(['\r', '\n']) {
                    return WidgetEventResult::IGNORED;
                }
                self.preedit = text.clone();
                self.preedit_cursor = cursor.map_or(text.len(), |(_, end)| end.min(text.len()));
                while !self.preedit.is_char_boundary(self.preedit_cursor) {
                    self.preedit_cursor -= 1;
                }
                self.reveal = true;
                WidgetEventResult::HANDLED
            }
            UiEvent::Ime(ImeEvent::Commit(text)) => {
                self.preedit.clear();
                self.preedit_cursor = 0;
                let mut d = self.state.0.lock().unwrap();
                d.buffer.reset_edit_type();
                d.insert(text, EditType::InsertChars);
                d.buffer.reset_edit_type();
                drop(d);
                self.changed();
                WidgetEventResult::VALUE_CHANGED
            }
            UiEvent::Ime(ImeEvent::DeleteSurrounding {
                before_bytes,
                after_bytes,
            }) => {
                self.preedit.clear();
                self.preedit_cursor = 0;
                let mut d = self.state.0.lock().unwrap();
                let mut start = d.head.saturating_sub(*before_bytes);
                let mut end = d.head.saturating_add(*after_bytes).min(d.buffer.len());
                while !d.buffer.text().is_codepoint_boundary(start) {
                    start -= 1;
                }
                while end < d.buffer.len() && !d.buffer.text().is_codepoint_boundary(end) {
                    end += 1;
                }
                d.select(start, end);
                d.insert("", EditType::Other);
                drop(d);
                self.changed();
                WidgetEventResult::VALUE_CHANGED
            }
            UiEvent::Ime(ImeEvent::Disabled) => {
                self.preedit.clear();
                self.preedit_cursor = 0;
                WidgetEventResult::HANDLED
            }
            UiEvent::Pointer(p)
                if p.phase == PointerPhase::Down && p.button == Some(PointerButton::Primary) =>
            {
                let hit = self.hit(p.position);
                self.preedit.clear();
                self.preedit_cursor = 0;
                self.cache = None;
                if let Some(pos) = hit {
                    self.move_to(pos, p.modifiers.shift());
                }
                self.dragging = true;
                self.preferred_x = None;
                WidgetEventResult::HANDLED
            }
            UiEvent::Pointer(p) if p.phase == PointerPhase::Move && self.dragging => {
                if let Some(pos) = self.hit(p.position) {
                    self.move_to(pos, true);
                }
                WidgetEventResult::HANDLED
            }
            UiEvent::Pointer(p)
                if matches!(
                    p.phase,
                    PointerPhase::Up | PointerPhase::Cancel | PointerPhase::Leave
                ) =>
            {
                self.dragging = false;
                WidgetEventResult::HANDLED
            }
            UiEvent::Wheel(w) => {
                if w.delta_x.is_finite() && w.delta_y.is_finite() {
                    self.scroll[0] = (self.scroll[0] + w.delta_x as f32).max(0.);
                    self.scroll[1] = (self.scroll[1] + w.delta_y as f32).max(0.);
                }
                self.reveal = false;
                WidgetEventResult::HANDLED
            }
            _ => WidgetEventResult::IGNORED,
        }
    }
    fn accepts_focus(&self) -> bool {
        true
    }
    fn accepts_text_input(&self) -> bool {
        true
    }
    fn ime_cursor_area(&self) -> Option<[f32; 4]> {
        self.focused.then_some(self.caret)
    }
    fn ime_state(&self) -> Option<plico_renderer::WidgetImeState> {
        if !self.focused {
            return None;
        }
        let d = self.state.0.lock().unwrap();
        let text = d.buffer.to_string();
        let head = text[..d.head].encode_utf16().count();
        let anchor = text[..d.anchor].encode_utf16().count();
        let marked = (!self.preedit.is_empty()).then(|| {
            text[..d.range().start].encode_utf16().count()
                ..text[..d.range().start].encode_utf16().count()
                    + self.preedit.encode_utf16().count()
        });
        Some(plico_renderer::WidgetImeState {
            cursor_area: self.caret,
            surrounding_cursor: d.head,
            surrounding_anchor: d.anchor,
            surrounding_text: text,
            selection_utf16: head.min(anchor)..head.max(anchor),
            selection_reversed: head < anchor,
            marked_range_utf16: marked,
            purpose: plico_renderer::WidgetImePurpose::Normal,
            multiline: true,
            completion: false,
            spellcheck: false,
        })
    }
}
