//! Native editor storage. Deliberately neither Debug nor Serialize.
use std::{
    collections::VecDeque,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
};
use unicode_segmentation::UnicodeSegmentation;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone)]
pub struct TextBuffer(Arc<Mutex<State>>);
struct State {
    value: Zeroizing<String>,
    preedit: Zeroizing<String>,
    secret: bool,
    head: usize,
    anchor: usize,
    revision: u64,
    value_revision: u64,
    submitted: u64,
    navigation: VecDeque<InputNavigation>,
}
/// Navigation emitted by an opt-in search/command input, without moving editor focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputNavigation {
    Previous,
    Next,
    PageUp,
    PageDown,
    Cancel,
}
/// Text insertion owns and zeroizes its temporary value.
pub enum InputEdit {
    Navigate(InputNavigation),
    Insert(Zeroizing<String>),
    Preedit(Zeroizing<String>),
    ClearPreedit,
    Backspace,
    Delete,
    Left(bool),
    Right(bool),
    Home(bool),
    End(bool),
    SelectAll,
    Select(usize, bool),
    DeleteSurrounding(usize, usize),
    Submit,
}
/// Safe to shape/paint: password buffers expose bullets only.
pub struct InputPresentation {
    pub text: String,
    pub head: usize,
    pub anchor: usize,
    pub composing: bool,
}
fn next_revision() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}
impl TextBuffer {
    pub fn new() -> Self {
        Self::create(false)
    }
    pub fn secret() -> Self {
        Self::create(true)
    }
    fn create(secret: bool) -> Self {
        Self(Arc::new(Mutex::new(State {
            value: Zeroizing::new(String::new()),
            preedit: Zeroizing::new(String::new()),
            secret,
            head: 0,
            anchor: 0,
            revision: next_revision(),
            value_revision: 0,
            submitted: 0,
            navigation: VecDeque::new(),
        })))
    }
    pub(crate) fn same_storage(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub(crate) fn invalidate(&self) {
        self.0.lock().unwrap().revision = next_revision();
    }
    pub(crate) fn take_navigation(&self) -> Option<InputNavigation> {
        self.0.lock().unwrap().navigation.pop_front()
    }
    pub fn is_composing(&self) -> bool {
        !self.0.lock().unwrap().preedit.is_empty()
    }
    pub fn selection_bytes(&self) -> (usize, usize) {
        let s = self.0.lock().unwrap();
        if s.secret { (0, 0) } else { (s.head, s.anchor) }
    }
    pub fn is_secret(&self) -> bool {
        self.0.lock().unwrap().secret
    }
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().value.is_empty()
    }
    pub fn revision(&self) -> u64 {
        self.0.lock().unwrap().revision
    }
    /// Submit counter; transports use it to detect Enter across images.
    pub fn submitted_count(&self) -> u64 {
        self.0.lock().unwrap().submitted
    }
    /// Committed content revision, excluding caret and preedit presentation changes.
    pub fn value_revision(&self) -> u64 {
        self.0.lock().unwrap().value_revision
    }
    /// Transport recovery/acknowledgment; does not insert text or create repeated events.
    #[doc(hidden)]
    pub fn restore_submit_count(&self, count: u64) {
        self.0.lock().unwrap().submitted = count;
    }
    pub(crate) fn counters(&self) -> (u64, u64) {
        let s = self.0.lock().unwrap();
        (s.value_revision, s.submitted)
    }
    /// Read ordinary text. Secret buffers refuse to return a plain String.
    pub fn text(&self) -> Option<String> {
        let s = self.0.lock().unwrap();
        (!s.secret).then(|| s.value.to_string())
    }
    /// Move the value out, leaving an empty editor. The caller owns zeroization.
    pub fn take(&self) -> Zeroizing<String> {
        let mut s = self.0.lock().unwrap();
        let result = std::mem::take(&mut s.value);
        s.preedit.zeroize();
        s.head = 0;
        s.anchor = 0;
        s.revision = next_revision();
        s.value_revision = s.value_revision.saturating_add(1);
        result
    }
    pub fn clear(&self) {
        drop(self.take());
    }
    pub fn set(&self, value: Zeroizing<String>) {
        self.edit(InputEdit::SelectAll);
        self.edit(InputEdit::Insert(value));
    }
    pub fn selected_text(&self) -> Option<String> {
        let s = self.0.lock().unwrap();
        (!s.secret).then(|| s.value[s.range()].to_owned())
    }
    pub fn selection_utf16(&self) -> Range<usize> {
        let s = self.0.lock().unwrap();
        if s.secret {
            return 0..0;
        }
        let r = s.range();
        s.value[..r.start].encode_utf16().count()..s.value[..r.end].encode_utf16().count()
    }
    pub fn presentation(&self) -> InputPresentation {
        let s = self.0.lock().unwrap();
        let range = s.range();
        let composing = !s.preedit.is_empty();
        if s.secret {
            let count = s.value.graphemes(true).count();
            let start = s.value[..range.start].graphemes(true).count();
            let head = s.value[..s.head].graphemes(true).count();
            let anchor = s.value[..s.anchor].graphemes(true).count();
            let pre = s.preedit.graphemes(true).count();
            let selected = s.value[range].graphemes(true).count();
            InputPresentation {
                text: "•".repeat(if composing {
                    count - selected + pre
                } else {
                    count
                }),
                head: if composing {
                    (start + pre) * 3
                } else {
                    head * 3
                },
                anchor: if composing {
                    (start + pre) * 3
                } else {
                    anchor * 3
                },
                composing,
            }
        } else {
            let mut text = s.value.to_string();
            let head = if composing {
                text.replace_range(range.clone(), &s.preedit);
                range.start + s.preedit.len()
            } else {
                s.head
            };
            InputPresentation {
                text,
                head,
                anchor: if composing { head } else { s.anchor },
                composing,
            }
        }
    }
    pub fn edit(&self, edit: InputEdit) {
        let mut s = self.0.lock().unwrap();
        let extend = |s: &mut State, head: usize, shift: bool| {
            s.head = head;
            if !shift {
                s.anchor = head;
            }
        };
        match edit {
            InputEdit::Navigate(action) => s.navigation.push_back(action),
            InputEdit::Preedit(value) => s.preedit = value,
            InputEdit::ClearPreedit => s.preedit.zeroize(),
            InputEdit::Insert(value) => {
                // Reject line breaks/control characters in single-line editors.
                let mut filtered = Zeroizing::new(String::new());
                for ch in value.chars().filter(|ch| !ch.is_control()) {
                    filtered.push(ch);
                }
                s.replace(&filtered);
                s.preedit.zeroize();
            }
            InputEdit::Backspace => {
                if s.head == s.anchor {
                    s.anchor = s.previous();
                }
                s.replace("");
            }
            InputEdit::Delete => {
                if s.head == s.anchor {
                    s.anchor = s.next();
                }
                s.replace("");
            }
            InputEdit::Left(shift) => {
                let at = if !shift && s.head != s.anchor {
                    s.range().start
                } else {
                    s.previous()
                };
                extend(&mut s, at, shift);
            }
            InputEdit::Right(shift) => {
                let at = if !shift && s.head != s.anchor {
                    s.range().end
                } else {
                    s.next()
                };
                extend(&mut s, at, shift);
            }
            InputEdit::Home(shift) => extend(&mut s, 0, shift),
            InputEdit::End(shift) => {
                let len = s.value.len();
                extend(&mut s, len, shift);
            }
            InputEdit::SelectAll => {
                s.anchor = 0;
                s.head = s.value.len();
            }
            InputEdit::Select(display_byte, shift) => {
                let at = if s.secret {
                    s.value
                        .grapheme_indices(true)
                        .nth(display_byte / 3)
                        .map_or(s.value.len(), |(i, _)| i)
                } else {
                    s.value
                        .grapheme_indices(true)
                        .map(|(i, _)| i)
                        .take_while(|i| *i <= display_byte)
                        .last()
                        .unwrap_or(0)
                        .min(s.value.len())
                };
                let at = if !s.secret && display_byte >= s.value.len() {
                    s.value.len()
                } else {
                    at
                };
                s.preedit.zeroize();
                extend(&mut s, at, shift);
            }
            InputEdit::DeleteSurrounding(before, after) => {
                let mut start = s.head.saturating_sub(before);
                let mut end = s.head.saturating_add(after).min(s.value.len());
                while !s.value.is_char_boundary(start) {
                    start -= 1;
                }
                while !s.value.is_char_boundary(end) {
                    end += 1;
                }
                s.anchor = start;
                s.head = end;
                s.replace("");
            }
            InputEdit::Submit => s.submitted = s.submitted.saturating_add(1),
        }
        s.revision = next_revision();
    }
}
impl State {
    fn range(&self) -> Range<usize> {
        self.head.min(self.anchor)..self.head.max(self.anchor)
    }
    fn previous(&self) -> usize {
        self.value[..self.head]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }
    fn next(&self) -> usize {
        self.value[self.head..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.value.len(), |(i, _)| self.head + i)
    }
    fn replace(&mut self, value: &str) {
        let range = self.range();
        if &self.value[range.clone()] == value {
            self.head = range.start + value.len();
            self.anchor = self.head;
            return;
        }
        // A new zeroizing buffer avoids leaving removed secret bytes in capacity.
        let mut next = Zeroizing::new(String::with_capacity(self.value.len() + value.len()));
        next.push_str(&self.value[..range.start]);
        next.push_str(value);
        next.push_str(&self.value[range.end..]);
        self.head = range.start + value.len();
        self.anchor = self.head;
        self.value = next;
        self.value_revision = self.value_revision.saturating_add(1);
    }
}
