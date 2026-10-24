//! Render shared text-quality cases through the production Parley/Glifo path.

#[path = "support/artifact_probe.rs"]
mod artifact_probe;
#[path = "support/hint_probe.rs"]
mod hint_probe;
#[path = "support/window_probe.rs"]
mod window_probe;

use std::path::PathBuf;
use std::sync::Arc;
use std::{fmt::Write as _, fs};

use parley::PositionedLayoutItem;
use plico_shell::style::TextAlign;
use plico_shell::text::{TextRun, layout_text_styled};
use plico_shell::{PaintCommand, PaintContext, PaintScene, Scene, TextContext, renderer};
use serde::Deserialize;
use skrifa::{FontRef, MetadataProvider, string::StringId};
use vello_common::kurbo::Affine;
use vello_common::peniko::Color;

#[derive(usage_rs::Cli)]
#[usage(about = "Render shared text fixtures through the production text pipeline")]
struct Options {
    #[usage(long)]
    output: PathBuf,
    #[usage(long)]
    cases: PathBuf,
    /// Experimental override; never changes the production renderer settings.
    #[usage(long)]
    no_hint: bool,
    /// Experimental physical-pixel position snapping: none, y, or xy.
    #[usage(long, default = "none")]
    snap: String,
    /// Quantize final physical X positions to 1/N pixels, without changing advances.
    #[usage(long)]
    x_steps: Option<u32>,
    /// Direct Skrifa outline target: none/lcd/light/normal (probe only).
    #[usage(long)]
    outline_mode: Option<String>,
    /// Direct-outline experiment: tolerance for geometric emboldening.
    #[usage(long)]
    embolden_tolerance: Option<f64>,
    /// Anchor each single-run fixture to its requested physical baseline exactly.
    #[usage(long)]
    exact_baselines: bool,
    /// Display the single-scale fixture through the real native window surface.
    #[usage(long)]
    window: bool,
    /// Check repeated frames, translations, clearing and cross-DPI history.
    #[usage(long)]
    check_artifacts: bool,
    /// Export physical paths for independent rasterization; requires outline-mode.
    #[usage(long)]
    export_outlines: bool,
    /// Pre-flatten direct physical outlines at this tolerance (probe only).
    #[usage(long)]
    outline_flatness: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cases {
    width: u32,
    height: u32,
    background: String,
    foreground: String,
    font_family: Option<String>,
    scales: Vec<f64>,
    #[serde(default, alias = "font_files")]
    font_files: Vec<PathBuf>,
    rows: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    text: String,
    size: f32,
    weight: f32,
    x: f64,
    baseline: f64,
}

fn color(value: &str) -> Color {
    let value = value.strip_prefix('#').unwrap_or(value);
    let packed = u32::from_str_radix(value, 16).expect("six-digit hex color");
    Color::from_rgb8(
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

fn rgba(value: &str) -> [u8; 4] {
    let color = color(value).to_rgba8();
    [color.r, color.g, color.b, color.a]
}

fn main() {
    let options = Options::parse();
    assert!(
        matches!(options.snap.as_str(), "none" | "y" | "xy"),
        "snap: none/y/xy"
    );
    let output = options.output;
    let manifest = options.cases;
    let cases: Cases = serde_json::from_slice(&std::fs::read(&manifest).unwrap_or_else(|error| {
        panic!("read text quality manifest {}: {error}", manifest.display())
    }))
    .expect("parse text quality manifest");
    assert!(
        !options.window || cases.scales.len() == 1,
        "window probe requires one scale"
    );
    std::fs::create_dir_all(&output).expect("create output directory");

    let fonts = cases
        .font_files
        .iter()
        .map(|path| (path, fs::read(path).expect("read fixture font")))
        .collect::<Vec<_>>();
    let mut artifacts = artifact_probe::Probe::default();
    for scale in cases.scales.iter().copied() {
        assert!(scale.is_finite() && scale > 0., "positive finite scale");
        let logical_size = [cases.width as f32, cases.height as f32];
        let mut text = TextContext::new();
        for (_, data) in &fonts {
            text.load_font(data.clone());
        }
        let foreground = rgba(&cases.foreground);
        let family: Option<Arc<str>> = cases.font_family.as_deref().map(Arc::from);
        let layouts = cases
            .rows
            .iter()
            .map(|row| {
                layout_text_styled(
                    &mut text,
                    Arc::from(row.text.as_str()),
                    row.size,
                    row.weight,
                    false,
                    None,
                    TextAlign::Start,
                    foreground,
                    Arc::<[TextRun]>::from([]),
                    family.as_ref(),
                    None,
                )
            })
            .collect::<Vec<_>>();
        let mut report = format!(
            "backend=plico-parley-vello\nscale={scale}\nrequested_family={}\n",
            cases
                .font_family
                .as_deref()
                .unwrap_or(plico_shell::text::DEFAULT_UI_FONT_FAMILY)
        );
        for (row_index, (row, layout)) in cases.rows.iter().zip(&layouts).enumerate() {
            let line = layout.lines().next().expect("text quality row has a line");
            let metrics = line.metrics();
            writeln!(
                report,
                "row={row_index} size={} weight={} baseline={} advance={} line_height={} text={}",
                row.size,
                row.weight,
                metrics.baseline,
                metrics.advance,
                metrics.line_height,
                row.text
            )
            .expect("write Plico text report");
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let font_file = fonts
                    .iter()
                    .find(|(_, data)| data.as_slice() == run.font().data.data())
                    .map(|(path, _)| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "system-fallback".into());
                writeln!(report, "  font_file={font_file}").expect("write font identity");
                let face = FontRef::from_index(run.font().data.data(), run.font().index)
                    .expect("resolved font metadata");
                let name = |id| {
                    face.localized_strings(id)
                        .english_or_first()
                        .map(|value| value.to_string())
                        .unwrap_or_default()
                };
                writeln!(
                    report,
                    "  resolved_family={} subfamily={} postscript={} synthetic_bold={}",
                    name(StringId::FAMILY_NAME),
                    name(StringId::SUBFAMILY_NAME),
                    name(StringId::POSTSCRIPT_NAME),
                    run.synthesis().embolden()
                )
                .expect("write resolved font names");
                write!(
                    report,
                    "  font_blob={:?} font_index={} glyphs=",
                    run.font().data.id(),
                    run.font().index
                )
                .expect("write Plico run report");
                for (index, glyph) in glyph_run.positioned_glyphs().enumerate() {
                    if index > 0 {
                        report.push(',');
                    }
                    write!(report, "{}:{}:{}", glyph.id, glyph.x, glyph.y)
                        .expect("write Plico glyph report");
                }
                report.push('\n');
            }
        }
        fs::write(
            output.join(format!("plico-text-quality-{scale}x.txt")),
            report,
        )
        .expect("write Plico text report");
        let mut paint = PaintContext::new(logical_size[0], logical_size[1], scale, &mut text);
        for (row, layout) in cases.rows.iter().zip(&layouts) {
            let layout_baseline = layout
                .lines()
                .next()
                .expect("text quality row has a line")
                .metrics()
                .baseline;
            paint.draw_text_layout(layout, [row.x, row.baseline - f64::from(layout_baseline)]);
        }
        // PaintContext returns a logical widget fragment. Production appends
        // that fragment under the window's device transform, so reproduce the
        // same final step before rendering a physical comparison image.
        let fragment = paint.finish();
        let mut scene = Scene::new();
        scene.append_scene(fragment, Affine::scale(scale));
        if options.exact_baselines {
            let mut index = 0;
            for command in scene.commands.iter_mut() {
                if let PaintCommand::GlyphRun {
                    transform,
                    glyphs,
                    glyph_transform,
                    ..
                } = command
                {
                    assert!(
                        glyph_transform.is_none(),
                        "exact fixture baseline requires no glyph transform"
                    );
                    let [a, b, c, d, _, _] = transform.as_coeffs();
                    assert!(
                        a == 1. && d == 1. && b == 0. && c == 0.,
                        "exact fixture requires physical translation"
                    );
                    let row = &cases.rows[index];
                    let first = glyphs.first().expect("nonempty fixture run");
                    *transform = Affine::translate((
                        row.x * scale,
                        row.baseline * scale - f64::from(first.y),
                    ));
                    index += 1;
                }
            }
            assert_eq!(
                index,
                cases.rows.len(),
                "exact fixture requires one glyph run per row"
            );
        }
        // Record actual composed positions before experimental rewrites. Rebuilding
        // them from a requested baseline loses f32 cancellation near half pixels.
        let physical: Vec<[f64; 3]> = scene
            .commands
            .iter()
            .flat_map(|command| {
                let PaintCommand::GlyphRun {
                    transform, glyphs, ..
                } = command
                else {
                    return Vec::new();
                };
                glyphs
                    .iter()
                    .map(|glyph| {
                        let p = *transform
                            * vello_common::kurbo::Point::new(
                                f64::from(glyph.x),
                                f64::from(glyph.y),
                            );
                        [f64::from(glyph.id), p.x, p.y]
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        fs::write(
            output.join(format!("plico-physical-glyphs-{scale}x.json")),
            serde_json::to_vec(&physical).expect("physical glyph JSON"),
        )
        .expect("write physical positions");
        for command in scene.commands.iter_mut() {
            if let PaintCommand::GlyphRun {
                hint,
                transform,
                glyph_transform,
                glyphs,
                ..
            } = command
            {
                if options.no_hint {
                    *hint = false;
                }
                if options.snap != "none" || options.x_steps.is_some() {
                    let [a, b, c, d, x, y] = transform.as_coeffs();
                    assert!(
                        (a - 1.).abs() < 0.00001
                            && b == 0.
                            && c == 0.
                            && (d - 1.).abs() < 0.00001
                            && glyph_transform.is_none(),
                        "snap probe requires physical translation only"
                    );
                    for glyph in glyphs {
                        if let Some(steps) = options.x_steps {
                            assert!(steps > 0, "positive X quantization steps");
                            let steps = f64::from(steps);
                            glyph.x =
                                (((f64::from(glyph.x) + x) * steps).round() / steps - x) as f32;
                        }
                        if options.snap == "xy" {
                            glyph.x = ((f64::from(glyph.x) + x).round() - x) as f32;
                        }
                        if options.snap != "none" {
                            glyph.y = ((f64::from(glyph.y) + y).round() - y) as f32;
                        }
                    }
                }
            }
        }
        if let Some(mode) = &options.outline_mode {
            hint_probe::rewrite(&mut scene, mode, options.embolden_tolerance);
        }
        if let Some(tolerance) = options.outline_flatness {
            assert!(options.outline_mode.is_some() && tolerance.is_finite() && tolerance > 0.);
            for command in scene.commands.iter_mut() {
                if let PaintCommand::Fill {
                    transform, shape, ..
                } = command
                {
                    let physical = *transform * shape.clone();
                    let mut flattened = vello_common::kurbo::BezPath::new();
                    vello_common::kurbo::flatten(physical.iter(), tolerance, |element| {
                        flattened.push(element)
                    });
                    *shape = flattened;
                    *transform = vello_common::kurbo::Affine::IDENTITY;
                }
            }
        }
        if options.export_outlines {
            assert!(
                options.outline_mode.is_some(),
                "export requires outline-mode"
            );
            let paths: Vec<Vec<Vec<f64>>> = scene
                .commands
                .iter()
                .filter_map(|command| {
                    let PaintCommand::Fill {
                        transform, shape, ..
                    } = command
                    else {
                        return None;
                    };
                    let path = *transform * shape.clone();
                    Some(
                        path.elements()
                            .iter()
                            .map(|element| {
                                use vello_common::kurbo::PathEl;
                                match element {
                                    PathEl::MoveTo(p) => vec![0., p.x, p.y],
                                    PathEl::LineTo(p) => vec![1., p.x, p.y],
                                    PathEl::QuadTo(p, q) => vec![2., p.x, p.y, q.x, q.y],
                                    PathEl::CurveTo(p, q, r) => {
                                        vec![3., p.x, p.y, q.x, q.y, r.x, r.y]
                                    }
                                    PathEl::ClosePath => vec![4.],
                                }
                            })
                            .collect(),
                    )
                })
                .collect();
            fs::write(
                output.join(format!("outlines-{scale}x.json")),
                serde_json::to_vec(&paths).unwrap(),
            )
            .unwrap();
        }
        let path = output.join(format!("plico-text-quality-{scale}x.png"));
        renderer::render_to_png(
            &scene,
            (f64::from(cases.width) * scale).round() as u32,
            (f64::from(cases.height) * scale).round() as u32,
            color(&cases.background),
            &path.to_string_lossy(),
        )
        .expect("render text quality sheet");
        println!("{}", path.display());
        if options.check_artifacts {
            artifacts.check(
                &scene,
                (f64::from(cases.width) * scale).round() as u32,
                (f64::from(cases.height) * scale).round() as u32,
                color(&cases.background),
                &output,
                scale,
            );
        }
        if options.window {
            window_probe::show(
                scene,
                cases.width,
                cases.height,
                scale,
                color(&cases.background),
            );
        }
    }
    if options.check_artifacts {
        artifacts.finish(&output);
    }
}
