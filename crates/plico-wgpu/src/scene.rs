//! Supported shared paint replay. Unsupported operations are errors, never omissions.
use crate::{Draw, Error, Quad, Renderer, Result};
use kurbo::{Affine, Rect};
use peniko::Brush;
use plico_renderer::{PaintCommand, PaintShape, PrimitiveShape, Scene};

/// Retained prepared draws. Font layouts remain owned until the frame is discarded.
pub struct PreparedScene {
    draws: Vec<PreparedDraw>,
    payloads: Payloads,
    source: Scene,
    flattened: Scene,
    root_clip: Rect,
    ink: Vec<Option<Rect>>,
}
/// An isolated source-over group. Children are drawn into a transparent GPU
/// texture, then alpha/clip are applied once to the completed group.
pub struct PreparedLayer {
    pub(crate) scene: std::rc::Rc<PreparedScene>,
    pub(crate) quad: Quad,
    pub(crate) alpha: f32,
    pub(crate) blur: Option<plico_renderer::filter::GaussianBlur>,
}
#[derive(Default)]
struct Payloads {
    quads: Vec<Quad>,
    layers: Vec<PreparedLayer>,
    effects: Vec<(plico_renderer::ShaderEffect, Quad)>,
    images: Vec<crate::image::ImageQuad>,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    text: Vec<PreparedText>,
}
#[cfg(all(target_os = "linux", feature = "system-text"))]
struct PreparedText {
    layout: std::rc::Rc<crate::text::GlyphLayout>,
    origin: [f64; 2],
    color: [f32; 4],
    clip: [f32; 4],
}
enum PreparedDraw {
    Quad(usize),
    Layer(usize),
    Effect(usize),
    Image(usize),
    SetMask(std::rc::Rc<crate::mask::CoverageMask>),
    ResetMask,
    #[cfg(all(target_os = "linux", feature = "system-text"))]
    Text(usize),
}
fn stash<T>(table: &mut Vec<T>, value: T) -> usize {
    let index = table.len();
    table.push(value);
    index
}
fn image_draw(payloads: &mut Payloads, image: crate::image::ImageQuad) -> PreparedDraw {
    PreparedDraw::Image(stash(&mut payloads.images, image))
}
fn layer_draw(payloads: &mut Payloads, layer: PreparedLayer) -> PreparedDraw {
    PreparedDraw::Layer(stash(&mut payloads.layers, layer))
}
impl PreparedDraw {
    fn ink_bounds(&self, payloads: &Payloads) -> Rect {
        let bounds = |q: &Quad| {
            let [x, y, w, h] = q.rect;
            Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64)
        };
        match self {
            Self::SetMask(_) | Self::ResetMask => Rect::ZERO,
            Self::Quad(i) => bounds(&payloads.quads[*i]),
            Self::Layer(i) => bounds(&payloads.layers[*i].quad),
            Self::Effect(i) => bounds(&payloads.effects[*i].1),
            Self::Image(i) => bounds(&payloads.images[*i].quad),
            #[cfg(all(target_os = "linux", feature = "system-text"))]
            Self::Text(i) => {
                let text = &payloads.text[*i];
                text.layout
                    .glyphs
                    .iter()
                    .filter_map(|glyph| {
                        let [x, y, w, h] = glyph.ink_bounds;
                        (w > 0. && h > 0.).then(|| {
                            Rect::new(
                                text.origin[0] + glyph.x + x,
                                text.origin[1] + glyph.y + y,
                                text.origin[0] + glyph.x + x + w,
                                text.origin[1] + glyph.y + y + h,
                            )
                            .inflate(4., 4.)
                        })
                    })
                    .reduce(|a, b| a.union(b))
                    .unwrap_or(Rect::ZERO)
            }
        }
    }
}
/// CPU vector storage owned by this prepared scene, excluding nested scenes,
/// command snapshots, shared font/image/shader backing resources and GPU/driver allocations.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreparedStorageStats {
    /// Painter-order entry count.
    pub draws: usize,
    /// Allocated capacity of the painter-order stream in bytes.
    pub order_bytes: usize,
    /// Allocated capacity of typed payload tables in bytes.
    pub payload_bytes: usize,
    /// Allocated capacity of per-command ink bounds in bytes.
    pub ink_bytes: usize,
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

    /// CPU vector capacities for this scene only; not process RSS.
    pub fn storage_stats(&self) -> PreparedStorageStats {
        use std::mem::size_of;
        let data = &self.payloads;
        let bytes = data.quads.capacity() * size_of::<Quad>()
            + data.layers.capacity() * size_of::<PreparedLayer>()
            + data.effects.capacity() * size_of::<(plico_renderer::ShaderEffect, Quad)>()
            + data.images.capacity() * size_of::<crate::image::ImageQuad>();
        #[cfg(all(target_os = "linux", feature = "system-text"))]
        let bytes = bytes + data.text.capacity() * size_of::<PreparedText>();
        PreparedStorageStats {
            draws: self.draws.len(),
            order_bytes: self.draws.capacity() * size_of::<PreparedDraw>(),
            payload_bytes: bytes,
            ink_bytes: self.ink.capacity() * size_of::<Option<Rect>>(),
        }
    }
    pub fn draws(&self) -> Vec<Draw<'_>> {
        let data = &self.payloads;
        self.draws
            .iter()
            .map(|draw| match draw {
                PreparedDraw::Quad(i) => Draw::Quad(&data.quads[*i]),
                PreparedDraw::Layer(i) => Draw::Layer(&data.layers[*i]),
                PreparedDraw::Effect(i) => {
                    let (effect, q) = &data.effects[*i];
                    Draw::Effect(effect, q)
                }
                PreparedDraw::Image(i) => Draw::Image(&data.images[*i]),
                PreparedDraw::SetMask(mask) => Draw::SetMask(mask),
                PreparedDraw::ResetMask => Draw::ResetMask,
                #[cfg(all(target_os = "linux", feature = "system-text"))]
                PreparedDraw::Text(i) => {
                    let text = &data.text[*i];
                    let mut run = crate::text::TextRun::new(&text.layout, text.origin, text.color);
                    run.clip = Some(text.clip);
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
pub(crate) fn uniform(transform: Affine, index: usize) -> Result<(f64, [f64; 2])> {
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
        self.validate_frame(0, size, [0.; 4], 0.)?;
        self.prepare_scene_with_clip(
            scene,
            size,
            Rect::new(0., 0., size[0] as f64, size[1] as f64),
        )
    }
    fn prepare_scene_with_clip(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        root_clip: Rect,
    ) -> Result<std::rc::Rc<PreparedScene>> {
        if self.scene_prepare_depth > 16 {
            return Err(unsupported(0, "layer preparation depth exceeds 16"));
        }
        self.scene_prepare_depth += 1;
        let result = self.prepare_scene_inner(scene, size, root_clip);
        self.scene_prepare_depth -= 1;
        result
    }
    fn prepare_scene_inner(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        root_clip: Rect,
    ) -> Result<std::rc::Rc<PreparedScene>> {
        if let Some((cached_size, cached)) = &self.prepared_cache
            && *cached_size == size
            && cached.root_clip == root_clip
            && cached.source.shares_commands_with(scene)
        {
            self.scene_cache_hits += 1;
            return Ok(cached.clone());
        }
        let source = scene.clone();
        let scene = scene.flattened();
        let mut clips = vec![root_clip];
        let mut masks: Vec<Option<std::rc::Rc<crate::mask::CoverageMask>>> = vec![None];
        let mut draws = Vec::new();
        let mut payloads = Payloads::default();
        let mut ink = vec![None; scene.commands.len()];
        let mut skip_until = 0;
        for (index, command) in scene.commands.iter().enumerate() {
            if index < skip_until {
                continue;
            }
            let clip = *clips.last().unwrap();
            let first_draw = draws.len();
            let image_path = match command {
                PaintCommand::Fill {
                    shape,
                    transform,
                    brush: Brush::Image(image),
                    brush_transform,
                    ..
                } => Some((shape, transform, image, brush_transform, false)),
                PaintCommand::Stroke {
                    shape,
                    transform,
                    brush: Brush::Image(image),
                    brush_transform,
                    ..
                } => Some((shape, transform, image, brush_transform, true)),
                _ => None,
            };
            if let Some((shape, transform, image, brush_transform, stroke)) = image_path {
                let axis = |t: Affine| {
                    let [a, b, c, d, e, f] = t.as_coeffs();
                    [a, b, c, d, e, f].iter().all(|v| v.is_finite())
                        && a > 0.
                        && d > 0.
                        && b == 0.
                        && c == 0.
                };
                let fast = !stroke
                    && geometry(shape, index).is_ok_and(|(_, radius)| {
                        axis(*transform)
                            && (radius == 0.
                                || (transform.as_coeffs()[0] - transform.as_coeffs()[3]).abs()
                                    < 1e-9)
                    })
                    && axis(brush_transform.unwrap_or(Affine::IDENTITY));
                if !fast {
                    let source = *transform * brush_transform.unwrap_or(Affine::IDENTITY);
                    let coefficients = source.as_coeffs();
                    let det = source.determinant();
                    if !coefficients.iter().all(|v| v.is_finite())
                        || !det.is_finite()
                        || det.abs() < 1e-12
                        || image.image.width == 0
                        || image.image.height == 0
                    {
                        return Err(unsupported(
                            index,
                            "invalid affine image transform or dimensions",
                        ));
                    }
                    let uv = Affine::scale_non_uniform(
                        1. / image.image.width as f64,
                        1. / image.image.height as f64,
                    ) * source.inverse();
                    let parent = masks.last().unwrap().clone();
                    let mut mask_command = command.clone();
                    match &mut mask_command {
                        PaintCommand::Fill {
                            brush,
                            brush_transform,
                            ..
                        }
                        | PaintCommand::Stroke {
                            brush,
                            brush_transform,
                            ..
                        } => {
                            *brush = peniko::Color::WHITE.into();
                            *brush_transform = None;
                        }
                        _ => unreachable!(),
                    }
                    let coverage =
                        self.paths
                            .raster(&mask_command)
                            .map_err(|error| match error {
                                Error::Unsupported { reason, .. } => unsupported(index, reason),
                                other => other,
                            })?;
                    if let Some((coverage, bounds)) = coverage {
                        let mask = self.masks.intersect(coverage, bounds, parent.as_deref())?;
                        let bounds = mask.bounds();
                        let q = quad(bounds, 0., Affine::IDENTITY, clip, index)?;
                        ink[index] = Some(bounds);
                        draws.push(PreparedDraw::SetMask(mask));
                        draws.push(image_draw(
                            &mut payloads,
                            crate::image::ImageQuad {
                                quad: q,
                                image: image.image.clone(),
                                uv: [0., 0., 1., 1.],
                                uv_transform: Some(uv.as_coeffs().map(|v| v as f32)),
                                quality: image.sampler.quality,
                                extend: [image.sampler.x_extend, image.sampler.y_extend],
                                alpha: image.sampler.alpha,
                            },
                        ));
                        draws.push(match parent {
                            Some(mask) => PreparedDraw::SetMask(mask),
                            None => PreparedDraw::ResetMask,
                        });
                    } else {
                        ink[index] = Some(Rect::ZERO);
                    }
                    continue;
                }
            }
            let fallback = match command {
                PaintCommand::Fill {
                    shape,
                    transform,
                    brush: paint,
                    brush_transform,
                    ..
                } => {
                    geometry(shape, index).is_err()
                        || (!matches!(paint, Brush::Image(_))
                            && uniform(*transform, index).is_err())
                        || (matches!(paint, Brush::Gradient(_))
                            && (brush_transform.is_some_and(|t| t != Affine::IDENTITY)
                                || geometry(shape, index).is_ok_and(|(rect, _)| {
                                    brush(
                                        &mut Quad::new([0., 0., 1., 1.], [0.; 4]),
                                        paint,
                                        rect,
                                        index,
                                    )
                                    .is_err()
                                })))
                }
                PaintCommand::Stroke {
                    shape,
                    transform,
                    style,
                    brush,
                    ..
                } => {
                    geometry(shape, index).is_err()
                        || uniform(*transform, index).is_err()
                        || !style.dash_pattern.is_empty()
                        || style.join == kurbo::Join::Bevel
                        || matches!(brush, Brush::Gradient(_))
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
                    draws.push(image_draw(
                        &mut payloads,
                        crate::image::ImageQuad {
                            quad: q,
                            image,
                            uv: [0., 0., 1., 1.],
                            uv_transform: None,
                            quality: peniko::ImageQuality::Low,
                            extend: [peniko::Extend::Pad; 2],
                            alpha: 1.,
                        },
                    ));
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
                            draws.push(image_draw(
                                &mut payloads,
                                crate::image::ImageQuad {
                                    quad: q,
                                    image: image.image.clone(),
                                    uv_transform: None,
                                    quality: image.sampler.quality,
                                    extend: [image.sampler.x_extend, image.sampler.y_extend],
                                    alpha: image.sampler.alpha,
                                    uv: [
                                        ((rect.x0 - tx) / iw) as f32,
                                        ((rect.y0 - ty) / ih) as f32,
                                        (rect.width() / iw) as f32,
                                        (rect.height() / ih) as f32,
                                    ],
                                },
                            ));
                        } else {
                            let mut q = quad(rect, radius, *transform, clip, index)?;
                            brush(&mut q, paint, rect, index)?;
                            draws.push(PreparedDraw::Quad(stash(&mut payloads.quads, q)));
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
                        draws.push(PreparedDraw::Quad(stash(&mut payloads.quads, q)));
                    }
                    PaintCommand::PushLayer {
                        blend,
                        alpha,
                        transform,
                        clip: shape,
                        filter,
                    } => {
                        if *blend != peniko::BlendMode::default() {
                            return Err(unsupported(index, "layer blend mode/filter"));
                        }
                        let blur = filter
                            .as_ref()
                            .map(|filter| {
                                let mut blur =
                                    plico_renderer::filter::GaussianBlur::from_filter(filter)
                                        .ok_or_else(|| unsupported(index, "layer filter graph"))?;
                                blur.sigma *= uniform(*transform, index)?.0 as f32;
                                if !blur.sigma.is_finite() || blur.sigma > 64. {
                                    return Err(unsupported(
                                        index,
                                        "blur sigma exceeds 64 physical pixels",
                                    ));
                                }
                                Ok(blur)
                            })
                            .transpose()?;
                        if !alpha.is_finite() || !(0. ..=1.).contains(alpha) {
                            return Err(unsupported(index, "invalid layer alpha"));
                        }
                        let mut depth = 1;
                        let mut end = None;
                        for (next, command) in scene.commands.iter().enumerate().skip(index + 1) {
                            match command {
                                PaintCommand::PushLayer { .. } | PaintCommand::PushClip { .. } => {
                                    depth += 1
                                }
                                PaintCommand::PopLayer => {
                                    depth -= 1;
                                    if depth == 0 {
                                        end = Some(next);
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                        let end =
                            end.ok_or_else(|| unsupported(index, "unclosed isolated layer"))?;
                        skip_until = end + 1;
                        let mut child = Scene::new();
                        child
                            .commands
                            .extend(scene.commands[index + 1..end].iter().cloned());
                        // The viewport is an output clip, not a limit on source
                        // samples needed by a filter. Carry its halo through
                        // nested groups and include it in prepared-cache identity.
                        let reach = blur.map_or(0., |b| (4. * b.sigma as f64).ceil());
                        let child = self
                            .prepare_scene_with_clip(&child, size, root_clip.inflate(reach, reach))
                            .map_err(|error| match error {
                                Error::Unsupported { command, reason } => {
                                    unsupported(index + 1 + command, reason)
                                }
                                other => other,
                            })?;
                        let parent = masks.last().unwrap().clone();
                        let mut layer_clip = clip;
                        let mut composite_mask = parent.clone();
                        if let Ok((rect, 0.)) = geometry(shape, index)
                            && uniform(*transform, index).is_ok()
                        {
                            layer_clip = layer_clip.intersect(transform.transform_rect_bbox(rect));
                        } else {
                            let command = PaintCommand::Fill {
                                fill: peniko::Fill::NonZero,
                                transform: *transform,
                                brush: peniko::Color::WHITE.into(),
                                brush_transform: None,
                                shape: shape.clone(),
                            };
                            if let Some((image, bounds)) = self.paths.raster(&command)? {
                                let mask =
                                    self.masks.intersect(image, bounds, parent.as_deref())?;
                                layer_clip = layer_clip.intersect(mask.bounds());
                                composite_mask = Some(mask);
                            } else {
                                layer_clip = Rect::ZERO;
                            }
                        }
                        if let Some(mask) = &composite_mask {
                            draws.push(PreparedDraw::SetMask(mask.clone()));
                        }
                        let q = quad(
                            Rect::new(
                                layer_clip.x0.floor() - reach,
                                layer_clip.y0.floor() - reach,
                                layer_clip.x1.ceil() + reach,
                                layer_clip.y1.ceil() + reach,
                            ),
                            0.,
                            Affine::IDENTITY,
                            layer_clip,
                            index,
                        )?;
                        draws.push(layer_draw(
                            &mut payloads,
                            PreparedLayer {
                                scene: child,
                                quad: q,
                                alpha: *alpha,
                                blur,
                            },
                        ));
                        draws.push(match parent {
                            Some(mask) => PreparedDraw::SetMask(mask),
                            None => PreparedDraw::ResetMask,
                        });
                    }
                    PaintCommand::PushClip {
                        transform,
                        clip: shape,
                    } => {
                        if clips.len() >= 128 {
                            return Err(unsupported(index, "clip depth exceeds 128"));
                        }
                        let previous = masks.last().unwrap().clone();
                        if let Ok((rect, 0.)) = geometry(shape, index)
                            && uniform(*transform, index).is_ok()
                        {
                            clips.push(clip.intersect(transform.transform_rect_bbox(rect)));
                            masks.push(previous);
                        } else {
                            let command = PaintCommand::Fill {
                                fill: peniko::Fill::NonZero,
                                transform: *transform,
                                brush: peniko::Color::WHITE.into(),
                                brush_transform: None,
                                shape: shape.clone(),
                            };
                            let raster =
                                self.paths.raster(&command).map_err(|error| match error {
                                    Error::Unsupported { reason, .. } => unsupported(index, reason),
                                    other => other,
                                })?;
                            if let Some((image, bounds)) = raster {
                                let mask =
                                    self.masks.intersect(image, bounds, previous.as_deref())?;
                                clips.push(clip.intersect(mask.bounds()));
                                masks.push(Some(mask.clone()));
                                draws.push(PreparedDraw::SetMask(mask));
                            } else {
                                clips.push(Rect::ZERO);
                                masks.push(previous);
                            }
                        }
                    }
                    PaintCommand::PopLayer => {
                        if clips.len() == 1 {
                            return Err(unsupported(index, "unbalanced clip pop"));
                        }
                        clips.pop();
                        let old = masks.pop().unwrap();
                        let restored = masks.last().unwrap();
                        let same = match (&old, restored) {
                            (None, None) => true,
                            (Some(a), Some(b)) => std::rc::Rc::ptr_eq(a, b),
                            _ => false,
                        };
                        if !same {
                            draws.push(match restored {
                                Some(mask) => PreparedDraw::SetMask(mask.clone()),
                                None => PreparedDraw::ResetMask,
                            });
                        }
                    }
                    #[cfg(all(target_os = "linux", feature = "system-text"))]
                    PaintCommand::GlyphRun { .. } => {
                        if let Some((image, bounds)) =
                            self.glyphs.raster(command).map_err(|error| match error {
                                Error::Unsupported { reason, .. } => unsupported(index, reason),
                                other => other,
                            })?
                        {
                            let q = quad(bounds, 0., Affine::IDENTITY, clip, index)?;
                            draws.push(image_draw(
                                &mut payloads,
                                crate::image::ImageQuad {
                                    quad: q,
                                    image,
                                    uv: [0., 0., 1., 1.],
                                    uv_transform: None,
                                    quality: peniko::ImageQuality::Low,
                                    extend: [peniko::Extend::Pad; 2],
                                    alpha: 1.,
                                },
                            ));
                        }
                    }
                    PaintCommand::BoxShadow {
                        transform,
                        rect,
                        brush,
                        radius,
                        std_dev,
                    } => {
                        if !std_dev.is_finite()
                            || *std_dev < 0.
                            || !radius.is_finite()
                            || *radius < 0.
                        {
                            return Err(unsupported(index, "invalid shadow radius or sigma"));
                        }
                        let (scale, _) = uniform(*transform, index)?;
                        let mut q = quad(
                            rect.inflate(4. * std_dev, 4. * std_dev),
                            *radius,
                            *transform,
                            clip,
                            index,
                        )?;
                        q.shadow_sigma = (std_dev * scale) as f32;
                        q.colors = [brush.components; 2];
                        draws.push(PreparedDraw::Quad(stash(&mut payloads.quads, q)));
                    }
                    PaintCommand::Shader { effect, transform } => {
                        let [w, h] = effect.logical_size();
                        let q = quad(
                            Rect::new(0., 0., w as f64, h as f64),
                            0.,
                            *transform,
                            clip,
                            index,
                        )?;
                        draws.push(PreparedDraw::Effect(stash(
                            &mut payloads.effects,
                            (effect.clone(), q),
                        )));
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
                        draws.push(image_draw(
                            &mut payloads,
                            crate::image::ImageQuad {
                                quad: q,
                                image,
                                uv: [0., 0., 1., 1.],
                                uv_transform: None,
                                quality: peniko::ImageQuality::Medium,
                                extend: [peniko::Extend::Pad; 2],
                                alpha: 1.,
                            },
                        ));
                    }
                    PaintCommand::Paragraph { request, transform } => {
                        #[cfg(all(target_os = "linux", feature = "system-text"))]
                        {
                            let (scale, origin) = uniform(*transform, index)?;
                            let layout = self.paragraph_layout(request.clone(), scale)?;
                            payloads.text.push(PreparedText {
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
                            draws.push(PreparedDraw::Text(payloads.text.len() - 1));
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
                ink[index] = Some(draw.ink_bounds(&payloads));
            }
        }
        if clips.len() != 1 {
            return Err(unsupported(scene.commands.len(), "unclosed clip scopes"));
        }
        let prepared = std::rc::Rc::new(PreparedScene {
            draws,
            payloads,
            source,
            flattened: scene,
            root_clip,
            ink,
        });
        let storage = prepared.storage_stats();
        tracing::trace!(target:"plico::perf",draws=storage.draws,order_bytes=storage.order_bytes,payload_bytes=storage.payload_bytes,ink_bytes=storage.ink_bytes,"wgpu prepared scene storage");
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

pub(crate) struct SceneFramePlan {
    prepared: std::rc::Rc<PreparedScene>,
    size: [u32; 2],
    background: [f32; 4],
    regions: Option<Vec<[u32; 4]>>,
}
impl SceneFramePlan {
    pub(crate) fn unchanged(&self) -> bool {
        self.regions.as_ref().is_some_and(|r| r.is_empty())
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
        let plan = self.plan_scene_frame(scene, size, background, damage)?;
        if plan.unchanged() {
            return Ok(false);
        }
        self.submit_scene_plan(plan, destination, format)
            .map(|()| true)
    }

    /// Planning advances damage history. A window that fails acquisition must
    /// invalidate it; do not acquire/drop a swapchain image just to skip a frame.
    pub(crate) fn plan_scene_frame(
        &mut self,
        scene: &Scene,
        size: [u32; 2],
        background: [f32; 4],
        damage: bool,
    ) -> Result<SceneFramePlan> {
        self.validate_frame(0, size, background, 0.)?;
        let prepared = self.prepare_scene(scene, size)?;
        let regions = self.scene_regions(&prepared, size, background, damage);
        Ok(SceneFramePlan {
            prepared,
            size,
            background,
            regions,
        })
    }
    pub(crate) fn submit_scene_plan(
        &mut self,
        plan: SceneFramePlan,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) -> Result<()> {
        let result = self
            .encode_frame_regions(
                &plan.prepared.draws(),
                plan.size,
                plan.background,
                0.,
                plan.regions.as_deref(),
            )
            .and_then(|encoder| self.submit_to_view(encoder, destination, format));
        if result.is_err() {
            self.scene_damage.invalidate();
        }
        result
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
        self.validate_frame(0, size, background, 0.)?;
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

#[cfg(test)]
mod storage_profile {
    use super::*;
    #[test]
    #[ignore = "manual CPU storage/materialization experiment"]
    fn prepared_scene_storage_and_materialization() {
        use std::{hint::black_box, mem::size_of, sync::Arc, time::Instant};
        let image = peniko::ImageData {
            data: peniko::Blob::new(Arc::new(vec![255; 4])),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
            width: 1,
            height: 1,
        };
        let mut draws = Vec::new();
        let mut payloads = Payloads::default();
        for i in 0..10_000 {
            let quad = Quad::new([(i % 100) as f32, (i / 100) as f32, 1., 1.], [1.; 4]);
            if i % 10 == 0 {
                draws.push(image_draw(
                    &mut payloads,
                    crate::image::ImageQuad::new(quad, image.clone()),
                ));
            } else {
                draws.push(PreparedDraw::Quad(stash(&mut payloads.quads, quad)));
            }
        }
        let prepared = PreparedScene {
            draws,
            payloads,
            source: Scene::new(),
            flattened: Scene::new(),
            root_clip: Rect::new(0., 0., 100., 100.),
            ink: Vec::new(),
        };
        eprintln!(
            "prepared layout: order={} quad={} image={} layer={} effect={}",
            size_of::<PreparedDraw>(),
            size_of::<Quad>(),
            size_of::<crate::image::ImageQuad>(),
            size_of::<PreparedLayer>(),
            size_of::<(plico_renderer::ShaderEffect, Quad)>()
        );
        eprintln!(
            "prepared payload capacity bytes={}",
            prepared.storage_stats().order_bytes + prepared.storage_stats().payload_bytes
        );
        for _ in 0..32 {
            black_box(prepared.draws());
        }
        let mut samples = Vec::new();
        for _ in 0..32 {
            let start = Instant::now();
            for _ in 0..32 {
                black_box(prepared.draws());
            }
            samples.push(start.elapsed().as_secs_f64() * 1e6 / 32.);
        }
        samples.sort_by(f64::total_cmp);
        eprintln!(
            "10000-draw materialization us p50={:.3} p95={:.3}",
            samples[16], samples[30]
        );
    }
}
