use plico::prelude::*;
use plico_desktop::{SurfaceSource, View, WindowOptions};
use plico_video::Player;
struct Demo {
    player: Player,
    paused: bool,
    status_key: (u64, bool, bool),
    started: Option<std::time::Duration>,
    close: bool,
    limit: Option<f64>,
}
impl View for Demo {
    fn poll_reload(&mut self) -> bool {
        let s = self.player.snapshot();
        let key = ((s.position * 10.) as u64, s.paused, s.error.is_some());
        let changed = key != self.status_key;
        self.status_key = key;
        changed
    }
    fn take_host_action(&mut self) -> Option<plico_desktop::HostAction> {
        std::mem::take(&mut self.close).then_some(plico_desktop::HostAction::Window(
            plico_desktop::WindowCommand::Close,
        ))
    }
    #[component]
    #[rustfmt::skip]
    fn show(&mut self, cx: &mut UiContext) {
        let status = self.player.snapshot();
        let now = cx.now();
        let elapsed = now.saturating_sub(*self.started.get_or_insert(now));
        self.close = self.limit.is_some_and(|limit| elapsed.as_secs_f64() >= limit);
        column(cx).width_percent(100.).height_percent(100.).padding(20).gap(12) + |cx| {
            text(cx, "Plico × libmpv · software output").font_size(22);
            column(cx).key("video").width_percent(100.).height(0.).grow(1.).min_height(180.)
                .background(Color::Literal(0x101014)).radius(16.).clip(true) + |_cx|
            {};
            row(cx).gap(12) + |cx| {
                if button(cx).text(if self.paused { "Play" } else { "Pause" }).primary().clicked() {
                    self.paused = !self.paused;
                    if let Err(e) = self.player.pause(self.paused) {
                        eprintln!("{e}");
                    }
                }
                if button(cx).text("−10 s").clicked()
                    && let Err(e) = self.player.seek(status.position - 10.)
                {
                    eprintln!("{e}");
                }
                if button(cx).text("+10 s").clicked()
                    && let Err(e) = self.player.seek(status.position + 10.)
                {
                    eprintln!("{e}");
                }
                text!(
                    cx,
                    "{:.1} / {:.1} s · {} frames",
                    status.position,
                    status.duration,
                    status.frames
                );
            };
            if let Some(error) = &status.error {
                text(cx, error);
            }
        };
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // One positional input only; this experiment has no custom option parser.
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: player <file-or-url>")?;
    let player = Player::open(path, 1280, 720, false)?;
    let surface = player.surface();
    let renderer = if cfg!(feature = "skia") {
        plico_desktop::RenderBackend::Skia
    } else {
        plico_desktop::RenderBackend::Vello
    };
    plico_desktop::run_source_with_options(
        SurfaceSource::new(
            Demo {
                player,
                paused: false,
                status_key: (0, false, false),
                started: None,
                close: false,
                limit: std::env::var("PLICO_VIDEO_PROBE_SECONDS")
                    .ok()
                    .and_then(|s| s.parse().ok()),
            },
            surface,
            "video",
        ),
        WindowOptions::new()
            .title("Plico video experiment")
            .initial_inner_size(1000, 700)
            .min_inner_size(640, 420)
            .renderer(renderer),
    )?;
    Ok(())
}
