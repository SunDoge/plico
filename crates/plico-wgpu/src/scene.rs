//! Supported shared paint replay. Unsupported operations are errors, never omissions.
use crate::{Draw, Error, Quad, Renderer, Result};
use kurbo::{Affine, Rect};
use peniko::Brush;
use plico_renderer::{PaintCommand, PaintShape, PrimitiveShape, Scene};

/// Retained prepared draws. Font layouts remain owned until the frame is discarded.
pub struct PreparedScene {
    draws: Vec<PreparedDraw>,
    source: Scene,
    flattened: Scene,
    ink: Vec<Option<Rect>>,
}
enum PreparedDraw {
    Quad(Quad),
    Image(crate::image::ImageQuad),
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    Text {
        layout: std::rc::Rc<crate::text::GlyphLayout>,
        origin: [f64; 2],
        color: [f32; 4],
        clip: [f32; 4],
    },
}
impl PreparedDraw {
    fn ink_bounds(&self) -> Rect {
        match self {
            Self::Quad(q) | Self::Image(crate::image::ImageQuad { quad: q, .. }) => {
                let [x, y, w, h] = q.rect;
                Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64)
            }
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            Self::Text { layout, origin, .. } => layout
                .glyphs
                .iter()
                .filter_map(|glyph| {
                    let [x, y, w, h] = glyph.ink_bounds;
                    (w > 0. && h > 0.).then(|| {
                        Rect::new(
                            origin[0] + glyph.x + x,
                            origin[1] + glyph.y + y,
                            origin[0] + glyph.x + x + w,
                            origin[1] + glyph.y + y + h,
                        )
                        .inflate(4., 4.)
                    })
                })
                .reduce(|a, b| a.union(b))
                .unwrap_or(Rect::ZERO),
        }
    }
}
impl PreparedScene {
    /// Estimate damage using this exact flattened stream and native ink geometry.
    /// Advancing a tracker does not draw pixels. The owner must invalidate it
    /// whenever the corresponding target was not successfully updated, and on
    /// background/resource changes not represented by scene commands.
    pub fn update_damage(
        &self,
        tracker: &mut plico_renderer::damage::SceneDamageTracker,
        size: [u32; 2],
    ) -> plico_renderer::DamageRegion {
        tracker.update_indexed(&self.flattened, size[0], size[1], |index, _| {
            self.ink[index]
        })
    }

