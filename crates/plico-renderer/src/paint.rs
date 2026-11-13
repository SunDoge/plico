//! Wabou's retained 2D paint command stream.
//!
//! Vello GPU builds viewport-specific strips eagerly, so widgets cannot retain
//! and append a `vello_gpu::Scene` directly. This small command stream is the
//! stable fragment boundary owned by Wabou; the window and headless renderers
//! replay it straight into Vello GPU at the end of a frame.

use std::sync::Arc;

use kurbo::{Affine, Rect, Shape, Stroke, Vec2};
use peniko::{BlendMode, Brush, BrushRef, Color, Fill, FontData, ImageBrushRef, Style, StyleRef};

use crate::{PaintShape, ShaderEffect};

/// Immutable identity and backend payload for a GPU-produced image.
///
/// A new handle revision must be recorded whenever the producer changes the
/// pixels. This keeps retained-scene equality and damage tracking correct even
/// though the underlying GPU allocation remains stable.
#[derive(Clone)]
pub struct ExternalSurface {
    id: u64,
    revision: u64,
    dimensions: [u32; 2],
    backend: &'static str,
    resource: Arc<dyn std::any::Any + Send + Sync>,
}

impl ExternalSurface {
    /// Construct a typed backend surface handle.
    pub fn new<T: std::any::Any + Send + Sync>(
        id: u64,
        revision: u64,
        dimensions: [u32; 2],
        backend: &'static str,
        resource: Arc<T>,
    ) -> Self {
        Self {
            id,
            revision,
            dimensions,
            backend,
            resource,
        }
    }

    /// Returns the stable identity assigned by the producer.
    pub const fn id(&self) -> u64 {
        self.id
    }
    /// Returns the producer-defined content revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Returns the surface dimensions in physical pixels.
    pub const fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }
    /// Returns the renderer backend that owns this surface.
    pub const fn backend(&self) -> &'static str {
        self.backend
    }
    /// Borrows the renderer-specific resource when its concrete type matches.
    pub fn downcast_ref<T: std::any::Any>(&self) -> Option<&T> {
        self.resource.downcast_ref()
    }
}

impl std::fmt::Debug for ExternalSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExternalSurface")
            .field("id", &self.id)
            .field("revision", &self.revision)
            .field("dimensions", &self.dimensions)
            .field("backend", &self.backend)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ExternalSurface {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.revision == other.revision
            && self.dimensions == other.dimensions
            && self.backend == other.backend
            && Arc::ptr_eq(&self.resource, &other.resource)
    }
}

/// Font variation coordinate in normalized OpenType representation.
pub type NormalizedCoord = i16;

/// A positioned glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// Glyph identifier in the selected font.
    pub id: u32,
    /// Horizontal glyph origin.
    pub x: f32,
    /// Vertical glyph origin.
    pub y: f32,
}

/// Parsed SVG content retained independently of the viewport-specific Vello GPU scene.
#[derive(Clone)]
pub struct SvgDocument(pub Arc<crate::svg::usvg::Tree>);

impl std::fmt::Debug for SvgDocument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SvgDocument")
            .finish_non_exhaustive()
    }
}

