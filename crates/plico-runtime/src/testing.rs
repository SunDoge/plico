//! Deterministic, renderer-independent application testing.
//!
//! Commands can be replayed from JSON or sent directly from Rust. No window,
//! GPU, OS focus, pixel coordinates or sleeps are needed.
use crate::{Source, View};
use plico_core::{ViewNode, VirtualClock};
use plico_renderer::{
    FrameSource, KeyEvent, KeyLocation, KeyPhase, Modifiers, SemanticAction, SemanticRole,
    SemanticSnapshot, TextContext, UiEvent,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::time::Duration;

/// Value-only commands for a test transport. They never contain arena handles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Snapshot,
    Click {
        role: String,
        label: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    Focus {
        role: String,
        label: String,
        #[serde(default)]
        occurrence: Option<usize>,
    },
    SetValue {
        label: String,
        value: f64,
    },
    Text {
        text: String,
    },
    Key {
        key: String,
    },
}
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("unknown semantic role: {role}"))]
    UnknownRole { role: String },
    #[snafu(display(
        "target {role} {label:?}: found {matches} matches (specify occurrence for repeated labels)"
    ))]
    Target {
        role: String,
        label: String,
        matches: usize,
    },
    #[snafu(display("control rejected the requested action: {label}"))]
    Rejected { label: String },
    #[snafu(display("UI declaration failed: {message}"))]
    Declaration { message: String },
    #[snafu(display("invalid test viewport; dimensions must be nonzero"))]
    Viewport,
    #[snafu(display("virtual clock overflow"))]
    Clock,
}

fn find_target<V: View>(
    source: &Source<V>,
    role: &str,
    label: &str,
    occurrence: Option<usize>,
) -> Result<u64, Error> {
    let role_value =
        SemanticRole::from_name(role).ok_or_else(|| Error::UnknownRole { role: role.into() })?;
    let tree = source.adapter.semantics(&source.engine, 0);
    let matches: Vec<_> = tree
        .exposed_nodes()
        .into_iter()
        .filter(|n| n.role == role_value && n.label.as_deref() == Some(label))
        .collect();
    match occurrence {
        Some(index) => matches.get(index).copied(),
        None if matches.len() == 1 => matches.first().copied(),
        _ => None,
    }
    .map(|n| n.id)
    .ok_or_else(|| Error::Target {
        role: role.into(),
        label: label.into(),
        matches: matches.len(),
    })
}

/// Apply a shared command to an existing source, including the development host.
/// The caller builds the resulting frame and owns transport/serialization.
pub fn dispatch<V: View>(
    source: &mut Source<V>,
    text: &mut TextContext,
    command: &Command,
) -> Result<(), Error> {
    source.prepare_for_event(text);
    match command {
        Command::Snapshot => {}
        Command::Click {
            role,
            label,
            occurrence,
        }
        | Command::Focus {
            role,
            label,
            occurrence,
        } => {
            let target = find_target(source, role, label, *occurrence)?;
            let action = if matches!(command, Command::Click { .. }) {
                SemanticAction::Click { target }
            } else {
                SemanticAction::Focus { target }
            };
            if !source.handle_semantic_action(action) {
                return Err(Error::Rejected {
                    label: label.clone(),
                });
            }
        }
        Command::SetValue { label, value } => {
            let target = find_target(source, "slider", label, None)?;
            if !source.handle_semantic_action(SemanticAction::SetNumericValue {
                target,
                value: *value,
            }) {
                return Err(Error::Rejected {
                    label: label.clone(),
                });
            }
        }
        Command::Text { text } => {
            source.handle_event(UiEvent::TextInput(text.clone()));
        }
        Command::Key { key } => {
            source.handle_event(UiEvent::Key(KeyEvent {
                phase: KeyPhase::Down,
                key: key.clone(),
                key_without_modifiers: key.clone(),
                code: String::new(),
                text: None,
                text_with_all_modifiers: None,
                location: KeyLocation::Standard,
                modifiers: Modifiers::empty(),
                repeat: false,
                synthetic: false,
            }));
        }
    }
    Ok(())
}

/// A headless application with an explicitly advanced clock. Layout and input
/// use the real runtime; this does not simulate an OS window or GPU swapchain.
pub struct Driver<V: View> {
    pub source: Source<V>,
    text: TextContext,
    clock: VirtualClock,
    size: (u32, u32),
}
impl<V: View> Driver<V> {
    pub fn new(view: V, width: u32, height: u32) -> Result<Self, Error> {
        if width == 0 || height == 0 {
            return Err(Error::Viewport);
        }
        let clock = VirtualClock::default();
        let mut driver = Self {
            source: Source::with_clock(view, clock.clone()),
            text: TextContext::new(),
            clock,
            size: (width, height),
        };
        driver.frame()?;
        Ok(driver)
    }
    fn frame(&mut self) -> Result<(), Error> {
        self.source
            .build_frame(&mut self.text, self.size.0, self.size.1);
        if let Some(message) = &self.source.last_error {
            return Err(Error::Declaration {
                message: message.to_string(),
            });
        }
        Ok(())
    }
    pub fn execute(&mut self, command: Command) -> Result<SemanticSnapshot, Error> {
        dispatch(&mut self.source, &mut self.text, &command)?;
        self.frame()?;
        Ok(self.snapshot())
    }
    pub fn snapshot(&self) -> SemanticSnapshot {
        self.source.adapter.semantics(&self.source.engine, 0)
    }
    pub fn declarations(&self) -> Vec<ViewNode> {
        self.source.engine.snapshot()
    }
    pub fn advance(&mut self, duration: Duration) -> Result<SemanticSnapshot, Error> {
        self.clock.advance(duration).map_err(|_| Error::Clock)?;
        self.frame()?;
        Ok(self.snapshot())
    }
    pub fn resize(&mut self, width: u32, height: u32) -> Result<SemanticSnapshot, Error> {
        if width == 0 || height == 0 {
            return Err(Error::Viewport);
        }
        self.size = (width, height);
        self.frame()?;
        Ok(self.snapshot())
    }
}
