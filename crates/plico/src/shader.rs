//! Portable authored shader parameters; GPU validation and resources belong to the host.
#[derive(Clone, Copy, Debug)]
pub struct ShaderAsset {
    source: &'static str,
    source_path: Option<&'static str>,
    module: bool,
    sksl: Option<&'static str>,
}
impl ShaderAsset {
    pub const fn new(source: &'static str) -> Self {
        Self {
            source,
            source_path: None,
            module: false,
            sksl: None,
        }
    }
    pub const fn module(source: &'static str) -> Self {
        Self {
            source,
            source_path: None,
            module: true,
            sksl: None,
        }
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
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    /// Authored resource path, used only to label validation diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub values: Vec<f32>,
    pub sksl: Option<String>,
    pub time: f32,
    pub speed: f32,
    pub animated: bool,
    pub paused: bool,
    pub module: bool,
}
impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            source_path: None,
            values: Vec::new(),
            sksl: None,
            time: 0.,
            speed: 1.,
            animated: false,
            paused: false,
            module: false,
        }
    }
}
impl ShaderParams {
    pub fn valid(&self) -> bool {
        self.values.len() <= 256
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
