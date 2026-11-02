//! Offline vertical CJK typography experiment, not a public layout API.
use harfrust::{Direction, Feature, FontRef, ShapeOptions, ShaperData, UnicodeBuffer};
use kurbo::{Affine, Rect};
use peniko::{Blob, Color, Fill, FontData, Style};
use plico_renderer::{
    Glyph, PaintCommand, PaintScene, Scene, paragraph::ParagraphRequest, style::Paint,
};
use plico_skia::SkiaSurface;
use std::{path::PathBuf, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

#[derive(usage_rs::Cli)]
#[usage(about = "Compare vertical glyph substitution and mixed CJK text with Skia")]
struct Options {
    /// Optional font file; otherwise resolve Noto Sans CJK SC/JP from the system.
    #[usage(long)]
    font: Option<PathBuf>,
    #[usage(long, default = "0")]
    face: u32,
    #[usage(long, default = "reports/vertical-text")]
    output: PathBuf,
    /// Translated Chinese sample for the replacement bubble.
    #[usage(
        long,
        default = "「真的要现在出发吗？」外面还在下雨呢……不过，大家都已经准备好了。那就一起走吧！"
    )]
    text: String,
    /// Grapheme clusters per vertical column; no linguistic line breaking.
    #[usage(long, default = "10")]
    rows: usize,
}

#[derive(Debug, snafu::Snafu)]
enum Error {
    #[snafu(display("font not found or font bytes unavailable: {family}; use --font and --face"))]
    Font { family: String },
    #[snafu(display("font data or face index is invalid"))]
    FontData,
    #[snafu(display("the fixed demo canvas requires 1..=12 rows and at most 8 columns"))]
    Layout,
    #[snafu(display("font has missing glyphs for {text:?}"))]
    MissingGlyph { text: String },
    #[snafu(display(
        "no vertical alternates found for {language}; choose a CJK font with vert/vrt2"
    ))]
    NoAlternates { language: String },
    #[snafu(display("filesystem operation failed: {source}"))]
    Io { source: std::io::Error },
    #[snafu(display("Skia rendering failed: {source}"))]
    Render { source: plico_skia::Error },
}

struct Font {
    data: FontData,
    skia: skia_safe::Font,
}
impl Font {
    fn load(options: &Options, family: &str, size: f32) -> Result<Self, Error> {
        let manager = skia_safe::FontMgr::new();
        let face = if let Some(path) = &options.font {
            let bytes = std::fs::read(path).map_err(|source| Error::Io { source })?;
            manager.new_from_bytes(&bytes, options.face)
        } else {
            manager.match_family_style(family, skia_safe::FontStyle::normal())
        }
        .ok_or_else(|| Error::Font {
            family: family.into(),
        })?;
        let (bytes, index) = face.to_existing_font_data().ok_or_else(|| Error::Font {
            family: family.into(),
        })?;
        println!("family={} face={index}", face.family_name());
        Ok(Self {
            data: FontData::new(Blob::new(Arc::new(bytes.as_bytes().to_vec())), index),
            skia: skia_safe::Font::new(face, size),
        })
    }

