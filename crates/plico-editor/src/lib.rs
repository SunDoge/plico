//! Host-owned code editing backed by Floem's independent editing core.
//! The public API intentionally exposes no Floem types.
#![forbid(unsafe_code)]
mod document;
mod widget;
pub use document::CodeEditorState;
pub use widget::CodeEditor;
