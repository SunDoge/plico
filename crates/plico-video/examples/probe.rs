use plico_video::Player;
use std::{
    thread,
    time::{Duration, Instant},
};
fn wait(player: &Player, condition: impl Fn(&plico_video::Snapshot) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = player.snapshot();
        assert!(state.error.is_none(), "{:?}", state.error);
        if condition(&state) {
            return;
        }
        assert!(Instant::now() < deadline, "timeout: {state:?}");
        thread::sleep(Duration::from_millis(20));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: probe <video-longer-than-3s>")?;
    let player = Player::open(path, 1280, 720, true)?;
    wait(&player, |s| s.frames >= 12 && s.position > 0.3);
    let mut surface = player.surface();
    let mut text = plico_renderer::TextContext::new();
    let paint = |surface: &mut plico_video::VideoSurface,
                 text: &mut plico_renderer::TextContext| {
        use plico_renderer::Widget;
        let mut cx = plico_renderer::PaintContext::new(640., 360., 1., text);
        surface.paint(&mut cx);
        cx.finish()
    };
    let first = paint(&mut surface, &mut text);
    assert!(!first.commands.is_empty());
    player.pause(true)?;
    wait(&player, |s| s.paused);
    let paused = player.snapshot();
    thread::sleep(Duration::from_millis(250));
    assert!((player.snapshot().position - paused.position).abs() < 0.05);
    player.seek(2.)?;
    wait(&player, |s| {
        (s.position - 2.).abs() < 0.1 && s.frames > paused.frames
    });
    player.pause(false)?;
    wait(&player, |s| s.position > 2.4);
    assert_ne!(
        first.commands,
        paint(&mut surface, &mut text).commands,
        "video pixels did not change"
    );
    let state = player.snapshot();
    println!(
        "PASS: real software frames, pause, seek, resume; frames={} mean_render_wall_us={:.1}",
        state.frames,
        state.render_micros as f64 / state.frames as f64
    );
    Ok(())
}