    pub fn draws(&self) -> Vec<Draw<'_>> {
        self.draws
            .iter()
            .map(|draw| match draw {
                PreparedDraw::Quad(q) => Draw::Quad(q),
                PreparedDraw::Image(image) => Draw::Image(image),
                #[cfg(all(target_os = "linux", feature = "system-text"))]
                PreparedDraw::Text {
                    layout,
                    origin,
                    color,
                    clip,
                } => {
                    let mut run = crate::text::TextRun::new(layout, *origin, *color);
                    run.clip = Some(*clip);
                    run.clip_antialias = true;
                    Draw::Text(run)
                }
            })
            .collect()
    }
}
fn unsupported(index: usize, reason: impl Into<String>) -> Error {
    Error::Unsupported {
        command: index,
        reason: reason.into(),
    }
}
fn uniform(transform: Affine, index: usize) -> Result<(f64, [f64; 2])> {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    if ![a, b, c, d, e, f].iter().all(|v| v.is_finite())
        || a <= 0.
        || b.abs() > 1e-9
        || c.abs() > 1e-9
        || (a - d).abs() > 1e-9
    {
        return Err(unsupported(
            index,
            "only positive uniform scale and translation are supported",
        ));
    }
    Ok((a, [e, f]))
}
fn geometry(shape: &PaintShape, index: usize) -> Result<(Rect, f64)> {
    match shape.primitive() {
        Some(PrimitiveShape::Rect(rect)) => Ok((rect, 0.)),
        Some(PrimitiveShape::RoundedRect(round)) => {
            let radii = round.radii();
            if [radii.top_right, radii.bottom_right, radii.bottom_left]
                .iter()
                .any(|r| (*r - radii.top_left).abs() > 1e-9)
            {
                return Err(unsupported(index, "nonuniform corner radii"));
            }
            Ok((round.rect(), radii.top_left))
        }
        None => Err(unsupported(
            index,
            "arbitrary paths need a path raster backend",
        )),
    }
}
fn quad(rect: Rect, radius: f64, transform: Affine, clip: Rect, index: usize) -> Result<Quad> {
    let (scale, [tx, ty]) = uniform(transform, index)?;
    let mut q = Quad::new(
        [
            (rect.x0 * scale + tx) as f32,
            (rect.y0 * scale + ty) as f32,
            (rect.width() * scale) as f32,
            (rect.height() * scale) as f32,
        ],
        [0.; 4],
    );
    q.radius = (radius * scale) as f32;
    q.clip_antialias = true;
    q.clip = Some([
        clip.x0 as f32,
        clip.y0 as f32,
        clip.width() as f32,
        clip.height() as f32,
    ]);
    Ok(q)
}
fn brush(q: &mut Quad, brush: &Brush, local: Rect, index: usize) -> Result<()> {
    match brush {
        Brush::Solid(color) => q.colors = [color.components; 2],
        Brush::Gradient(gradient) => {
            use peniko::{Extend, GradientKind};
            let GradientKind::Linear(line) = &gradient.kind else {
                return Err(unsupported(index, "nonlinear gradient"));
            };
            if gradient.stops.len() != 2
                || gradient.stops[0].offset != 0.
                || gradient.stops[1].offset != 1.
                || gradient.extend != Extend::Pad
                || gradient.interpolation_cs != peniko::color::ColorSpaceTag::Srgb
            {
                return Err(unsupported(
                    index,
                    "gradient requires two endpoint stops, sRGB interpolation and pad extension",
                ));
            }
            let a = gradient.stops[0]
                .color
                .to_alpha_color::<peniko::color::Srgb>();
            let b = gradient.stops[1]
                .color
                .to_alpha_color::<peniko::color::Srgb>();
            q.gradient_premultiplied = gradient.interpolation_alpha_space
                == peniko::InterpolationAlphaSpace::Premultiplied;
            q.colors = [a.components, b.components];
            q.gradient = [
                ((line.start.x - local.x0) / local.width()) as f32,
                ((line.start.y - local.y0) / local.height()) as f32,
                ((line.end.x - local.x0) / local.width()) as f32,
                ((line.end.y - local.y0) / local.height()) as f32,
            ];
        }
        _ => return Err(unsupported(index, "image brush")),
    }
    Ok(())
}
impl Renderer {
    /// Compile supported shared commands while preserving order and clip scopes.
    /// Fails atomically: no partial frame is submitted if an operation is unsupported.
    pub fn prepare_scene(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
    ) -> Result<std::rc::Rc<PreparedScene>> {
        if let Some((cached_size, cached)) = &self.prepared_cache
            && *cached_size == size
            && cached.source.shares_commands_with(scene)
        {
            self.scene_cache_hits += 1;
            return Ok(cached.clone());
        }
        let source = scene.clone();
        let scene = scene.flattened();
        let mut clips = vec![Rect::new(0., 0., size[0] as f64, size[1] as f64)];
        let mut draws = Vec::new();
        let mut ink = vec![None; scene.commands.len()];
        for (index, command) in scene.commands.iter().enumerate() {
            let clip = *clips.last().unwrap();
            let first_draw = draws.len();
            let fallback = match command {
                PaintCommand::Fill {
                    shape,
                    transform,
                    brush,
                    ..
                } => {
                    geometry(shape, index).is_err()
                        || (!matches!(brush, Brush::Image(_))
                            && uniform(*transform, index).is_err())
                }
                PaintCommand::Stroke {
                    shape,
                    transform,
                    style,
                    ..
                } => {
                    geometry(shape, index).is_err()
                        || uniform(*transform, index).is_err()
                        || !style.dash_pattern.is_empty()
                        || style.join == kurbo::Join::Bevel
                }
                _ => false,
            };
            if fallback {
                let raster = self.paths.raster(command).map_err(|error| match error {
                    Error::Unsupported { reason, .. } => unsupported(index, reason),
                    other => other,
                })?;
                if let Some((image, bounds)) = raster {
                    let q = quad(bounds, 0., Affine::IDENTITY, clip, index)?;
                    draws.push(PreparedDraw::Image(crate::image::ImageQuad {
                        quad: q,
                        image,
                        uv: [0., 0., 1., 1.],
                        quality: peniko::ImageQuality::Low,
                        alpha: 1.,
                    }));
                }
            } else {
                match command {
                    PaintCommand::Fill {
                        transform,
                        brush: paint,
                        brush_transform,
                        shape,
                        fill: _,
                    } => {
                        if !matches!(paint, Brush::Image(_))
                            && brush_transform.is_some_and(|t| t != Affine::IDENTITY)
                        {
                            return Err(unsupported(index, "brush transform"));
                        }
                        let (rect, radius) = geometry(shape, index)?;
                        if rect.width() <= 0. || rect.height() <= 0. {
                            continue;
                        }
                        if let Brush::Image(image) = paint {
                            use peniko::{Extend, ImageQuality};
                            if image.sampler.x_extend != Extend::Pad
                                || image.sampler.y_extend != Extend::Pad
                                || image.sampler.quality == ImageQuality::High
                            {
                                return Err(unsupported(
                                    index,
                                    "repeated/reflected or bicubic image sampling",
                                ));
                            }
                            let [a, b, c, d, e, f] = transform.as_coeffs();
                            if ![a, b, c, d, e, f].iter().all(|v| v.is_finite())
                                || a <= 0.
                                || d <= 0.
                                || b != 0.
                                || c != 0.
                                || (radius > 0. && (a - d).abs() > 1e-9)
                            {
                                return Err(unsupported(
                                    index,
                                    "rotated/sheared image or elliptical image corners",
                                ));
                            }
                            let mut q = quad(rect, radius, Affine::IDENTITY, clip, index)?;
                            q.rect = [
                                (rect.x0 * a + e) as f32,
                                (rect.y0 * d + f) as f32,
                                (rect.width() * a) as f32,
                                (rect.height() * d) as f32,
                            ];
                            q.radius = (radius * a) as f32;
                            let [sx, by, cx, sy, tx, ty] =
                                brush_transform.unwrap_or(Affine::IDENTITY).as_coeffs();
                            if ![sx, by, cx, sy, tx, ty].iter().all(|v| v.is_finite())
                                || sx <= 0.
                                || sy <= 0.
                                || by != 0.
                                || cx != 0.
                            {
                                return Err(unsupported(index, "rotated/sheared image brush"));
                            }
                            let iw = image.image.width as f64 * sx;
                            let ih = image.image.height as f64 * sy;
                            draws.push(PreparedDraw::Image(crate::image::ImageQuad {
                                quad: q,
                                image: image.image.clone(),
                                quality: image.sampler.quality,
                                alpha: image.sampler.alpha,
                                uv: [
                                    ((rect.x0 - tx) / iw) as f32,
                                    ((rect.y0 - ty) / ih) as f32,
                                    (rect.width() / iw) as f32,
                                    (rect.height() / ih) as f32,
                                ],
                            }));
                        } else {
                            let mut q = quad(rect, radius, *transform, clip, index)?;
                            brush(&mut q, paint, rect, index)?;
                            draws.push(PreparedDraw::Quad(q));
                        }
                    }
                    PaintCommand::Stroke {
                        style,
                        transform,
                        brush: paint,
                        brush_transform,
                        shape,
                    } => {
                        if brush_transform.is_some_and(|t| t != Affine::IDENTITY)
                            || !style.dash_pattern.is_empty()
                        {
                            return Err(unsupported(index, "transformed/dashed stroke"));
                        }
                        let Brush::Solid(color) = paint else {
                            return Err(unsupported(index, "non-solid stroke"));
                        };
                        let (rect, radius) = geometry(shape, index)?;
                        let half = style.width * 0.5;
                        let outer_radius = if radius > 0. || style.join == kurbo::Join::Round {
                            radius + half
                        } else if style.join == kurbo::Join::Miter {
                            0.
                        } else {
                            return Err(unsupported(index, "beveled rectangle stroke"));
                        };
                        let mut q = quad(rect.inset(half), outer_radius, *transform, clip, index)?;
                        q.border_width = (style.width * uniform(*transform, index)?.0) as f32;
                        q.border_color = color.components;
                        draws.push(PreparedDraw::Quad(q));
                    }
                    PaintCommand::PushClip {
                        transform,
                        clip: shape,
                    } => {
                        let (rect, radius) = geometry(shape, index)?;
                        if radius != 0. {
                            return Err(unsupported(index, "rounded clip"));
                        }
                        uniform(*transform, index)?;
                        let rect = transform.transform_rect_bbox(rect);
                        clips.push(clip.intersect(rect));
                    }
                    PaintCommand::PopLayer => {
                        if clips.len() == 1 {
                            return Err(unsupported(index, "unbalanced clip pop"));
                        }
                        clips.pop();
                    }
                    PaintCommand::Svg {
                        document,
                        transform,
                    } => {
                        let (scale, [tx, ty]) = uniform(*transform, index)?;
                        let image = self.svgs.raster(document, scale)?;
                        let size = document.0.size();
                        let q = quad(
                            Rect::new(0., 0., size.width() as f64, size.height() as f64),
                            0.,
                            Affine::new([scale, 0., 0., scale, tx, ty]),
                            clip,
                            index,
                        )?;
                        draws.push(PreparedDraw::Image(crate::image::ImageQuad {
                            quad: q,
                            image,
                            uv: [0., 0., 1., 1.],
                            quality: peniko::ImageQuality::Medium,
                            alpha: 1.,
                        }));
                    }
                    PaintCommand::Paragraph { request, transform } => {
                        #[cfg(all(target_os = "linux", feature = "system-text"))]
                        {
                            let (scale, origin) = uniform(*transform, index)?;
                            let layout = self.paragraph_layout(request.clone(), scale)?;
                            draws.push(PreparedDraw::Text {
                                layout,
                                origin,
                                color: request.color.map(|c| c as f32 / 255.),
                                clip: [
                                    clip.x0 as f32,
                                    clip.y0 as f32,
                                    clip.width() as f32,
                                    clip.height() as f32,
                                ],
                            });
                        }
                        #[cfg(not(all(target_os = "linux", feature = "system-text")))]
                        {
                            let _ = (request, transform);
                            return Err(unsupported(
                                index,
                                "system paragraph provider unavailable",
                            ));
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            index,
                            format!(
                                "{} command",
                                match command {
                                    PaintCommand::GlyphRun { .. } => "positioned glyph",
                                    PaintCommand::BoxShadow { .. } => "shadow",
                                    PaintCommand::Shader { .. } => "shader",
                                    PaintCommand::PushLayer { .. } => "isolated layer",
                                    PaintCommand::Fragment { .. } => "unflattened fragment",
                                    _ => "unknown",
                                }
                            ),
                        ));
                    }
                }
            }
            // Bounds come from the native geometry actually submitted, not a
            // second paragraph engine with potentially different metrics.
            if let Some(draw) = draws.get(first_draw) {
                ink[index] = Some(draw.ink_bounds());
            }
        }
        if clips.len() != 1 {
            return Err(unsupported(scene.commands.len(), "unclosed clip scopes"));
        }
        let prepared = std::rc::Rc::new(PreparedScene {
            draws,
            source,
            flattened: scene,
            ink,
        });
        self.scene_preparations += 1;
        self.prepared_cache = Some((size, prepared.clone()));
        Ok(prepared)
    }
}