impl PartialEq for SvgDocument {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// One retained drawing operation.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum PaintCommand {
    /// An independently encoded, balanced retained subtree.
    Fragment { scene: Scene, transform: Affine },
    /// A host-native ordinary paragraph in logical coordinates.
    Paragraph {
        request: Arc<crate::paragraph::ParagraphRequest>,
        transform: Affine,
    },
    /// Begin an isolated, clipped layer.
    PushLayer {
        /// Compositing mode.
        blend: BlendMode,
        /// Layer opacity.
        alpha: f32,
        /// Clip transform.
        transform: Affine,
        /// Clip outline.
        clip: PaintShape,
        /// Optional Vello GPU filter.
        filter: Option<Arc<crate::filter::Filter>>,
    },
    /// Begin a clip-only scope.
    PushClip { transform: Affine, clip: PaintShape },
    /// End the most recent layer or clip.
    PopLayer,
    /// Stroke a path.
    Stroke {
        style: Stroke,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        shape: PaintShape,
    },
    /// Fill a path.
    Fill {
        fill: Fill,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        shape: PaintShape,
    },
    /// Draw positioned glyphs.
    GlyphRun {
        font: FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: Arc<[NormalizedCoord]>,
        embolden: Vec2,
        style: Style,
        brush: Brush,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: Arc<[Glyph]>,
    },
    /// Draw a blurred rounded rectangle.
    BoxShadow {
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
        corner_smoothing: f32,
    },
    /// Paint the complement of a blurred rounded hole within bounds.
    InsetBoxShadow {
        transform: Affine,
        bounds: Rect,
        hole: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
        corner_smoothing: f32,
    },
    /// Draw an already parsed SVG document.
    Svg {
        document: SvgDocument,
        transform: Affine,
    },
    /// Draw a renderer-owned custom WGSL effect surface.
    Shader {
        effect: ShaderEffect,
        transform: Affine,
    },
    /// Draw pixels produced directly into a backend GPU resource.
    ExternalSurface {
        surface: ExternalSurface,
        transform: Affine,
        alpha: f32,
    },
}

impl PaintCommand {
    fn transformed(mut self, parent: Affine) -> Self {
        let transform = match &mut self {
            Self::Fragment { transform, .. }
            | Self::PushLayer { transform, .. }
            | Self::PushClip { transform, .. }
            | Self::Stroke { transform, .. }
            | Self::Fill { transform, .. }
            | Self::GlyphRun { transform, .. }
            | Self::BoxShadow { transform, .. }
            | Self::InsetBoxShadow { transform, .. }
            | Self::Svg { transform, .. }
            | Self::Shader { transform, .. }
            | Self::ExternalSurface { transform, .. }
            | Self::Paragraph { transform, .. } => transform,
            Self::PopLayer => return self,
        };
        *transform = parent * *transform;
        self
    }
}

/// Copy-on-write command storage: immutable scene snapshots share their commands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandBuffer(Arc<Vec<PaintCommand>>);
impl CommandBuffer {
    /// Discard shared commands without copying them first. Unique buffers keep capacity.
    pub fn clear(&mut self) {
        if let Some(commands) = Arc::get_mut(&mut self.0) {
            commands.clear();
        } else {
            self.0 = Arc::default();
        }
    }
}
impl From<Vec<PaintCommand>> for CommandBuffer {
    fn from(commands: Vec<PaintCommand>) -> Self {
        Self(Arc::new(commands))
    }
}
impl std::ops::Deref for CommandBuffer {
    type Target = Vec<PaintCommand>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for CommandBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}
impl IntoIterator for CommandBuffer {
    type Item = PaintCommand;
    type IntoIter = std::vec::IntoIter<PaintCommand>;
    fn into_iter(self) -> Self::IntoIter {
        Arc::try_unwrap(self.0)
            .unwrap_or_else(|shared| (*shared).clone())
            .into_iter()
    }
}
impl<'a> IntoIterator for &'a CommandBuffer {
    type Item = &'a PaintCommand;
    type IntoIter = std::slice::Iter<'a, PaintCommand>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Cloneable, backend-specific retained scene fragment used by Wabou widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    tolerance: f64,
    /// Recorded paint commands.
    pub commands: CommandBuffer,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            tolerance: 0.1,
            commands: CommandBuffer::default(),
        }
    }
}

