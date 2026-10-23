use plico_core::{DeclareView, Engine, FrameError, Theme, prelude::*};
use plico_lvgl::{Adapter, oxivgl::driver::LvglDriver};
use std::cell::RefCell;

// The BSP initialized LVGL before create(). Never call init() or timer_handler().
static DRIVER: LvglDriver = LvglDriver;
thread_local! {
    static DEMO: RefCell<Option<Demo>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Counter {
    count: u32,
    extra: bool,
}
impl DeclareView for Counter {
    fn theme(&self) -> Theme {
        Theme::default()
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(24).gap(16) + |cx| {
            text(cx, "Plico / ESP32-P4").font_size(24);
            text!(cx, "Count: {}", self.count);
            row(cx).gap(12) + |cx| {
                if button(cx).text("Increment").primary().clicked() {
                    self.count = self.count.saturating_add(1);
                }
                if button(cx).text("Toggle details").clicked() {
                    self.extra = !self.extra;
                }
            };
            if self.extra {
                text(cx, "Rust std + ESP-IDF + native LVGL");
            }
        };
    }
}
struct Demo {
    adapter: Adapter<'static>,
    engine: Engine,
    app: Counter,
}
impl Demo {
    fn declare(&mut self) -> i32 {
        if self.engine.declare_until_settled(&mut self.app).is_err() {
            return -3;
        }
        if self.adapter.sync(&self.engine).is_err() {
            return -4;
        }
        0
    }
}

/// Mount beneath the active screen. Returns 0, or a negative error code.
///
/// # Safety
/// The BSP must have initialized LVGL and a display. All three entry points
/// must execute on the same GUI task with the LVGL lock held. Destroy before
/// deleting the screen; only one instance is supported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_p4_create(width: u32, height: u32) -> i32 {
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return -1;
    }
    DEMO.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return -2;
        }
        let Ok(adapter) = Adapter::new(&DRIVER) else {
            return -4;
        };
        let mut engine = Engine::default();
        let viewport: Result<_, FrameError> = engine.set_viewport(width as f32, height as f32);
        if viewport.is_err() {
            return -1;
        }
        let mut demo = Demo {
            adapter,
            engine,
            app: Counter::default(),
        };
        let result = demo.declare();
        if result == 0 {
            *slot = Some(demo);
        }
        result
    })
}

/// Drain click messages; clean ticks do not declare or synchronize the tree.
///
/// # Safety
/// Same GUI task, lock and screen lifetime requirements as `plico_p4_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_p4_update() -> i32 {
    DEMO.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(demo) = slot.as_mut() else { return -2 };
        if demo.adapter.dispatch(&mut demo.engine) == 0 {
            return 0;
        }
        demo.declare()
    })
}

/// Stop the caller's update timer before destroying the projection.
///
/// # Safety
/// Same GUI task, lock and screen lifetime requirements as `plico_p4_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_p4_destroy() {
    DEMO.with(|slot| drop(slot.borrow_mut().take()));
}
