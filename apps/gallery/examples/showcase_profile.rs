//! Measure the live Showcase's automatic animations on a virtual clock.
use gallery::{Gallery, Page};
use plico_desktop::{
    Source,
    testing::{Command, dispatch},
};
use plico_renderer::{FrameSource, Scene, TextContext};
use std::time::{Duration, Instant};

#[derive(usage_rs::Cli)]
struct Options {
    #[usage(long, default = "120")]
    fps: u32,
    #[usage(long, default = "1920")]
    width: u32,
    #[usage(long, default = "1080")]
    height: u32,
    #[usage(long, default = "1")]
    scale: f64,
    #[usage(long, default = "8")]
    seconds: u32,
}

fn percentiles(mut samples: Vec<f64>) -> [f64; 3] {
    samples.sort_unstable_by(f64::total_cmp);
    [50, 95, 99].map(|p| samples[(samples.len() * p).div_ceil(100) - 1])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _trace = (std::env::var_os("PLICO_TRACE_FILE").is_some()
        || std::env::var_os("RUST_LOG").is_some())
    .then(plico_shell::telemetry::init_tracing);
    let options = Options::parse();
    if !(1..=1000).contains(&options.fps)
        || !(1..=120).contains(&options.seconds)
        || !(1..=8192).contains(&options.width)
        || !(1..=8192).contains(&options.height)
        || !options.scale.is_finite()
        || !(0.5..=4.).contains(&options.scale)
    {
        return Err("invalid profile dimensions, scale, sample duration or frame rate".into());
    }
    let size =
        [options.width, options.height].map(|d| (f64::from(d) * options.scale).round() as u32);
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new())?;
    let gpu = renderer.external_gpu_context();
    let limit = gpu.device.limits().max_texture_dimension_2d;
    if size.iter().any(|&dimension| dimension > limit) {
        return Err(format!("profile target exceeds GPU texture limit of {limit}").into());
    }
    let format = wgpu::TextureFormat::Bgra8Unorm;
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Showcase profile"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let destination = target.create_view(&Default::default());
    let clock = plico::VirtualClock::default();
    let mut source = Source::with_clock(
        Gallery {
            router: plico::Router::new(Page::LiquidGlassShowcase),
            ..Default::default()
        },
        clock.clone(),
    );
    let viewport = (options.width, options.height);
    let mut text = TextContext::new();
    renderer.configure_text(&mut text, options.scale)?;
    source.prepare_layout(&mut text, viewport)?;
    dispatch(
        &mut source,
        &mut text,
        &Command::PointerClick {
            role: "checkbox".into(),
            label: "Autoplay animations".into(),
            occurrence: None,
        },
    )?;
    let mut scene = Scene::new();
    let mut cpu = Vec::new();
    let mut completed = Vec::new();
    let mut device = Vec::new();
    let mut skipped = 0;
    let mut backdrop_copies = 0;
    let mut backdrop_prefilter_passes = 0;
    let warmup = options.fps;
    let frames = options.fps * options.seconds;
    let deadline_ms = 1000. / f64::from(options.fps);
    for frame in 0..warmup + frames {
        clock.set(Duration::from_nanos(
            u64::from(frame) * 1_000_000_000 / u64::from(options.fps),
        ))?;
        let started = Instant::now();
        source.render_frame(&mut scene, &mut text, viewport, options.scale)?;
        let rendered = renderer.render_scene_to_view(
            &scene,
            size,
            source.base_color().components,
            true,
            &destination,
            format,
        )?;
        let submit_ms = started.elapsed().as_secs_f64() * 1_000.;
        // A completed frame is a stricter sample than queue submission. This
        // wait enforces serial work and never maps or copies image pixels.
        gpu.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })?;
        if frame >= warmup {
            if !rendered {
                skipped += 1;
                continue;
            }
            cpu.push(submit_ms);
            completed.push(started.elapsed().as_secs_f64() * 1_000.);
            let budget = renderer.frame_budget();
            backdrop_copies += budget.backdrop_copies;
            backdrop_prefilter_passes += budget.backdrop_mip_passes;
            if let Some(ms) = budget.gpu_ms {
                device.push(ms);
            }
        }
    }
    if completed.is_empty() {
        return Err("autoplay submitted no frames".into());
    }
    println!(
        "{} ({:?}), {}×{} physical",
        gpu.adapter.name, gpu.adapter.backend, size[0], size[1]
    );
    println!(
        "{} Hz budget {deadline_ms:.3} ms, {frames} samples, {skipped} unchanged",
        options.fps
    );
    let misses = completed.iter().filter(|&&ms| ms > deadline_ms).count();
    println!("deadline misses {misses}/{}", completed.len());
    println!("backdrop copies {backdrop_copies}, prefilter passes {backdrop_prefilter_passes}");
    println!("p50/p95/p99 CPU to submit (ms) {:.3?}", percentiles(cpu));
    println!(
        "p50/p95/p99 CPU + GPU completion (ms) {:.3?}",
        percentiles(completed)
    );
    if !device.is_empty() {
        println!(
            "p50/p95/p99 GPU timestamps (ms) {:.3?}",
            percentiles(device)
        );
    }
    Ok(())
}