impl Scene {
    /// Whether two retained snapshots share the immutable command storage.
    /// Retain the snapshot itself when caching: a bare pointer is not a version.
    pub fn shares_commands_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.commands.0, &other.commands.0)
    }

    /// Whether shaders, filters, or unclassified blends require conservative redraw.
    /// Retained fragments are inspected recursively; command equality alone does
    /// not prove that these effects are independent of external pixels or time.
    pub fn has_nonlocal_effects(&self) -> bool {
        self.commands.iter().any(|command| match command {
            PaintCommand::Fragment { scene, .. } => scene.has_nonlocal_effects(),
            PaintCommand::Shader { .. } => true,
            PaintCommand::PushLayer { filter, blend, .. } => {
                crate::damage::layer_requires_full_damage(*blend, filter.is_some())
            }
            _ => false,
        })
    }

    /// Create an empty scene fragment.
    pub fn new() -> Self {
        Self::default()
    }

    /// Draw one backend-owned GPU image. Unsupported renderers return a typed
    /// error instead of silently omitting it.
    pub fn draw_external_surface(
        &mut self,
        surface: ExternalSurface,
        transform: Affine,
        alpha: f32,
    ) {
        self.commands.push(PaintCommand::ExternalSurface {
            surface,
            transform,
            alpha,
        });
    }

    /// Expand retained subtree references into the ordinary flat command stream.
    pub fn flattened(&self) -> Self {
        let mut flat = Self::with_tolerance(self.tolerance);
        flat.append_scene(self.clone(), Affine::IDENTITY);
        flat
    }

    /// Create an empty scene with a path flattening tolerance.
    pub fn with_tolerance(tolerance: f64) -> Self {
        Self {
            tolerance,
            commands: CommandBuffer::default(),
        }
    }

    /// Record a fill whose compatible path and analytical primitive have
    /// already been resolved by the renderer. This avoids flattening an
    /// internal [`PaintShape`] into a second allocation through the generic
    /// [`Shape`] entry point.
    pub(crate) fn fill_paint_shape<'a>(
        &mut self,
        fill: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'a>>,
        brush_transform: Option<Affine>,
        shape: PaintShape,
    ) {
        self.commands.push(PaintCommand::Fill {
            fill,
            transform,
            brush: own_brush(brush.into()),
            brush_transform,
            shape,
        });
    }
}

/// Geometry shared by outer and inset shadow replay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowProfile {
    /// Uniform caster or hole radius in logical pixels.
    pub radius: f64,
    /// Gaussian standard deviation in logical pixels.
    pub std_dev: f64,
    /// Continuous-corner blend in 0..=1. Fully rounded circles/capsules keep arcs.
    pub corner_smoothing: f32,
}

