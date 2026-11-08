//! Renderer-independent material declarations and reusable elevation tokens.
//! GPU resource ownership stays in the renderer. Enabled glass requires a backend
//! that supports WGSL backdrop effects (currently wgpu); disabled glass is opaque.
use crate::{
    BoxShadow, Color, Style, UiContext,
    widgets::{column, shader},
};

static LIQUID_GLASS: crate::ShaderAsset =
    crate::shader_asset!("../assets/materials/liquid_glass.wgsl").read_backdrop();

/// Embedded material source for renderer validation and backend conformance tests.
/// Applications should normally use `Layout::material` instead of raw uniforms.
pub const fn liquid_glass_shader() -> crate::ShaderAsset {
    LIQUID_GLASS
}

/// A scene-backed liquid glass material. Surface content is painted afterward,
/// so text and controls remain sharp. The container's radius supplies its shape.
///
/// ```
/// use plico::{Engine, prelude::*};
/// Engine::default().frame_declarations(|cx| {
///     column(cx).width(240.).padding(20).radius(18.)
///         .material(LiquidGlass::default()).elevation(Elevation::PANEL)
///         .children(|cx| { text(cx, "On glass"); });
/// }).unwrap();
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidGlass {
    pub enabled: bool,
    /// Blur amount in 0..=1.
    pub frost: f32,
    pub tint: Color,
    /// Amount of the tint in 0..=1, independent of content opacity.
    pub tint_opacity: f32,
    /// Edge displacement relative to the bevel width, in 0..=4.
    pub refraction: f32,
    pub saturation: f32,
    /// Lit hairline intensity in 0..=1.
    pub rim: f32,
    /// Curved edge width in logical pixels, capped to half the pane's short side.
    pub bevel: f32,
    pub animated: bool,
    /// Stops reflection animation without changing the material or its shape.
    pub reduce_motion: bool,
    /// Opaque replacement when enabled is false; no shader is declared.
    pub fallback: Color,
}
impl Default for LiquidGlass {
    fn default() -> Self {
        Self {
            enabled: true,
            frost: 0.42,
            tint: Color::Literal(0xf9fcff),
            tint_opacity: 0.26,
            refraction: 1.6,
            saturation: 1.12,
            rim: 0.5,
            bevel: 48.,
            animated: false,
            reduce_motion: false,
            fallback: Color::Literal(0xf9fcff),
        }
    }
}
impl LiquidGlass {
    /// Clamp finite values to supported ranges; non-finite values use defaults.
    pub fn validated(self) -> Self {
        let defaults = Self::default();
        let bounded = |value: f32, max: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(0., max)
            } else {
                fallback
            }
        };
        Self {
            frost: bounded(self.frost, 1., defaults.frost),
            tint_opacity: bounded(self.tint_opacity, 1., defaults.tint_opacity),
            refraction: bounded(self.refraction, 4., defaults.refraction),
            saturation: bounded(self.saturation, 3., defaults.saturation),
            rim: bounded(self.rim, 1., defaults.rim),
            bevel: bounded(self.bevel, 128., defaults.bevel),
            ..self
        }
    }
    #[crate::component]
    pub(crate) fn show(self, cx: &mut UiContext, radius: f32, corner_smoothing: f32) {
        let options = self.validated();
        if options.enabled {
            let animated = options.animated && !options.reduce_motion;
            let tint = cx.theme().resolve(options.tint);
            shader(cx, &LIQUID_GLASS)
                .absolute(0., 0.)
                .radius(0.)
                .width_percent(100.)
                .height_percent(100.)
                .values(&[
                    radius,
                    options.frost,
                    options.tint_opacity,
                    f32::from(animated),
                    options.refraction,
                    options.saturation,
                    options.rim,
                    options.bevel,
                    ((tint >> 16) & 255) as f32 / 255.,
                    ((tint >> 8) & 255) as f32 / 255.,
                    (tint & 255) as f32 / 255.,
                    0.,
                    corner_smoothing,
                ])
                .animated(animated);
        } else {
            column(cx)
                .absolute(0., 0.)
                .width_percent(100.)
                .height_percent(100.)
                .radius(radius)
                .corner_smoothing(corner_smoothing)
                .background(options.fallback)
                .border(1., Color::Border)
                .end();
        }
    }
}

