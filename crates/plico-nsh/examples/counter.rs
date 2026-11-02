//! Minimal native NSH proof, using only ordinary Plico application code.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use plico::{Theme, View, prelude::*};
use plico_nsh::native::{platform, root_with_viewport};

struct Counter {
    count: u32,
    extra: bool,
    disabled: bool,
    rows: Vec<u64>,
}

impl Default for Counter {
    fn default() -> Self {
        Self {
            count: 0,
            extra: false,
            disabled: false,
            rows: vec![1, 2],
        }
    }
}

impl View for Counter {
    fn theme(&self) -> Theme {
        Theme::default()
    }

    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        column(cx).padding(24).gap(14).width_percent(100.) + |cx| {
            text(cx, "Plico / NSH").font_size(24);
            text!(cx, "Count: {}", self.count);
            row(cx).gap(12) + |cx| {
                if button(cx).text("Increment").disabled(self.disabled).primary().clicked() {
                    self.count += 1;
                }
                if button(cx).text("Toggle extra").clicked() {
                    self.extra = !self.extra;
                }
            };
            if self.extra && button(cx).text("Extra action").clicked() {
                self.count += 10;
            }
            row(cx).gap(12) + |cx| {
                if button(cx).text("Reverse items").clicked() {
                    self.rows.reverse();
                }
                if button(cx).text(if self.disabled { "Enable increment" } else { "Disable increment" }).clicked() {
                    self.disabled = !self.disabled;
                }
            };
            row(cx).gap(12) + |cx| {
                for id in &self.rows {
                    text!(cx, "Item {id}").key(*id);
                }
            };
        };
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() -> impl std::process::Termination {
    platform::run_app(
        platform::AppSource::Rust(root_with_viewport(Counter::default, 480., 360.)),
        platform::AppOptions {
            app_id: "dev.plico.nsh-counter".to_owned(),
            title: "Plico / NSH counter".to_owned(),
            size: (480, 360),
            ..Default::default()
        },
    )
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
compile_error!("The NSH counter example requires Linux, macOS or Windows");
