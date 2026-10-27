use crate::Error;
use plico_renderer::{DamageRegion, PaintCommand, Scene};
use skia_safe::{Color4f, Font, FontHinting, FontMgr, Matrix, Paint, Path, PathBuilder, Point};
use snafu::ResultExt;
use vello_common::{
    kurbo::{Affine, PathEl, Shape},
    peniko::{
        Brush, Color, Extend, Fill, GradientKind, InterpolationAlphaSpace, Style,
        color::{ColorSpaceTag, Srgb},
    },
};

/// Reusable CPU raster surface. This does not create a window or GPU context.
pub struct SkiaSurface {
    pub(crate) surface: skia_safe::Surface,
    pub(crate) resources: std::rc::Rc<std::cell::RefCell<Resources>>,
    frame_valid: bool,
    background: Option<Color>,
    damage_tracker: plico_renderer::damage::SceneDamageTracker,
    tracking_cooldown: u8,
}
/// CPU assets are shared by the render targets of one GPU context. Per-image
/// caches duplicate complete CJK font files and upload identical textures again.
pub(crate) struct Resources {
    pub(crate) paragraphs: std::rc::Rc<std::cell::RefCell<crate::paragraph::ParagraphCache>>,
    fonts: std::collections::HashMap<u64, FontResource>,
    font_manager: FontMgr,
    svg_scenes:
        std::collections::HashMap<usize, (std::sync::Weak<plico_renderer::svg::usvg::Tree>, Scene)>,
    images: ImageCache,
}
struct FontResource {
    source: vello_common::peniko::WeakBlob<u8>,
    data: skia_safe::Data,
    faces: std::collections::HashMap<u32, skia_safe::Typeface>,
}
impl Default for Resources {
    fn default() -> Self {
        Self {
            paragraphs: Default::default(),
            fonts: Default::default(),
            font_manager: FontMgr::new(),
            svg_scenes: Default::default(),
            images: Default::default(),
        }
    }
}
impl Resources {
    fn font(
        &mut self,
        font: &vello_common::peniko::FontData,
        size: f32,
        hint: bool,
    ) -> Result<Font, Error> {
        // A TTC contains multiple faces. new_from_bytes copies the complete
        // collection for every face; share one owned SkData allocation instead.
        let resource = self
            .fonts
            .entry(font.data.id())
            .or_insert_with(|| FontResource {
                source: font.data.downgrade(),
                data: skia_safe::Data::new_copy(font.data.as_ref()),
                faces: Default::default(),
            });
        if let std::collections::hash_map::Entry::Vacant(entry) = resource.faces.entry(font.index) {
            let face = self
                .font_manager
                .new_from_data(resource.data.clone(), font.index)
                .ok_or(Error::Font)?;
            entry.insert(face);
        }
        let mut skfont = Font::new(resource.faces[&font.index].clone(), size);
        skfont.set_edging(skia_safe::font::Edging::AntiAlias);
        skfont.set_subpixel(true);
        skfont.set_hinting(if hint {
            FontHinting::Normal
        } else {
            FontHinting::None
        });
        Ok(skfont)
    }
    pub(crate) fn ink_bounds(
        &mut self,
        command: &PaintCommand,
    ) -> Option<vello_common::kurbo::Rect> {
        use vello_common::kurbo::Rect;
        match command {
            PaintCommand::Paragraph { request, transform } => Some(
                transform.transform_rect_bbox(
                    self.paragraphs
                        .borrow_mut()
                        .layout(request.clone())
                        .ink_bounds(),
                ),
            ),
            PaintCommand::Fill {
                shape, transform, ..
            } => Some(transform.transform_rect_bbox(shape.bounding_box())),
            PaintCommand::Stroke {
                shape,
                transform,
                style,
                ..
            } => {
                // Include the largest miter/cap extension before transforming.
                let extra = style.width.abs() * style.miter_limit.max(2.) * 0.5;
                Some(transform.transform_rect_bbox(shape.bounding_box().inflate(extra, extra)))
            }
            PaintCommand::GlyphRun {
                font,
                font_size,
                hint,
                glyphs,
                transform,
                normalized_coords,
                embolden,
                style,
                glyph_transform,
                ..
            } if normalized_coords.iter().all(|v| *v == 0)
                && *embolden == Default::default()
                && glyph_transform.is_none()
                && matches!(style, Style::Fill(Fill::NonZero)) =>
            {
                let font = self.font(font, *font_size, *hint).ok()?;
                let ids = glyphs
                    .iter()
                    .map(|g| u16::try_from(g.id))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                let mut bounds = vec![skia_safe::Rect::default(); ids.len()];
                font.get_bounds(&ids, &mut bounds, None);
                let mut result: Option<Rect> = None;
                for (glyph, r) in glyphs.iter().zip(bounds) {
                    if r.is_empty() {
                        continue;
                    }
                    let r = Rect::new(
                        (r.left + glyph.x) as f64,
                        (r.top + glyph.y) as f64,
                        (r.right + glyph.x) as f64,
                        (r.bottom + glyph.y) as f64,
                    );
                    result = Some(result.map_or(r, |old| old.union(r)));
                }
                Some(transform.transform_rect_bbox(result.unwrap_or(Rect::ZERO)))
            }
            // Effects and SVG use their enclosing clip; without a clip, full.
            _ => None,
        }
    }
}
type ImageCache = std::collections::HashMap<
    (u64, u32, u32, u8, u8),
    (vello_common::peniko::WeakBlob<u8>, skia_safe::Image),
