use super::*;

impl Renderer {
    /// CPU raster cache entries and payload bytes, in path/SVG/clip-mask order.
    /// Excludes scene-owned source assets and externally retained prepared scenes.
    pub fn raster_cache_stats(&self) -> [(usize, usize); 3] {
        [self.paths.stats(), self.svgs.stats(), self.masks.stats()]
    }
    /// Release CPU path/SVG/clip rasters and the local prepared-scene snapshot.
    /// Retained final pixels remain valid; subsequent changes rebuild derived data.
    pub fn trim_raster_cache(&mut self) {
        self.clear_frame_references();
        self.prepared_cache.clear();
        self.paths = Default::default();
        self.svgs = Default::default();
        self.masks = Default::default();
    }
    /// Release immutable-image GPU textures and bindings, preserving cumulative
    /// counters and retained final pixels. Source images remain owned by the scene.
    pub fn trim_image_cache(&mut self) {
        self.clear_frame_images();
        self.images.clear();
    }
    /// Cached source textures, resident texture bytes, uploads and source hits.
    /// Sampling bindings are counted separately; native atlas textures are excluded.
    pub fn image_cache_stats(&self) -> (usize, usize, u64, u64) {
        self.images.stats()
    }
    /// Resident immutable-image sampling bindings and cumulative creations. Several
    /// bindings can share one source texture without another upload.
    pub fn image_binding_stats(&self) -> (usize, u64) {
        self.images.binding_stats()
    }
}
