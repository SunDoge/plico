//! Experimental direct Skrifa outlines; no changes to production hinting defaults.
use plico_shell::{PaintCommand, PaintScene, Scene};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{NormalizedCoord, Size},
    outline::{
        DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen, SmoothMode, Target,
    },
};
use vello_common::{
    kurbo::{Affine, BezPath, Diagonal2, Join, expand_path},
    peniko::Fill,
};
#[derive(Default)]
struct Pen(BezPath);
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x, y));
    }
    fn quad_to(&mut self, x: f32, y: f32, ex: f32, ey: f32) {
        self.0.quad_to((x, y), (ex, ey));
    }
    fn curve_to(&mut self, x: f32, y: f32, x2: f32, y2: f32, ex: f32, ey: f32) {
        self.0.curve_to((x, y), (x2, y2), (ex, ey));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
pub fn rewrite(scene: &mut Scene, mode: &str, embolden_tolerance: Option<f64>) {
    let mut output = Scene::new();
    for command in std::mem::take(&mut scene.commands) {
        let PaintCommand::GlyphRun {
            font,
            font_size,
            normalized_coords,
            embolden,
            brush,
            brush_alpha,
            transform,
            glyph_transform,
            glyphs,
            ..
        } = command
        else {
            output.commands.push(command);
            continue;
        };
        assert!(
            (embolden_tolerance.is_some() || (embolden.x == 0. && embolden.y == 0.))
                && glyph_transform.is_none(),
            "probe requires native regular glyphs without synthesis"
        );
        let font = FontRef::from_index(font.data.data(), font.index).expect("fixture font");
        let outlines = font.outline_glyphs();
        let coords: Vec<_> = normalized_coords
            .into_iter()
            .map(NormalizedCoord::from_bits)
            .collect();
        let size = Size::new(font_size);
        let symmetric = mode.ends_with("-ft") || mode == "lcd-symmetric";
        let preserve = !mode.ends_with("-ft");
        let mode = match mode {
            "none" => None,
            "lcd" | "lcd-symmetric" | "lcd-ft" => Some(SmoothMode::Lcd),
            "light" | "light-ft" => Some(SmoothMode::Light),
            "normal" | "normal-ft" => Some(SmoothMode::Normal),
            _ => panic!("outline-mode: none/lcd/light/normal"),
        };
        let hint = mode.map(|mode| {
            HintingInstance::new(
                &outlines,
                size,
                coords.as_slice(),
                HintingOptions {
                    engine: Engine::AutoFallback,
                    target: Target::Smooth {
                        mode,
                        symmetric_rendering: symmetric,
                        preserve_linear_metrics: preserve,
                    },
                },
            )
            .expect("hinting instance")
        });
        let brush = brush.multiply_alpha(brush_alpha);
        for glyph in glyphs {
            let outline = outlines.get(GlyphId::new(glyph.id)).expect("outline glyph");
            let mut pen = Pen::default();
            let settings = hint.as_ref().map_or_else(
                || DrawSettings::unhinted(size, coords.as_slice()),
                |hint| DrawSettings::hinted(hint, false),
            );
            outline.draw(settings, &mut pen).expect("draw outline");
            if embolden.x != 0. || embolden.y != 0. {
                let tolerance = embolden_tolerance.expect("explicit expansion tolerance");
                assert!(tolerance.is_finite() && tolerance > 0.);
                pen.0 = expand_path(
                    &pen.0,
                    Diagonal2::new(embolden.x, embolden.y),
                    Join::Miter,
                    4.,
                    tolerance,
                );
            }
            let mut world = (transform
                * Affine::translate((f64::from(glyph.x), f64::from(glyph.y)))
                * Affine::scale_non_uniform(1., -1.))
            .as_coeffs();
            if hint.is_some() {
                world[5] = world[5].round();
            }
            output.fill(Fill::NonZero, Affine::new(world), &brush, None, &pen.0);
        }
    }
    *scene = output;
}
