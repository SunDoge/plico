//! Experimental ABI v5. All counts are bounded by the decoder; all pointers are
//! borrowed synchronously. Handles belong to the thread that created them.
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Command {
    pub id: u64,
    pub parent: u32,
    pub kind: u32,
    pub flags: u32,
    pub text_start: u32,
    pub text_len: u32,
    pub value_start: u32,
    pub value_len: u32,
    pub width: f32,
    pub height: f32,
    pub grow: f32,
    pub gap: f32,
    pub padding: f32,
    pub radius: f32,
    pub font_size: u32,
    pub weight: u32,
    pub background: u32,
    pub foreground: u32,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub resource: u64,
    pub border_width: f32,
    pub border_color: u32,
    pub shrink: f32,
    pub min_width: f32,
    pub min_height: f32,
    pub hover_background: u32,
    pub pressed_background: u32,
    pub semantic_role: u32,
    pub opacity: f32,
    pub properties_start: u32,
    pub properties_len: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Batch {
    pub version: u32,
    pub dark: u32,
    pub animate: u32,
    pub commands: *const Command,
    pub command_count: u32,
    pub utf8: *const u8,
    pub utf8_len: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub id: u64,
    pub kind: u32,
    pub text_start: u32,
    pub text_len: u32,
    pub value: f64,
}
#[repr(C)]
pub struct Frame {
    pub version: u32,
    pub width: f32,
    pub height: f32,
    pub seconds: f64,
    pub events: *const Event,
    pub event_count: u32,
    pub utf8: *const u8,
    pub utf8_len: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub callbacks: u64,
    pub batches: u64,
    pub declarations: u64,
    pub commands: u64,
    pub callback_micros: u64,
    pub decode_micros: u64,
    pub declare_micros: u64,
}
pub type Build = unsafe extern "C" fn(*mut c_void, *const Frame, *mut Batch) -> i32;

/// Managed property/event schema revision, independent from memory layout ABI.
/// Increment for schema/behavior changes required by the managed SDK. App and
/// managed widget edits do not change this revision or require a Rust build.
#[unsafe(no_mangle)]
pub extern "C" fn plico_protocol_revision() -> u32 {
    3
}

/// ABI/size handshake before the first callback.
#[unsafe(no_mangle)]
pub extern "C" fn plico_abi_version() -> u32 {
    5
}
#[unsafe(no_mangle)]
pub extern "C" fn plico_command_size() -> u32 {
    std::mem::size_of::<Command>() as u32
}
/// Callback/user must remain valid until destroy/run returns. Callback is only
/// invoked synchronously on the creating thread, and must catch managed errors.
/// # Safety
/// The callback and user data must obey that lifetime and calling convention.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_create(build: Option<Build>, user: *mut c_void) -> u64 {
    crate::create(build, user)
}
#[unsafe(no_mangle)]
pub extern "C" fn plico_destroy(handle: u64) -> i32 {
    crate::destroy(handle)
}
/// # Safety
/// UTF-8 path must point to len readable bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_load_image(handle: u64, path: *const u8, len: u32) -> u64 {
    crate::load_image(handle, path, len)
}
/// # Safety
/// UTF-8 output path must point to len readable bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_capture(
    handle: u64,
    path: *const u8,
    len: u32,
    width: u32,
    height: u32,
) -> i32 {
    crate::capture(handle, path, len, width, height)
}
/// Consumes the handle. Must run on the OS main thread; no callback survives it.
/// # Safety
/// UTF-8 title must point to len readable bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_run(
    handle: u64,
    title: *const u8,
    len: u32,
    width: u32,
    height: u32,
) -> i32 {
    crate::run(handle, title, len, width, height)
}
/// Rebuild C# declarations on the next native declaration (external app state).
#[unsafe(no_mangle)]
pub extern "C" fn plico_invalidate(handle: u64) -> i32 {
    crate::invalidate(handle)
}
/// CPU-only preview/test step; no GPU or window. Explicitly evaluates the view.
#[unsafe(no_mangle)]
pub extern "C" fn plico_test_frame(handle: u64, width: u32, height: u32) -> i32 {
    crate::test_frame(handle, width, height)
}
/// Semantic protocol route; uses the real runtime's event/settle handling.
/// # Safety
/// role and label are UTF-8 buffers readable for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_test_click(
    handle: u64,
    role: *const u8,
    role_len: u32,
    label: *const u8,
    label_len: u32,
) -> i32 {
    crate::test_click(handle, role, role_len, label, label_len)
}
/// # Safety
/// out must point to writable Stats storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_stats(handle: u64, out: *mut Stats) -> i32 {
    crate::stats(handle, out)
}
/// Copies a thread-local diagnostic (no terminator). Returns required byte count.
/// # Safety
/// A non-null dst must point to capacity writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_last_error(dst: *mut u8, capacity: u32) -> u32 {
    crate::last_error(dst, capacity)
}

/// # Safety
/// label/text are valid UTF-8 buffers readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_test_text(
    handle: u64,
    label: *const u8,
    label_len: u32,
    text: *const u8,
    text_len: u32,
) -> i32 {
    crate::test_text(handle, label, label_len, text, text_len)
}
/// # Safety
/// label is a valid UTF-8 buffer readable for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_test_value(
    handle: u64,
    label: *const u8,
    label_len: u32,
    value: f64,
) -> i32 {
    crate::test_value(handle, label, label_len, value)
}