    fn shape(
        &self,
        text: &str,
        language: &str,
        vertical: bool,
        alternates: bool,
        size: f32,
    ) -> Result<Run, Error> {
        let face = FontRef::from_index(self.data.data.as_ref(), self.data.index)
            .map_err(|_| Error::FontData)?;
        let data = ShaperData::new(&face);
        let shaper = data.shaper(&face).build();
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(if vertical {
            Direction::TopToBottom
        } else {
            Direction::LeftToRight
        });
        buffer.set_language(language.parse().expect("fixed language tag"));
        buffer.guess_segment_properties();
        let disabled = [
            "vert=0".parse::<Feature>().unwrap(),
            "vrt2=0".parse::<Feature>().unwrap(),
        ];
        let shaped = shaper.shape(
            buffer,
            ShapeOptions::new().features(if alternates { &[] } else { &disabled }),
        );
        let scale = size / shaper.units_per_em() as f32;
        let mut pen = [0., 0.];
        let mut glyphs = Vec::new();
        for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            if info.glyph_id == 0 {
                return Err(Error::MissingGlyph { text: text.into() });
            }
            glyphs.push(Glyph {
                id: info.glyph_id,
                x: (pen[0] + position.x_offset as f32) * scale,
                y: -(pen[1] + position.y_offset as f32) * scale,
            });
            pen[0] += position.x_advance as f32;
            pen[1] += position.y_advance as f32;
        }
        Ok(Run {
            glyphs,
            advance: if vertical {
                -pen[1] * scale
            } else {
                pen[0] * scale
            },
        })
    }

    fn ink(&self, run: &Run) -> Rect {
        let ids: Vec<_> = run.glyphs.iter().map(|g| g.id as u16).collect();
        let mut bounds = vec![skia_safe::Rect::default(); ids.len()];
        self.skia.get_bounds(&ids, &mut bounds, None);
        run.glyphs
            .iter()
            .zip(bounds)
            .filter(|(_, r)| !r.is_empty())
            .fold(None, |total: Option<Rect>, (g, r)| {
                let rect = Rect::new(
                    f64::from(g.x + r.left),
                    f64::from(g.y + r.top),
                    f64::from(g.x + r.right),
                    f64::from(g.y + r.bottom),
                );
                Some(total.map_or(rect, |old| old.union(rect)))
            })
            .unwrap_or(Rect::ZERO)
    }
}
struct Run {
    glyphs: Vec<Glyph>,
    advance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Upright,
    Sideways,
    CombinedDigits,
}
#[derive(Clone, Copy)]
enum Mode {
    NoAlternates,
    Vertical,
    Mixed,
}
fn digit_pair(text: &str) -> bool {
    text.len() == 2 && text.bytes().all(|c| c.is_ascii_digit())
}
fn orientation(text: &str) -> Kind {
    match unicode_vo::char_orientation(text.chars().next().unwrap()) {
        unicode_vo::Orientation::Rotated => Kind::Sideways,
        _ => Kind::Upright,
    }
}
fn segments(text: &str) -> Vec<(Kind, String)> {
    let mut output: Vec<(Kind, String)> = Vec::new();
    let mut clusters = text.graphemes(true).peekable();
    while let Some(cluster) = clusters.next() {
        if cluster.bytes().all(|c| c.is_ascii_digit()) {
            let mut number = cluster.to_owned();
            while clusters
                .peek()
                .is_some_and(|s| s.bytes().all(|c| c.is_ascii_digit()))
            {
                number.push_str(clusters.next().unwrap());
            }
            output.push((
                if digit_pair(&number) {
                    Kind::CombinedDigits
                } else {
                    Kind::Sideways
                },
                number,
            ));
            continue;
        }
        let kind = orientation(cluster);
        if let Some((_, text)) = output.last_mut().filter(|(previous, _)| *previous == kind) {
            text.push_str(cluster);
        } else {
            output.push((kind, cluster.to_owned()));
        }
    }
    output
}

fn paint_run(scene: &mut Scene, font: &Font, run: &Run, size: f32, transform: Affine) {
    scene.draw_glyphs(
        &font.data,
        size,
        false,
        &[],
        Default::default(),
        &Style::Fill(Fill::NonZero),
        Color::from_rgb8(30, 35, 45),
        1.,
        transform,
        None,
        run.glyphs.iter().copied(),
    );
}
fn column(
    scene: &mut Scene,
    font: &Font,
    text: &str,
    language: &str,
    mode: Mode,
    (center, top): (f64, f64),
    size: f32,
) -> Result<f64, Error> {
    let parts = if matches!(mode, Mode::Mixed) {
        segments(text)
    } else {
        vec![(Kind::Upright, text.to_owned())]
    };
    let mut y = top;
    for (kind, text) in parts {
        let vertical = kind == Kind::Upright;
        let run = font.shape(
            &text,
            language,
            vertical,
            !matches!(mode, Mode::NoAlternates),
            size,
        )?;
        let ink = font.ink(&run);
        let (transform, advance) = match kind {
            Kind::Upright => (Affine::translate((center, y)), f64::from(run.advance)),
            Kind::Sideways => (
                Affine::translate((center + ink.center().y, y))
                    * Affine::rotate(std::f64::consts::FRAC_PI_2),
                f64::from(run.advance),
            ),
            Kind::CombinedDigits => {
                let scale = (f64::from(size) / ink.width().max(1.))
                    .min(f64::from(size) / ink.height().max(1.))
                    .min(1.);
                (
                    Affine::translate((
                        center - ink.center().x * scale,
                        y + f64::from(size) * 0.5 - ink.center().y * scale,
                    )) * Affine::scale(scale),
                    f64::from(size),
                )
            }
        };
        paint_run(scene, font, &run, size, transform);
        y += advance;
    }
    Ok(y - top)
}
fn label(scene: &mut Scene, text: &str, x: f64, y: f64, size: u16) {
    scene.commands.push(PaintCommand::Paragraph {
        request: Arc::new(
            ParagraphRequest::from_paint(
                &Paint {
                    text: Some(text.into()),
                    font_size: f32::from(size),
                    ..Default::default()
                },
                None,
            )
            .unwrap(),
        ),
        transform: Affine::translate((x, y)),
    });
}

