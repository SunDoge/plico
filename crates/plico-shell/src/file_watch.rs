//! Application-level native filesystem watching.
//!
//! [`FileWatcher`] wraps the platform's recommended backend (inotify, FSEvents,
//! or ReadDirectoryChangesW) and delivers coalesced events through a wake
//! callback, so application code never polls the filesystem on a timer.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use snafu::{ResultExt, Snafu};

use plico_renderer::WakeCallback;

/// Errors raised while creating or updating a native filesystem watch.
#[derive(Debug, Snafu)]
pub enum FileWatchError {
    /// The platform watcher could not start or accept the path.
    #[snafu(display("native file watch failed: {source}"))]
    Watch {
        /// Backend error.
        source: notify::Error,
    },
}

/// Coarse classification of a native filesystem notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileWatchKind {
    /// A path was created.
    Created,
    /// A path's contents or metadata changed.
    Modified,
    /// A path was removed.
    Removed,
    /// A path was renamed or moved.
    Renamed,
    /// A path was opened or read.
    Accessed,
    /// A notification the backend could not classify.
    Other,
    /// The watcher itself reported an error.
    Error,
}

/// One filesystem notification, already normalized across platforms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWatchEvent {
    /// What happened.
    pub kind: FileWatchKind,
    /// Affected paths. Renames usually report the old and new path here.
    pub paths: Vec<PathBuf>,
    /// Backend diagnostic for [`FileWatchKind::Error`].
    pub message: Option<String>,
}

impl FileWatchEvent {
    /// First affected path, if the backend reported one.
    pub fn path(&self) -> Option<&Path> {
        self.paths.first().map(PathBuf::as_path)
    }

    fn from_notify(event: Event) -> Self {
        let kind = match event.kind {
            EventKind::Create(_) => FileWatchKind::Created,
            EventKind::Modify(ModifyKind::Name(_)) => FileWatchKind::Renamed,
            EventKind::Modify(_) => FileWatchKind::Modified,
            EventKind::Remove(_) => FileWatchKind::Removed,
            EventKind::Access(_) => FileWatchKind::Accessed,
            _ => FileWatchKind::Other,
        };
        Self {
            kind,
            paths: event.paths,
            message: None,
        }
    }
}

/// Native recursive or single-directory filesystem watcher.
///
/// The wake callback is invoked from the backend's notification thread after
/// an event is queued. Hosts typically wake their event loop from it and call
/// [`Self::drain`] on the next iteration. Dropping the watcher stops events.
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    events: Arc<Mutex<VecDeque<FileWatchEvent>>>,
}

impl FileWatcher {
    /// Start a watcher and wake `wake` whenever an event becomes available.
    pub fn new(wake: WakeCallback) -> Result<Self, FileWatchError> {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let queue = Arc::clone(&events);
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            let event = match result {
                Ok(event) => FileWatchEvent::from_notify(event),
                Err(error) => FileWatchEvent {
                    kind: FileWatchKind::Error,
                    paths: Vec::new(),
                    message: Some(error.to_string()),
                },
            };
            if let Ok(mut queue) = queue.lock() {
                queue.push_back(event);
            }
            wake();
        })
        .context(WatchSnafu)?;
        Ok(Self { watcher, events })
    }

    /// Watch a file or directory. Adding the same path twice is harmless.
    pub fn watch(&mut self, path: impl AsRef<Path>, recursive: bool) -> Result<(), FileWatchError> {
        self.watcher
            .watch(
                path.as_ref(),
                if recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .context(WatchSnafu)
    }

    /// Stop watching a previously registered path.
    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> Result<(), FileWatchError> {
        self.watcher.unwatch(path.as_ref()).context(WatchSnafu)
    }

    /// Take all events queued since the previous drain.
    pub fn drain(&mut self) -> Vec<FileWatchEvent> {
        self.events
            .lock()
            .map(|mut queue| queue.drain(..).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for_event(
        watcher: &mut FileWatcher,
        predicate: impl Fn(&FileWatchEvent) -> bool,
    ) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if watcher.drain().iter().any(&predicate) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn reports_file_creation_and_modification() {
        let root = tempfile::tempdir().unwrap();
        let mut watcher = FileWatcher::new(Arc::new(|| {})).unwrap();
        watcher.watch(root.path(), false).unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, b"one").unwrap();
        assert!(
            wait_for_event(&mut watcher, |event| {
                event.kind == FileWatchKind::Created && event.path() == Some(path.as_path())
            }),
            "missing create event"
        );
        std::fs::write(&path, b"two").unwrap();
        assert!(
            wait_for_event(&mut watcher, |event| {
                event.kind == FileWatchKind::Modified && event.path() == Some(path.as_path())
            }),
            "missing modify event"
        );
    }

    #[test]
    fn reports_recursive_events_and_wakes() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        let woken = Arc::new(Mutex::new(false));
        let wake_flag = Arc::clone(&woken);
        let mut watcher = FileWatcher::new(Arc::new(move || {
            *wake_flag.lock().unwrap() = true;
        }))
        .unwrap();
        watcher.watch(root.path(), true).unwrap();
        let path = nested.join("deep.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert!(
            wait_for_event(&mut watcher, |event| event.path() == Some(path.as_path())),
            "missing recursive event"
        );
        assert!(*woken.lock().unwrap(), "wake callback was not invoked");
    }

    #[test]
    fn unwatch_stops_delivery() {
        let root = tempfile::tempdir().unwrap();
        let mut watcher = FileWatcher::new(Arc::new(|| {})).unwrap();
        watcher.watch(root.path(), false).unwrap();
        watcher.unwatch(root.path()).unwrap();
        let path = root.path().join("ignored.txt");
        std::fs::write(&path, b"x").unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(
            !watcher
                .drain()
                .iter()
                .any(|event| event.path() == Some(path.as_path()))
        );
    }
}
