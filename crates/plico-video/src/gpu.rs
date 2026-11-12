//! Linux libmpv OpenGL rendering into a texture owned by Plico's wgpu device.

use crate::{Error, PlayerOptions, Snapshot};
use glow::HasContext;
use libloading::Library;
use libmpv2::Mpv;
use libmpv2_sys as ffi;
use plico_renderer::{
    ExternalSurface, GpuContext, PaintContext, WakeCallback, Widget, WidgetChanges,
};
use plico_wgpu::{ExternalGpuContext, ExternalTexture, NativeDisplay};
use std::{
    collections::VecDeque,
    ffi::{CStr, CString, c_char, c_void},
    num::NonZeroU32,
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

static NEXT_SURFACE_ID: AtomicU64 = AtomicU64::new(1);
type EglGetProcAddress = unsafe extern "C" fn(*const c_char) -> *const c_void;

enum Command {
    Load {
        video: String,
        audio: Option<String>,
    },
    Volume(f64),
    Pause(bool),
    Seek(f64),
}

struct Shared {
    commands: VecDeque<Command>,
    snapshot: Snapshot,
    wake: WakeCallback,
}

/// UI-thread command handle for the Linux libmpv GPU-render player.
pub struct GpuPlayer {
    shared: Arc<Mutex<Shared>>,
}

impl GpuPlayer {
    /// Create the command handle and its unique render surface.
    pub fn with_options(options: PlayerOptions) -> Result<(Self, GpuVideoSurface), Error> {
        if options.width == 0
            || options.height == 0
            || options.width > 4096
            || options.height > 4096
        {
            return Err(Error::Dimensions);
        }
        let shared = Arc::new(Mutex::new(Shared {
            commands: VecDeque::new(),
            snapshot: Snapshot::default(),
            wake: Arc::new(|| {}),
        }));
        Ok((
            Self {
                shared: shared.clone(),
            },
            GpuVideoSurface {
                shared,
                options,
                engine: None,
                revision: 0,
                surface_id: NEXT_SURFACE_ID.fetch_add(1, Ordering::Relaxed),
                last_status: Instant::now(),
            },
        ))
    }

    fn command(&self, command: Command) {
        let wake = {
            let mut shared = self.shared.lock().unwrap();
            shared.commands.push_back(command);
            shared.wake.clone()
        };
        wake();
    }

    /// Replace playback; an optional external audio URL supports DASH streams.
    pub fn load(&self, video: impl Into<String>, audio: Option<String>) -> Result<(), Error> {
        self.command(Command::Load {
            video: video.into(),
            audio,
        });
        Ok(())
    }

    /// Set normalized output volume.
    pub fn volume(&self, volume: f64) -> Result<(), Error> {
        if volume.is_finite() {
            self.command(Command::Volume(volume.clamp(0., 1.)));
        }
        Ok(())
    }

    /// Pause or resume playback.
    pub fn pause(&self, paused: bool) -> Result<(), Error> {
        self.command(Command::Pause(paused));
        Ok(())
    }

    /// Seek to an absolute timestamp in seconds.
    pub fn seek(&self, seconds: f64) -> Result<(), Error> {
        if seconds.is_finite() {
            self.command(Command::Seek(seconds.max(0.)));
        }
        Ok(())
    }

    /// Latest playback state.
    pub fn snapshot(&self) -> Snapshot {
        self.shared.lock().unwrap().snapshot.clone()
    }
}

/// Passive media widget sampling a libmpv-updated wgpu texture.
pub struct GpuVideoSurface {
    shared: Arc<Mutex<Shared>>,
    options: PlayerOptions,
    engine: Option<Engine>,
    revision: u64,
    surface_id: u64,
    last_status: Instant,
}

impl GpuVideoSurface {
    fn fail(&mut self, message: impl Into<String>) {
        let wake = {
            let mut shared = self.shared.lock().unwrap();
            shared.snapshot.error = Some(message.into());
            shared.snapshot.loading = false;
            shared.wake.clone()
        };
        wake();
    }

    fn poll_engine(&mut self) -> Result<bool, Error> {
        let Some(engine) = &mut self.engine else {
            return Ok(false);
        };
        let commands = {
            let mut shared = self.shared.lock().unwrap();
            shared.commands.drain(..).collect::<Vec<_>>()
        };
        let mut changed = !commands.is_empty();
        for command in commands {
            engine.command(command, &self.shared)?;
        }
        changed |= engine.poll_events(&self.shared)?;
        let loading = self.shared.lock().unwrap().snapshot.loading;
        if !loading && engine.callback.dirty.swap(false, Ordering::AcqRel) {
            let start = Instant::now();
            if engine.render()? {
                self.revision = self.revision.wrapping_add(1);
                let mut shared = self.shared.lock().unwrap();
                shared.snapshot.frames = shared.snapshot.frames.wrapping_add(1);
                shared.snapshot.render_micros = shared
                    .snapshot
                    .render_micros
                    .wrapping_add(start.elapsed().as_micros() as u64);
                changed = true;
            }
        }
        if self.last_status.elapsed() >= Duration::from_millis(100) {
            engine.update_status(&self.shared);
            self.last_status = Instant::now();
            changed = true;
        }
        Ok(changed)
    }
}

impl Widget for GpuVideoSurface {
    fn accepts_input(&self) -> bool {
        false
    }

    fn set_gpu_context(&mut self, context: Option<GpuContext>) {
        self.engine = None;
        let Some(context) = context else { return };
        let Some(context) = context.downcast_ref::<ExternalGpuContext>() else {
            self.fail(format!(
                "video GPU output requires wgpu, received {}",
                context.backend()
            ));
            return;
        };
        match Engine::new(context.clone(), &self.options, &self.shared) {
            Ok(engine) => self.engine = Some(engine),
            Err(error) => self.fail(error.to_string()),
        }
    }

    fn set_wake_callback(&mut self, wake: WakeCallback) {
        self.shared.lock().unwrap().wake = wake.clone();
        if let Some(engine) = &self.engine {
            *engine.callback.wake.lock().unwrap() = wake;
        }
    }

    fn poll_async_changes(&mut self) -> WidgetChanges {
        match self.poll_engine() {
            Ok(true) => WidgetChanges::REDRAW,
            Ok(false) => WidgetChanges::empty(),
            Err(error) => {
                self.fail(error.to_string());
                WidgetChanges::REDRAW
            }
        }
    }

    fn animation_deadline(&self) -> Option<Instant> {
        self.engine.as_ref().and_then(|_| {
            let snapshot = &self.shared.lock().unwrap().snapshot;
            (!snapshot.paused && !snapshot.ended && snapshot.error.is_none())
                .then(|| Instant::now() + Duration::from_millis(100))
        })
    }

    fn paint(&mut self, cx: &mut PaintContext<'_>) {
        let Some(engine) = &self.engine else { return };
        if self.revision == 0 {
            return;
        }
        let [width, height] = engine.surface.size();
        let [available_width, available_height] = cx.size();
        let scale = (available_width / width as f32).min(available_height / height as f32);
        let draw_width = width as f32 * scale;
        let draw_height = height as f32 * scale;
        cx.draw_external_surface_in(
            ExternalSurface::new(
                self.surface_id,
                self.revision,
                [width, height],
                "wgpu",
                engine.surface.clone(),
            ),
            [
                f64::from((available_width - draw_width) * 0.5),
                f64::from((available_height - draw_height) * 0.5),
                f64::from(draw_width),
                f64::from(draw_height),
            ],
        );
    }
}

struct CallbackState {
    dirty: AtomicBool,
    wake: Mutex<WakeCallback>,
}

unsafe extern "C" fn render_ready(data: *mut c_void) {
    let callback = unsafe { &*data.cast::<CallbackState>() };
    callback.dirty.store(true, Ordering::Release);
    (callback.wake.lock().unwrap())();
}

struct GlLoader {
    _egl: Library,
    gl: Library,
    egl_get_proc_address: EglGetProcAddress,
}

impl GlLoader {
    unsafe fn new() -> Result<Self, Error> {
        let egl = unsafe { Library::new("libEGL.so.1") }.map_err(|error| Error::Gpu {
            message: error.to_string(),
        })?;
        let gl = unsafe { Library::new("libGL.so.1") }.map_err(|error| Error::Gpu {
            message: error.to_string(),
        })?;
        let egl_get_proc_address = unsafe {
            *egl.get::<EglGetProcAddress>(b"eglGetProcAddress\0")
                .map_err(|error| Error::Gpu {
                    message: error.to_string(),
                })?
        };
        Ok(Self {
            _egl: egl,
            gl,
            egl_get_proc_address,
        })
    }

    unsafe fn get(&self, name: *const c_char) -> *mut c_void {
        let symbol = unsafe { (self.egl_get_proc_address)(name) };
        if !symbol.is_null() {
            return symbol.cast_mut();
        }
        let bytes = unsafe { CStr::from_ptr(name) }.to_bytes_with_nul();
        unsafe {
            self.gl
                .get::<*mut c_void>(bytes)
                .map_or(ptr::null_mut(), |symbol| *symbol)
        }
    }
}

unsafe extern "C" fn get_proc_address(data: *mut c_void, name: *const c_char) -> *mut c_void {
    unsafe { (&*data.cast::<GlLoader>()).get(name) }
}

unsafe fn reset_gl(gl: &glow::Context, framebuffer: glow::Framebuffer, width: u32, height: u32) {
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);
        gl.read_buffer(glow::COLOR_ATTACHMENT0);
        gl.viewport(0, 0, width as i32, height as i32);
        gl.disable(glow::SCISSOR_TEST);
        gl.disable(glow::BLEND);
        gl.disable(glow::DEPTH_TEST);
        gl.disable(glow::STENCIL_TEST);
        gl.disable(glow::CULL_FACE);
        gl.color_mask(true, true, true, true);
        gl.depth_mask(true);
        gl.use_program(None);
        gl.bind_vertex_array(None);
        gl.bind_buffer(glow::ARRAY_BUFFER, None);
        gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, None);
        gl.active_texture(glow::TEXTURE0);
    }
}

