mod icons;
mod media;
mod ui;

use media::{Asset, Catalog, IMAGES, MediaError, VIDEOS};
use plico::{Theme, UiContext};
use plico_desktop::{
    EffectPayload, EffectQueue, EffectRequest, HostView, KeyEvent, WakeCallback, WindowOptions,
};
use plico_dev::App;
use plico_runtime::background::Worker;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::path::PathBuf;
#[cfg(feature = "video")]
use ui::Playback;
use ui::{Effect, Library};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Design {
    Framebank,
    Stillroom,
}

impl Design {
    fn name(self) -> &'static str {
        match self {
            Self::Framebank => "Framebank",
            Self::Stillroom => "Stillroom",
        }
    }
}

#[derive(usage_rs::Cli)]
#[usage(about = "Framebank — a local reference and media library")]
struct Options {
    /// Media files to add to the library on launch.
    files: Vec<PathBuf>,
    #[usage(long)]
    scenario: Option<PathBuf>,
    /// Write a PNG from the bundled sample library.
    #[usage(long)]
    capture: Option<PathBuf>,
    /// Record a scripted preview as MP4.
    #[usage(long)]
    record: Option<PathBuf>,
    #[usage(long, default = "30")]
    fps: u32,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not load the sample library: {source}"))]
    Samples { source: MediaError },
    #[snafu(display("Could not open the Framebank window: {source}"))]
    Window { source: plico_desktop::DesktopError },
    #[snafu(display("Could not capture preview: {source}"))]
    Preview { source: plico_desktop::PreviewError },
    #[snafu(display("Could not save preview: {source}"))]
    Capture { source: plico_desktop::CaptureError },
    #[snafu(display("--scenario requires --capture or --record; choose one output"))]
    Options,
    #[cfg(feature = "video")]
    #[snafu(display("Could not start the video player: {source}"))]
    Video { source: plico_video::Error },
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PreviewState {
    selected: Option<u64>,
}

enum LoadEvent {
    Asset(Box<Asset>),
    Failed(String),
    Complete,
}

struct Desktop {
    app: Library,
    worker: Worker<Vec<PathBuf>, LoadEvent>,
    catalog_path: Option<PathBuf>,
    stored: Catalog,
    pending: usize,
    effects: EffectQueue,
    sample_video: Option<tempfile::NamedTempFile>,
    #[cfg(feature = "video")]
    player: plico_video::Player,
}

impl Desktop {
    fn new(app: Library, #[cfg(feature = "video")] player: plico_video::Player) -> Self {
        let worker = Worker::new(
            || (),
            |_, files: Vec<PathBuf>, context| {
                for path in files {
                    if !context.is_current() {
                        break;
                    }
                    match media::load(path.clone()) {
                        Ok(asset) => context.emit(LoadEvent::Asset(Box::new(asset))),
                        Err(error) => {
                            context.emit(LoadEvent::Failed(format!("{} · {error}", path.display())))
                        }
                    }
                }
                context.emit(LoadEvent::Complete);
            },
        );
        let mut desktop = Self {
            catalog_path: Catalog::path(app.design.name()),
            app,
            worker,
            stored: Catalog::default(),
            pending: 0,
            effects: EffectQueue::default(),
            sample_video: None,
            #[cfg(feature = "video")]
            player,
        };
        desktop.restore();
        desktop
    }

    fn restore(&mut self) {
        let Some(path) = &self.catalog_path else {
            return;
        };
        match Catalog::read(path) {
            Ok(stored) => {
                for asset in &mut self.app.assets {
                    asset.favorite = stored.favorites.contains(&asset.fingerprint());
                }
                let files = stored.files.clone();
                self.stored = stored;
                self.import(files);
            }
            Err(error) => {
                self.app.notice = format!("Could not read library data: {error}");
                self.catalog_path = None;
            }
        }
    }

