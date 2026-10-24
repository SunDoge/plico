//! Small-text rendering probe: renders the same sentences at 12/13/14/16px
//! through the plico pipeline, and prints ink statistics per line for
//! comparison against a reference browser rendering of the same page.
//! `PLICO_TEXT_STEM_BOOST` (milli-em, default 0) controls the
//! geometric weight tuning described in text.rs.
use plico_renderer::{
    PaintScene, Scene, TextContext,
    style::TextAlign,
    text::{TextRun, layout_text_styled},
};
use std::sync::Arc;

const TEXT: &str = "The quick brown fox jumps over the lazy dog 0123456789";

fn main() {
    let font = std::fs::read("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf").unwrap();
    let mut tcx = TextContext::new();
    tcx.load_font(font);
    let sizes = [12., 13., 14., 16.];
    let text: Arc<str> = Arc::from(TEXT);
    let runs: Arc<[TextRun]> = Vec::new().into();
    let width = 480u32;
    let height = 140u32;
    let bg = vello_common::peniko::Color::from_rgba8(0xf4, 0xf4, 0xf5, 255);
    let mut scene = Scene::new();
    scene.fill(
        vello_common::peniko::Fill::NonZero,
        vello_common::kurbo::Affine::IDENTITY,
        bg,
        None,
        &vello_common::kurbo::Rect::new(0., 0., width as f64, height as f64),
    );
    let mut y = 0.0;
    for size in sizes.iter() {
        let layout = layout_text_styled(
            &mut tcx,
            text.clone(),
            *size,
            400.,
            false,
            None,
            TextAlign::Start,
            [0x20, 0x20, 0x24, 255],
            runs.clone(),
            None,
            None,
        );
        y += *size as f64 * 1.35;
        let transform = vello_common::kurbo::Affine::translate((0., y));
        plico_renderer::scene::draw_text_layout_into(
            &mut scene,
            &mut tcx,
            &layout,
            vello_common::kurbo::Affine::IDENTITY,
            transform,
            1.0,
        );
    }
    plico_shell::renderer::render_to_png(&scene, width, height, bg, "/tmp/plico-text.png")
        .expect("offscreen render");

    // Ink statistics per line, measured with the image crate.
    let img = image::open("/tmp/plico-text.png").unwrap().to_rgba8();
    // Detect text line bands: rows containing any ink pixel.
    let mut bands: Vec<(u32, u32)> = Vec::new();
    let mut in_band = false;
    let mut start = 0u32;
    for y in 0..height {
        let has_ink = (0..width).any(|x| img.get_pixel(x, y).0[0] < 0xf2);
        if has_ink && !in_band {
            in_band = true;
            start = y;
        } else if !has_ink && in_band {
            in_band = false;
            bands.push((start, y - 1));
        }
    }
    if in_band {
        bands.push((start, height - 1));
    }
    println!("text line bands: {bands:?}");
    // R-value histogram over the first band, to sanity-check measurement.
    if let Some((b0, b1)) = bands.first().copied() {
        let mut hist = std::collections::BTreeMap::new();
        for y in b0..=b1 {
            for x in 0..width {
                let r = img.get_pixel(x, y).0[0];
                *hist.entry(r).or_insert(0u32) += 1;
            }
        }
        let total: u32 = hist.values().sum();
        let non_bg: u32 = hist
            .iter()
            .filter(|(r, _)| **r < 244)
            .map(|(_, c)| *c)
            .sum();
        println!(
            "band1 rows {b0}-{b1}: total px {total}, non-bg {non_bg}, darkest {}",
            hist.keys().next_back().copied().unwrap_or(0)
        );
        let light: Vec<_> = hist.iter().filter(|(r, _)| **r < 244).take(10).collect();
        println!("  lightest non-bg R values: {light:?}");
    }
    println!("wrote /tmp/plico-text.png ({}x{})", width, height);

    for (i, size) in sizes.iter().enumerate() {
        let Some(&(y0, y1)) = bands.get(i) else {
            continue;
        };
        let mut covs = Vec::new();
        for y in y0..=y1 {
            for x in 0..width {
                let p = img.get_pixel(x, y).0;
                if p[0] < 244 {
                    let c = (244usize - p[0] as usize) as f32 / (244 - 0x20) as f32;
                    covs.push(c);
                }
            }
        }
        let ink_px = covs.len();
        let avg = covs.iter().sum::<f32>() / ink_px.max(1) as f32;
        let strong = covs.iter().filter(|c| **c >= 0.8).count();
        println!(
            "  {:>4.0}px (rows {y0}-{y1}): ink={ink_px:5} avg_coverage={avg:.3} strong%={}",
            size,
            100 * strong / ink_px.max(1)
        );
    }
}
