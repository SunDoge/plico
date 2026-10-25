use crate::Error;
use plico_renderer::{PaintCommand, Scene};
use skia_safe::{Color4f, Font, FontHinting, FontMgr, Matrix, Paint, Path, PathBuilder, Point};
use snafu::ResultExt;
use vello_common::{
    kurbo::{Affine, PathEl, Shape},
    peniko::{Brush, Color, Fill, Style},
};

/// Reusable CPU raster surface. This does not create a window or GPU context.
pub struct SkiaSurface {
    pub(crate) surface: skia_safe::Surface,
    fonts: std::collections::HashMap<(u64, u32), skia_safe::Typeface>,
    font_manager: FontMgr,
    svg_scenes:
        std::collections::HashMap<usize, (std::sync::Weak<plico_renderer::svg::usvg::Tree>, Scene)>,
    frame_valid: bool,
}

impl SkiaSurface {
    /// Allocate a surface; dimensions must be nonzero signed 32-bit values.
    pub fn new(width: u32, height: u32) -> Result<Self, Error> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(Error::InvalidSize { width, height });
        }
        let surface = skia_safe::surfaces::raster_n32_premul((width as i32, height as i32))
            .ok_or(Error::Surface)?;
        Ok(Self::from_surface(surface))
    }

    pub(crate) fn from_surface(surface: skia_safe::Surface) -> Self {
        Self {
            surface,
            fonts: Default::default(),
            font_manager: FontMgr::new(),
            svg_scenes: Default::default(),
            frame_valid: false,
        }
    }

    /// Replace the previous frame and return tightly packed, premultiplied RGBA.
    /// An error never returns partial frame pixels. The next call resets the surface.
    pub fn render_scene(&mut self, scene: &Scene, background: Color) -> Result<Vec<u8>, Error> {
        self.draw_scene(scene, background)?;
        self.read_pixels()
    }

    /// Record drawing without allocating or reading back a pixel buffer.
    pub fn draw_scene(&mut self, scene: &Scene, background: Color) -> Result<(), Error> {
        self.frame_valid = false;
        self.svg_scenes
            .retain(|_, (tree, _)| tree.strong_count() > 0);
        self.surface.canvas().restore_to_count(1);
        self.surface.canvas().clear(color(background));
        let result = self.replay(scene);
        self.surface.canvas().restore_to_count(1);
        result?;
        self.frame_valid = true;
        Ok(())
    }

    /// Read the successful frame as premultiplied RGBA.
    pub fn read_pixels(&mut self) -> Result<Vec<u8>, Error> {
        if !self.frame_valid {
            return Err(Error::NoFrame);
        }
        let info = skia_safe::ImageInfo::new(
            (self.surface.width(), self.surface.height()),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let mut bytes = vec![0; info.min_row_bytes() * self.surface.height() as usize];
        if !self
            .surface
            .read_pixels(&info, &mut bytes, info.min_row_bytes(), (0, 0))
        {
            self.frame_valid = false;
            return Err(Error::ReadPixels);
        }
        self.frame_valid = true;
        Ok(bytes)
    }

    #[cfg(feature = "vulkan")]
    pub(crate) fn invalidate(&mut self) {
        self.frame_valid = false;
    }

    /// Save the most recently rendered frame as PNG.
    pub fn write_png(&mut self, path: &std::path::Path) -> Result<(), Error> {
        if !self.frame_valid {
            return Err(Error::NoFrame);
        }
        let data = self
            .surface
            .image_snapshot()
            .encode(None, skia_safe::EncodedImageFormat::PNG, 100)
            .ok_or(Error::Encode)?;
        std::fs::write(path, data.as_bytes()).context(crate::WriteSnafu)
    }

    fn replay(&mut self, scene: &Scene) -> Result<(), Error> {
        let mut depth = 0usize;
        for command in &scene.commands {
            let canvas = self.surface.canvas();
            match command {
                PaintCommand::Fill {
                    fill,
                    transform,
                    brush,
                    shape,
                    ..
                } => {
                    let paint = paint(brush)?;
                    let mut path = path(shape);
                    path.set_fill_type(match fill {
                        Fill::NonZero => skia_safe::PathFillType::Winding,
                        Fill::EvenOdd => skia_safe::PathFillType::EvenOdd,
                    });
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    canvas.draw_path(&path, &paint);
                    canvas.restore();
                }
                PaintCommand::PushClip { transform, clip } => {
                    canvas.save();
                    canvas.clip_path(&path(clip).make_transform(&matrix(*transform)), None, true);
                    depth += 1;
                }
                PaintCommand::PopLayer => {
                    if depth == 0 {
                        return Err(Error::ClipStack);
                    }
                    canvas.restore();
                    depth -= 1;
                }
                PaintCommand::GlyphRun {
                    font,
                    font_size,
                    hint,
                    normalized_coords,
                    embolden,
                    style,
                    brush,
                    brush_alpha,
                    transform,
                    glyph_transform,
                    glyphs,
                } => {
                    if normalized_coords.iter().any(|c| *c != 0) {
                        return unsupported("variable font coordinates");
                    }
                    if *embolden != Default::default() || glyph_transform.is_some() {
                        return unsupported("synthetic glyph transform/embolden");
                    }
                    if !matches!(style, Style::Fill(Fill::NonZero)) {
                        return unsupported("stroked/even-odd glyphs");
                    }
                    let key = (font.data.id(), font.index);
                    if let std::collections::hash_map::Entry::Vacant(entry) = self.fonts.entry(key)
                    {
                        let face = self
                            .font_manager
                            .new_from_bytes(font.data.as_ref(), font.index)
                            .ok_or(Error::Font)?;
                        entry.insert(face);
                    }
                    let mut skfont = Font::new(self.fonts[&key].clone(), *font_size);
                    skfont.set_edging(skia_safe::font::Edging::AntiAlias);
                    skfont.set_subpixel(true);
                    skfont.set_hinting(if *hint {
                        FontHinting::Normal
                    } else {
                        FontHinting::None
                    });
                    let mut paint = paint(brush)?;
                    paint.set_alpha_f(paint.alpha_f() * brush_alpha);
                    let ids: Vec<u16> = glyphs
                        .iter()
                        .map(|g| u16::try_from(g.id).map_err(|_| Error::GlyphId))
                        .collect::<Result<_, _>>()?;
                    let positions: Vec<Point> =
                        glyphs.iter().map(|g| Point::new(g.x, g.y)).collect();
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    canvas.draw_glyphs_at(&ids, positions.as_slice(), (0., 0.), &skfont, &paint);
                    canvas.restore();
                }
                PaintCommand::PushLayer {
                    blend,
                    alpha,
                    transform,
                    clip,
                    filter,
                } => {
                    if filter.is_some() {
                        return unsupported("layer filter");
                    }
                    if *blend != vello_common::peniko::BlendMode::default() {
                        return unsupported("layer blend mode");
                    }
                    let mut layer_paint = Paint::default();
                    layer_paint.set_alpha_f(*alpha);
                    canvas.save_layer(
                        &skia_safe::canvas::SaveLayerRec::default().paint(&layer_paint),
                    );
                    canvas.clip_path(&path(clip).make_transform(&matrix(*transform)), None, true);
                    depth += 1;
                }
                PaintCommand::Stroke {
                    style,
                    transform,
                    brush,
                    shape,
                    ..
                } => {
                    if style.start_cap != style.end_cap {
                        return unsupported("asymmetric stroke caps");
                    }
                    let mut paint = paint(brush)?;
                    paint.set_style(skia_safe::paint::Style::Stroke);
                    paint.set_stroke_width(style.width as f32);
                    paint.set_stroke_miter(style.miter_limit as f32);
                    paint.set_stroke_cap(match style.start_cap {
                        vello_common::kurbo::Cap::Butt => skia_safe::paint::Cap::Butt,
                        vello_common::kurbo::Cap::Round => skia_safe::paint::Cap::Round,
                        vello_common::kurbo::Cap::Square => skia_safe::paint::Cap::Square,
                    });
                    paint.set_stroke_join(match style.join {
                        vello_common::kurbo::Join::Miter => skia_safe::paint::Join::Miter,
                        vello_common::kurbo::Join::Round => skia_safe::paint::Join::Round,
                        vello_common::kurbo::Join::Bevel => skia_safe::paint::Join::Bevel,
                    });
                    if !style.dash_pattern.is_empty() {
                        let dashes: Vec<f32> =
                            style.dash_pattern.iter().map(|d| *d as f32).collect();
                        let effect = skia_safe::PathEffect::dash(&dashes, style.dash_offset as f32)
                            .ok_or(Error::Unsupported {
                                command: "invalid dash pattern",
                            })?;
                        paint.set_path_effect(effect);
                    }
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    canvas.draw_path(&path(shape), &paint);
                    canvas.restore();
                }
                PaintCommand::BoxShadow {
                    transform,
                    rect,
                    brush,
                    radius,
                    std_dev,
                } => {
                    // Matches Vello's BlurredRoundedRectangle: a rounded rect
                    // blurred by a Gaussian whose sigma is std_dev, painted in
                    // the brush color. Degenerate (inverted) rects are made
                    // absolute, and a zero std_dev paints the solid shape.
                    let mut paint = Paint::new(color(*brush), None);
                    paint.set_anti_alias(true);
                    if *std_dev > 0. {
                        let blur = skia_safe::MaskFilter::blur(
                            skia_safe::BlurStyle::Normal,
                            *std_dev as f32,
                            None,
                        )
                        .ok_or(Error::Unsupported {
                            command: "BoxShadow blur",
                        })?;
                        paint.set_mask_filter(blur);
                    }
                    let rect = rect.abs();
                    let radius = (*radius).clamp(0., rect.width().min(rect.height()) * 0.5);
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    let rrect = skia_safe::RRect::new_rect_xy(
                        skia_safe::Rect::new(
                            rect.x0 as f32,
                            rect.y0 as f32,
                            rect.x1 as f32,
                            rect.y1 as f32,
                        ),
                        radius as f32,
                        radius as f32,
                    );
                    canvas.draw_rrect(rrect, &paint);
                    canvas.restore();
                }
                PaintCommand::Svg {
                    document,
                    transform,
                } => {
                    let key = std::sync::Arc::as_ptr(&document.0) as usize;
                    let commands = match self.svg_scenes.entry(key) {
                        std::collections::hash_map::Entry::Occupied(entry) => entry.get().1.clone(),
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            let commands = crate::svg::commands(&document.0)?;
                            entry
                                .insert((std::sync::Arc::downgrade(&document.0), commands.clone()));
                            commands
                        }
                    };
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    let result = self.replay(&commands);
                    self.surface.canvas().restore();
                    result?;
                }
                PaintCommand::Shader { .. } => return unsupported("Shader"),
            }
        }
        if depth != 0 {
            return Err(Error::ClipStack);
        }
        Ok(())
    }
}