    fn persist(&mut self) {
        let Some(path) = &self.catalog_path else {
            return;
        };
        for asset in &self.app.assets {
            if let Some(file) = &asset.path
                && !self.stored.files.contains(file)
            {
                self.stored.files.push(file.clone());
            }
            let key = asset.fingerprint();
            let contains = self.stored.favorites.contains(&key);
            if asset.favorite && !contains {
                self.stored.favorites.push(key);
            } else if !asset.favorite && contains {
                self.stored.favorites.retain(|favorite| *favorite != key);
            }
        }
        if let Err(error) = self.stored.write(path) {
            self.app.notice = format!("Could not save library data: {error}");
        }
    }

    fn import(&mut self, paths: Vec<PathBuf>) {
        if !paths.is_empty() && self.worker.send_background(paths).is_ok() {
            self.pending += 1;
            self.app.importing = true;
            self.app.notice.clear();
        }
    }

    fn choose_files(&mut self) -> bool {
        if self.app.importing {
            return false;
        }
        let picked = rfd::FileDialog::new()
            .set_title("Import images and videos")
            .add_filter(
                "Media files",
                &IMAGES.iter().chain(VIDEOS).copied().collect::<Vec<_>>(),
            )
            .pick_files();
        if let Some(paths) = picked {
            self.import(paths);
            true
        } else {
            false
        }
    }

    fn video_file(&mut self, path: PathBuf) -> std::io::Result<PathBuf> {
        if !path.as_os_str().is_empty() {
            return Ok(path);
        }
        if self.sample_video.is_none() {
            self.sample_video = Some(media::demo_clip()?);
        }
        Ok(self.sample_video.as_ref().unwrap().path().to_owned())
    }

    fn open(&mut self, path: PathBuf) -> bool {
        let result = self.video_file(path).and_then(open::that_detached);
        if let Err(error) = result {
            self.app.notice = format!("Could not open file: {error}");
            return true;
        }
        false
    }

    #[cfg(feature = "video")]
    fn load_video(&mut self, id: u64) -> bool {
        let path = self
            .app
            .assets
            .iter()
            .find(|asset| asset.id == id)
            .and_then(|asset| asset.path.clone());
        let result = match self.video_file(path.unwrap_or_default()) {
            Ok(path) => {
                // Canonical Windows paths can include the extended-length syntax,
                // which is understood by the Windows filesystem but not libmpv's URL parser.
                let path = path.to_string_lossy();
                let path = path.strip_prefix(r"\\?\").unwrap_or(&path);
                self.player
                    .load(path, None)
                    .and_then(|_| self.player.pause(true))
                    .map_err(|error| error.to_string())
            }
            Err(error) => Err(error.to_string()),
        };
        self.app.playback = Some(Playback {
            paused: true,
            error: result.err(),
            ..Playback::default()
        });
        true
    }

    fn effects(&mut self) -> bool {
        let mut repaint = false;
        while let Some(effect) = self.app.effects.pop_front() {
            match effect {
                Effect::Import => repaint |= self.choose_files(),
                Effect::Save => {
                    self.persist();
                    repaint = true;
                }
                Effect::Open(path) => repaint |= self.open(path),
                Effect::Copy(text) => {
                    self.effects.push(EffectPayload::ClipboardWrite { text });
                }
                #[cfg(feature = "video")]
                Effect::LoadVideo(id) => repaint |= self.load_video(id),
                #[cfg(feature = "video")]
                Effect::Pause(paused) => {
                    let _ = self.player.pause(paused);
                }
                #[cfg(feature = "video")]
                Effect::Seek(seconds) => {
                    let _ = self.player.seek(seconds);
                }
                #[cfg(feature = "video")]
                Effect::Mute(muted) => {
                    let _ = self.player.volume(if muted { 0. } else { 1. });
                }
                #[cfg(not(feature = "video"))]
                Effect::LoadVideo(value) => {
                    let _ = value;
                }
                #[cfg(not(feature = "video"))]
                Effect::Pause(value) | Effect::Mute(value) => {
                    let _ = value;
                }
                #[cfg(not(feature = "video"))]
                Effect::Seek(value) => {
                    let _ = value;
                }
            }
        }
        repaint
    }
}

impl HostView for Desktop {
    fn show(&mut self, cx: &mut UiContext) {
        self.app.show(cx);
    }

