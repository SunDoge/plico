//! Raw-binding glue: init, host display, snapshot. Everything unsafe stays
//! here; the adapter above is safe code.
#![allow(unsafe_code)]
use lightvgl_sys as sys;

/// Zero-sized token proving `lv_init()` ran; only one may exist per process.
pub struct Driver {
    _private: (),
}
static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "native")]
unsafe extern "C" {
    fn plico_sdl_install_close_filter();
    fn plico_sdl_close_requested() -> std::ffi::c_int;
}

unsafe extern "C" fn tick_cb() -> u32 {
    #[cfg(not(target_os = "none"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u32)
            .unwrap_or(0)
    }
    #[cfg(target_os = "none")]
    {
        0
    }
}
unsafe extern "C" fn flush_cb(drv: *mut sys::lv_display_t, _: *const sys::lv_area_t, _: *mut u8) {
    unsafe { sys::lv_display_flush_ready(drv) };
}
/// LV_ASSERT_HANDLER target: panic with the log line LVGL printed just above.
#[unsafe(no_mangle)]
unsafe extern "C" fn plico_lv_assert_handler() {
    panic!("LVGL assertion failed (see the log line above)");
}
impl Driver {
    /// Open a fixed-size SDL2 test window with mouse and keyboard navigation.
    /// LVGL runs on this thread; drop adapters and exit when close_requested().
    #[cfg(feature = "native")]
    pub fn sdl(w: i32, h: i32) -> std::io::Result<Driver> {
        assert!(w > 0 && h > 0, "display dimensions must be positive");
        assert!(
            !INITIALIZED.swap(true, std::sync::atomic::Ordering::AcqRel),
            "only one Driver may exist per process"
        );
        unsafe {
            sys::lv_init();
            plico_sdl_install_close_filter();
            let display = sys::lv_sdl_window_create(w, h);
            if display.is_null() {
                return Err(std::io::Error::other("LVGL could not open an SDL2 window"));
            }
            sys::lv_sdl_window_set_title(display, c"Plico / LVGL".as_ptr());
            sys::lv_sdl_window_set_resizeable(display, false);
            let group = sys::lv_group_create();
            let mouse = sys::lv_sdl_mouse_create();
            let keyboard = sys::lv_sdl_keyboard_create();
            if group.is_null() || mouse.is_null() || keyboard.is_null() {
                return Err(std::io::Error::other(
                    "LVGL could not create SDL2 input devices",
                ));
            }
            sys::lv_group_set_default(group);
            sys::lv_indev_set_display(mouse, display);
            sys::lv_indev_set_display(keyboard, display);
            sys::lv_indev_set_group(keyboard, group);
        }
        Ok(Driver { _private: () })
    }

    /// SDL close events are deferred so Adapter can release its widgets first.
    #[cfg(feature = "native")]
    pub fn close_requested(&self) -> bool {
        unsafe { plico_sdl_close_requested() != 0 }
    }

    /// Attach to LVGL initialized and driven by an external BSP.
    ///
    /// # Safety
    /// LVGL, its display and clock must already be initialized. The caller
    /// owns the event loop and must serialize all adapter calls on its GUI
    /// thread. Keep LVGL alive until every adapter is destroyed.
    pub unsafe fn attach() -> Driver {
        assert!(
            unsafe { sys::lv_is_initialized() },
            "LVGL is not initialized"
        );
        assert!(
            !INITIALIZED.swap(true, std::sync::atomic::Ordering::AcqRel),
            "only one Driver may exist per process"
        );
        Driver { _private: () }
    }

