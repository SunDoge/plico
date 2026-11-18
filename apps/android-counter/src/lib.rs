#![cfg(target_os = "android")]

use plico::prelude::*;
use plico::{CalendarState, WeekStart};
use plico_runtime::{HostAction, HostView, Source};

#[derive(Default)]
struct Counter {
    action: Option<HostAction>,
    count: u32,
    page: usize,
    effect: usize,
    intensity: f64,
    paused: bool,
    dark: bool,
    show_calendar: bool,
    calendar: Option<CalendarState>,
    level: f64,
    note: plico::TextBuffer,
    increment_at: Option<std::time::Duration>,
}

impl HostView for Counter {
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.action.take()
    }
    fn theme(&self) -> Theme {
        if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        if let Some(deadline) = self.increment_at {
            if cx.time().now >= deadline {
                self.increment_at = None;
                self.count += 1;
                cx.request_rebuild();
            } else {
                cx.request_rebuild_at(deadline);
            }
        }
        column(cx).padding(24).gap(12) + |cx| {
            segmented(cx, &mut self.page, &["Counter", "Components", "Shaders"]);
            if self.page == 2 {
                segmented(cx, &mut self.effect, &["Orb", "Wave", "Edge"]);
                let asset = match self.effect {
                    0 => plico::shader_asset!("../../gallery/assets/shaders/orb.wgsl"),
                    1 => plico::effects::voice_wave(),
                    _ => plico::effects::edge_light(),
                };
                column(cx).width_percent(100.).height(280.).radius(20.).clip(true)
                    .background(Color::Literal(0x090b14)).justify(Justify::Center).align(Align::Center)
                    .position(Position::Relative) + |cx|
                {
                    if self.effect == 2 {
                        text(cx, "Edge light").color(Color::Literal(0xffffff)).font_size(24);
                    }
                    shader(cx, &asset).absolute(0., 0.).width_percent(100.).height_percent(100.)
                        .values(&[self.intensity as f32, 0.1]).animated(true).paused(self.paused);
                };
                text(cx, "Intensity");
                slider(cx, "Intensity", &mut self.intensity, 0.0..=1.0).width_percent(100.);
                if button(cx).text(if self.paused { "Play" } else { "Pause" }).clicked() {
                    self.paused = !self.paused;
                }
                return;
            }
            if self.page == 1 {
                switch(cx, "Dark appearance", &mut self.dark);
                slider(cx, "Progress", &mut self.level, 0.0..=100.0).step(1.).width_percent(100.);
                progress(cx, "Completion", self.level as f32 / 100.);
                separator(cx);
                checkbox(cx, "Show calendar", &mut self.show_calendar);
                if self.show_calendar {
                    let state = self.calendar.get_or_insert_with(|| {
                        let day = CalendarDate::new(2026, 11, 18).unwrap();
                        CalendarState::new(day).with_selected(day)
                    });
                    column(cx).align(Align::Center) + |cx| {
                        column(cx).padding(16).radius(16.).border(1., Color::Border).background(Color::Panel) + |cx| {
                            calendar(cx, state, WeekStart::Monday);
                        };
                    };
                }
                return;
            }
            text!(cx, "Count: {}", self.count);
            if button(cx).text("Increment").clicked() {
                self.count += 1;
            }
            text_input(cx, "Note", &self.note).placeholder("Type a note").width_percent(100.).height(36.);
            text!(cx, "Note: {}", self.note.text().unwrap_or_default());
            slider(cx, "Level", &mut self.level, 0.0..=100.0).step(1.).width_percent(100.);
            text!(cx, "Level: {}", self.level);
            if button(cx).text("Increment in 2 seconds").clicked() {
                self.increment_at = Some(cx.time().now + std::time::Duration::from_secs(2));
            }
            if button(cx).text("Open Plico on GitHub").clicked() {
                self.action = Some(HostAction::OpenUrl("https://github.com/SunDoge/plico".into()));
            }
            column(cx).position(Position::Relative).width_percent(100.).height(24.) + |cx| {
                shader(cx, &plico::shader_asset!("color_bands.wgsl")).width_percent(100.).height_percent(100.);
                shader(cx, &plico::shader_asset!("backdrop_channels.wgsl").read_backdrop()).absolute(0., 12.).width_percent(100.).height(12.);
            };
            shader(cx, &plico::effects::fluid_dots()).width_percent(100.).height(96.).animated(true);
        };
    }
}

#[unsafe(no_mangle)]
fn android_main(app: plico_android::AndroidApp) {
    plico_android::run(
        app,
        Source::new(Counter {
            show_calendar: true,
            intensity: 0.8,
            ..Default::default()
        }),
    )
    .expect("run Plico counter");
}
