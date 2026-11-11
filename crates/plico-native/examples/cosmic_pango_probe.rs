//! Compare Cosmic Text/Swash with Pango/Cairo under the same fonts and fixtures.
#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use cosmic_text::{
        Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Wrap,
    };
    use image::{Rgba, RgbaImage};
    use pango::prelude::*;
    use plico_native::text::{SystemText, TextRequest};
    use std::{hint::black_box, path::Path, time::Instant};

    const WIDTH: u32 = 520;
    const ROW_HEIGHT: u32 = 76;
    const RUNS: usize = 300;
    const FAMILY: &str = "Noto Sans, Noto Sans CJK SC, sans-serif";
    const CASES: &[(&str, &str)] = &[
        ("Latin", "Settings · ffi office — AVATAR 0123456789"),
        ("CJK", "中文排版与字体回退：设置、密码和安全密钥"),
        ("Arabic", "مرحبا بالعالم — إعدادات الأمان 123"),
        ("Devanagari", "नमस्ते दुनिया — सुरक्षा सेटिंग्स"),
        ("BiDi", "Account حساب 123 · password كلمة المرور 456"),
        ("Emoji", "Family 👨‍👩‍👧‍👦 · technologist 👩🏽‍💻 · key 🔐"),
        ("Combining", "Cafe\u{301} · A\u{30a}ngstro\u{308}m · Z͑͗͛algo"),
        (
            "Wrap",
            "A long line mixes English, 中文与 العربية so word and character fallback wrapping can be compared.",
        ),
    ];

    fn blend(dst: &mut Rgba<u8>, src: Color) {
        let [sr, sg, sb, sa] = src.as_rgba();
        let alpha = u16::from(sa);
        for (out, input) in dst.0[..3].iter_mut().zip([sr, sg, sb]) {
            *out = ((u16::from(input) * alpha + u16::from(*out) * (255 - alpha) + 127) / 255) as u8;
        }
        dst.0[3] = 255;
    }

    fn cosmic_layout(
        fonts: &mut FontSystem,
        cache: &mut SwashCache,
        text: &str,
        size: f32,
        line_height: f32,
    ) -> (RgbaImage, [f32; 2], usize, usize) {
        let mut buffer = Buffer::new(fonts, Metrics::new(size, line_height));
        buffer.set_wrap(Wrap::WordOrGlyph);
        buffer.set_size(Some(WIDTH as f32), Some(ROW_HEIGHT as f32));
        let attrs = Attrs::new().family(Family::Name("Noto Sans"));
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(fonts, false);
        let mut width = 0_f32;
        let mut height = 0_f32;
        let mut lines = 0;
        let mut glyphs = 0;
        for run in buffer.layout_runs() {
            width = width.max(run.line_w);
            height = height.max(run.line_top + run.line_height);
            lines += 1;
            glyphs += run.glyphs.len();
        }
        let mut image = RgbaImage::from_pixel(WIDTH, ROW_HEIGHT, Rgba([255, 255, 255, 255]));
        buffer.draw(fonts, cache, Color::rgb(28, 31, 36), |x, y, w, h, color| {
            for py in y.max(0)..(y + h as i32).min(ROW_HEIGHT as i32) {
                for px in x.max(0)..(x + w as i32).min(WIDTH as i32) {
                    blend(image.get_pixel_mut(px as u32, py as u32), color);
                }
            }
        });
        (image, [width, height], lines, glyphs)
    }

    fn paste(target: &mut RgbaImage, source: &RgbaImage, x: u32, y: u32) {
        for (sx, sy, pixel) in source.enumerate_pixels() {
            if x + sx < target.width() && y + sy < target.height() {
                let target = target.get_pixel_mut(x + sx, y + sy);
                let alpha = u16::from(pixel[3]);
                for channel in 0..3 {
                    target[channel] = ((u16::from(pixel[channel]) * alpha
                        + u16::from(target[channel]) * (255 - alpha)
                        + 127)
                        / 255) as u8;
                }
                target[3] = 255;
            }
        }
    }

    let output = Path::new("reports/cosmic-pango");
    std::fs::create_dir_all(output)?;
    let size = std::env::var("COSMIC_PROBE_SIZE")
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value >= 8. && *value <= 72.)
        .unwrap_or(14.);
    let line_height = size * 1.5;
    let mut fonts = FontSystem::new();
    let mut swash = SwashCache::new();
    let mut pango = SystemText::new(32 * 1024 * 1024)?;
    let mut page = RgbaImage::from_pixel(
        WIDTH * 2 + 40,
        ROW_HEIGHT * CASES.len() as u32,
        Rgba([244, 245, 247, 255]),
    );

    println!(
        "case\tcosmic_px\tpango_px\tdelta_px\tcosmic_lines\tpango_lines\tcosmic_glyphs\tpango_missing"
    );
    for (row, (name, text)) in CASES.iter().enumerate() {
        let (cosmic, measured, lines, glyphs) =
            cosmic_layout(&mut fonts, &mut swash, text, size, line_height);
        let mut request = TextRequest::new(text);
        request.family = FAMILY;
        request.size = f64::from(size);
        request.width = Some(f64::from(WIDTH));
        let native = pango.rasterize(&request)?;
        let pango_image =
            RgbaImage::from_raw(native.pixels[0], native.pixels[1], native.rgba.clone())
                .ok_or("invalid Pango bitmap")?;
        let y = row as u32 * ROW_HEIGHT;
        paste(&mut page, &cosmic, 0, y);
        paste(&mut page, &pango_image, WIDTH + 40, y);
        println!(
            "{}\t{:.2}x{:.2}\t{:.2}x{:.2}\t{:+.2}x{:+.2}\t{}\t{}\t{}\t{}",
            name,
            measured[0],
            measured[1],
            native.logical_size[0],
            native.logical_size[1],
            measured[0] - native.logical_size[0] as f32,
            measured[1] - native.logical_size[1] as f32,
            lines,
            native.lines,
            glyphs,
            native.missing_glyphs,
        );
    }
    let image_path = output.join(format!("cosmic-left-pango-right-{size}px.png"));
    page.save(&image_path)?;

    // Layout-only throughput. Use unique strings so neither engine can answer from a complete
    // paragraph cache; font discovery and process initialization are intentionally excluded.
    let pango_map = pangocairo::FontMap::new();
    let pango_context = pango_map.create_context();
    let mut description = pango::FontDescription::new();
    description.set_family(FAMILY);
    description.set_absolute_size(f64::from(size) * f64::from(pango::SCALE));
    let start = Instant::now();
    let mut pango_glyphs = 0_i32;
    for index in 0..RUNS {
        let layout = pango::Layout::new(&pango_context);
        layout.set_font_description(Some(&description));
        layout.set_width(WIDTH as i32 * pango::SCALE);
        layout.set_wrap(pango::WrapMode::WordChar);
        layout.set_text(&format!("{} #{index}", CASES[index % CASES.len()].1));
        pango_glyphs += layout.character_count();
        black_box(layout.pixel_size());
    }
    let pango_time = start.elapsed();

    let start = Instant::now();
    let mut cosmic_glyphs = 0_usize;
    for index in 0..RUNS {
        let mut buffer = Buffer::new(&mut fonts, Metrics::new(size, line_height));
        buffer.set_wrap(Wrap::WordOrGlyph);
        buffer.set_size(Some(WIDTH as f32), None);
        buffer.set_text(
            &format!("{} #{index}", CASES[index % CASES.len()].1),
            &Attrs::new().family(Family::Name("Noto Sans")),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fonts, false);
        cosmic_glyphs += buffer
            .layout_runs()
            .map(|run| run.glyphs.len())
            .sum::<usize>();
        black_box(cosmic_glyphs);
    }
    let cosmic_time = start.elapsed();
    println!(
        "layout {RUNS} unique paragraphs: cosmic={cosmic_time:?} ({cosmic_glyphs} glyphs), pango={pango_time:?} ({pango_glyphs} chars)"
    );
    println!("image={}", image_path.display());
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("cosmic_pango_probe currently supports Linux only");
}
