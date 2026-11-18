#![cfg(target_os = "android")]

use plico::prelude::*;
use plico_runtime::{HostAction, HostView, Source};

#[derive(Default)]
struct Counter {
    action: Option<HostAction>,
    count: u32,
    level: f64,
    note: plico::TextBuffer,
    increment_at: Option<std::time::Duration>,
}

impl HostView for Counter {
    fn take_host_action(&mut self) -> Option<HostAction> {
        self.action.take()
    }
    fn theme(&self) -> Theme {
        Theme::light()
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        column(cx).padding(24).gap(12) + |cx| {
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
            if let Some(deadline) = self.increment_at {
                if cx.time().now >= deadline {
                    self.increment_at = None;
                    self.count += 1;
                    cx.request_rebuild();
                } else {
                    cx.request_rebuild_at(deadline);
                }
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
    plico_android::run(app, Source::new(Counter::default())).expect("run Plico counter");
}
