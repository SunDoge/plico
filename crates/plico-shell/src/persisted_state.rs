//! Durable application persisted-state file (MyGO's WindowStateStore model).
//!
//! The engine's persisted-state storage - everything applications declared
//! through `persisted_state` - is one host-owned file, loaded before the
//! first frame and saved when the window closes and the app quits. A
//! damaged file is ignored and replaced by the next save; a failed save
//! keeps the bytes for the next one. Writes are atomic (temporary file +
//! rename), exactly as `window_state.rs` writes geometry.

use crate::{ExtensionContext, ShellExtension, WindowResourceKey};

/// Save the primary window's persisted-state storage to `path` at close and
/// shutdown, restoring it on [`ShellExtension::initialize`]. Sources without
/// persisted state (no snapshot) never write the file.
pub struct PersistedStatePersistence {
    path: std::path::PathBuf,
    /// Restored the file already; later polls only compare and save.
    restored: bool,
    /// Bytes last saved; polling skips an unchanged snapshot.
    last_saved: Option<Vec<u8>>,
}

impl PersistedStatePersistence {
    /// Persist to `path`: restored before the first frame, saved on window
    /// close and extension shutdown.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            path: path.into(),
            restored: false,
            last_saved: None,
        }
    }

    fn save(&mut self, context: &mut ExtensionContext<'_>) {
        let Some(bytes) = context.persisted_snapshot() else {
            return;
        };
        if self.last_saved.as_ref() == Some(&bytes) {
            return;
        }
        if let Some(parent) = self.path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(path = %parent.display(), %error, "failed to create persisted state directory");
            return;
        }
        let temporary = self.path.with_extension("plico.tmp");
        let result =
            std::fs::write(&temporary, &bytes).and_then(|()| replace_file(&temporary, &self.path));
        match result {
            Ok(()) => self.last_saved = Some(bytes),
            Err(error) => {
                tracing::warn!(path = %self.path.display(), %error, "failed to persist state")
            }
        }
    }
}

impl ShellExtension for PersistedStatePersistence {
    fn initialize(&mut self, _wake: plico_renderer::WakeCallback) -> Result<(), String> {
        Ok(())
    }

    /// Restore happens before the first frame: read the file, hand the bytes
    /// to the source, which either accepts them or keeps its defaults.
    /// Later polls save when the snapshot changed: the event loop only
    /// polls on real wakes (input, service, animation), so this is
    /// MyGO's debounced save for free - every user-driven change lands
    /// within one wake, without a timer and without a dedicated save
    /// moment that SIGKILL could still miss.
    fn poll(&mut self, context: &mut ExtensionContext<'_>) {
        if !self.restored {
            self.restored = true;
            match std::fs::read(&self.path) {
                Ok(bytes) => {
                    context.restore_persisted(&bytes);
                }
                // No file yet is the common first launch; a damaged file is
                // the source's to reject, keeping defaults active.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(path = %self.path.display(), %error, "persisted state could not be read");
                }
            }
        }
        // Polls only arrive on real wakes; save anything that changed since
        // the last snapshot. Close and shutdown below remain the final saves.
        self.save(context);
    }

    /// Pointer movement is the user's most common signal that editing may
    /// have happened; the change-detected save makes this cheap (no bytes,
    /// no file) until the source actually holds new values.
    fn pointer_moved(
        &mut self,
        _window_key: WindowResourceKey,
        _position: plico_renderer::Point,
        context: &mut ExtensionContext<'_>,
    ) {
        self.save(context);
    }

    fn close_requested(
        &mut self,
        _window_key: WindowResourceKey,
        context: &mut ExtensionContext<'_>,
    ) -> bool {
        self.save(context);
        false
    }

    fn shutdown(&mut self, context: &mut ExtensionContext<'_>) {
        self.save(context);
    }
}

fn replace_file(source: &std::path::Path, destination: &std::path::Path) -> std::io::Result<()> {
    if cfg!(windows) && destination.exists() {
        std::fs::remove_file(destination)?;
    }
    std::fs::rename(source, destination)
}