/// Thread-safe notification. Does not access UI state or invoke managed code.
#[unsafe(no_mangle)]
pub extern "C" fn plico_wake(handle: u64) -> i32 {
    crate::wake(handle)
}

/// Thread-safe retained canvas command: fit=0, actual size=1, zoom factor=2.
#[unsafe(no_mangle)]
pub extern "C" fn plico_canvas_control(handle: u64, operation: u32, value: f64) -> i32 {
    crate::canvas::control(handle, operation, value)
}

#[repr(C)]
#[derive(Default)]
pub struct CanvasViewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}
/// # Safety
/// out points to writable CanvasViewport storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_canvas_viewport(handle: u64, out: *mut CanvasViewport) -> i32 {
    crate::canvas::viewport(handle, out)
}
/// Renderer-independent pointer input for automation; coordinates are logical pixels.
#[unsafe(no_mangle)]
pub extern "C" fn plico_test_pointer(handle: u64, phase: u32, x: f64, y: f64, pressed: u32) -> i32 {
    crate::canvas::pointer(handle, phase, x, y, pressed != 0)
}

/// Releases application ownership. A resource referenced by the current batch
/// stays live until the replacement batch no longer references it.
#[unsafe(no_mangle)]
pub extern "C" fn plico_release_image(handle: u64, resource: u64) -> i32 {
    crate::release_image(handle, resource)
}

/// # Safety
/// Callback and user data obey the same lifetime contract as plico_create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_create_virtual(build: Option<Build>, user: *mut c_void) -> u64 {
    crate::create_with_clock(build, user, true)
}
/// Set monotonic virtual time; rejects system-clock hosts and backwards time.
#[unsafe(no_mangle)]
pub extern "C" fn plico_set_time(handle: u64, milliseconds: u64) -> i32 {
    crate::status(|| {
        crate::with_host(handle, |host| {
            host.clock
                .as_ref()
                .ok_or("host does not use virtual time")?
                .set(std::time::Duration::from_millis(milliseconds))
                .map_err(|e| e.to_string())?;
            host.source.source.view.dirty = true;
            Ok(())
        })
    })
}

/// Pixel wheel input for renderer-independent automation.
#[unsafe(no_mangle)]
pub extern "C" fn plico_test_scroll(handle: u64, x: f64, y: f64, delta_y: f64) -> i32 {
    crate::status(|| {
        use plico_renderer::{
            FrameSource, GesturePhase, Point, UiEvent, WheelDeltaMode, WheelEvent,
        };
        if ![x, y, delta_y].iter().all(|v| v.is_finite()) {
            return Err("invalid wheel input".into());
        }
        crate::with_host(handle, |host| {
            host.source.prepare_for_event(&mut host.text);
            host.source.handle_event(UiEvent::Wheel(WheelEvent {
                position: Point { x, y },
                delta_x: 0.,
                delta_y,
                delta_mode: WheelDeltaMode::Pixel,
                phase: GesturePhase::Changed,
                modifiers: Default::default(),
            }));
            host.source.poll_async();
            Ok(())
        })
    })
}

/// # Safety
/// Key UTF-8 is borrowed for len readable bytes during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_test_key(handle: u64, key: *const u8, len: u32) -> i32 {
    crate::status(|| {
        use plico_renderer::{FrameSource, KeyEvent, KeyLocation, KeyPhase, UiEvent};
        let key = crate::utf8(key, len)?.to_owned();
        crate::with_host(handle, |host| {
            host.source.prepare_for_event(&mut host.text);
            host.source.handle_event(UiEvent::Key(KeyEvent {
                phase: KeyPhase::Down,
                code: key.clone(),
                key_without_modifiers: key.clone(),
                key,
                text: None,
                text_with_all_modifiers: None,
                location: KeyLocation::Standard,
                modifiers: Default::default(),
                repeat: false,
                synthetic: false,
            }));
            host.source.poll_async();
            Ok(())
        })
    })
}

/// Decode/copy preflight for protocol tests and benchmarks; no Engine or renderer.
/// # Safety
/// Foreign command and UTF-8 arrays must be readable for their declared lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_test_decode_batch(batch: Batch) -> i32 {
    crate::status(|| {
        let owned = unsafe { crate::batch::OwnedBatch::copy(batch) }?;
        std::hint::black_box(&owned);
        Ok(())
    })
}

/// Consumes the host and opens a window with validated SDK options.
/// # Safety
/// `json` must point to `len` readable UTF-8 bytes for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_run_options(handle: u64, json: *const u8, len: u32) -> i32 {
    crate::run_configured(handle, json, len)
}

/// Validate WGSL on the calling thread without a window or GPU. Flags: bit 0
/// complete module, bit 1 backdrop effect. Errors use plico_last_error.
/// # Safety
/// Source must reference len readable bytes for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_validate_shader(source: *const u8, len: u32, flags: u32) -> i32 {
    crate::validate_shader(source, len, flags)
}