/// Minimal paint sink shared by Wabou scene construction and the Vello GPU replay.
pub trait PaintScene {
    /// Remove every recorded operation.
    fn reset(&mut self);
    /// Push an isolated clipped layer.
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<Arc<crate::filter::Filter>>,
        backdrop_filter: Option<Arc<crate::filter::Filter>>,
    );
    /// Push a clip-only scope.
    fn push_clip_layer(&mut self, transform: Affine, clip: &impl Shape);
    /// Pop the most recent layer or clip.
    fn pop_layer(&mut self);
    /// Stroke a shape.
    fn stroke<'a>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    );
    /// Fill a shape.
    fn fill<'a>(
        &mut self,
        fill: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    );
    /// Draw positioned glyphs.
    #[allow(clippy::too_many_arguments)]
    fn draw_glyphs<'a, 's: 'a>(
        &'s mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: Vec2,
        style: impl Into<StyleRef<'a>>,
        brush: impl Into<BrushRef<'a>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph> + Clone,
    );

    /// Draw positioned glyphs already collected for a command.
    ///
    /// The default delegates through [`Self::draw_glyphs`]; scenes that store
    /// the glyphs in the command move the buffer in without a second collect.
    #[allow(clippy::too_many_arguments)]
    fn draw_glyphs_vec<'a, 's: 'a>(
        &'s mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: Vec2,
        style: impl Into<StyleRef<'a>>,
        brush: impl Into<BrushRef<'a>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: Vec<Glyph>,
    ) {
        self.draw_glyphs(
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
            glyphs.into_iter(),
        )
    }
    /// Draw a continuous-corner blurred box shadow.
    fn draw_box_shadow(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.draw_box_shadow_profile(
            transform,
            rect,
            brush,
            ShadowProfile {
                radius,
                std_dev,
                corner_smoothing: 1.,
            },
        );
    }
    /// Paint the blurred complement of a continuous rounded hole inside bounds.
    fn draw_inset_box_shadow(
        &mut self,
        transform: Affine,
        bounds: Rect,
        hole: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.draw_inset_box_shadow_profile(
            transform,
            bounds,
            hole,
            brush,
            ShadowProfile {
                radius,
                std_dev,
                corner_smoothing: 1.,
            },
        );
    }
    /// Draw a shadow using the authored continuous-corner profile.
    fn draw_box_shadow_profile(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        profile: ShadowProfile,
    );
    /// Draw a clipped hole complement using the authored corner profile.
    fn draw_inset_box_shadow_profile(
        &mut self,
        transform: Affine,
        bounds: Rect,
        hole: Rect,
        brush: Color,
        profile: ShadowProfile,
    );
    /// Draw a retained SVG document.
    fn draw_svg(&mut self, document: SvgDocument, transform: Affine);
    /// Draw a retained custom WGSL effect.
    fn draw_shader_effect(&mut self, effect: ShaderEffect, transform: Affine);
    /// Draw a backend-owned GPU surface.
    fn draw_external_surface(
        &mut self,
        _surface: ExternalSurface,
        _transform: Affine,
        _alpha: f32,
    ) {
        panic!("external GPU surface sent to an incompatible painter");
    }
    /// Native paragraph commands require a compatible host painter.
    fn draw_paragraph(
        &mut self,
        _request: Arc<crate::paragraph::ParagraphRequest>,
        _transform: Affine,
    ) {
        panic!("native paragraph sent to a non-native painter");
    }

    /// Append a reusable subtree. Ordinary painters may expand it immediately;
    /// the backend-neutral Scene retains it as one cheap command.
    fn append_cached_scene(&mut self, scene: Scene, transform: Affine) {
        self.append_scene(scene, transform);
    }

    /// Append a retained fragment under `transform`.
    fn append_scene(&mut self, scene: Scene, transform: Affine) {
        for command in scene.commands {
            match command {
                PaintCommand::Fragment {
                    scene: fragment,
                    transform: local,
                } => self.append_scene(fragment, transform * local),
                PaintCommand::Paragraph {
                    request,
                    transform: local,
                } => self.draw_paragraph(request, transform * local),
                PaintCommand::PushLayer {
                    blend,
                    alpha,
                    transform: local,
                    clip,
                    filter,
                } => self.push_layer(blend, alpha, transform * local, &clip, filter, None),
                PaintCommand::PushClip {
                    transform: local,
                    clip,
                } => self.push_clip_layer(transform * local, &clip),
                PaintCommand::PopLayer => self.pop_layer(),
                PaintCommand::Stroke {
                    style,
                    transform: local,
                    brush,
                    brush_transform,
                    shape,
                } => self.stroke(&style, transform * local, &brush, brush_transform, &shape),
                PaintCommand::Fill {
                    fill,
                    transform: local,
                    brush,
                    brush_transform,
                    shape,
                } => self.fill(fill, transform * local, &brush, brush_transform, &shape),
                PaintCommand::GlyphRun {
                    font,
                    font_size,
                    hint,
                    normalized_coords,
                    embolden,
                    style,
                    brush,
                    brush_alpha,
                    transform: local,
                    glyph_transform,
                    glyphs,
                } => self.draw_glyphs(
                    &font,
                    font_size,
                    hint,
                    &normalized_coords,
                    embolden,
                    &style,
                    &brush,
                    brush_alpha,
                    transform * local,
                    glyph_transform,
                    glyphs.iter().copied(),
                ),
                PaintCommand::BoxShadow {
                    transform: local,
                    rect,
                    brush,
                    radius,
                    std_dev,
                    corner_smoothing,
                } => self.draw_box_shadow_profile(
                    transform * local,
                    rect,
                    brush,
                    ShadowProfile {
                        radius,
                        std_dev,
                        corner_smoothing,
                    },
                ),
                PaintCommand::InsetBoxShadow {
                    transform: local,
                    bounds,
                    hole,
                    brush,
                    radius,
                    std_dev,
                    corner_smoothing,
                } => {
                    self.draw_inset_box_shadow_profile(
                        transform * local,
                        bounds,
                        hole,
                        brush,
                        ShadowProfile {
                            radius,
                            std_dev,
                            corner_smoothing,
                        },
                    );
                }
                PaintCommand::Svg {
                    document,
                    transform: local,
                } => self.draw_svg(document, transform * local),
                PaintCommand::Shader {
                    effect,
                    transform: local,
                } => self.draw_shader_effect(effect, transform * local),
                PaintCommand::ExternalSurface {
                    surface,
                    transform: local,
                    alpha,
                } => self.draw_external_surface(surface, transform * local, alpha),
            }
        }
    }

    /// Draw an image at its intrinsic size.
    fn draw_image(&mut self, image: ImageBrushRef<'_>, transform: Affine) {
        let bounds = Rect::new(
            0.0,
            0.0,
            f64::from(image.image.width),
            f64::from(image.image.height),
        );
        self.fill(Fill::NonZero, transform, image, None, &bounds);
    }
}