fn probe(options: &Options, language: &str, family: &str, lines: &[&str]) -> Result<(), Error> {
    const SIZE: f32 = 34.;
    let font = Font::load(options, family, SIZE)?;
    let mut substitutions = 0;
    for ch in ["、", "。", "「", "」", "（", "）", "ー", "…"] {
        let horizontal = font.shape(ch, language, false, true, SIZE)?;
        let vertical = font.shape(ch, language, true, true, SIZE)?;
        let h: Vec<_> = horizontal.glyphs.iter().map(|g| g.id).collect();
        let v: Vec<_> = vertical.glyphs.iter().map(|g| g.id).collect();
        substitutions += usize::from(h != v);
        println!(
            "language={language} text={ch:?} horizontal={h:?} vertical={v:?} advance={}",
            vertical.advance
        );
    }
    if substitutions == 0 {
        return Err(Error::NoAlternates {
            language: language.into(),
        });
    }
    let mut scene = Scene::new();
    label(
        &mut scene,
        &format!("{language} vertical typography / HarfRust shaping + Plico Skia glyph replay"),
        24.,
        18.,
        22,
    );
    for (panel, (mode, title)) in [
        (Mode::NoAlternates, "1. TTB, vert/vrt2 disabled"),
        (Mode::Vertical, "2. OpenType vertical forms"),
        (Mode::Mixed, "3. Mixed orientation + 2-digit TCY"),
    ]
    .into_iter()
    .enumerate()
    {
        let left = 20. + panel as f64 * 480.;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(247, 247, 244),
            None,
            &Rect::new(left, 65., left + 465., 765.),
        );
        label(&mut scene, title, left + 14., 80., 18);
        for (index, line) in lines.iter().enumerate() {
            let x = left + 410. - index as f64 * 88.;
            // Em guides make the digit-combination policy visible.
            for cell in 0..16 {
                scene.stroke(
                    &kurbo::Stroke::new(0.5),
                    Affine::IDENTITY,
                    Color::from_rgb8(225, 226, 222),
                    None,
                    &Rect::new(
                        x - 17.,
                        140. + cell as f64 * f64::from(SIZE),
                        x + 17.,
                        140. + (cell + 1) as f64 * f64::from(SIZE),
                    ),
                );
            }
            let height = column(&mut scene, &font, line, language, mode, (x, 140.), SIZE)?;
            if height > 590. {
                return Err(Error::Layout);
            }
        }
        label(
            &mut scene,
            "Columns: right to left; explicit line breaks",
            left + 14.,
            720.,
            15,
        );
    }
    let mut surface = SkiaSurface::new(1480, 790).map_err(|source| Error::Render { source })?;
    surface
        .draw_scene(&scene, Color::WHITE)
        .map_err(|source| Error::Render { source })?;
    let path = options.output.join(format!("{language}.png"));
    surface
        .write_png(&path)
        .map_err(|source| Error::Render { source })?;
    println!(
        "wrote {} (vertical alternates={substitutions})",
        path.display()
    );
    Ok(())
}
/// Intentionally simple: keep grapheme clusters intact, honor explicit newlines,
/// and wrap at a fixed count. No kinsoku, hyphenation, or auto font fitting.
fn wrap_columns(text: &str, rows: usize) -> Vec<String> {
    assert!(rows > 0);
    let mut output = Vec::new();
    for line in text.split('\n') {
        let mut column = String::new();
        for (index, cluster) in line.graphemes(true).enumerate() {
            if index > 0 && index.is_multiple_of(rows) {
                output.push(std::mem::take(&mut column));
            }
            column.push_str(cluster);
        }
        output.push(column);
    }
    output
}

