//! Portable authored shader parameters; GPU validation and resources belong to the host.
#[derive(Clone, Copy, Debug)]
pub struct ShaderAsset {
    source: &'static str,
    module: bool,
}
impl ShaderAsset {
    pub const fn new(source: &'static str) -> Self {
        Self {
            source,
            module: false,
        }
    }
    pub const fn module(source: &'static str) -> Self {
        Self {
            source,
            module: true,
        }
    }
    pub const fn source(self) -> &'static str {
        self.source
    }
    pub const fn is_module(self) -> bool {
        self.module
    }
}
#[macro_export]
macro_rules! shader_asset {
    ($path:literal) => {
        $crate::ShaderAsset::new(include_str!($path))
    };
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShaderParams {
    pub values: Vec<f32>,
    pub time: f32,
    pub speed: f32,
    pub animated: bool,
    pub paused: bool,
    pub module: bool,
}
impl Default for ShaderParams {
    fn default() -> Self {
        Self {
            values: Vec::new(),
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
