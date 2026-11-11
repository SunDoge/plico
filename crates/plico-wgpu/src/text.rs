//! Retained system-shaped layouts, sampled at the physical pixel scale they were
//! rasterized for. No paragraph bitmap scaling and no geometric emboldening.
pub use plico_native::text::{TextRequest, atlas::GlyphLayout};
/// One text draw. Origin/clip are physical pixels; layout metrics remain logical.
#[derive(Clone, Copy)]
pub struct TextRun<'a> {
    pub layout: &'a GlyphLayout,
    pub origin: [f64; 2],
    /// Presentation-only scale of retained native coverage; defaults to exact native size.
    pub scale: f64,
    pub color: [f32; 4],
    pub clip: Option<[f32; 4]>,
    pub clip_radii: [f32; 4],
    pub clip_antialias: bool,
    /// Default false: four horizontal phases and pixel-snapped vertical placement.
    /// True permits continuous bitmap movement, at the cost of sampling softness.
    pub continuous: bool,
}
impl<'a> TextRun<'a> {
    pub fn new(layout: &'a GlyphLayout, origin: [f64; 2], color: [f32; 4]) -> Self {
        Self {
            layout,
            origin,
            scale: 1.,
            color,
            clip: None,
            clip_radii: [0.; 4],
            clip_antialias: false,
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