fn translation_demo(options: &Options) -> Result<(), Error> {
    if !(1..=12).contains(&options.rows) {
        return Err(Error::Layout);
    }
    let columns = wrap_columns(&options.text, options.rows);
    if columns.len() > 8 {
        return Err(Error::Layout);
    }
    let font = Font::load(options, "Noto Sans CJK SC", 34.)?;
    let mut scene = Scene::new();
    label(
        &mut scene,
        "Chinese replacement / fixed-length vertical columns",
        24.,
        18.,
        22,
    );
    let bubble = kurbo::RoundedRect::new(80., 80., 720., 535., 90.);
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &bubble);
    scene.stroke(
        &kurbo::Stroke::new(3.),
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &bubble,
    );
    let right = 400. + columns.len().saturating_sub(1) as f64 * 27.;
    let max_height = columns
        .iter()
        .map(|s| s.graphemes(true).count())
        .max()
        .unwrap_or(0) as f64
        * 34.;
    let top = 307.5 - max_height * 0.5;
    for (index, text) in columns.iter().enumerate() {
        column(
            &mut scene,
            &font,
            text,
            "zh-Hans",
            Mode::Vertical,
            (right - index as f64 * 54., top),
            34.,
        )?;
    }
    label(
        &mut scene,
        "No auto-fit or kinsoku; font size 34, column pitch 54",
        28.,
        555.,
        17,
    );
    let mut surface = SkiaSurface::new(800, 600).map_err(|source| Error::Render { source })?;
    surface
        .draw_scene(&scene, Color::from_rgb8(225, 228, 232))
        .map_err(|source| Error::Render { source })?;
    let path = options.output.join("translated.png");
    surface
        .write_png(&path)
        .map_err(|source| Error::Render { source })?;
    println!(
        "wrote {} (rows={}, columns={})",
        path.display(),
        options.rows,
        columns.len()
    );
    Ok(())
}
fn main() -> Result<(), Error> {
    let options = Options::parse();
    std::fs::create_dir_all(&options.output).map_err(|source| Error::Io { source })?;
    probe(
        &options,
        "zh-Hans",
        "Noto Sans CJK SC",
        &[
            "「你好，世界！」",
            "今天是第12话。",
            "……等等——真的？",
            "中文与 AI 混排",
        ],
    )?;
    probe(
        &options,
        "ja",
        "Noto Sans CJK JP",
        &[
            "「えっ、本当！？」",
            "コーヒーを飲もう。",
            "……きゃっ、待ってー！",
            "第12話 AI・か\u{3099}",
        ],
    )?;
    translation_demo(&options)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_length_wrapping_preserves_text_and_combining_kana() {
        assert_eq!(wrap_columns("你好，世界！", 3), ["你好，", "世界！"]);
        assert_eq!(
            wrap_columns("か\u{3099}く\n次", 1),
            ["か\u{3099}", "く", "次"]
        );
        assert_eq!(wrap_columns("你好\n\n世界", 10), ["你好", "", "世界"]);
        let input = "「今天是第12话。」";
        assert_eq!(wrap_columns(input, 4).concat(), input);
    }
    #[test]
    fn two_digit_combination_does_not_split_longer_numbers_or_graphemes() {
        assert!(digit_pair("12"));
        assert!(!digit_pair("123"));
        assert_eq!(
            segments("第12話"),
            vec![
                (Kind::Upright, "第".into()),
                (Kind::CombinedDigits, "12".into()),
                (Kind::Upright, "話".into())
            ]
        );
        assert_eq!(
            segments("OCR123"),
            vec![
                (Kind::Sideways, "OCR".into()),
                (Kind::Sideways, "123".into())
            ]
        );
        assert_eq!(
            segments("か\u{3099}"),
            vec![(Kind::Upright, "か\u{3099}".into())]
        );
    }
}