impl Renderer {
    /// Successful shared-scene preparations and immutable-snapshot cache hits.
    pub fn scene_prepare_stats(&self) -> (u64, u64) {
        (self.scene_preparations, self.scene_cache_hits)
    }
}

impl Renderer {
    fn scene_regions(
        &mut self,
        prepared: &PreparedScene,
        size: [u32; 2],
        background: [f32; 4],
        damage: bool,
    ) -> Option<Vec<[u32; 4]>> {
        if !damage || self.scene_background != Some(background) {
            self.scene_damage.invalidate();
        }
        self.scene_background = Some(background);
        self.scene_damage.set_multi_rect_damage(true);
        let region = prepared.update_damage(&mut self.scene_damage, size);
        tracing::trace!(target: "plico::perf",
            damage_coverage = self.scene_damage.damage_rects().coverage(size[0], size[1]),
            damage_rect_count = self.scene_damage.damage_rects().rects().len(),
            full_redraw = matches!(region, plico_renderer::DamageRegion::Full),
            "wgpu scene damage");
        let rects: Vec<[u32; 4]> = self
            .scene_damage
            .damage_rects()
            .rects()
            .iter()
            .map(|r| {
                [
                    r.x0 as u32,
                    r.y0 as u32,
                    r.width() as u32,
                    r.height() as u32,
                ]
            })
            .collect();
        (!matches!(region, plico_renderer::DamageRegion::Full)).then_some(rects)
    }

