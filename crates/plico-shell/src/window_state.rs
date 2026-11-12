//! Durable native window geometry and presentation state.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{ExtensionContext, ShellExtension, WindowResourceKey};
use plico_renderer::WindowOptions;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct SavedWindowState {
    width: u32,
    height: u32,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    #[serde(default)]
    maximized: bool,
    #[serde(default)]
    fullscreen: bool,
}

impl SavedWindowState {
    fn normal_size(self) -> SavedWindowSize {
        SavedWindowSize {
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
struct SavedWindowSize {
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct NormalState {
    size: Option<SavedWindowSize>,
    position: Option<(i32, i32)>,
}

/// Restore and persist the normal logical geometry and presentation state of
/// one native window.
///
/// The saved size/position come from the last non-maximized, non-fullscreen
/// frame, so unmaximizing does not restore a screen-sized window. The current
/// maximized/fullscreen flags are persisted separately. Legacy files that only
/// contain `width` and `height` remain valid.
pub struct WindowStatePersistence {
    path: PathBuf,
    window_key: WindowResourceKey,
    normal: NormalState,
    maximized: bool,
    fullscreen: bool,
}

/// Historical name of [`WindowStatePersistence`].
pub type WindowSizePersistence = WindowStatePersistence;

impl WindowStatePersistence {
    /// Restore valid saved geometry into `options` and create its persistence hook.
    pub fn restore(
        path: impl Into<PathBuf>,
        window_key: WindowResourceKey,
        options: &mut WindowOptions,
    ) -> Self {
        let path = path.into();
        let saved = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<SavedWindowState>(&bytes).ok())
            .filter(|state| valid_size(&state.normal_size()));
        let mut normal = NormalState::default();
        let mut maximized = false;
        let mut fullscreen = false;
        if let Some(saved) = saved {
            options.initial_inner_size = (saved.width, saved.height);
            if let Some((x, y)) = saved
                .x
                .zip(saved.y)
                .filter(|position| valid_position(*position))
            {
                options.initial_position = Some((x, y));
                options.centered = false;
                normal.position = Some((x, y));
            }
            maximized = saved.maximized && !saved.fullscreen;
            fullscreen = saved.fullscreen;
            options.maximized = maximized;
            options.fullscreen = fullscreen;
            *options = options.clone().normalized();
            normal.size = Some(SavedWindowSize {
                width: options.initial_inner_size.0,
                height: options.initial_inner_size.1,
            });
        }
        Self {
            path,
            window_key,
            normal,
            maximized,
            fullscreen,
        }
    }

    fn observe(&mut self, context: &mut ExtensionContext<'_>) {
        let Some(metrics) = context.window_metrics(self.window_key) else {
            return;
        };
        self.maximized = metrics.maximized;
        self.fullscreen = metrics.fullscreen;
        if metrics.maximized || metrics.fullscreen {
            return;
        }
        let size = SavedWindowSize {
            width: metrics.logical_width,
            height: metrics.logical_height,
        };
        if valid_size(&size) {
            self.normal.size = Some(size);
        }
        if let Some(position) = metrics.outer_x.zip(metrics.outer_y)
            && valid_position(position)
        {
            self.normal.position = Some(position);
        }
    }

    fn save(&self) {
        let Some(size) = self.normal.size else {
            return;
        };
        if let Some(parent) = self.path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(path = %parent.display(), %error, "failed to create window state directory");
            return;
        }
        let state = SavedWindowState {
            width: size.width,
            height: size.height,
            x: self.normal.position.map(|position| position.0),
            y: self.normal.position.map(|position| position.1),
            maximized: self.maximized,
            fullscreen: self.fullscreen,
        };
        let temporary = self.path.with_extension("json.tmp");
        let result = serde_json::to_vec(&state)
            .map_err(std::io::Error::other)
            .and_then(|bytes| std::fs::write(&temporary, bytes))
            .and_then(|()| replace_file(&temporary, &self.path));
        if let Err(error) = result {
            tracing::warn!(path = %self.path.display(), %error, "failed to persist window state");
        }
    }
}

impl ShellExtension for WindowStatePersistence {
    fn initialize(&mut self, _wake: plico_renderer::WakeCallback) -> Result<(), String> {
        Ok(())
    }

    fn poll(&mut self, context: &mut ExtensionContext<'_>) {
        self.observe(context);
    }

    fn window_metrics_changed(
        &mut self,
        window_key: WindowResourceKey,
        context: &mut ExtensionContext<'_>,
    ) {
        if window_key == self.window_key {
            self.observe(context);
        }
    }

    fn close_requested(
        &mut self,
        window_key: WindowResourceKey,
        context: &mut ExtensionContext<'_>,
    ) -> bool {
        if window_key == self.window_key {
            self.observe(context);
            self.save();
        }
        false
    }

    fn shutdown(&mut self, context: &mut ExtensionContext<'_>) {
        self.observe(context);
        self.save();
    }
}

fn valid_size(size: &SavedWindowSize) -> bool {
    (1..=16_384).contains(&size.width) && (1..=16_384).contains(&size.height)
}

fn valid_position((x, y): (i32, i32)) -> bool {
    (-1_000_000..=1_000_000).contains(&x) && (-1_000_000..=1_000_000).contains(&y)
}

fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    if cfg!(windows) && destination.exists() {
        std::fs::remove_file(destination)?;
    }
    std::fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_clamps_to_the_current_minimum_size() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window.json");
        std::fs::write(&path, br#"{"width":640,"height":480}"#).unwrap();
        let mut options = WindowOptions::new().min_inner_size(900, 600);

        let _state = WindowStatePersistence::restore(
            &path,
            crate::initial_window_resource_key(0),
            &mut options,
        );

        assert_eq!(options.initial_inner_size, (900, 600));
    }

    #[test]
    fn restored_size_respects_maximum_and_locked_dimensions() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window.json");
        std::fs::write(&path, br#"{"width":1200,"height":1600}"#).unwrap();
        let mut options = WindowOptions::new()
            .min_inner_size(480, 240)
            .max_inner_size(480, 900);
        let _state = WindowStatePersistence::restore(
            &path,
            crate::initial_window_resource_key(0),
            &mut options,
        );
        assert_eq!(options.initial_inner_size, (480, 900));
    }

    #[test]
    fn legacy_files_restore_size_only() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window.json");
        std::fs::write(&path, br#"{"width":1024,"height":720}"#).unwrap();
        let mut options = WindowOptions::new();
        let _state = WindowStatePersistence::restore(
            &path,
            crate::initial_window_resource_key(0),
            &mut options,
        );
        assert_eq!(options.initial_inner_size, (1024, 720));
        assert_eq!(options.initial_position, None);
        assert!(!options.maximized);
        assert!(!options.fullscreen);
    }

    #[test]
    fn restore_applies_position_and_presentation_state() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window.json");
        std::fs::write(
            &path,
            br#"{"width":900,"height":700,"x":120,"y":-40,"maximized":false,"fullscreen":true}"#,
        )
        .unwrap();
        let mut options = WindowOptions::new();
        let _state = WindowStatePersistence::restore(
            &path,
            crate::initial_window_resource_key(0),
            &mut options,
        );
        assert_eq!(options.initial_position, Some((120, -40)));
        assert!(options.fullscreen);
        assert!(!options.maximized);
    }

    #[test]
    fn invalid_positions_are_ignored() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("window.json");
        std::fs::write(&path, br#"{"width":900,"height":700,"x":-9999999,"y":0}"#).unwrap();
        let mut options = WindowOptions::new();
        let _state = WindowStatePersistence::restore(
            &path,
            crate::initial_window_resource_key(0),
            &mut options,
        );
        assert_eq!(options.initial_position, None);
    }

    #[test]
    fn saved_state_replaces_the_previous_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state/window.json");
        let state = WindowStatePersistence {
            path: path.clone(),
            window_key: crate::initial_window_resource_key(0),
            normal: NormalState {
                size: Some(SavedWindowSize {
                    width: 1280,
                    height: 840,
                }),
                position: Some((40, 60)),
            },
            maximized: true,
            fullscreen: false,
        };

        state.save();

        let saved: SavedWindowState =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.width, 1280);
        assert_eq!(saved.height, 840);
        assert_eq!((saved.x, saved.y), (Some(40), Some(60)));
        assert!(saved.maximized);
        assert!(!saved.fullscreen);
    }
}