/// A reusable pair of outer shadows. Ambient is painted first, contact second.
/// Neither shadow darkens the pane interior. Override the colors or individual
/// BoxShadow values when the application's visual language calls for it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Elevation {
    pub contact: Option<BoxShadow>,
    pub ambient: Option<BoxShadow>,
}
const fn shadow(y: f32, blur: f32, spread: f32, opacity: f32, color: u32) -> BoxShadow {
    BoxShadow {
        offset_x: 0.,
        offset_y: y,
        blur,
        spread,
        opacity,
        color: Color::Literal(color),
    }
}
impl Elevation {
    pub const NONE: Self = Self {
        contact: None,
        ambient: None,
    };
    pub const SUBTLE: Self = Self {
        contact: Some(shadow(1., 6., 0., 0.06, 0)),
        ambient: None,
    };
    pub const CONTROL: Self = Self {
        contact: Some(shadow(1., 3., 0., 0.12, 0)),
        ambient: Some(shadow(4., 10., -2., 0.14, 0)),
    };
    pub const PANEL: Self = Self {
        contact: Some(shadow(2., 6., -2., 0.11, 0)),
        ambient: Some(shadow(10., 24., -6., 0.16, 0x344d63)),
    };
    pub const FLOATING: Self = Self {
        contact: Some(shadow(2., 5., -1., 0.10, 0)),
        ambient: Some(shadow(8., 20., -4., 0.16, 0)),
    };
    pub const OVERLAY: Self = Self {
        contact: Some(shadow(2., 6., -2., 0.10, 0)),
        ambient: Some(shadow(10., 28., -6., 0.18, 0)),
    };
    pub(crate) fn apply(self, style: &mut Style) {
        style.shadow = self.contact;
        style.ambient_shadow = self.ambient;
    }
}

/// Side whose scene pixels receive the strongest progressive blur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollEdgeSide {
    Top,
    Bottom,
    Left,
    Right,
}
/// App-local scene blur/tint. Does not read desktop or other-window pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackdropBlur {
    pub radius: f32,
    pub tint: Color,
    pub tint_opacity: f32,
}
impl Default for BackdropBlur {
    fn default() -> Self {
        Self {
            radius: 8.,
            tint: Color::Literal(0xffffff),
            tint_opacity: 0.,
        }
    }
}
impl BackdropBlur {
    pub fn validated(self) -> Self {
        Self {
            radius: if self.radius.is_finite() {
                self.radius.clamp(0., 64.)
            } else {
                8.
            },
            tint_opacity: if self.tint_opacity.is_finite() {
                self.tint_opacity.clamp(0., 1.)
            } else {
                0.
            },
            ..self
        }
    }
}
static BACKDROP_BLUR: crate::ShaderAsset =
    crate::shader_asset!("../assets/materials/backdrop_blur.wgsl").read_backdrop();
/// Shared source for renderer and managed compatibility entry points.
pub const fn backdrop_blur_shader() -> crate::ShaderAsset {
    BACKDROP_BLUR
}
/// Noninteractive uniform scene blur. Position/size it over existing content,
/// before the sharp foreground that should remain unblurred.
pub fn backdrop_blur<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    options: BackdropBlur,
) -> crate::Shader<'u, 'e, 'static> {
    blur_surface(cx, options, 0.)
}
/// A noninteractive progressive scene blur; the opposite edge is unchanged.
pub fn scroll_edge<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    side: ScrollEdgeSide,
    options: BackdropBlur,
) -> crate::Shader<'u, 'e, 'static> {
    blur_surface(
        cx,
        options,
        match side {
            ScrollEdgeSide::Top => 1.,
            ScrollEdgeSide::Bottom => 2.,
            ScrollEdgeSide::Left => 3.,
            ScrollEdgeSide::Right => 4.,
        },
    )
}
fn blur_surface<'u, 'e>(
    cx: &'u mut UiContext<'e>,
    options: BackdropBlur,
    mode: f32,
) -> crate::Shader<'u, 'e, 'static> {
    let options = options.validated();
    let tint = cx.theme().resolve(options.tint);
    shader(cx, &BACKDROP_BLUR).values(&[
        options.radius,
        mode,
        options.tint_opacity,
        0.,
        ((tint >> 16) & 255) as f32 / 255.,
        ((tint >> 8) & 255) as f32 / 255.,
        (tint & 255) as f32 / 255.,
        0.,
    ])
}
