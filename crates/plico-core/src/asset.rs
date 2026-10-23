/// An explicitly embedded SVG. No asset directory scan or global registry is required.
#[derive(Clone, Copy, Debug)]
pub struct SvgAsset {
    source: &'static str,
}
impl SvgAsset {
    pub const fn new(source: &'static str) -> Self {
        Self { source }
    }
    pub const fn source(self) -> &'static str {
        self.source
    }
}
/// Embed one SVG relative to the calling source file, using Rust's include_str!.
#[macro_export]
macro_rules! svg_asset {
    ($path:literal) => {
        $crate::SvgAsset::new(include_str!($path))
    };
}
