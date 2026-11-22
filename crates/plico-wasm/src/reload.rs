use notify::{RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

/// Prepare replacement modules off the UI thread and wake the host when ready.
/// A failed or superseded build leaves the currently active instance with its owner.
pub struct Reloads<T> {
    _watcher: notify::RecommendedWatcher,
    latest: Arc<AtomicU64>,
    candidates: mpsc::Receiver<(u64, T)>,
}

impl<T: Send + 'static> Reloads<T> {
    pub fn watch(
        module: PathBuf,
        mut load: impl FnMut(&Path) -> Option<T> + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) -> std::io::Result<Self> {
        let module = std::path::absolute(module)?;
        let watched = module.clone();
        let latest = Arc::new(AtomicU64::new(0));
        let generation = latest.clone();
        let (changes, requests) = mpsc::channel();
        let (completed, candidates) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok_and(|event| {
                    !matches!(event.kind, notify::EventKind::Access(_))
                        && event.paths.contains(&watched)
                }) {
                    let id = generation.fetch_add(1, Ordering::Relaxed) + 1;
                    let _ = changes.send(id);
                }
            })
            .map_err(std::io::Error::other)?;
        watcher
            .watch(
                module.parent().ok_or(std::io::ErrorKind::InvalidInput)?,
                RecursiveMode::NonRecursive,
            )
            .map_err(std::io::Error::other)?;
        let generation = latest.clone();
        std::thread::Builder::new()
            .name("plico-wasm-loader".into())
            .spawn(move || {
                while let Ok(first) = requests.recv() {
                    let id = requests.try_iter().last().unwrap_or(first);
                    if let Some(module) = load(&module)
                        && id == generation.load(Ordering::Relaxed)
                    {
                        if completed.send((id, module)).is_err() {
                            break;
                        }
                        wake();
                    }
                }
            })?;
        Ok(Self {
            _watcher: watcher,
            latest,
            candidates,
        })
    }

    pub fn take_candidate(&self) -> Option<T> {
        let (id, module) = self.candidates.try_iter().last()?;
        (id == self.latest.load(Ordering::Relaxed)).then_some(module)
    }
}