>;

impl SkiaSurface {
    /// Select SkParagraph for this host's ordinary text measurement and selection.
    /// Call before constructing the first frame; painting shares the same cache.
    pub fn configure_text(&self, text: &mut plico_renderer::TextContext) {
        text.set_paragraph_engine(Box::new(crate::paragraph::Engine(
            self.resources.borrow().paragraphs.clone(),
        )));
    }
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
        Self::with_resources(surface, Default::default())
    }
    pub(crate) fn with_resources(
        surface: skia_safe::Surface,
        resources: std::rc::Rc<std::cell::RefCell<Resources>>,
    ) -> Self {
        Self {
            surface,
            resources,
            frame_valid: false,
            background: None,
            damage_tracker: Default::default(),
            tracking_cooldown: 0,
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
        self.draw_scene_damaged(scene, background, DamageRegion::Full)
    }

    /// Redraw a preserved surface through an outer pixel clip. The complete
    /// scene is replayed so clipping, overlap and alpha composition retain
    /// their normal ordering. This does not cull command traversal.
    ///
    /// First draw, failed draw and background changes force a full redraw.
    /// The caller must supply conservative old + new ink bounds. Swapchain
    /// callers additionally need buffer-age history; this API alone does not
    /// make an acquired swapchain image safe for partial presentation.
    pub fn draw_scene_damaged(
        &mut self,
        scene: &Scene,
        background: Color,
        damage: DamageRegion,
    ) -> Result<(), Error> {
        self.draw_scene_region(scene, background, damage, None)
    }

    fn draw_scene_region(
        &mut self,
        scene: &Scene,
        background: Color,
        damage: DamageRegion,
        culling: Option<&plico_renderer::damage::SceneDamageTracker>,
    ) -> Result<(), Error> {
        self.damage_tracker.invalidate();
        let damage = if !self.frame_valid || self.background != Some(background) {
            DamageRegion::Full
        } else {
            damage.for_surface(self.surface.width() as u32, self.surface.height() as u32)
        };
        {
            let mut resources = self.resources.borrow_mut();
            resources
                .fonts
                .retain(|_, resource| resource.source.upgrade().is_some());
            resources
                .images
                .retain(|_, (source, _)| source.upgrade().is_some());
            resources
                .svg_scenes
                .retain(|_, (tree, _)| tree.strong_count() > 0);
        }
        if damage == DamageRegion::Empty {
            return Ok(());
        }
        self.frame_valid = false;
        self.surface.canvas().restore_to_count(1);
        if let DamageRegion::Rect(r) = damage {
            self.surface.canvas().save();
            self.surface.canvas().clip_rect(
                skia_safe::Rect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32),
                None,
                false,
            );
        }
        self.surface.canvas().clear(color(background));
        let result = self.replay(scene, culling.map(|tracker| (tracker, damage)));
        self.surface.canvas().restore_to_count(1);
        result?;
        self.frame_valid = true;
        self.background = Some(background);
        Ok(())
    }

    /// Compute command-stream damage against this target's last successful
    /// tracked draw, then redraw it. Each target owns an independent history.
    /// Explicit draws and invalidation discard history conservatively.
    pub fn draw_scene_tracked(
        &mut self,
        scene: &Scene,
        background: Color,
    ) -> Result<DamageRegion, Error> {
        // Dense redraws cannot repay command comparison. Probe again after a
        // bounded cooldown; content remains correct through full redraws.
        if self.tracking_cooldown > 0 {
            self.tracking_cooldown -= 1;
            self.draw_scene(scene, background)?;
            return Ok(DamageRegion::Full);
        }
        let resources = &self.resources;
        let mut tracker = std::mem::take(&mut self.damage_tracker);
        #[cfg(feature = "profiling")]
        let damage_span = tracing::trace_span!(target: "plico::skia", "skia.damage").entered();
        let damage = tracker.update(
            scene,
            self.surface.width() as u32,
            self.surface.height() as u32,
            |command| resources.borrow_mut().ink_bounds(command),
        );
        #[cfg(feature = "profiling")]
        drop(damage_span);
        let actual = if !self.frame_valid || self.background != Some(background) {
            DamageRegion::Full
        } else {
            damage
        };
        let actual = if matches!(actual, DamageRegion::Rect(_))
            && actual.coverage(self.surface.width() as u32, self.surface.height() as u32) > 0.8
        {
            self.tracking_cooldown = 8;
            DamageRegion::Full
        } else {
            actual
        };
        let result = self.draw_scene_region(scene, background, actual, Some(&tracker));
        if result.is_ok() {
            self.damage_tracker = tracker;
        }
        result.map(|()| actual)
    }

    #[cfg(feature = "window")]
    pub(crate) fn validate_scene(&mut self, scene: &Scene, background: Color) -> Result<(), Error> {
        let pixels = self.read_pixels()?;
        let history = std::mem::take(&mut self.damage_tracker);
        self.draw_scene(scene, background)?;
        if self.read_pixels()? != pixels {
            self.invalidate();
            return Err(Error::DamageMismatch);
        }
        self.damage_tracker = history;
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

    fn replay(
        &mut self,
        scene: &Scene,
        culling: Option<(&plico_renderer::damage::SceneDamageTracker, DamageRegion)>,
    ) -> Result<(), Error> {
        #[cfg(feature = "profiling")]
        let _span = tracing::trace_span!(target: "plico::skia", "skia.replay", commands = scene.commands.len()).entered();
        let culling = culling.filter(|(_, damage)| matches!(damage, DamageRegion::Rect(_)));
        let mut depth = 0usize;
        let mut skipped_scope = 0usize;
        for (index, command) in scene.commands.iter().enumerate() {
            if skipped_scope > 0 {
                match command {
                    PaintCommand::PushClip { .. } | PaintCommand::PushLayer { .. } => {
                        skipped_scope += 1
                    }
                    PaintCommand::PopLayer => skipped_scope -= 1,
                    _ => {}
                }
                continue;
            }
            if let Some((tracker, damage)) = culling
                && !matches!(command, PaintCommand::PopLayer)
                && !tracker.command_bounds(index).intersects(damage)
            {
                if matches!(
                    command,
                    PaintCommand::PushClip { .. } | PaintCommand::PushLayer { .. }
                ) {
                    skipped_scope = 1;
                }
                continue;
            }
            let canvas = self.surface.canvas();
            match command {
                PaintCommand::Paragraph { request, transform } => {
                    let paragraphs = self.resources.borrow().paragraphs.clone();
                    let paragraph = paragraphs.borrow_mut().layout(request.clone());
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    paragraph.paint(canvas);
                    canvas.restore();
                }
                PaintCommand::Fill {
                    fill,
                    transform,
                    brush,
                    brush_transform,
                    shape,
                } => {
                    let paint = paint(
                        brush,
                        *brush_transform,
                        &mut self.resources.borrow_mut().images,
                    )?;
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
                    let skfont = self.resources.borrow_mut().font(font, *font_size, *hint)?;
                    let mut paint = paint(brush, None, &mut self.resources.borrow_mut().images)?;
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
                    brush_transform,
                    shape,
                } => {
                    if style.start_cap != style.end_cap {
                        return unsupported("asymmetric stroke caps");
                    }
                    let mut paint = paint(
                        brush,
                        *brush_transform,
                        &mut self.resources.borrow_mut().images,
                    )?;
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
                    let commands = {
                        let mut resources = self.resources.borrow_mut();
                        match resources.svg_scenes.entry(key) {
                            std::collections::hash_map::Entry::Occupied(entry) => {
                                entry.get().1.clone()
                            }
                            std::collections::hash_map::Entry::Vacant(entry) => {
                                let commands = crate::svg::commands(&document.0)?;
                                entry.insert((
                                    std::sync::Arc::downgrade(&document.0),
                                    commands.clone(),
                                ));
                                commands
                            }
                        }
                    };
                    canvas.save();
                    canvas.concat(&matrix(*transform));
                    let result = self.replay(&commands, None);
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
fn paint(
    brush: &Brush,
    transform: Option<Affine>,
    images: &mut ImageCache,
) -> Result<Paint, Error> {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    match brush {
        Brush::Solid(value) => {
            paint.set_color4f(color(*value), None);
        }
        Brush::Gradient(value) => {
            use skia_safe::gradient::{self, interpolation, shaders};
            if value.interpolation_cs != ColorSpaceTag::Srgb {
                return unsupported("gradient interpolation color space");
            }
            let colors: Vec<Color4f> = value
                .stops
                .iter()
                .map(|stop| color(stop.color.to_alpha_color::<Srgb>()))
                .collect();
            let positions: Vec<f32> = value.stops.iter().map(|stop| stop.offset).collect();
            let tile = match value.extend {
                Extend::Pad => skia_safe::TileMode::Clamp,
                Extend::Repeat => skia_safe::TileMode::Repeat,
                Extend::Reflect => skia_safe::TileMode::Mirror,
            };
            let gradient = gradient::Gradient::new(
                gradient::Colors::new(&colors, Some(&positions), tile, None),
                gradient::Interpolation {
                    in_premul: match value.interpolation_alpha_space {
                        InterpolationAlphaSpace::Premultiplied => interpolation::InPremul::Yes,
                        InterpolationAlphaSpace::Unpremultiplied => interpolation::InPremul::No,
                    },
                    color_space: interpolation::ColorSpace::SRGB,
                    ..Default::default()
                },
            );
            let matrix = matrix(transform.unwrap_or(Affine::IDENTITY));
            let point = |p: vello_common::kurbo::Point| Point::new(p.x as f32, p.y as f32);
            let shader = match value.kind {
                GradientKind::Linear(g) => shaders::linear_gradient(
                    (point(g.start), point(g.end)),
                    &gradient,
                    Some(&matrix),
                ),
                GradientKind::Radial(g) => shaders::two_point_conical_gradient(
                    (point(g.start_center), g.start_radius),
                    (point(g.end_center), g.end_radius),
                    &gradient,
                    Some(&matrix),
                ),
                GradientKind::Sweep(_) => return unsupported("sweep gradient"),
            }
            .ok_or(Error::Unsupported {
                command: "invalid gradient",
            })?;
            paint.set_shader(shader);
        }
        Brush::Image(brush) => {
            use skia_safe::{
                AlphaType, ColorType, FilterMode, ImageInfo, MipmapMode, SamplingOptions,
            };
            use vello_common::peniko::{ImageAlphaType, ImageFormat, ImageQuality};
            let data = &brush.image;
            let key = (
                data.data.id(),
                data.width,
                data.height,
                data.format as u8,
                data.alpha_type as u8,
            );
            if let std::collections::hash_map::Entry::Vacant(entry) = images.entry(key) {
                let required = data.format.size_in_bytes(data.width, data.height).ok_or(
                    Error::Unsupported {
                        command: "image dimensions",
                    },
                )?;
                if data.width == 0
                    || data.height == 0
                    || data.width > i32::MAX as u32
                    || data.height > i32::MAX as u32
                    || data.data.data().len() < required
                {
                    return unsupported("image dimensions or pixel data");
                }
                let format = match data.format {
                    ImageFormat::Rgba8 => ColorType::RGBA8888,
                    ImageFormat::Bgra8 => ColorType::BGRA8888,
                    _ => return unsupported("image format"),
                };
                let alpha = match data.alpha_type {
                    ImageAlphaType::Alpha => AlphaType::Unpremul,
                    ImageAlphaType::AlphaPremultiplied => AlphaType::Premul,
                };
                let info = ImageInfo::new(
                    (data.width as i32, data.height as i32),
                    format,
                    alpha,
                    skia_safe::ColorSpace::new_srgb(),
                );
                // Copy once into immutable Skia storage; never rebuild/upload the
                // same image resource on every replay. Metadata is part of the key.
                let image = skia_safe::images::raster_from_data(
                    &info,
                    skia_safe::Data::new_copy(data.data.data()),
                    data.width as usize * 4,
                )
                .ok_or(Error::Unsupported {
                    command: "image pixel data",
                })?;
                entry.insert((data.data.downgrade(), image));
            }
            let tile = |extend| match extend {
                Extend::Pad => skia_safe::TileMode::Clamp,
                Extend::Repeat => skia_safe::TileMode::Repeat,
                Extend::Reflect => skia_safe::TileMode::Mirror,
            };
            let sampling = match brush.sampler.quality {
                ImageQuality::Low => SamplingOptions::new(FilterMode::Nearest, MipmapMode::None),
                ImageQuality::Medium => SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
                ImageQuality::High => skia_safe::CubicResampler::mitchell().into(),
            };
            let local = matrix(transform.unwrap_or(Affine::IDENTITY));
            let shader = images[&key]
                .1
                .to_shader(
                    (tile(brush.sampler.x_extend), tile(brush.sampler.y_extend)),
                    sampling,
                    &local,
                )
                .ok_or(Error::Unsupported {
                    command: "image shader",
                })?;
            paint.set_shader(shader);
            paint.set_alpha_f(brush.sampler.alpha);
        }
    }
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

#[cfg(test)]
mod resource_tests {
    use super::*;
    use plico_renderer::PaintScene;
    use std::{cell::RefCell, rc::Rc, sync::Arc};
    use vello_common::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};

    #[test]
    fn font_resources_release_dead_sources_and_share_sizes() {
        let font = vello_common::peniko::FontData::new(
            Blob::new(Arc::new(
                include_bytes!("../../../backend/examples/web-preview/public/DejaVuSans.ttf")
                    .to_vec(),
            )),
            0,
        );
        let mut surface = SkiaSurface::new(4, 4).unwrap();
        surface.draw_scene(&Scene::new(), Color::WHITE).unwrap();
        for size in [12., 24., 36.] {
            surface
                .resources
                .borrow_mut()
                .font(&font, size, true)
                .unwrap();
        }
        assert_eq!(surface.resources.borrow().fonts.len(), 1);
        assert_eq!(
            surface.resources.borrow().fonts[&font.data.id()]
                .faces
                .len(),
            1
        );
        drop(font);
        surface
            .draw_scene_damaged(&Scene::new(), Color::WHITE, DamageRegion::Empty)
            .unwrap();
        assert!(surface.resources.borrow().fonts.is_empty());
    }

    #[test]
    #[ignore = "requires PLICO_TEST_TTC pointing to a multi-face TTC collection"]
    fn collection_faces_share_one_font_data_allocation() {
        let bytes = std::fs::read(std::env::var_os("PLICO_TEST_TTC").unwrap()).unwrap();
        assert_eq!(&bytes[..4], b"ttcf");
        let count = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        assert!((2..=64).contains(&count));
        let blob = Blob::new(Arc::new(bytes));
        let mut resources = Resources::default();
        for index in 0..count {
            let font = vello_common::peniko::FontData::new(blob.clone(), index);
            resources.font(&font, 24., true).unwrap();
        }
        assert_eq!(resources.fonts.len(), 1);
        let resource = &resources.fonts[&blob.id()];
        assert_eq!(resource.data.size(), blob.as_ref().len());
        assert_eq!(resource.faces.len(), count as usize);
    }

    #[test]
    fn targets_share_one_image_resource_and_release_dead_sources() {
        let resources = Rc::new(RefCell::new(Resources::default()));
        let brush = ImageBrush::new(ImageData {
            data: Blob::new(Arc::new(vec![255u8, 0, 0, 255])),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        });
        let mut scene = Scene::new();
        scene.draw_image((&brush).into(), Affine::scale(4.));
        let mut targets: Vec<_> = (0..3)
            .map(|_| {
                let surface = skia_safe::surfaces::raster_n32_premul((4, 4)).unwrap();
                SkiaSurface::with_resources(surface, resources.clone())
            })
            .collect();
        for target in &mut targets {
            let pixels = target.render_scene(&scene, Color::WHITE).unwrap();
            assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
            assert_eq!(
                resources.borrow().images.len(),
                1,
                "one conversion for all targets"
            );
        }
        drop(scene);
        drop(brush);
        targets[0].draw_scene(&Scene::new(), Color::WHITE).unwrap();
        assert!(resources.borrow().images.is_empty());
    }
}