    /// GPU-only scene replay into a window view. Returns false when identical
    /// retained pixels allow skipping both rendering and presentation.
    pub fn render_scene_to_view(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        background: [f32; 4],
        damage: bool,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<bool> {
        let prepared = self.prepare_scene(scene, size)?;
        let regions = self.scene_regions(&prepared, size, background, damage);
        if regions.as_ref().is_some_and(|r| r.is_empty()) {
            return Ok(false);
        }
        let result = self
            .encode_frame_regions(&prepared.draws(), size, background, 0., regions.as_deref())
            .and_then(|encoder| self.submit_to_view(encoder, destination, format));
        if result.is_err() {
            self.scene_damage.invalidate();
        }
        result.map(|()| true)
    }

    /// Forget retained presentation after target loss, external replacement or resource changes.
    pub fn invalidate_scene_damage(&mut self) {
        self.scene_damage.invalidate();
    }
    pub fn scene_damage_coverage(&self, size: [u32; 2]) -> f64 {
        self.scene_damage.damage_rects().coverage(size[0], size[1])
    }

    /// Offscreen full/partial scene replay. Damage is relative to this renderer's
    /// retained color target, never to a swapchain image with unknown contents.
    pub fn render_scene(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        background: [f32; 4],
        damage: bool,
    ) -> Result<Vec<u8>> {
        let prepared = self.prepare_scene(scene, size)?;
        let regions = self.scene_regions(&prepared, size, background, damage);
        let result = self
            .encode_frame_regions(&prepared.draws(), size, background, 0., regions.as_deref())
            .and_then(|encoder| self.read_frame(encoder, size));
        if result.is_err() {
            self.scene_damage.invalidate();
        }
        result
    }
}