struct RenderContext(*mut ffi::mpv_render_context);
impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe { ffi::mpv_render_context_free(self.0) };
    }
}

struct Engine {
    render: Option<RenderContext>,
    mpv: Mpv,
    loader: Box<GlLoader>,
    callback: Box<CallbackState>,
    gpu: ExternalGpuContext,
    source: Arc<ExternalTexture>,
    surface: Arc<ExternalTexture>,
    frame_pixels: Vec<u8>,
    zero_copy: bool,
    pending_audio: Option<String>,
}

impl Engine {
    fn new(
        gpu: ExternalGpuContext,
        options: &PlayerOptions,
        shared: &Mutex<Shared>,
    ) -> Result<Self, Error> {
        if gpu.adapter.backend != wgpu::Backend::Gl {
            return Err(Error::Gpu {
                message: format!(
                    "libmpv GPU output currently requires the wgpu GL backend, got {:?}; launch with WGPU_BACKEND=gl",
                    gpu.adapter.backend
                ),
            });
        }
        let size = wgpu::Extent3d {
            width: options.width,
            height: options.height,
            depth_or_array_layers: 1,
        };
        let zero_copy = std::env::var("PLICO_VIDEO_ZERO_COPY")
            .map(|value| !matches!(value.as_str(), "0" | "false" | "off"))
            .unwrap_or(options.prefer_zero_copy);
        let descriptor = wgpu::TextureDescriptor {
            label: Some("libmpv external video"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | if zero_copy {
                    wgpu::TextureUsages::empty()
                } else {
                    wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC
                },
            view_formats: &[],
        };
        let texture = if zero_copy {
            create_imported_gl_texture(&gpu, &descriptor)?
        } else {
            let texture = gpu.device.create_texture(&descriptor);
            gpu.queue.write_texture(
                texture.as_image_copy(),
                &vec![0; options.width as usize * options.height as usize * 4],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(options.width * 4),
                    rows_per_image: Some(options.height),
                },
                size,
            );
            let initialized = gpu.queue.submit([]);
            gpu.device
                .poll(wgpu::PollType::Wait {
                    submission_index: Some(initialized),
                    timeout: Some(Duration::from_secs(2)),
                })
                .map_err(|error| Error::Gpu {
                    message: error.to_string(),
                })?;
            texture
        };
        let source = Arc::new(ExternalTexture::new(
            texture,
            [options.width, options.height],
        ));
        let surface = if zero_copy {
            source.clone()
        } else {
            let bridge = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("libmpv wgpu video bridge"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            Arc::new(ExternalTexture::new(
                bridge,
                [options.width, options.height],
            ))
        };
        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("hwdec", "auto")?;
            init.set_property("terminal", std::env::var_os("PLICO_VIDEO_DEBUG").is_some())?;
            if std::env::var_os("PLICO_VIDEO_DEBUG").is_some() {
                init.set_property("msg-level", "all=v")?;
            }
            init.set_property("idle", true)?;
            init.set_property("keep-open", true)?;
            if options.muted {
                init.set_property("audio", false)?;
            }
            if let Some(referrer) = &options.referrer {
                init.set_property("referrer", referrer.as_str())?;
            }
            if let Some(agent) = &options.user_agent {
                init.set_property("user-agent", agent.as_str())?;
            }
            Ok(())
        })
        .map_err(|source| Error::Mpv { source })?;
        let loader = Box::new(unsafe { GlLoader::new()? });
        let callback = Box::new(CallbackState {
            dirty: AtomicBool::new(true),
            wake: Mutex::new(shared.lock().unwrap().wake.clone()),
        });
        let mut init = ffi::mpv_opengl_init_params {
            get_proc_address: Some(get_proc_address),
            get_proc_address_ctx: (&*loader as *const GlLoader).cast_mut().cast(),
        };
        let mut advanced = 1;
        let mut params = vec![
            super::mpv::param(
                ffi::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                ffi::MPV_RENDER_API_TYPE_OPENGL.as_ptr().cast_mut().cast(),
            ),
            super::mpv::param(
                ffi::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                (&mut init as *mut ffi::mpv_opengl_init_params).cast(),
            ),
            super::mpv::param(
                ffi::mpv_render_param_type_MPV_RENDER_PARAM_ADVANCED_CONTROL,
                (&mut advanced as *mut i32).cast(),
            ),
        ];
        if let Some(display) = gpu.native_display {
            let (kind, address) = match display {
                NativeDisplay::X11(address) => (
                    ffi::mpv_render_param_type_MPV_RENDER_PARAM_X11_DISPLAY,
                    address,
                ),
                NativeDisplay::Wayland(address) => (
                    ffi::mpv_render_param_type_MPV_RENDER_PARAM_WL_DISPLAY,
                    address,
                ),
            };
            params.push(super::mpv::param(kind, address as *mut c_void));
        }
        params.push(super::mpv::param(
            ffi::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
            ptr::null_mut(),
        ));
        let mut render = ptr::null_mut();
        // libmpv queries GL capabilities during context creation, so the
        // wgpu GLES context must be current on this thread for the call.
        let hal_device =
            unsafe { gpu.device.as_hal::<wgpu::hal::api::Gles>() }.ok_or_else(|| Error::Gpu {
                message: "wgpu did not expose a GLES device".into(),
            })?;
        let gl_context = hal_device.context();
        let gl = gl_context.lock();
        let wgpu_vertex_array = unsafe {
            NonZeroU32::new(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32)
                .map(glow::NativeVertexArray)
        };
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.use_program(None);
            gl.bind_vertex_array(None);
        }
        super::mpv::check(unsafe {
            ffi::mpv_render_context_create(&mut render, mpv.ctx.as_ptr(), params.as_mut_ptr())
        })?;
        unsafe { gl.bind_vertex_array(wgpu_vertex_array) };
        unsafe {
            ffi::mpv_render_context_set_update_callback(
                render,
                Some(render_ready),
                (&*callback as *const CallbackState).cast_mut().cast(),
            )
        };
        drop(gl);
        drop(hal_device);
        Ok(Self {
            render: Some(RenderContext(render)),
            mpv,
            loader,
            callback,
            gpu,
            source,
            surface,
            frame_pixels: if zero_copy {
                Vec::new()
            } else {
                vec![0; options.width as usize * options.height as usize * 4]
            },
            zero_copy,
            pending_audio: None,
        })
    }

    fn command(&mut self, command: Command, shared: &Mutex<Shared>) -> Result<(), Error> {
        let hal_device =
            unsafe { self.gpu.device.as_hal::<wgpu::hal::api::Gles>() }.ok_or_else(|| {
                Error::Gpu {
                    message: "GLES device was lost".into(),
                }
            })?;
        let gl_context = hal_device.context();
        let _gl = gl_context.lock();
        match command {
            Command::Load { video, audio } => {
                self.pending_audio = audio.filter(|audio| !audio.is_empty());
                {
                    let mut shared = shared.lock().unwrap();
                    shared.snapshot.error = None;
                    shared.snapshot.loading = true;
                    shared.snapshot.ended = false;
                }
                self.mpv
                    .set_property("pause", false)
                    .map_err(|source| Error::Mpv { source })?;
                let command = CString::new("loadfile").map_err(|error| Error::Gpu {
                    message: error.to_string(),
                })?;
                let video = CString::new(video).map_err(|error| Error::Gpu {
                    message: error.to_string(),
                })?;
                let replace = CString::new("replace").expect("literal has no NUL");
                let mut args = [
                    command.as_ptr(),
                    video.as_ptr(),
                    replace.as_ptr(),
                    ptr::null(),
                ];
                super::mpv::check(unsafe {
                    ffi::mpv_command_async(self.mpv.ctx.as_ptr(), 1, args.as_mut_ptr())
                })?;
                Ok(())
            }
            Command::Volume(volume) => self
                .mpv
                .set_property("volume", volume * 100.)
                .map_err(|source| Error::Mpv { source }),
            Command::Pause(paused) => self
                .mpv
                .set_property("pause", paused)
                .map_err(|source| Error::Mpv { source }),
            Command::Seek(seconds) => self
                .mpv
                .command("seek", &[&seconds.to_string(), "absolute+exact"])
                .map_err(|source| Error::Mpv { source }),
        }
    }

    fn poll_events(&mut self, shared: &Mutex<Shared>) -> Result<bool, Error> {
        let mut changed = false;
        while let Some(event) = self.mpv.wait_event(0.) {
            match event {
                Ok(libmpv2::events::Event::FileLoaded) => {
                    shared.lock().unwrap().snapshot.loading = false;
                    if let Some(audio) = self.pending_audio.take() {
                        self.mpv
                            .command("audio-add", &[&audio, "select"])
                            .map_err(|source| Error::Mpv { source })?;
                    }
                    changed = true;
                }
                Ok(libmpv2::events::Event::EndFile(reason))
                    if reason == libmpv2::mpv_end_file_reason::Error =>
                {
                    let mut shared = shared.lock().unwrap();
                    shared.snapshot.error = Some("Could not load/decode media".into());
                    shared.snapshot.loading = false;
                    changed = true;
                }
                _ => {}
            }
        }
        Ok(changed)
    }

    fn update_status(&self, shared: &Mutex<Shared>) {
        let mut shared = shared.lock().unwrap();
        shared.snapshot.position = self.mpv.get_property::<f64>("time-pos").unwrap_or(0.);
        shared.snapshot.duration = self.mpv.get_property::<f64>("duration").unwrap_or(0.);
        shared.snapshot.paused = self.mpv.get_property::<bool>("pause").unwrap_or(false);
        shared.snapshot.volume = self.mpv.get_property::<f64>("volume").unwrap_or(100.) / 100.;
        shared.snapshot.ended = self
            .mpv
            .get_property::<bool>("eof-reached")
            .unwrap_or(false);
        shared.snapshot.audio_channels = self
            .mpv
            .get_property::<i64>("audio-params/channel-count")
            .unwrap_or(0)
            .max(0) as u32;
        shared.snapshot.hwdec = self
            .mpv
            .get_property::<String>("hwdec-current")
            .ok()
            .filter(|value| !value.is_empty() && value != "no");
    }

    fn render(&mut self) -> Result<bool, Error> {
        let render = self.render.as_ref().expect("live render context");
        let hal_device =
            unsafe { self.gpu.device.as_hal::<wgpu::hal::api::Gles>() }.ok_or_else(|| {
                Error::Gpu {
                    message: "GLES device was lost".into(),
                }
            })?;
        let context = hal_device.context();
        let gl = context.lock();
        // wgpu-hal binds one VAO when it opens the GLES device and relies on
        // that binding across submissions. Preserve it around foreign GL.
        let wgpu_vertex_array = unsafe {
            NonZeroU32::new(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32)
                .map(glow::NativeVertexArray)
        };
        let flags = unsafe { ffi::mpv_render_context_update(render.0) };
        if flags & u64::from(ffi::mpv_render_update_flag_MPV_RENDER_UPDATE_FRAME) == 0 {
            return Ok(false);
        }
        let [width, height] = self.surface.size();
        let hal_texture = unsafe { self.source.texture().as_hal::<wgpu::hal::api::Gles>() }
            .ok_or_else(|| Error::Gpu {
                message: "GLES texture was lost".into(),
            })?;
        let raw_texture = match hal_texture.inner {
            wgpu::hal::gles::TextureInner::Texture { raw, .. } => raw,
            _ => {
                return Err(Error::Gpu {
                    message: "external target is not a GL texture".into(),
                });
            }
        };
        let framebuffer =
            unsafe { gl.create_framebuffer() }.map_err(|message| Error::Gpu { message })?;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(raw_texture),
                0,
            );
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                gl.delete_framebuffer(framebuffer);
                return Err(Error::Gpu {
                    message: "libmpv framebuffer is incomplete".into(),
                });
            }
        }
        unsafe { reset_gl(&gl, framebuffer, width, height) };
        let mut fbo = ffi::mpv_opengl_fbo {
            fbo: framebuffer.0.get() as i32,
            w: width as i32,
            h: height as i32,
            internal_format: 0,
        };
        let mut flip = 1;
        let mut params = [
            super::mpv::param(
                ffi::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                (&mut fbo as *mut ffi::mpv_opengl_fbo).cast(),
            ),
            super::mpv::param(
                ffi::mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                (&mut flip as *mut i32).cast(),
            ),
            super::mpv::param(
                ffi::mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                ptr::null_mut(),
            ),
        ];
        super::mpv::check(unsafe {
            ffi::mpv_render_context_render(render.0, params.as_mut_ptr())
        })?;
        unsafe {
            if !self.zero_copy {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(framebuffer));
                gl.read_buffer(glow::COLOR_ATTACHMENT0);
                gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
                gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
                gl.read_pixels(
                    0,
                    0,
                    width as i32,
                    height as i32,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut self.frame_pixels)),
                );
            }
            ffi::mpv_render_context_report_swap(render.0);
            // Both sides issue work through wgpu-hal's single locked GL
            // context, so command ordering already provides the dependency.
            // Flush submits the foreign draw without a CPU/GPU rendezvous.
            gl.flush();
            reset_gl(&gl, framebuffer, width, height);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.pixel_store_i32(glow::PACK_ROW_LENGTH, 0);
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.delete_framebuffer(framebuffer);
            gl.bind_vertex_array(wgpu_vertex_array);
        }
        drop(gl);
        drop(hal_texture);
        drop(hal_device);
        if !self.zero_copy {
            self.gpu.queue.write_texture(
                self.surface.texture().as_image_copy(),
                &self.frame_pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(true)
    }
}

