use floem_editor_core::{
    buffer::{Buffer, rope_text::RopeText},
    cursor::{CursorAffinity, CursorMode},
    editor::EditType,
    selection::Selection,
};
use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

/// Shared document identity. Cloning shares edits and undo history, not text copies.
/// This host-owned handle is not serialized across a reload boundary.
#[derive(Clone)]
pub struct CodeEditorState(pub(crate) Arc<Mutex<Document>>);
pub(crate) struct Document {
    pub buffer: Buffer,
    pub anchor: usize,
    pub head: usize,
    pub revision: u64,
}
impl CodeEditorState {
    pub fn new(text: &str) -> Self {
        Self(Arc::new(Mutex::new(Document {
            buffer: Buffer::new(text),
            anchor: 0,
            head: 0,
            revision: 0,
        })))
    }
    /// Explicit text export. Rendering and ordinary edits do not call this.
    pub fn text(&self) -> String {
        self.0.lock().unwrap().buffer.to_string()
    }
    pub fn revision(&self) -> u64 {
        self.0.lock().unwrap().revision
    }
    /// UTF-8 byte offsets, in anchor/head order.
    pub fn selection(&self) -> [usize; 2] {
        let d = self.0.lock().unwrap();
        [d.anchor, d.head]
    }
    /// Returns false for non-UTF-8 boundaries or offsets outside the document.
    pub fn select(&self, anchor: usize, head: usize) -> bool {
        let mut d = self.0.lock().unwrap();
        if !d.boundary(anchor) || !d.boundary(head) {
            return false;
        }
        d.select(anchor, head);
        true
    }
    pub fn insert(&self, text: &str) {
        self.0.lock().unwrap().insert(text, EditType::InsertChars);
    }
    pub fn undo(&self) -> bool {
        self.0.lock().unwrap().history(false)
    }
    pub fn redo(&self) -> bool {
        self.0.lock().unwrap().history(true)
    }
    pub fn selected_text(&self) -> String {
        let d = self.0.lock().unwrap();
        d.buffer.slice_to_cow(d.range()).into_owned()
    }
}
impl Default for CodeEditorState {
    fn default() -> Self {
        Self::new("")
    }
}
impl Document {
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
    fn boundary(&self, offset: usize) -> bool {
        offset <= self.buffer.len()
            && (offset == self.buffer.len() || self.buffer.text().is_codepoint_boundary(offset))
    }
    fn cursor(&self) -> CursorMode {
        CursorMode::Insert(Selection::region(
            self.anchor,
            self.head,
            CursorAffinity::Forward,
        ))
    }
    pub fn select(&mut self, anchor: usize, head: usize) {
        self.buffer.reset_edit_type();
        self.anchor = anchor;
        self.head = head;
    }
    pub fn insert(&mut self, text: &str, kind: EditType) {
        if text.is_empty() && self.range().is_empty() {
            return;
        }
        let selection = Selection::region(self.anchor, self.head, CursorAffinity::Forward);
        let start = self.range().start;
        let before = self.cursor();
        let (_, delta, _) = self.buffer.edit([(&selection, text)], kind);
        self.buffer.set_cursor_before(before);
        let (_, inserted) = delta.summary();
        self.head = start + inserted;
        self.anchor = self.head;
        self.buffer.set_cursor_after(self.cursor());
        self.revision += 1;
    }
    pub fn history(&mut self, redo: bool) -> bool {
        self.buffer.reset_edit_type();
        let change = if redo {
            self.buffer.do_redo()
        } else {
            self.buffer.do_undo()
        };
        let Some((_, _, _, cursor)) = change else {
            return false;
        };
        if let Some(CursorMode::Insert(selection)) = cursor
            && let Some(region) = selection.regions().first()
        {
            self.anchor = region.start;
            self.head = region.end;
        }
        self.head = self.head.min(self.buffer.len());
        self.anchor = self.anchor.min(self.buffer.len());
        self.revision += 1;
        true
    }
    pub fn delete(&mut self, backward: bool) {
        let before = self.cursor();
        if self.range().is_empty() {
            self.anchor = if backward {
                self.buffer.prev_grapheme_offset(self.head, 1, 0)
            } else {
                self.buffer
                    .next_grapheme_offset(self.head, 1, self.buffer.len())
            };
        }
        if self.range().is_empty() {
            return;
        }
        self.insert("", EditType::Delete);
        self.buffer.set_cursor_before(before);
    }
}
