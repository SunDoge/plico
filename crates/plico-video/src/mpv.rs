use libmpv2::Mpv;
use libmpv2_sys as ffi;
use plico_renderer::{PaintContext, WakeCallback, Widget, WidgetChanges, WidgetRasterImage};
use snafu::Snafu;
use std::{
    ffi::c_void,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Errors at the player boundary.
#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("video output dimensions must be nonzero, at most 4096 each"))]
    Dimensions,
    #[snafu(display("libmpv failed: {source}"))]
    Mpv { source: libmpv2::Error },
    #[snafu(display("libmpv could not load or decode the input"))]
    Playback,
    #[snafu(display("could not start video thread: {source}"))]
    Thread { source: std::io::Error },
    #[snafu(display("video worker has stopped"))]
    Disconnected,
}
/// Host-owned software output and network playback settings.
#[derive(Clone, Debug)]
pub struct PlayerOptions {
    pub width: u32,
    pub height: u32,
    /// Use a null audio device for probes.
    pub muted: bool,
    pub referrer: Option<String>,
    pub user_agent: Option<String>,
}
impl Default for PlayerOptions {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            muted: false,
            referrer: None,
            user_agent: None,
        }
    }
}
/// Host service snapshot. Decoded pixels are never serialized with app state.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub volume: f64,
    /// Decoded audio channels, zero until an audio stream is ready.
    pub audio_channels: u32,
    pub loading: bool,
    pub ended: bool,
    pub frames: u64,
    /// Cumulative render-call wall time, including mpv frame pacing; excludes GPU upload.
    pub render_micros: u64,
    pub error: Option<String>,
}
struct Shared {
    frame: Option<WidgetRasterImage>,
    revision: u64,
    snapshot: Snapshot,
    wake: WakeCallback,
}
enum Command {
    Load {
        video: String,
        audio: Option<String>,
    },
    Volume(f64),
    Pause(bool),
    Seek(f64),
    Stop,
}
/// Owns the worker. Drop stops playback and joins it; clones of the surface do
/// not keep the player alive. Control and rendering use separate worker threads, as libmpv recommends.
pub struct Player {
    commands: mpsc::SyncSender<Command>,
    shared: Arc<Mutex<Shared>>,
    worker: Option<JoinHandle<()>>,
}
impl Player {
    /// Fixed-resolution SDR software output, intentionally not a zero-copy path.
    pub fn open(
        path: impl Into<String>,
        width: u32,
        height: u32,
        muted: bool,
    ) -> Result<Self, Error> {
        let player = Self::with_options(PlayerOptions {
            width,
            height,
            muted,
            ..Default::default()
        })?;
        player.load(path, None)?;
        Ok(player)
    }
    /// Create an idle player. Subsequent loads retain the same video surface.
    pub fn with_options(options: PlayerOptions) -> Result<Self, Error> {
        let (width, height) = (options.width, options.height);
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(Error::Dimensions);
        }
        let shared = Arc::new(Mutex::new(Shared {
            frame: None,
            revision: 0,
            snapshot: Snapshot::default(),
            wake: Arc::new(|| {}),
        }));
        let (commands, receiver) = mpsc::sync_channel(8);
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("plico-video".into())
            .spawn(move || {
                if let Err(error) = run(options, receiver, &state) {
                    let wake = {
                        let mut state = state.lock().unwrap();
                        state.snapshot.error = Some(error.to_string());
                        state.revision += 1;
                        state.wake.clone()
                    };
                    wake();
                }
            })
            .map_err(|source| Error::Thread { source })?;
        Ok(Self {
            commands,
            shared,
            worker: Some(worker),
        })
    }
    /// Replace playback; an optional external audio URL supports DASH streams.
    pub fn load(&self, video: impl Into<String>, audio: Option<String>) -> Result<(), Error> {
        self.commands
            .send(Command::Load {
                video: video.into(),
                audio,
            })
            .map_err(|_| Error::Disconnected)
    }
    /// Set output volume in the normalized range 0..=1.
    pub fn volume(&self, volume: f64) -> Result<(), Error> {
        if !volume.is_finite() {
            return Ok(());
        }
        self.commands
            .send(Command::Volume(volume.clamp(0., 1.)))
            .map_err(|_| Error::Disconnected)
    }
    pub fn snapshot(&self) -> Snapshot {
        self.shared.lock().unwrap().snapshot.clone()
    }
    pub fn surface(&self) -> VideoSurface {
        VideoSurface {
            shared: self.shared.clone(),
            revision: 0,
        }
    }
    pub fn pause(&self, paused: bool) -> Result<(), Error> {
        self.commands
            .send(Command::Pause(paused))
            .map_err(|_| Error::Disconnected)
    }
    pub fn seek(&self, seconds: f64) -> Result<(), Error> {
        if !seconds.is_finite() {
            return Ok(());
        }
        self.commands
            .send(Command::Seek(seconds.max(0.)))
            .map_err(|_| Error::Disconnected)
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
/// Renderer-neutral retained surface; new frames change image identity, so
/// damage tracking/present skipping cannot mistake them for an unchanged scene.
pub struct VideoSurface {
    shared: Arc<Mutex<Shared>>,
    revision: u64,
}
impl Widget for VideoSurface {
    fn accepts_input(&self) -> bool {
        false
    }
    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.shared.lock().unwrap().wake = wake;
    }
    fn poll_async_changes(&mut self) -> WidgetChanges {
        let revision = self.shared.lock().unwrap().revision;
        if revision == self.revision {
            return WidgetChanges::empty();
        }
        self.revision = revision;
        WidgetChanges::REDRAW
    }
    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        let frame = self.shared.lock().unwrap().frame.clone();
        if let Some(frame) = frame {
            let [w, h] = cx.size();
            let [iw, ih] = frame.dimensions();
            let scale = (w / iw as f32).min(h / ih as f32);
            let (dw, dh) = (iw as f32 * scale, ih as f32 * scale);
            cx.draw_raster_image_in(
                &frame,
                [
                    ((w - dw) / 2.) as f64,
                    ((h - dh) / 2.) as f64,
                    dw as f64,
                    dh as f64,
                ],
            );
        }
    }
}
// The render context must be destroyed before the Mpv owner. It never leaves
// its render thread; its callback only borrows the owned notification sender.
struct Software {
    context: *mut ffi::mpv_render_context,
    _notify: Box<mpsc::SyncSender<()>>,
}
impl Drop for Software {
    fn drop(&mut self) {
        unsafe { ffi::mpv_render_context_free(self.context) };
    }
}
fn check(code: i32) -> Result<(), Error> {
    if code < 0 {
        Err(Error::Mpv {
            source: libmpv2::Error::Raw(code),
        })
    } else {
        Ok(())
    }
}
fn param(kind: u32, data: *mut c_void) -> ffi::mpv_render_param {
    ffi::mpv_render_param { type_: kind, data }
}
fn run(
    options: PlayerOptions,
    receiver: mpsc::Receiver<Command>,
    shared: &Mutex<Shared>,
) -> Result<(), Error> {
    let (width, height) = (options.width, options.height);
    let mpv = Mpv::with_initializer(|init| {
        init.set_property("vo", "libmpv")?;
        init.set_property("hwdec", "no")?;
        init.set_property("terminal", false)?;
        init.set_property("idle", true)?;
        init.set_property("keep-open", true)?;
        if let Some(referrer) = &options.referrer {
            init.set_property("referrer", referrer.as_str())?;
        }
        if let Some(agent) = &options.user_agent {
            init.set_property("user-agent", agent.as_str())?;
        }
        if options.muted {
            init.set_property("ao", "null")?;
        }
        Ok(())
    })
    .map_err(|source| Error::Mpv { source })?;
    let render_mpv = mpv
        .create_client(None)
        .map_err(|source| Error::Mpv { source })?;
    thread::scope(|scope| {
        let stop = Arc::new(AtomicBool::new(false));
        let render_stop = stop.clone();
        let (notify, updates) = mpsc::sync_channel(1);
        let render_notify = notify.clone();
        let (ready, initialized) = mpsc::sync_channel(1);
        let renderer = scope.spawn(move || -> Result<(), Error> {
            let mpv = render_mpv;
            let mut context = ptr::null_mut();
            let mut init = [
                param(
                    ffi::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                    ffi::MPV_RENDER_API_TYPE_SW.as_ptr().cast_mut().cast(),
                ),
                param(
                    ffi::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                    ptr::null_mut(),
                ),
            ];
            // SAFETY: mpv and all parameter storage are alive for this synchronous call.
            check(unsafe {
                ffi::mpv_render_context_create(&mut context, mpv.ctx.as_ptr(), init.as_mut_ptr())
            })?;
            let notify = Box::new(render_notify);
            // SAFETY: the boxed sender stays at a stable address until context free has
            // stopped callbacks. The callback never calls back into libmpv or blocks.
            unsafe {
                ffi::mpv_render_context_set_update_callback(
                    context,
                    Some(render_ready),
                    (&*notify as *const mpsc::SyncSender<()>).cast_mut().cast(),
                )
            };
            let context = Software {
                context,
                _notify: notify,
            };

            let _ = ready.send(());
            while !render_stop.load(Ordering::Acquire) {
                if updates.recv().is_err() {
                    break;
                }
                if render_stop.load(Ordering::Acquire) {
                    break;
                }
                // SAFETY: this is the only thread using this live render context.
                let update = unsafe { ffi::mpv_render_context_update(context.context) };
                if update & u64::from(ffi::mpv_render_update_flag_MPV_RENDER_UPDATE_FRAME) != 0 {
                    let start = Instant::now();
                    // u32 storage guarantees the software API's minimum 4-byte alignment.
                    let mut pixels = vec![0u32; width as usize * height as usize];
                    let mut size = [width as i32, height as i32];
                    let mut stride = width as usize * 4;
                    let mut params = [
                        param(
                            ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_SIZE,
                            size.as_mut_ptr().cast(),
                        ),
                        param(
                            ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_FORMAT,
                            c"rgb0".as_ptr().cast_mut().cast(),
                        ),
                        param(
                            ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_STRIDE,
                            (&mut stride as *mut usize).cast(),
                        ),
                        param(
                            ffi::mpv_render_param_type_MPV_RENDER_PARAM_SW_POINTER,
                            pixels.as_mut_ptr().cast(),
                        ),
                        param(
                            ffi::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                            ptr::null_mut(),
                        ),
                    ];
                    // SAFETY: validated dimensions, aligned writable buffer, correct stride;
                    // the C API borrows these pointers only during this synchronous call.
                    check(unsafe {
                        ffi::mpv_render_context_render(context.context, params.as_mut_ptr())
                    })?;
                    let micros = start.elapsed().as_micros() as u64;
                    // rgb0's fourth byte is unspecified, not an alpha channel.
                    let mut rgba = Vec::with_capacity(pixels.len() * 4);
                    for pixel in pixels {
                        let [r, g, b, _] = pixel.to_ne_bytes();
                        rgba.extend_from_slice(&[r, g, b, 255]);
                    }
                    let image = WidgetRasterImage::from_rgba8(width, height, rgba)
                        .expect("validated frame size");
                    let wake = {
                        let mut state = shared.lock().unwrap();
                        state.frame = Some(image); // latest-only slot, never an unbounded frame queue
                        state.revision += 1;
                        state.snapshot.frames += 1;
                        state.snapshot.render_micros += micros;
                        state.wake.clone()
                    };
                    wake();
                }
            }
            Ok(())
        });
        let result = (|| {
            initialized.recv().map_err(|_| Error::Playback)?;
            let events = mpv
                .create_client(None)
                .map_err(|source| Error::Mpv { source })?;
            let mut pending_audio = None;
            let mut last_status = Instant::now();
            loop {
                if renderer.is_finished() {
                    return Ok(());
                }
                match receiver.recv_timeout(Duration::from_millis(100)) {
                    Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Ok(command) => {
                        let result = match command {
                            Command::Load { video, audio } => {
                                pending_audio = audio.filter(|url| !url.is_empty());
                                {
                                    let mut state = shared.lock().unwrap();
                                    state.snapshot.error = None;
                                    state.snapshot.loading = true;
                                    state.snapshot.ended = false;
                                }
                                mpv.set_property("pause", false)
                                    .map_err(|source| Error::Mpv { source })?;
                                mpv.command("loadfile", &[&video, "replace"])
                            }
                            Command::Volume(volume) => mpv.set_property("volume", volume * 100.),
                            Command::Pause(paused) => mpv.set_property("pause", paused),
                            Command::Seek(seconds) => {
                                mpv.command("seek", &[&seconds.to_string(), "absolute+exact"])
                            }
                            Command::Stop => unreachable!(),
                        };
                        if let Err(error) = result {
                            let mut state = shared.lock().unwrap();
                            state.snapshot.error = Some(error.to_string());
                            state.snapshot.loading = false;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                while let Some(event) = events.wait_event(0.) {
                    match event {
                        Ok(libmpv2::events::Event::FileLoaded) => {
                            shared.lock().unwrap().snapshot.loading = false;
                            if let Some(audio) = pending_audio.take()
                                && let Err(error) = mpv.command("audio-add", &[&audio, "select"])
                            {
                                shared.lock().unwrap().snapshot.error = Some(error.to_string());
                            }
                        }
                        Ok(libmpv2::events::Event::EndFile(reason))
                            if reason == libmpv2::mpv_end_file_reason::Error =>
                        {
                            let mut state = shared.lock().unwrap();
                            state.snapshot.error = Some("Could not load/decode media".into());
                            state.snapshot.loading = false;
                        }
                        _ => {}
                    }
                }
                if last_status.elapsed() >= Duration::from_millis(100) {
                    let position = mpv.get_property::<f64>("time-pos").unwrap_or(0.);
                    let duration = mpv.get_property::<f64>("duration").unwrap_or(0.);
                    let paused = mpv.get_property::<bool>("pause").unwrap_or(false);
                    let volume = mpv.get_property::<f64>("volume").unwrap_or(100.) / 100.;
                    let ended = mpv.get_property::<bool>("eof-reached").unwrap_or(false);
                    let audio_channels = mpv
                        .get_property::<i64>("audio-params/channel-count")
                        .unwrap_or(0)
                        .max(0) as u32;
                    let mut state = shared.lock().unwrap();
                    state.snapshot.volume = volume;
                    state.snapshot.audio_channels = audio_channels;
                    state.snapshot.ended = ended;
                    state.snapshot.position = position;
                    state.snapshot.duration = duration;
                    state.snapshot.paused = paused;
                    let wake = state.wake.clone();
                    drop(state);
                    wake();
                    last_status = Instant::now();
                }
            }
        })();
        stop.store(true, Ordering::Release);
        let _ = notify.try_send(());
        let rendering = renderer.join().map_err(|_| Error::Playback)?;
        result.and(rendering)
    })
}

unsafe extern "C" fn render_ready(data: *mut c_void) {
    // SAFETY: registered only while Software owns this stable boxed sender.
    let sender = unsafe { &*data.cast::<mpsc::SyncSender<()>>() };
    let _ = sender.try_send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_dimensions_fail_before_starting_workers() {
        assert!(matches!(
            Player::open("unused", 0, 720, true),
            Err(Error::Dimensions)
        ));
        assert!(matches!(
            Player::open("unused", 4097, 720, true),
            Err(Error::Dimensions)
        ));
    }
    #[test]
    fn surface_reuses_same_frame_but_new_pixels_invalidate_scene() {
        let shared = Arc::new(Mutex::new(Shared {
            frame: Some(WidgetRasterImage::from_rgba8(2, 1, vec![255; 8]).unwrap()),
            revision: 1,
            snapshot: Snapshot::default(),
            wake: Arc::new(|| {}),
        }));
        let mut surface = VideoSurface {
            shared: shared.clone(),
            revision: 0,
        };
        let mut text = plico_renderer::TextContext::new();
        let paint = |surface: &mut VideoSurface, text: &mut plico_renderer::TextContext| {
            let mut context = PaintContext::new(100., 100., 1., text);
            surface.paint(&mut context);
            context.finish()
        };
        assert!(surface.poll_async_changes().contains(WidgetChanges::REDRAW));
        let first = paint(&mut surface, &mut text);
        assert!(!first.commands.is_empty());
        assert!(surface.poll_async_changes().is_empty());
        assert_eq!(first.commands, paint(&mut surface, &mut text).commands);
        {
            let mut state = shared.lock().unwrap();
            state.frame = Some(WidgetRasterImage::from_rgba8(2, 1, vec![0; 8]).unwrap());
            state.revision += 1;
        }
        assert!(surface.poll_async_changes().contains(WidgetChanges::REDRAW));
        assert_ne!(first.commands, paint(&mut surface, &mut text).commands);
    }
}
