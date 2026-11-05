#[cfg(all(target_os = "linux", feature = "system-text"))]
mod support {
    pub mod text_page;
}
#[cfg(all(target_os = "linux", feature = "system-text"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = pollster::block_on(plico_wgpu::Renderer::new())?;
    println!("{:?}", renderer.adapter_info());
    let output = std::path::Path::new("reports/wgpu-text");
    std::fs::create_dir_all(output)?;
    for scale in [1., 1.25, 1.5, 2.] {
        for dark in [false, true] {
            let page = support::text_page::Page::new(&mut renderer, scale, dark)?;
            let size = [(840. * scale) as u32, (620. * scale) as u32];
            let image = renderer.render_items(
                &page.draws(&page.quads, 0., false),
                size,
                page.background,
                0.,
            )?;
            image::save_buffer(
                output.join(format!(
                    "settings-{scale}x-{}.png",
                    if dark { "dark" } else { "light" }
                )),
                &image,
                size[0],
                size[1],
                image::ColorType::Rgba8,
            )?;
        }
    }
    let page = support::text_page::Page::new(&mut renderer, 1., false)?;
    for frame in 0..16 {
        let _ = renderer.render_items(
            &page.draws(&page.quads, (frame % 4) as f64 / 4., false),
            [840, 620],
            page.background,
            0.,
        )?;
    }
    let before = renderer.text_stats();
    for frame in 0..120 {
        let _ = renderer.render_items(
            &page.draws(&page.quads, (frame % 4) as f64 / 4., false),
            [840, 620],
            page.background,
            0.,
        )?;
    }
    let after = renderer.text_stats();
    assert_eq!(before.cached_glyphs, after.cached_glyphs);
    assert_eq!(before.uploaded_bytes, after.uploaded_bytes);
    println!("120 warmed frames: no glyph growth or texture upload; {after:?}");
    Ok(())
}
#[cfg(not(all(target_os = "linux", feature = "system-text")))]
fn main() {
    eprintln!("The system-text quality example currently requires Linux and system-text.");
}