fn unsupported<T>(command: &'static str) -> Result<T, Error> {
    Err(Error::Unsupported { command })
}
fn color(color: Color) -> Color4f {
    let [r, g, b, a] = color.components;
    Color4f::new(r, g, b, a)
}
fn paint(brush: &Brush) -> Result<Paint, Error> {
    let Brush::Solid(value) = brush else {
        return unsupported("gradient/image brush");
    };
    let mut paint = Paint::new(color(*value), None);
    paint.set_anti_alias(true);
    Ok(paint)
}
fn matrix(transform: Affine) -> Matrix {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    Matrix::new_all(
        a as f32, c as f32, e as f32, b as f32, d as f32, f as f32, 0., 0., 1.,
    )
}
fn path(shape: &impl Shape) -> Path {
    let mut builder = PathBuilder::new();
    for element in shape.to_path(0.1).elements() {
        match *element {
            PathEl::MoveTo(p) => {
                builder.move_to((p.x as f32, p.y as f32));
            }
            PathEl::LineTo(p) => {
                builder.line_to((p.x as f32, p.y as f32));
            }
            PathEl::QuadTo(c, p) => {
                builder.quad_to((c.x as f32, c.y as f32), (p.x as f32, p.y as f32));
            }
            PathEl::CurveTo(a, b, p) => {
                builder.cubic_to(
                    (a.x as f32, a.y as f32),
                    (b.x as f32, b.y as f32),
                    (p.x as f32, p.y as f32),
                );
            }
            PathEl::ClosePath => {
                builder.close();
            }
        }
    }
    builder.detach()
}