    fn theme(&self) -> Theme {
        self.app.theme()
    }

    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.worker.set_wake_callback(wake);
    }

    fn take_effect(&mut self) -> Option<EffectRequest> {
        self.effects.pop()
    }

    fn on_unhandled_key(&mut self, key: &KeyEvent) -> bool {
        self.app.on_key_event(key)
    }

    fn commit_declaration(&mut self) -> bool {
        self.effects()
    }

    fn discard_declaration(&mut self) {
        self.app.effects.clear();
    }

    fn poll_updates(&mut self) -> bool {
        let mut repaint = false;
        let mut persist = false;
        for event in self.worker.drain() {
            repaint = true;
            match event {
                LoadEvent::Asset(mut asset) => {
                    asset.favorite = self.stored.favorites.contains(&asset.fingerprint());
                    persist |= self.app.insert(*asset);
                }
                LoadEvent::Failed(message) => self.app.notice = message,
                LoadEvent::Complete => {
                    self.pending = self.pending.saturating_sub(1);
                    self.app.importing = self.pending != 0;
                }
            }
        }
        if persist {
            self.persist();
        }
        #[cfg(feature = "video")]
        if let Some(current) = &mut self.app.playback {
            let snapshot = self.player.snapshot();
            let next = Playback {
                position: (snapshot.position * 8.).floor() / 8.,
                duration: snapshot.duration,
                paused: snapshot.paused,
                ended: snapshot.ended,
                muted: snapshot.volume == 0.,
                error: current.error.clone().or(snapshot.error),
            };
            if *current != next {
                *current = next;
                repaint = true;
            }
        }
        repaint
    }
}

pub fn run(design: Design) -> Result<(), Error> {
    let args = Options::parse();
    if (args.capture.is_some() && args.record.is_some())
        || (args.scenario.is_some() && args.capture.is_none() && args.record.is_none())
    {
        return Err(Error::Options);
    }
    let assets = media::samples().map_err(|source| Error::Samples { source })?;
    if args.capture.is_some() || args.record.is_some() {
        let scenario = if let Some(path) = args.scenario {
            plico_desktop::PreviewScenario::load(path)
                .map_err(|source| Error::Preview { source })?
        } else {
            plico_desktop::PreviewScenario::<PreviewState> {
                viewport: plico_desktop::PreviewViewport {
                    width: 1280,
                    height: 850,
                    scale: 1.,
                },
                ..Default::default()
            }
        };
        let create = |state: PreviewState| {
            let mut app = Library::new(assets, false, design);
            if let Some(id) = state.selected {
                app.select(id);
            }
            plico_desktop::NativeApp(app)
        };
        if let Some(path) = args.capture {
            scenario
                .render(create)
                .map_err(|source| Error::Preview { source })?
                .write_png(path)
                .map_err(|source| Error::Capture { source })?;
        } else if let Some(path) = args.record {
            scenario
                .record_mp4(path, args.fps, create)
                .map_err(|source| Error::Preview { source })?;
        }
        return Ok(());
    }

    let options = WindowOptions::new()
        .title(format!("{} · Media Library", design.name()))
        .initial_inner_size(1280, 850)
        .min_inner_size(830, 600)
        .centered(true);
    #[cfg(feature = "video")]
    {
        let player = plico_video::Player::with_options(plico_video::PlayerOptions {
            width: 960,
            height: 540,
            ..Default::default()
        })
        .map_err(|source| Error::Video { source })?;
        let surface = player.surface();
        let mut desktop = Desktop::new(Library::new(assets, true, design), player);
        desktop.import(args.files);
        plico_desktop::run_source(
            plico_desktop::SurfaceSource::new(desktop, surface, "framebank-video"),
            options,
        )
        .map_err(|source| Error::Window { source })
    }
    #[cfg(not(feature = "video"))]
    {
        let mut desktop = Desktop::new(Library::new(assets, false, design));
        desktop.import(args.files);
        plico_desktop::run_source(plico_desktop::Source::new(desktop), options)
            .map_err(|source| Error::Window { source })
    }
}