fn create_imported_gl_texture(
    gpu: &ExternalGpuContext,
    descriptor: &wgpu::TextureDescriptor<'_>,
) -> Result<wgpu::Texture, Error> {
    let hal_device =
        unsafe { gpu.device.as_hal::<wgpu::hal::api::Gles>() }.ok_or_else(|| Error::Gpu {
            message: "wgpu did not expose a GLES device".into(),
        })?;
    let context = hal_device.context();
    let gl = context.lock();
    let raw = unsafe { gl.create_texture() }.map_err(|message| Error::Gpu { message })?;
    let framebuffer =
        unsafe { gl.create_framebuffer() }.map_err(|message| Error::Gpu { message })?;
    unsafe {
        gl.bind_texture(glow::TEXTURE_2D, Some(raw));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            descriptor.size.width as i32,
            descriptor.size.height as i32,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(raw),
            0,
        );
        if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_framebuffer(framebuffer);
            gl.delete_texture(raw);
            return Err(Error::Gpu {
                message: "imported GL framebuffer is incomplete".into(),
            });
        }
        gl.viewport(
            0,
            0,
            descriptor.size.width as i32,
            descriptor.size.height as i32,
        );
        gl.clear_color(0., 0., 0., 1.);
        gl.clear(glow::COLOR_BUFFER_BIT);
        gl.finish();
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.delete_framebuffer(framebuffer);
    }

    let hal_descriptor = wgpu::hal::TextureDescriptor {
        label: descriptor.label,
        size: descriptor.size,
        mip_level_count: descriptor.mip_level_count,
        sample_count: descriptor.sample_count,
        dimension: descriptor.dimension,
        format: descriptor.format,
        usage: wgpu::TextureUses::COLOR_TARGET | wgpu::TextureUses::RESOURCE,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: descriptor.view_formats.to_vec(),
    };
    let cleanup_device = gpu.device.clone();
    let cleanup_raw = raw;
    let cleanup: wgpu::hal::DropCallback = Box::new(move || {
        if let Some(device) = unsafe { cleanup_device.as_hal::<wgpu::hal::api::Gles>() } {
            let gl = device.context().lock();
            unsafe { gl.delete_texture(cleanup_raw) };
        }
    });
    let hal_texture = unsafe { hal_device.texture_from_raw(raw.0, &hal_descriptor, Some(cleanup)) };
    drop(gl);
    drop(hal_device);
    Ok(unsafe {
        gpu.device.create_texture_from_hal::<wgpu::hal::api::Gles>(
            hal_texture,
            descriptor,
            wgpu::TextureUses::RESOURCE,
        )
    })
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.render.take();
        let _ = &self.loader;
    }
}
