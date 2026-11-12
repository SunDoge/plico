//! Portable authored shader parameters; GPU validation and resources belong to the host.
/// Framework-owned shader whose host contract includes behavior beyond raw WGSL.
/// Application shaders remain ordinary [`ShaderAsset`] values with no builtin tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ShaderBuiltin {
    /// Plico's MyGO-style liquid-glass material. The host resolves its fixed
    /// Gaussian sigma once from the final surface size and material values.
    LiquidGlass,
}

#[derive(Clone, Copy, Debug)]
pub struct ShaderAsset {
    source: &'static str,
    source_path: Option<&'static str>,
    module: bool,
    sksl: Option<&'static str>,
    /// The effect samples what is painted behind it (mygo's scene.Effect
    /// Backdrop): the host binds the region's rendered content as an input.
    reads_backdrop: bool,
    prefilter_backdrop: Option<f32>,
    builtin: Option<ShaderBuiltin>,
}
impl ShaderAsset {
    pub const fn new(source: &'static str) -> Self {
        Self {
            source,
            source_path: None,
            module: false,
            sksl: None,
            reads_backdrop: false,
            prefilter_backdrop: None,
            builtin: None,
        }
    }
    pub const fn module(source: &'static str) -> Self {
        Self {
            source,
            source_path: None,
            module: true,
            sksl: None,
            reads_backdrop: false,
            prefilter_backdrop: None,
            builtin: None,
        }
    }
    /// Declare that the effect's `plico_effect` calls `plico.backdrop_at(uv)`.
    /// Backdrop readers see everything painted before them. Hosts with bounded
    /// input dependencies may preserve unchanged pixels; unknown consumers keep
    /// conservative full damage (mygo's scene.Effect.Backdrop).
    pub const fn read_backdrop(mut self) -> Self {
        self.reads_backdrop = true;
        self
    }
    /// Request area-prefiltered levels up to a maximum logical-pixel sigma.
    /// This also enables backdrop reading. The host owns all transient levels.
    pub const fn prefilter_backdrop(mut self, max_sigma: f32) -> Self {
        self.reads_backdrop = true;
        self.prefilter_backdrop = Some(max_sigma);
        self
    }
    pub const fn backdrop_prefiltered(self) -> Option<f32> {
        self.prefilter_backdrop
    }
    pub(crate) const fn builtin(mut self, builtin: ShaderBuiltin) -> Self {
        self.builtin = Some(builtin);
        self
    }
    pub const fn builtin_kind(self) -> Option<ShaderBuiltin> {
        self.builtin
    }
    pub const fn reads_backdrop(self) -> bool {
        self.reads_backdrop
    }
    /// Attach the authored resource path for tooling and diagnostics.
    /// This is metadata only; it does not read or watch the file at runtime.
    pub const fn with_source_path(mut self, path: &'static str) -> Self {
        self.source_path = Some(path);
        self
    }
    pub const fn source_path(self) -> Option<&'static str> {
        self.source_path
    }
    /// Add a Skia implementation defining `half4 plico_effect(float2 uv)`.
    pub const fn with_sksl(mut self, source: &'static str) -> Self {
        self.sksl = Some(source);
        self
    }
    pub const fn sksl(self) -> Option<&'static str> {
        self.sksl
    }
    pub const fn source(self) -> &'static str {
        self.source
    }
    pub const fn is_module(self) -> bool {
        self.module
    }
}
/// Embed a WGSL effect function, or a complete module with `module, "path.wgsl"`.
/// Paths are relative to the invoking Rust source file, as with `include_str!`.
#[macro_export]
macro_rules! shader_asset {
    (module, $path:literal) => {
        $crate::ShaderAsset::module(include_str!($path)).with_source_path($path)
    };
    ($path:literal) => {
        $crate::ShaderAsset::new(include_str!($path)).with_source_path($path)
    };
}
/// A named WGSL scalar/vector parameter. Layout is resolved by the host from
/// WGSL; Rust struct memory and padding are never uploaded as uniforms.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ShaderValue {
    /// One WGSL f32 scalar.
    Float(f32),
    /// One WGSL vec2f.
    Vec2([f32; 2]),
    /// One WGSL vec3f; adjacent fields retain their own WGSL offsets.
    Vec3([f32; 3]),
    /// One WGSL vec4f.
    Vec4([f32; 4]),
}
impl ShaderValue {
    /// Consecutive components, excluding any WGSL layout padding.
    pub fn components(&self) -> &[f32] {
        match self {
            Self::Float(v) => std::slice::from_ref(v),
            Self::Vec2(v) => v,
            Self::Vec3(v) => v,
            Self::Vec4(v) => v,
        }
    }
}
impl From<f32> for ShaderValue {
    fn from(v: f32) -> Self {
        Self::Float(v)
    }
}
impl From<[f32; 2]> for ShaderValue {
    fn from(v: [f32; 2]) -> Self {
        Self::Vec2(v)
    }
}
impl From<[f32; 3]> for ShaderValue {
    fn from(v: [f32; 3]) -> Self {
        Self::Vec3(v)
    }
}
impl From<[f32; 4]> for ShaderValue {
    fn from(v: [f32; 4]) -> Self {
        Self::Vec4(v)
    }
}
/// A retained, clock-driven transition for one scalar shader value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShaderTransition {
    pub index: usize,
    pub milliseconds: u16,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    /// Authored resource path, used only to label validation diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub values: Vec<f32>,
    /// Scalar transitions sampled by the runtime, without redeclaring the view.
    pub transitions: Vec<ShaderTransition>,
    /// Symmetric paint overflow in logical pixels; layout and hit boxes are unchanged.
    pub paint_outset: [f32; 2],
    /// Complete-module top-level f32/vector fields, resolved by WGSL name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<(String, ShaderValue)>,
    pub sksl: Option<String>,
    pub time: f32,
    pub speed: f32,
    pub animated: bool,
    pub paused: bool,
    pub module: bool,
    /// The effect samples its backdrop through `plico.backdrop_at`.
    pub reads_backdrop: bool,
    /// Maximum logical-pixel sigma for an area-prefiltered backdrop, or None.
    pub prefilter_backdrop: Option<f32>,
    /// Framework-owned shader behavior. Custom shaders should leave this unset.
    #[doc(hidden)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin: Option<ShaderBuiltin>,
}
impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            source_path: None,
            values: Vec::new(),
            transitions: Vec::new(),
            paint_outset: [0.; 2],
            parameters: Vec::new(),
            sksl: None,
            time: 0.,
            speed: 1.,
            animated: false,
            paused: false,
            module: false,
            reads_backdrop: false,
            prefilter_backdrop: None,
            builtin: None,
        }
    }
}
impl ShaderParams {
    pub fn valid(&self) -> bool {
        self.prefilter_backdrop.is_none_or(|sigma| {
            self.reads_backdrop
                && !self.module
                && sigma.is_finite()
                && (0. ..=128.).contains(&sigma)
        }) && self
            .builtin
            .is_none_or(|_| self.reads_backdrop && !self.module)
            && self
                .paint_outset
                .iter()
                .all(|v| v.is_finite() && (0. ..=64.).contains(v))
            && self.transitions.len() <= 256
            && self.transitions.iter().enumerate().all(|(i, t)| {
                t.index < 256
                    && !self.transitions[..i]
                        .iter()
                        .any(|previous| previous.index == t.index)
            })
            && self.parameters.len() <= 256
            && self.parameters.iter().all(|(name, value)| {
                !name.is_empty()
                    && name.len() <= 1024
                    && value.components().iter().all(|v| v.is_finite())
            })
            && self.values.len() <= 256
            && self.values.iter().all(|v| v.is_finite())
            && self.time.is_finite()
            && self.speed.is_finite()
            && self.speed >= 0.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use this source file to check embedding/metadata without introducing a GPU
    // dependency into the portable UI crate. Validation belongs to the host.
    const EFFECT: ShaderAsset = crate::shader_asset!("shader.rs");
    const MODULE: ShaderAsset = crate::shader_asset!(module, "shader.rs");

    #[test]
    fn named_parameters_round_trip_and_old_configs_default_to_empty() {
        let old: ShaderParams = serde_json::from_str("{}").unwrap();
        assert!(old.parameters.is_empty());
        let mut params = ShaderParams {
            parameters: vec![("color".into(), [0.1, 0.2, 0.3].into())],
            ..Default::default()
        };
        assert!(params.valid());
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(serde_json::from_str::<ShaderParams>(&json).unwrap(), params);
        params.parameters[0].1 = [0., f32::NAN, 1.].into();
        assert!(!params.valid());
    }
    #[test]
    fn backdrop_prefilter_is_opt_in_and_requires_a_function_backdrop() {
        assert_eq!(ShaderAsset::new("source").backdrop_prefiltered(), None);
        let asset = ShaderAsset::new("source").prefilter_backdrop(8.);
        assert!(asset.reads_backdrop());
        assert_eq!(asset.backdrop_prefiltered(), Some(8.));
        assert_eq!(ShaderParams::default().prefilter_backdrop, None);
        for sigma in [-1., f32::NAN, f32::INFINITY, 129.] {
            assert!(
                !ShaderParams {
                    reads_backdrop: true,
                    prefilter_backdrop: Some(sigma),
                    ..Default::default()
                }
                .valid()
            );
        }
        assert!(
            !ShaderParams {
                prefilter_backdrop: Some(8.),
                ..Default::default()
            }
            .valid()
        );
        assert!(
            !ShaderParams {
                builtin: Some(ShaderBuiltin::LiquidGlass),
                ..Default::default()
            }
            .valid()
        );
        assert!(
            ShaderParams {
                prefilter_backdrop: Some(8.),
                reads_backdrop: true,
                ..Default::default()
            }
            .valid()
        );
        assert!(
            !ShaderParams {
                prefilter_backdrop: Some(8.),
                reads_backdrop: true,
                module: true,
                ..Default::default()
            }
            .valid()
        );
    }
    #[test]
    fn embedded_assets_preserve_source_and_interface() {
        assert_eq!(EFFECT.source(), include_str!("shader.rs"));
        assert_eq!(MODULE.source(), EFFECT.source());
        assert_eq!(EFFECT.source_path(), Some("shader.rs"));
        assert_eq!(MODULE.source_path(), EFFECT.source_path());
        assert!(!EFFECT.is_module());
        assert!(MODULE.is_module());
        assert_eq!(ShaderAsset::new("inline").source_path(), None);
        assert_eq!(MODULE.with_sksl("skia").source_path(), MODULE.source_path());
    }
}

#[cfg(test)]
mod transition_validation {
    use super::*;
    #[test]
    fn shader_transition_metadata_is_bounded_and_legacy_configs_stay_static() {
        let mut params: ShaderParams = serde_json::from_str("{}").unwrap();
        assert!(params.transitions.is_empty());
        assert_eq!(params.paint_outset, [0.; 2]);
        params.transitions = vec![ShaderTransition {
            index: 0,
            milliseconds: 120,
        }];
        params.paint_outset = [2., 2.];
        assert!(params.valid());
        params.transitions.push(params.transitions[0]);
        assert!(!params.valid());
        params.transitions.pop();
        params.paint_outset[0] = f32::NAN;
        assert!(!params.valid());
        params.paint_outset = [65., 0.];
        assert!(!params.valid());
    }
}
