use plico_core::{DeclareView, Engine, Entity, FrameError, Kind, Theme, prelude::*};
use plico_lvgl::{Adapter, Driver};
use std::cell::RefCell;

unsafe extern "C" {
    fn esp_timer_get_time() -> i64;
}

#[derive(Default)]
struct Profile {
    frames: u32,
    declare_us: i64,
    sync_us: i64,
    declare_max_us: i64,
    sync_max_us: i64,
    property_writes: usize,
    style_resets: usize,
}

// The BSP initialized LVGL before create(). Never call init() or timer_handler().
static DRIVER: std::sync::LazyLock<Driver> =
    std::sync::LazyLock::new(|| unsafe { Driver::attach() });
thread_local! {
    static DEMO: RefCell<Option<Demo>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct Counter {
    count: u32,
    extra: bool,
    time_ms: u32,
    paused: bool,
}
impl DeclareView for Counter {
    fn theme(&self) -> Theme {
        Theme::default()
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).width_percent(100.).height_percent(100.).padding(24).gap(16).background(Color::Surface) + |cx| {
            text(cx, "Plico / Signal lab").font_size(24);
            text(cx, "ANIMATION DEMO / NOT LIVE MODEM DATA").font_size(16);
            column(cx).width(432).padding(16).gap(12).radius(20.).background(Color::Literal(0x10283f)) + |cx| {
                text(cx, "AIR LINK").font_size(24).color(Color::Literal(0x7ee8d1));
                row(cx).height(136).gap(8).align(Align::End) + |cx| {
                    for index in 0..12u32 {
                        let wave = ((self.time_ms as f32 * 0.0035
                            + index as f32 * 0.55).sin()
                            + 1.)
                            * 0.5;
                        column(cx).key(index as u64).width(24).height(20. + wave * 108.).radius(10.)
                            .background(Color::Literal(if index < 8 { 0x55dcc3 } else { 0x62aaff })) + |_cx|
                        {};
                    }
                };
                text(cx, "PACKET FLOW").color(Color::Literal(0xa8bfd5));
                row(cx).width(400).height(14).radius(7.).background(Color::Literal(0x233e58)) + |cx| {
                    let pulse = ((self.time_ms as f32 * 0.0017).sin() + 1.) * 0.5;
                    column(cx).width(20. + pulse * 380.).height(14).radius(7.)
                        .background(Color::Literal(0x62aaff)) + |_cx|
                    {};
                };
            };
            if button(cx).text(if self.paused { "Resume animation" } else { "Pause animation" }).height(56)
                .padding_x(16).clicked()
            {
                self.paused = !self.paused;
            }
            text!(cx, "Count: {}", self.count).font_size(24);
            row(cx).gap(12) + |cx| {
                if button(cx).text("Increment").height(64).padding_x(16).primary().clicked()
                {
                    self.count = self.count.saturating_add(1);
                }
                if button(cx).text("Toggle details").height(64).padding_x(16).clicked()
                {
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
    adapter: Adapter,
    engine: Engine,
    app: Counter,
    last_time_ms: u32,
    profile: Profile,
}
impl Demo {
    fn declare(&mut self) -> i32 {
        let begin = unsafe { esp_timer_get_time() };
        if self.engine.declare_until_settled(&mut self.app).is_err() {
            return -3;
        }
        let delta = self.engine.take_frame_delta();
        let declared = unsafe { esp_timer_get_time() };
        if self.adapter.sync_delta(&self.engine, Some(delta)).is_err() {
            return -4;
        }
        let synced = unsafe { esp_timer_get_time() };
        let profile = &mut self.profile;
        profile.frames += 1;
        profile.property_writes += self.adapter.stats().property_writes;
        profile.style_resets += self.adapter.stats().style_resets;
        profile.declare_us += declared - begin;
        profile.sync_us += synced - declared;
        profile.declare_max_us = profile.declare_max_us.max(declared - begin);
        profile.sync_max_us = profile.sync_max_us.max(synced - declared);
        if profile.frames == 200 {
            eprintln!(
                "PLICO-STAGE frames={} declare_avg_us={} declare_max_us={} sync_avg_us={} sync_max_us={} property_writes={} style_resets={}",
                profile.frames,
                profile.declare_us / 200,
                profile.declare_max_us,
                profile.sync_us / 200,
                profile.sync_max_us,
                profile.property_writes,
                profile.style_resets,
            );
            *profile = Profile::default();
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
            last_time_ms: 0,
            profile: Profile::default(),
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
pub unsafe extern "C" fn plico_p4_update(time_ms: u32) -> i32 {
    DEMO.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(demo) = slot.as_mut() else { return -2 };
        let elapsed = time_ms.wrapping_sub(demo.last_time_ms);
        let animate = !demo.app.paused && elapsed >= 40;
        if demo.app.paused || animate {
            demo.last_time_ms = time_ms;
        }
        if animate {
            demo.app.time_ms = demo.app.time_ms.wrapping_add(elapsed.min(100));
        }
        if demo.adapter.dispatch(&mut demo.engine) == 0 && !animate {
            return 0;
        }
        let before = (demo.app.count, demo.app.extra);
        let result = demo.declare();
        if result == 0 && before != (demo.app.count, demo.app.extra) {
            eprintln!(
                "PLICO-CLICK count={} details={}",
                demo.app.count, demo.app.extra
            );
        }
        if result == 0 { 1 } else { result }
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

/// Current counter for diagnostics. Same GUI task requirements as create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_p4_count() -> u32 {
    DEMO.with(|slot| slot.borrow().as_ref().map_or(0, |demo| demo.app.count))
}

fn find(engine: &Engine, id: Entity, text: &str) -> Option<Entity> {
    let node = engine.node(id)?;
    if node.kind == Kind::ButtonBase && node.text == text {
        return Some(id);
    }
    node.children
        .iter()
        .find_map(|child| find(engine, *child, text))
}

/// Exercise actual LVGL CLICKED callbacks on-device; leaves the counter at zero.
/// Same GUI task requirements as create. This does not simulate physical touch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plico_p4_self_test() -> i32 {
    DEMO.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(demo) = slot.as_mut() else {
            return -2;
        };
        let Some(increment) = find(&demo.engine, demo.engine.root(), "Increment") else {
            return -10;
        };
        let Some(label) = demo
            .engine
            .node(increment)
            .and_then(|n| n.children.first().copied())
        else {
            return -16;
        };
        let Some(button_bounds) = demo.adapter.bounds(increment) else {
            return -16;
        };
        let Some(label_bounds) = demo.adapter.bounds(label) else {
            return -16;
        };
        // Compare twice the centers to avoid rounding away a one-pixel error.
        if (button_bounds[0] + button_bounds[2] - label_bounds[0] - label_bounds[2]).abs() > 2
            || (button_bounds[1] + button_bounds[3] - label_bounds[1] - label_bounds[3]).abs() > 2
        {
            return -17;
        }
        let object = demo.adapter.object_id(increment);
        if !demo.adapter.send_click(increment)
            || demo.adapter.dispatch(&mut demo.engine) != 1
            || demo.declare() != 0
            || demo.app.count != 1
            || demo.adapter.object_id(increment) != object
            || demo.adapter.stats().created != 0
        {
            return -11;
        }
        let Some(toggle) = find(&demo.engine, demo.engine.root(), "Toggle details") else {
            return -12;
        };
        for visible in [true, false] {
            if !demo.adapter.send_click(toggle)
                || demo.adapter.dispatch(&mut demo.engine) != 1
                || demo.declare() != 0
                || demo.app.extra != visible
            {
                return -13;
            }
        }
        demo.app.count = 0;
        if demo.declare() != 0 || demo.adapter.sync(&demo.engine).is_err() {
            return -14;
        }
        if demo.adapter.stats().style_updates != 0 || demo.adapter.stats().text_updates != 0 {
            return -15;
        }
        0
    })
}
