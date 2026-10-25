//! Shared Plico UI, layout, input and Winit shell; only the graphics consumer changes.
use plico_core::prelude::*;
use plico_vello::{RenderBackend, Source, View, WindowOptions};

#[derive(Default)]
struct Counter {
    count: u32,
    close: bool,
}
impl View for Counter {
    fn on_unhandled_key(&mut self, key: &plico_vello::KeyEvent) -> bool {
        if key.key == "Escape" {
            self.close = true;
            true
        } else {
            false
        }
    }
    fn take_host_action(&mut self) -> Option<plico_vello::HostAction> {
        std::mem::take(&mut self.close).then_some(plico_vello::HostAction::Window(
            plico_vello::WindowCommand::Close,
        ))
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext<'_>) {
        column(cx).padding(28).gap(18).width_percent(100.).height_percent(100.).background(Color::Literal(0xf7f8fa))
        + |cx|
        {
            text(cx, "Plico · Skia Vulkan window").font_size(26);
            text(cx, "Same widgets, Taffy layout, Parley text and native input.").font_size(16);
            text!(cx, "Count: {}", self.count).font_size(24);
            row(cx).gap(12) + |cx| {
                if button(cx).text("Increment").clicked() {
                    self.count += 1;
                    println!("count={}", self.count);
                }
                if button(cx).text("Reset").clicked() {
                    self.count = 0;
                    println!("count=0");
                }
            };
            text(cx, "Resize the window, click the buttons, or use Tab and Enter.").font_size(16);
            text(cx, "中文窗口：文字、布局和交互复用现有框架。").font_size(18);
        };
    }
}
fn main() -> Result<(), plico_vello::DesktopError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let renderer = if std::env::var_os("PLICO_COMPARE_VELLO").is_some() {
        RenderBackend::Vello
    } else {
        RenderBackend::Skia
    };
    plico_vello::run_source_with_options(
        Source::new(Counter::default()),
        WindowOptions::new()
            .title(format!("Plico · {renderer:?} window"))
            .initial_inner_size(820, 480)
            .renderer(renderer),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_vello::testing::{Command, Driver};

    #[test]
    fn counter_keyboard_flow_uses_explicit_semantic_focus() {
        let mut ui = Driver::new(Counter::default(), 820, 480).unwrap();
        ui.execute(Command::Click {
            role: "button".into(),
            label: "Increment".into(),
            occurrence: None,
        })
        .unwrap();
        assert_eq!(ui.source.view.count, 1);
        ui.resize(1000, 640).unwrap();
        ui.execute(Command::Focus {
            role: "button".into(),
            label: "Increment".into(),
            occurrence: None,
        })
        .unwrap();
        ui.execute(Command::Key {
            key: "Enter".into(),
        })
        .unwrap();
        assert_eq!(ui.source.view.count, 2);
        ui.execute(Command::Key { key: "Tab".into() }).unwrap();
        ui.execute(Command::Key {
            key: "Enter".into(),
        })
        .unwrap();
        assert_eq!(ui.source.view.count, 0);
    }
}
