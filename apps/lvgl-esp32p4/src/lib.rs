//! ESP-IDF owns LVGL initialization, tick, display, touch and the GUI task.
//! The C entry points are enabled only for the firmware target.
#[cfg(all(feature = "firmware", target_os = "espidf"))]
mod firmware;
