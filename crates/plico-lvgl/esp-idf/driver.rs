// ESP-IDF clock/log helpers. The Plico experiment never calls LvglDriver::init: the
// vendor BSP already owns initialization and the LVGL timer task.
#[cfg(target_os = "espidf")]
pub extern "C" fn get_tick_ms() -> u32 {
    unsafe extern "C" { fn esp_timer_get_time() -> i64; }
    (unsafe { esp_timer_get_time() } / 1000) as u32
}

#[cfg(target_os = "espidf")]
pub unsafe extern "C" fn lvgl_log_print(_level: i8, text: *const c_char) {
    if !text.is_null() {
        let message = unsafe { core::ffi::CStr::from_ptr(text) };
        eprintln!("LVGL: {}", message.to_string_lossy());
    }
}
