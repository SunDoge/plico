//! Portable authored shader parameters; GPU validation and resources belong to the host.
#[derive(Clone, Copy, Debug)]
pub struct ShaderAsset {
    source: &'static str,
    source_path: Option<&'static str>,
    module: bool,
    sksl: Option<&'static str>,
    /// The effect samples what is painted behind it (mygo's scene.Effect
    /// Backdrop): the host binds the region's rendered content as an input.
    reads_backdrop: bool,
}
impl ShaderAsset {
    pub const fn new(source: &'static str) -> Self {
        Self {
            source,
            source_path: None,
            module: false,
            sksl: None,
            reads_backdrop: false,
        }
    }
    pub const fn module(source: &'static str) -> Self {
        Self {
            source,
            source_path: None,
            module: true,
            sksl: None,
            reads_backdrop: false,
        }
    }
    /// Declare that the effect's `plico_effect` calls `plico.backdrop_at(uv)`.
    /// Backdrop readers see everything painted before them and cannot be
    /// damage-skipped; hosts apply full-damage semantics (mygo's
    /// scene.Effect.Backdrop).
    pub const fn read_backdrop(mut self) -> Self {
        self.reads_backdrop = true;
        self
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    /// Authored resource path, used only to label validation diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub values: Vec<f32>,
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
}
impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            source_path: None,
            values: Vec::new(),
            parameters: Vec::new(),
            sksl: None,
            time: 0.,
            speed: 1.,
            animated: false,
            paused: false,
            module: false,
            reads_backdrop: false,
        }
    }
}
impl ShaderParams {
    pub fn valid(&self) -> bool {
        self.parameters.len() <= 256
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
