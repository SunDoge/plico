//! Retained animated-image surface. IO and decoding remain in the host worker.
use crate::{
    PaintContext, PaintScene, WakeCallback, Widget, WidgetChanges,
    animated_image::{AnimatedImage, ImagePlayer},
};
use kurbo::{Affine, Rect};
use peniko::{Color, Fill};
use plico::{Clock, SystemClock};
use std::sync::{Arc, Mutex};
use web_time::Instant;
/// Serializable UI state need not contain decoded pixels or player handles.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageLoadStatus {
    /// No request yet.
    Idle,
    /// A host request is in progress.
    Loading,
    /// Decoded frames ready for playback.
    Ready {
        /// Full canvas pixel dimensions.
        size: [f32; 2],
        /// Cached frame count.
        frames: usize,
        /// Retained decoded RGBA bytes.
        bytes: u64,
    },
    /// Download/decoding failed; the host may retry.
    Failed(String),
}
struct Shared {
    image: Option<Arc<AnimatedImage>>,
    status: ImageLoadStatus,
    generation: u64,
    paused: bool,
    restart: u64,
    revision: u64,
    wake: WakeCallback,
}
/// Thread-safe host resource handle. Completions use generation tickets so an
/// older download cannot overwrite a newer selection.
#[derive(Clone)]
pub struct ImageSurfaceState(Arc<Mutex<Shared>>);
impl std::fmt::Debug for ImageSurfaceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ImageSurfaceState")
            .field(&self.status())
            .finish()
    }
}
impl PartialEq for ImageSurfaceState {
    /// Observable surface state, not allocation identity: two hosts holding
    /// the same status compare equal, so a serialized app state restored
    /// into a fresh surface still matches the live one. A poisoned lock
    /// compares unequal rather than panicking.
    fn eq(&self, other: &Self) -> bool {
        let Ok(a) = self.0.lock() else { return false };
        let Ok(b) = other.0.lock() else { return false };
        a.status == b.status
            && a.paused == b.paused
            && a.revision == b.revision
            && a.image.as_ref().map(|image| Arc::as_ptr(image) as usize)
                == b.image.as_ref().map(|image| Arc::as_ptr(image) as usize)
    }
}
impl Default for ImageSurfaceState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Shared {
            image: None,
            status: ImageLoadStatus::Idle,
            generation: 0,
            paused: false,
            restart: 0,
            revision: 0,
            wake: Arc::new(|| {}),
        })))
    }
}
impl ImageSurfaceState {
    fn update<R>(&self, f: impl FnOnce(&mut Shared) -> R) -> R {
        let (result, wake) = {
            let mut shared = self.0.lock().unwrap();
            {
                let result = f(&mut shared);
                shared.revision += 1;
                (result, shared.wake.clone())
            }
        };
        wake();
        result
    }
    /// Start a request and clear the previous resource; returns its completion ticket.
    pub fn begin_loading(&self) -> u64 {
        self.update(|s| {
            s.generation += 1;
            s.image = None;
            s.status = ImageLoadStatus::Loading;
            s.paused = false;
            s.generation
        })
    }
    /// Publish a decoded resource or error. Returns false for an obsolete completion.
    pub fn complete(&self, ticket: u64, result: Result<Arc<AnimatedImage>, String>) -> bool {
        self.update(|s| {
            if s.generation != ticket {
                return false;
            }
            match result {
                Ok(image) => {
                    s.status = ImageLoadStatus::Ready {
                        size: image.size(),
                        frames: image.frame_count(),
                        bytes: image.byte_len(),
                    };
                    s.image = Some(image);
                }
                Err(error) => {
                    s.status = ImageLoadStatus::Failed(error);
                    s.image = None;
                }
            }
            true
        })
    }
    /// Current loading metadata (never clones the pixels).
    pub fn status(&self) -> ImageLoadStatus {
        self.0.lock().unwrap().status.clone()
    }
    /// Whether playback is explicitly paused.
    pub fn paused(&self) -> bool {
        self.0.lock().unwrap().paused
    }
    /// Pause/resume on the next host paint; no request to repaint continuously.
    pub fn set_paused(&self, paused: bool) {
        self.update(|s| s.paused = paused);
    }
    /// Restart from frame zero and resume playback.
    pub fn restart(&self) {
        self.update(|s| {
            s.restart += 1;
            s.paused = false;
        });
    }
}
/// Framework widget hosting full-resolution cached frames in a contain-fit slot.
/// Requests only the next frame deadline; hidden, paused and finished images idle.
pub struct ImageSurface {
    state: ImageSurfaceState,
    clock: Arc<dyn Clock>,
    image: Option<Arc<AnimatedImage>>,
    player: Option<ImagePlayer>,
    paused: bool,
    restart: u64,
    revision: u64,
    visible: bool,
    deadline: Option<Instant>,
}
impl ImageSurface {
    /// Use a monotonic system clock at the host boundary.
    pub fn new(state: ImageSurfaceState) -> Self {
        Self::with_clock(state, Arc::new(SystemClock::default()))
    }
    /// Inject a clock, including VirtualClock for deterministic tests.
    pub fn with_clock(state: ImageSurfaceState, clock: Arc<dyn Clock>) -> Self {
        Self {
            state,
            clock,
            image: None,
            player: None,
            paused: false,
            restart: 0,
            revision: 0,
            visible: false,
            deadline: None,
        }
    }
    fn synchronize(&mut self) -> WidgetChanges {
        let shared = self.state.0.lock().unwrap();
        let replaced = match (&self.image, &shared.image) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        let changed = self.revision != shared.revision
            || replaced
            || self.paused != shared.paused
            || self.restart != shared.restart;
        if replaced {
            self.image = shared.image.clone();
            self.player = self.image.clone().map(ImagePlayer::new);
        }
        let now = self.clock.now();
        if let Some(player) = self.player.as_mut() {
            if self.restart != shared.restart {
                player.restart(now);
            }
            if shared.paused {
                player.pause(now);
            } else {
                player.play(now);
            }
        }
        self.paused = shared.paused;
        self.restart = shared.restart;
        self.revision = shared.revision;
        if changed {
            self.deadline = None;
            WidgetChanges::REDRAW
        } else {
            WidgetChanges::empty()
        }
    }
}
impl Widget for ImageSurface {
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.state.0.lock().unwrap().wake = wake;
    }
    fn poll_async_changes(&mut self) -> WidgetChanges {
        self.synchronize()
    }
    fn visibility_changed(&mut self, visible: bool) -> WidgetChanges {
        self.visible = visible;
        if !visible {
            self.deadline = None;
        }
        WidgetChanges::REDRAW
    }
    fn animation_deadline(&self) -> Option<Instant> {
        if self.visible { self.deadline } else { None }
    }
    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        self.synchronize();
        let [width, height] = cx.size();
        let Some(player) = self.player.as_mut() else {
            self.deadline = None;
            return;
        };
        let sample = player.sample(self.clock.now());
        self.deadline = sample
            .next_frame_after
            .and_then(|delay| Instant::now().checked_add(delay));
        let scene = cx.scene_mut();
        for y in 0..(height / 20.).ceil() as u32 {
            for x in 0..(width / 20.).ceil() as u32 {
                let shade = if (x + y) % 2 == 0 { 224 } else { 242 };
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgb8(shade, shade, shade),
                    None,
                    &Rect::new(
                        (x * 20) as f64,
                        (y * 20) as f64,
                        ((x + 1) * 20) as f64,
                        ((y + 1) * 20) as f64,
                    ),
                );
            }
        }
        let [iw, ih] = sample.image.size();
        let scale = (width / iw).min(height / ih);
        let transform = Affine::translate((
            ((width - iw * scale) / 2.) as f64,
            ((height - ih * scale) / 2.) as f64,
        )) * Affine::scale(scale as f64);
        scene.draw_image(sample.image.brush().into(), transform);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn resource() -> Arc<AnimatedImage> {
        Arc::new(
            AnimatedImage::decode_gif(
                &crate::animated_image::gif_tests::fixture(Some(
                    image::codecs::gif::Repeat::Infinite,
                )),
                crate::animated_image::AnimationLimits::default(),
            )
            .unwrap(),
        )
    }
    fn paint(widget: &mut ImageSurface, text: &mut crate::TextContext) -> crate::Scene {
        let mut context = PaintContext::new(100., 100., 1., text);
        widget.paint(&mut context);
        context.finish()
    }
    #[test]
    fn playback_uses_injected_time_and_stale_loads_cannot_replace_newer_resources() {
        let state = ImageSurfaceState::default();
        let stale = state.begin_loading();
        let current = state.begin_loading();
        assert!(!state.complete(stale, Err("old failure".into())));
        assert_eq!(state.status(), ImageLoadStatus::Loading);
        assert!(state.complete(current, Ok(resource())));
        let clock = plico::VirtualClock::default();
        let mut widget = ImageSurface::with_clock(state.clone(), Arc::new(clock.clone()));
        widget.visibility_changed(true);
        let mut text = crate::TextContext::new();
        let first = paint(&mut widget, &mut text);
        assert!(widget.animation_deadline().is_some());
        clock.advance(Duration::from_millis(100)).unwrap();
        let second = paint(&mut widget, &mut text);
        // The frame handles have different pixel identity, not newly encoded PNGs.
        let sample = widget.player.as_mut().unwrap().sample(clock.now());
        assert_eq!(sample.index, 1);
        assert!(!first.commands.is_empty() && !second.commands.is_empty());
        state.set_paused(true);
        paint(&mut widget, &mut text);
        clock.advance(Duration::from_secs(1)).unwrap();
        paint(&mut widget, &mut text);
        assert_eq!(widget.player.as_mut().unwrap().sample(clock.now()).index, 1);
        assert!(widget.animation_deadline().is_none());
        state.restart();
        paint(&mut widget, &mut text);
        assert_eq!(widget.player.as_mut().unwrap().sample(clock.now()).index, 0);
        widget.visibility_changed(false);
        assert!(widget.animation_deadline().is_none());
    }
    #[test]
    fn failure_also_requests_redraw_when_no_image_was_loaded() {
        let state = ImageSurfaceState::default();
        let mut widget = ImageSurface::new(state.clone());
        let ticket = state.begin_loading();
        widget.poll_async_changes();
        state.complete(ticket, Err("offline".into()));
        assert!(widget.poll_async_changes().contains(WidgetChanges::REDRAW));
    }
}