    /// Headless software display: renders into an RGB565 buffer, no window.
    pub fn init(w: i32, h: i32) -> Driver {
        assert!(
            !INITIALIZED.swap(true, std::sync::atomic::Ordering::AcqRel),
            "lv_init() called twice — only one Driver may exist per process"
        );
        unsafe {
            sys::lv_init();
            sys::lv_tick_set_cb(Some(tick_cb));
            // Full-frame buffer: transformed sub-layers can span the whole
            // screen; a partial band buffer SIGSEGVs on their bounding boxes.
            let stride = w as usize * 2; // RGB565
            let size = stride * h as usize;
            let cbuf = Box::into_raw(vec![0u8; size].into_boxed_slice()) as *mut std::ffi::c_void;
            let disp = sys::lv_display_create(w, h);
            assert!(!disp.is_null(), "lv_display_create returned NULL");
            sys::lv_display_set_color_format(disp, sys::lv_color_format_t_LV_COLOR_FORMAT_RGB565);
            sys::lv_display_set_buffers(
                disp,
                cbuf,
                std::ptr::null_mut(),
                size as u32,
                sys::lv_display_render_mode_t_LV_DISPLAY_RENDER_MODE_FULL,
            );
            sys::lv_display_set_flush_cb(disp, Some(flush_cb));
        }
        Driver { _private: () }
    }
    /// LVGL periodic work; returns the suggested next delay in milliseconds.
    pub fn timer_handler(&self) -> u32 {
        unsafe { sys::lv_timer_handler() }
    }
}

/// Off-screen capture of the active screen, RGB565.
pub struct Snapshot {
    buf: *mut sys::lv_draw_buf_t,
    width: u32,
    height: u32,
    stride: usize,
}
impl Snapshot {
    /// Capture the active screen. Fails when allocation fails.
    pub fn take() -> Option<Snapshot> {
        let buf = unsafe {
            sys::lv_snapshot_take(
                sys::lv_screen_active(),
                sys::lv_color_format_t_LV_COLOR_FORMAT_RGB565,
            )
        };
        if buf.is_null() {
            return None;
        }
        unsafe {
            let header = &(*buf).header;
            Some(Snapshot {
                buf,
                width: header.w(),
                height: header.h(),
                stride: header.stride() as usize,
            })
        }
    }
    /// Write as PNG, converting RGB565 to RGB8.
    pub fn write_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        let w = self.width as usize;
        let h = self.height as usize;
        let data =
            unsafe { std::slice::from_raw_parts((*self.buf).data, (*self.buf).data_size as usize) };
        let mut rgb = Vec::with_capacity(w * h * 3);
        for row in 0..h {
            for col in 0..w {
                let off = row * self.stride + col * 2;
                let p = u16::from_le_bytes([data[off], data[off + 1]]);
                let r = ((p >> 11) & 0x1F) as u8;
                let g = ((p >> 5) & 0x3F) as u8;
                let b = (p & 0x1F) as u8;
                rgb.push((r << 3) | (r >> 2));
                rgb.push((g << 2) | (g >> 4));
                rgb.push((b << 3) | (b >> 2));
            }
        }
        let file = std::fs::File::create(path)?;
        let buf = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(buf, self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
        writer
            .write_image_data(&rgb)
            .map_err(std::io::Error::other)?;
        Ok(())
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe { sys::lv_draw_buf_destroy(self.buf) };
    }
}

/// LVGL's content-sized coordinate.
pub const SIZE_CONTENT: i32 = (sys::LV_COORD_MAX | sys::LV_COORD_TYPE_SPEC) as i32;
/// Percentage coordinate helper.
pub fn pct(v: i32) -> i32 {
    unsafe { sys::lv_pct(v) }
}
/// Colors are 0xRRGGBB packed through LVGL's own converter.
pub fn color(hex: u32) -> sys::lv_color_t {
    unsafe { sys::lv_color_hex(hex) }
}
/// NUL-terminate into a stack/heap buffer for a C call.
pub fn with_cstr<R>(s: &str, f: impl FnOnce(*const std::ffi::c_char) -> R) -> R {
    let mut buf: Vec<u8> = Vec::with_capacity(s.len() + 1);
    buf.extend_from_slice(s.as_bytes());
    buf.push(0);
    f(buf.as_ptr() as *const std::ffi::c_char)
}
/// Bump a raw pointer as an object handle without wrapping type noise.
pub type Obj = sys::lv_obj_t;
