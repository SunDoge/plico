use plico_video::{Player, PlayerOptions};
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
    let audio = std::env::args().nth(2);
    let player = Player::with_options(PlayerOptions {
        muted: true,
        ..Default::default()
    })?;
    assert_eq!(
        player.snapshot().frames,
        0,
        "idle player must not generate frames"
    );
    player.load(path.clone(), audio.clone())?;
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
    if audio.is_some() {
        wait(&player, |s| s.audio_channels > 0);
    }
    player.volume(0.42)?;
    wait(&player, |s| (s.volume - 0.42).abs() < 0.01);
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
    let previous = player.snapshot().frames;
    player.load(path, audio)?;
    wait(&player, |s| {
        s.frames > previous + 6 && s.position > 0.1 && s.position < 2.
    });
    assert!(
        !paint(&mut surface, &mut text).commands.is_empty(),
        "same surface must survive replacing media"
    );
    let state = player.snapshot();
    println!(
        "PASS: idle/load, software frames, volume, pause, seek, resume, replace; frames={} mean_render_wall_us={:.1}",
        state.frames,
        state.render_micros as f64 / state.frames as f64
    );
    Ok(())
}
