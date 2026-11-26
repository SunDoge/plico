use plico::prelude::*;
use plico_desktop::{EffectPayload, EffectQueue, EffectRequest};
use plico_desktop::{HostView, SurfaceSource, WindowOptions};
use plico_video::Player;
struct Demo {
    player: Player,
    paused: bool,
    status_key: (u64, bool, bool),
    started: Option<std::time::Duration>,
    close: bool,
    effects: EffectQueue,
    limit: Option<f64>,
}
impl HostView for Demo {
    fn poll_updates(&mut self) -> bool {
        let s = self.player.snapshot();
        let key = ((s.position * 10.) as u64, s.paused, s.error.is_some());
        let changed = key != self.status_key;
        self.status_key = key;
        changed
    }
    fn take_effect(&mut self) -> Option<EffectRequest> {
        if std::mem::take(&mut self.close) {
            self.effects
                .push(EffectPayload::window(plico_desktop::WindowCommand::Close));
        }
        self.effects.pop()
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
    plico_desktop::run_source(
        SurfaceSource::new(
            Demo {
                player,
                paused: false,
                status_key: (0, false, false),
                started: None,
                close: false,
                effects: EffectQueue::default(),
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
            .min_inner_size(640, 420),
    )?;
    Ok(())
}
