#[cfg(not(target_arch = "wasm32"))]
pub mod icons;
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use plico::prelude::*;
    use plico_desktop::{SurfaceSource, View, WindowOptions};
    use plico_renderer::{Widget, WidgetNodeEvent};
    use plico_terminal::{TerminalWidget, event};
    use snafu::{ResultExt, Snafu};
    #[derive(usage_rs::Cli)]
    #[usage(bin = "terminal", about = "Plico native terminal", version)]
    struct Args {
        #[usage(long)]
        command: Option<String>,
        /// JSON array passed directly to the command; never interpreted by a shell.
        #[usage(long)]
        command_args: Option<String>,
        #[usage(long)]
        cwd: Option<String>,
    }
    #[derive(Debug, Snafu)]
    pub enum Error {
        #[snafu(display("invalid command argument JSON: {source}"))]
        Arguments { source: serde_json::Error },
        #[snafu(display("terminal window failed: {source}"))]
        Window { source: plico_desktop::DesktopError },
    }
    #[derive(Default)]
    struct Chrome {
        title: String,
        exited: bool,
        compact: bool,
    }
    impl View for Chrome {
        #[component]
        #[rustfmt::skip]
        fn show(&mut self, cx: &mut UiContext) {
            column(cx).width_percent(100.).height_percent(100.).gap(8).padding(12).background(Color::Literal(0x10141c))
            + |cx|
            {
                row(cx).height(if self.compact { 26. } else { 36. }).gap(12).align(Align::Center) + |cx| {
                    svg(cx, &crate::icons::TERMINAL).size(20., 20.).color(Color::Literal(0x8aaaff));
                    text(cx, "Plico Terminal").font_size(16).weight(650).color(Color::Literal(0xe6eaf2));
                    text(cx, &self.title).grow(1.).color(Color::Literal(0x98a2b3));
                    if button(cx).text("Compact chrome").clicked() {
                        self.compact = !self.compact;
                    }
                    if self.exited {
                        text(cx, "Process exited").color(Color::Literal(0xf6c177));
                    }
                };
                // The host owns the PTY and native widget. This is its layout slot.
                column(cx).key("terminal").width_percent(100.).grow(1.).clip(true).radius(8.)
                    .background(Color::Literal(0x171b24)) + |_cx|
                {};
            };
        }
    }
    fn terminal_event(view: &mut Chrome, event: WidgetNodeEvent) -> bool {
        match event.event_code {
            event::TERMINALTITLECHANGE => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&event.json) {
                    view.title = value["title"].as_str().unwrap_or("Shell").to_owned();
                }
                true
            }
            event::TERMINALEXIT => {
                view.exited = true;
                true
            }
            _ => false,
        }
    }
    pub fn run() -> Result<(), Error> {
        let args = Args::parse();
        let mut terminal = TerminalWidget::lazy_default_shell();
        if let Some(command) = args.command {
            terminal.attribute_changed("command", &command);
        }
        if let Some(arguments) = args.command_args {
            let _: Vec<String> = serde_json::from_str(&arguments).context(ArgumentsSnafu)?;
            terminal.attribute_changed("args", &arguments);
        }
        if let Some(cwd) = args.cwd {
            terminal.attribute_changed("cwd", &cwd);
        }
        let source = SurfaceSource::new(
            Chrome {
                title: "Shell".into(),
                ..Default::default()
            },
            terminal,
            "terminal",
        )
        .with_events(terminal_event);
        plico_desktop::run_source_with_options(
            source,
            WindowOptions::new()
                .title("Plico Terminal")
                .initial_inner_size(1000, 680)
                .min_inner_size(640, 400),
        )
        .context(WindowSnafu)
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), native::Error> {
    native::run()
}
#[cfg(target_arch = "wasm32")]
fn main() {}
