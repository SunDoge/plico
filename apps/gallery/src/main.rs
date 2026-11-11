#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(usage_rs::Cli)]
    struct Options {
        #[usage(long)]
        scenario: Option<std::path::PathBuf>,
        #[usage(long)]
        capture: Option<std::path::PathBuf>,
        /// Capture the scenario repeatedly and keep the last frame, so caches
        /// and pipelines are warm when `PLICO_FRAME_BUDGET` is printed.
        #[usage(long)]
        frames: Option<u32>,
        /// Draw every wgpu quad class through the general shader for A/B profiles.
        #[usage(long)]
        general_classes: bool,
    }
    let options = Options::parse();
    #[cfg(not(any(feature = "desktop", feature = "wgpu", feature = "skia")))]
    if options.capture.is_some() || options.scenario.is_some() {
        return Err("scenario capture requires the gallery desktop, wgpu or skia feature".into());
    }
    #[cfg(any(feature = "desktop", feature = "wgpu", feature = "skia"))]
    if let Some(output) = options.capture {
        let mut session = plico_desktop::PreviewSession::new(plico_desktop::RenderBackend::Auto);
        #[cfg(feature = "wgpu")]
        session.force_general_pipelines(options.general_classes);
        let mut captured = None;
        for _ in 0..options.frames.unwrap_or(1).max(1) {
            let scenario = if let Some(path) = &options.scenario {
                plico_desktop::PreviewScenario::<gallery::Gallery>::load(path)?
            } else {
                Default::default()
            };
            captured = Some(scenario.render_in(&mut session, plico_desktop::NativeApp)?);
        }
        captured
            .expect("at least one frame was captured")
            .write_png(output)?;
        if std::env::var_os("PLICO_FRAME_BUDGET").is_some()
            && let Some(budget) = session.last_frame_budget()
        {
            println!("adapter: {}", session.adapter_name().unwrap_or("unknown"));
            println!(
                "frame budget: prepare={:.2}ms upload={:.2}ms encode={:.2}ms submit={:.2}ms wait={:.2}ms gpu={} | {} submits, {} command buffers, {} instances, {} draws, {} KiB uploaded",
                budget.prepare_ms,
                budget.upload_ms,
                budget.encode_ms,
                budget.submit_ms,
                budget.wait_ms,
                budget
                    .gpu_ms
                    .map_or_else(|| "n/a".to_owned(), |ms| format!("{ms:.2}ms")),
                budget.submissions,
                budget.command_buffers,
                budget.instances,
                budget.draw_calls,
                budget.upload_bytes / 1024,
            );
            #[cfg(feature = "wgpu")]
            if let Some(stats) = session.last_batch_stats() {
                println!(
                    "class draws: plain={} fill={} shadow={} glyph={} color-glyph={} image={} general-or-joined={}",
                    stats.plain_draw_calls,
                    stats.fill_draw_calls,
                    stats.shadow_draw_calls,
                    stats.glyph_draw_calls,
                    stats.color_glyph_draw_calls,
                    stats.image_draw_calls,
                    stats.draw_calls.saturating_sub(
                        stats.plain_draw_calls
                            + stats.fill_draw_calls
                            + stats.shadow_draw_calls
                            + stats.glyph_draw_calls
                            + stats.color_glyph_draw_calls
                            + stats.image_draw_calls
                    ),
                );
            }
        }
        return Ok(());
    }
    plico_desktop::run_source_with_options(
        plico_desktop::ZoomSource::new(Box::new({
            let app = gallery::Gallery::default();
            let widget =
                plico_renderer::image_surface::ImageSurface::new(app.images.surface.clone());
            plico_desktop::SurfaceSource::new(
                gallery::images::GalleryHost { app },
                widget,
                "image-preview",
            )
        })),
        plico_desktop::WindowOptions::new()
            .title("Plico gallery")
            .initial_inner_size(1120, 820)
            .min_inner_size(640, 740),
    )?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn main() {}
