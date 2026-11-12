//! End-to-end GPU-render probe: libmpv handles rendering, then the ordinary
//! Plico scene compositor samples the staged wgpu texture.

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use plico_renderer::{GpuContext, PaintContext, Widget};
    use plico_video::{GpuPlayer, PlayerOptions};
    use plico_wgpu::Renderer;
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    let input = std::env::args()
        .nth(1)
        .ok_or("usage: gpu_probe <file-or-url>")?;
    let width = std::env::var("PLICO_VIDEO_PROBE_WIDTH")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(640);
    let height = std::env::var("PLICO_VIDEO_PROBE_HEIGHT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(360);
    let mut renderer = pollster::block_on(Renderer::new())?;
    let gpu = renderer.external_gpu_context();
    let adapter = gpu.adapter.clone();
    let (player, mut surface) = GpuPlayer::with_options(PlayerOptions {
        width,
        height,
        muted: true,
        ..Default::default()
    })?;
    surface.set_wake_callback(Arc::new(|| {}));
    surface.set_gpu_context(Some(GpuContext::new("wgpu", gpu.clone())));
    player.load(input, None)?;

    let started = Instant::now();
    let deadline = started + Duration::from_secs(8);
    let target_frames = std::env::var("PLICO_VIDEO_PROBE_FRAMES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3);
    let mut pixels = Vec::new();
    while Instant::now() < deadline {
        surface.poll_async_changes();
        if player.snapshot().frames >= target_frames {
            let mut text = plico_renderer::TextContext::new();
            let mut paint = PaintContext::new(width as f32, height as f32, 1., &mut text);
            surface.paint(&mut paint);
            let scene = paint.finish();
            pixels = renderer.render_scene(&scene, [width, height], [0., 0., 0., 1.], false)?;
            break;
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    let snapshot = player.snapshot();
    let lit = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
        .count();
    if snapshot.frames < target_frames || lit < width as usize * height as usize / 100 {
        return Err(format!(
            "GPU video produced {} frames and {lit} lit pixels at {:.2}/{:.2}s: {:?}",
            snapshot.frames, snapshot.position, snapshot.duration, snapshot.error
        )
        .into());
    }
    println!(
        "PASS: {} libmpv GPU-rendered frames composed through Plico on {} ({:?}); {lit} lit pixels",
        snapshot.frames, adapter.name, adapter.backend
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("gpu_probe currently implements the Linux EGL/GLES path only".into())
}
