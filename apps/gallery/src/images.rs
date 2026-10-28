//! Network/decode jobs stay in GalleryHost. UI sends commands; the framework owns painting.
use plico::prelude::*;
use plico_renderer::{
    animated_image::{AnimatedImage, AnimationLimits},
    image_surface::{ImageLoadStatus, ImageSurfaceState},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
const WEBP: &str = "https://www.gstatic.com/webp/animated/1.webp";
const GIF: &str = "https://www.gstatic.com/webp/animated/1.gif";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    WebP,
    Gif,
}
impl Format {
    pub fn url(self) -> &'static str {
        match self {
            Self::WebP => WEBP,
            Self::Gif => GIF,
        }
    }
    fn file(self) -> &'static str {
        match self {
            Self::WebP => "google-animation-v1.webp",
            Self::Gif => "google-animation-v1.gif",
        }
    }
}
#[derive(PartialEq)]
struct Job {
    format: Format,
    ticket: u64,
    refresh: bool,
}
struct Inner {
    format: Format,
    jobs: VecDeque<Job>,
    running: bool,
}
#[derive(Clone)]
pub struct Images {
    pub surface: ImageSurfaceState,
    inner: Arc<Mutex<Inner>>,
}
impl std::fmt::Debug for Images {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Images").field(&self.surface).finish()
    }
}
impl PartialEq for Images {
    /// Surface state and queue content, not allocation identity: a
    /// serialized state restored into a fresh host must compare equal to
    /// the live one (the hot-reload and roundtrip snapshot contract).
    /// A poisoned queue compares unequal rather than panicking.
    fn eq(&self, other: &Self) -> bool {
        let Ok(a) = self.inner.lock() else {
            return false;
        };
        let Ok(b) = other.inner.lock() else {
            return false;
        };
        self.surface == other.surface
            && a.format == b.format
            && a.jobs == b.jobs
            && a.running == b.running
    }
}
impl Default for Images {
    fn default() -> Self {
        Self {
            surface: ImageSurfaceState::default(),
            inner: Arc::new(Mutex::new(Inner {
                format: Format::WebP,
                jobs: VecDeque::new(),
                running: false,
            })),
        }
    }
}
impl Images {
    pub fn request(&self, format: Format, refresh: bool) {
        let ticket = self.surface.begin_loading();
        let mut inner = self.inner.lock().unwrap();
        inner.format = format;
        inner.jobs.clear();
        inner.jobs.push_back(Job {
            format,
            ticket,
            refresh,
        });
    }
    /// Invoked only by the resident host, never from a UI declaration.
    /// Verification tools may block outside the UI/event loop and pre-load a resource.
    pub fn load_for_capture(&self, format: Format) -> Result<(), String> {
        let ticket = self.surface.begin_loading();
        self.inner.lock().unwrap().format = format;
        match load(format, false) {
            Ok(image) => {
                self.surface.complete(ticket, Ok(image));
                Ok(())
            }
            Err(error) => {
                self.surface.complete(ticket, Err(error.clone()));
                Err(error)
            }
        }
    }
    pub fn poll_requests(&self) {
        let job = {
            let mut inner = self.inner.lock().unwrap();
            if inner.running {
                return;
            }
            let job = inner.jobs.pop_front();
            if job.is_some() {
                inner.running = true;
            }
            job
        };
        if let Some(job) = job {
            let images = self.clone();
            std::thread::spawn(move || {
                let result = load(job.format, job.refresh);
                images.inner.lock().unwrap().running = false;
                images.surface.complete(job.ticket, result);
            });
        }
    }
    #[component]
    #[rustfmt::skip]
    pub fn show(&self, cx: &mut UiContext) {
        if matches!(self.surface.status(), ImageLoadStatus::Idle) {
            self.request(Format::WebP, false);
        }
        let status = self.surface.status();
        let format = self.inner.lock().unwrap().format;
        let ready = matches!(status, ImageLoadStatus::Ready { .. });
        column(cx).gap(6) + |cx| {
            text(cx, "Animated images").font_size(28).weight(700);
            text(cx, "Real WebP and GIF, loaded from the network.").color(Color::Muted);
        };
        row(cx).gap(8).wrap(true) + |cx| {
            for (kind, label) in [(Format::WebP, "WebP"), (Format::Gif, "GIF")] {
                if button(cx).key(label).text(label)
                    .background(if format == kind { Color::Selection } else { Color::Control }).clicked()
                {
                    self.request(kind, false);
                }
            }
            column(cx).grow(1.).children(|_| {});
            if button(cx).text(if self.surface.paused() { "Play" } else { "Pause" }).disabled(!ready).clicked() {
                self.surface.set_paused(!self.surface.paused());
            }
            if button(cx).text("Replay").disabled(!ready).clicked() {
                self.surface.restart();
            }
            if button(cx).text("Reload from network").clicked() {
                self.request(format, true);
            }
        };
        column(cx).key("image-preview").width_percent(100.).height(340.).clip(true).radius(14.).background(Color::Panel)
            .align(Align::Center).justify(Justify::Center)
            + |cx| match &status {
                ImageLoadStatus::Loading => {
                    text(cx, "Downloading and decoding…").color(Color::Muted);
                }
                ImageLoadStatus::Failed(error) => {
                    text(cx, "Could not load the image").weight(600);
                    text(cx, error).color(Color::Muted);
                }
                _ => {}
            };
        match status {
            ImageLoadStatus::Ready {
                size,
                frames,
                bytes,
            } => {
                text!(
                    cx,
                    "{} × {} px · {} frames · {:.1} MiB decoded",
                    size[0],
                    size[1],
                    frames,
                    bytes as f64 / 1048576.
                ).weight(600);
            }
            ImageLoadStatus::Failed(_) => {
                if button(cx).text("Retry").clicked() {
                    self.request(format, true);
                }
            }
            _ => {}
        }
        text(cx, format.url()).font_size(12).color(Color::Muted);
        text(
            cx,
            "Google WebP animation comparison sample · local disk cache · original aspect ratio",
        ).font_size(12).color(Color::Muted);
    }
}
const MAX_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Debug, snafu::Snafu)]
#[snafu(display("image load failed: {message}"))]
struct LoadError {
    message: String,
}
fn decode(format: Format, bytes: &[u8]) -> Result<Arc<AnimatedImage>, String> {
    let limits = AnimationLimits::default();
    match format {
        Format::WebP => AnimatedImage::decode_webp(bytes, limits),
        Format::Gif => AnimatedImage::decode_gif(bytes, limits),
    }
    .map(Arc::new)
    .map_err(|e| e.to_string())
}
fn load(format: Format, refresh: bool) -> Result<Arc<AnimatedImage>, String> {
    use std::{io::Read, time::Duration};
    let cache = directories::ProjectDirs::from("dev", "plico", "gallery")
        .map(|p| p.cache_dir().join("images").join(format.file()));
    if !refresh
        && let Some(path) = &cache
        && let Ok(file) = std::fs::File::open(path)
        && file.metadata().is_ok_and(|m| m.len() <= MAX_BYTES)
    {
        let mut bytes = Vec::new();
        if file.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_ok()
            && bytes.len() as u64 <= MAX_BYTES
            && let Ok(image) = decode(format, &bytes)
        {
            return Ok(image);
        }
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(25)))
        .https_only(true)
        .build()
        .into();
    let mut response = agent.get(format.url()).call().map_err(|e| {
        LoadError {
            message: e.to_string(),
        }
        .to_string()
    })?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .map_err(|e| {
            LoadError {
                message: e.to_string(),
            }
            .to_string()
        })?;
    let image = decode(format, &bytes)?;
    if let Some(path) = cache {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, &bytes);
    }
    Ok(image)
}
/// Desktop host wrapper; workers and decoded resources do not enter app checkpoints.
pub struct GalleryHost {
    pub app: super::Gallery,
}
impl plico_desktop::View for GalleryHost {
    fn show(&mut self, cx: &mut UiContext) {
        plico_dev::App::show(&mut self.app, cx);
    }
    fn theme(&self) -> Theme {
        plico_dev::App::theme(&self.app)
    }
    fn set_frame_time(&mut self, time: plico::FrameTime) {
        plico_dev::App::set_frame_time(&mut self.app, time);
    }
    fn on_unhandled_key(&mut self, key: &plico::KeyEvent) -> bool {
        plico_dev::App::on_key_event(&mut self.app, key)
    }
    fn poll_reload(&mut self) -> bool {
        self.app.images.poll_requests();
        false
    }
}
