//! Retained system-shaped layouts, sampled at the physical pixel scale they were
//! rasterized for. No paragraph bitmap scaling and no geometric emboldening.
pub use plico_native::text::{TextRequest, atlas::GlyphLayout};
/// One text draw. Origin/clip are physical pixels; layout metrics remain logical.
#[derive(Clone, Copy)]
pub struct TextRun<'a> {
    pub layout: &'a GlyphLayout,
    pub origin: [f64; 2],
    pub color: [f32; 4],
    pub clip: Option<[f32; 4]>,
    /// Default false: four horizontal phases and pixel-snapped vertical placement.
    /// True permits continuous bitmap movement, at the cost of sampling softness.
    pub continuous: bool,
}
impl<'a> TextRun<'a> {
    pub fn new(layout: &'a GlyphLayout, origin: [f64; 2], color: [f32; 4]) -> Self {
        Self {
            layout,
            origin,
            color,
            clip: None,
            continuous: false,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct TextStats {
    pub cached_glyphs: usize,
    pub atlas_cpu_bytes: usize,
    pub atlas_gpu_bytes: usize,
    pub uploaded_bytes: u64,
    pub uploaded_regions: u64,
    pub recycled_frames: u64,
}