fn own_brush(brush: BrushRef<'_>) -> Brush {
    match brush {
        BrushRef::Solid(color) => Brush::Solid(color),
        BrushRef::Gradient(gradient) => Brush::Gradient(gradient.clone()),
        BrushRef::Image(image) => Brush::Image(image.to_owned()),
    }
}

impl PaintScene for Scene {
    fn draw_paragraph(
        &mut self,
        request: Arc<crate::paragraph::ParagraphRequest>,
        transform: Affine,
    ) {
        self.commands
            .push(PaintCommand::Paragraph { request, transform });
    }
    fn reset(&mut self) {
        self.commands.clear();
    }

    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
        filter: Option<Arc<crate::filter::Filter>>,
        _backdrop_filter: Option<Arc<crate::filter::Filter>>,
    ) {
        self.commands.push(PaintCommand::PushLayer {
            blend: blend.into(),
            alpha,
            transform,
            clip: PaintShape::record(clip, self.tolerance),
            filter,
        });
    }

    fn push_clip_layer(&mut self, transform: Affine, clip: &impl Shape) {
        self.commands.push(PaintCommand::PushClip {
            transform,
            clip: PaintShape::record(clip, self.tolerance),
        });
    }

    fn pop_layer(&mut self) {
        self.commands.push(PaintCommand::PopLayer);
    }

    fn stroke<'a>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.commands.push(PaintCommand::Stroke {
            style: style.clone(),
            transform,
            brush: own_brush(brush.into()),
            brush_transform,
            shape: PaintShape::record(shape, self.tolerance),
        });
    }

    fn fill<'a>(
        &mut self,
        fill: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'a>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.commands.push(PaintCommand::Fill {
            fill,
            transform,
            brush: own_brush(brush.into()),
            brush_transform,
            shape: PaintShape::record(shape, self.tolerance),
        });
    }

    fn draw_glyphs<'a, 's: 'a>(
        &'s mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: Vec2,
        style: impl Into<StyleRef<'a>>,
        brush: impl Into<BrushRef<'a>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph> + Clone,
    ) {
        self.draw_glyphs_vec(
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
            glyphs.collect(),
        );
    }
    fn draw_glyphs_vec<'a, 's: 'a>(
        &'s mut self,
        font: &'a FontData,
        font_size: f32,
        hint: bool,
        normalized_coords: &'a [NormalizedCoord],
        embolden: Vec2,
        style: impl Into<StyleRef<'a>>,
        brush: impl Into<BrushRef<'a>>,
        brush_alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: Vec<Glyph>,
    ) {
        self.commands.push(PaintCommand::GlyphRun {
            font: font.clone(),
            font_size,
            hint,
            normalized_coords: Arc::from(normalized_coords),
            embolden,
            style: style.into().to_owned(),
            brush: own_brush(brush.into()),
            brush_alpha,
            transform,
            glyph_transform,
            glyphs: glyphs.into(),
        });
    }

    fn draw_box_shadow_profile(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        profile: ShadowProfile,
    ) {
        self.commands.push(PaintCommand::BoxShadow {
            transform,
            rect,
            brush,
            radius: profile.radius,
            std_dev: profile.std_dev,
            corner_smoothing: profile.corner_smoothing,
        });
    }
    fn draw_inset_box_shadow_profile(
        &mut self,
        transform: Affine,
        bounds: Rect,
        hole: Rect,
        brush: Color,
        profile: ShadowProfile,
    ) {
        self.commands.push(PaintCommand::InsetBoxShadow {
            transform,
            bounds,
            hole,
            brush,
            radius: profile.radius,
            std_dev: profile.std_dev,
            corner_smoothing: profile.corner_smoothing,
        });
    }

    fn draw_svg(&mut self, document: SvgDocument, transform: Affine) {
        self.commands.push(PaintCommand::Svg {
            document,
            transform,
        });
    }

    fn append_scene(&mut self, scene: Scene, transform: Affine) {
        for command in scene.commands {
            match command {
                PaintCommand::Fragment {
                    scene: fragment,
                    transform: local,
                } => self.append_scene(fragment, transform * local),
                command => self.commands.push(command.transformed(transform)),
            }
        }
    }

    fn append_cached_scene(&mut self, scene: Scene, transform: Affine) {
        if !scene.commands.is_empty() {
            self.commands
                .push(PaintCommand::Fragment { scene, transform });
        }
    }

    fn draw_shader_effect(&mut self, effect: ShaderEffect, transform: Affine) {
        self.commands
            .push(PaintCommand::Shader { effect, transform });
    }

    fn draw_external_surface(&mut self, surface: ExternalSurface, transform: Affine, alpha: f32) {
        Scene::draw_external_surface(self, surface, transform, alpha);
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn shadow_convenience_methods_follow_the_continuous_default() {
        let mut scene = Scene::new();
        scene.draw_box_shadow(
            Affine::IDENTITY,
            Rect::new(0., 0., 40., 24.),
            Color::BLACK,
            6.,
            2.,
        );
        scene.draw_inset_box_shadow(
            Affine::IDENTITY,
            Rect::new(0., 0., 40., 24.),
            Rect::new(2., 2., 38., 22.),
            Color::BLACK,
            4.,
            2.,
        );
        assert!(matches!(
            scene.commands[0],
            PaintCommand::BoxShadow {
                corner_smoothing: 1.,
                ..
            }
        ));
        assert!(matches!(
            scene.commands[1],
            PaintCommand::InsetBoxShadow {
                corner_smoothing: 1.,
                ..
            }
        ));
    }
    #[test]
    fn cloned_scenes_share_commands_and_mutation_preserves_the_snapshot() {
        let mut scene = Scene::new();
        scene.commands.push(PaintCommand::PopLayer);
        let snapshot = scene.clone();
        assert_eq!(scene.commands.as_ptr(), snapshot.commands.as_ptr());
        scene.commands.push(PaintCommand::PopLayer);
        assert_eq!(snapshot.commands.len(), 1);
        assert_eq!(scene.commands.len(), 2);
        assert_ne!(scene.commands.as_ptr(), snapshot.commands.as_ptr());
        let moved: Vec<_> = snapshot.commands.into_iter().collect();
        assert_eq!(moved.len(), 1);
    }
    #[test]
    fn resetting_a_shared_scene_discards_commands_without_copying_them() {
        let mut scene = Scene::new();
        scene.commands.reserve(4096);
        scene.commands.push(PaintCommand::PopLayer);
        let snapshot = scene.clone();
        scene.reset();
        assert!(scene.commands.is_empty());
        assert_eq!(scene.commands.capacity(), 0);
        assert_eq!(snapshot.commands.len(), 1);
        assert!(snapshot.commands.capacity() >= 4096);
        // Consuming one of two snapshots must preserve the other.
        let another = snapshot.clone();
        assert_eq!(snapshot.commands.into_iter().count(), 1);
        assert_eq!(another.commands.len(), 1);
    }
}
