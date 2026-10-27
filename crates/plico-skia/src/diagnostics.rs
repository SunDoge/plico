//! Profiling-only lifetime counters. Cache IDs distinguish independent contexts.
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct CacheCounters {
    pub id: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub invalidations: u64,
}
impl Default for CacheCounters {
    fn default() -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            hits: 0,
            misses: 0,
            evictions: 0,
            invalidations: 0,
        }
    }
}

#[derive(Default)]
pub(crate) struct ReplayWork {
    pub visited: usize,
    pub culled: usize,
    pub paragraphs: usize,
    pub glyphs: usize,
    pub image_brushes: usize,
    pub layers: usize,
    pub svgs: usize,
}
impl ReplayWork {
    pub fn replayed(&mut self, command: &plico_renderer::PaintCommand) {
        use peniko::Brush;
        use plico_renderer::PaintCommand;
        match command {
            PaintCommand::Paragraph { .. } => self.paragraphs += 1,
            PaintCommand::GlyphRun { glyphs, .. } => self.glyphs += glyphs.len(),
            PaintCommand::PushLayer { .. } => self.layers += 1,
            PaintCommand::Svg { .. } => self.svgs += 1,
            _ => {}
        }
        if matches!(
            command,
            PaintCommand::Fill {
                brush: Brush::Image(_),
                ..
            } | PaintCommand::Stroke {
                brush: Brush::Image(_),
                ..
            } | PaintCommand::GlyphRun {
                brush: Brush::Image(_),
                ..
            }
        ) {
            self.image_brushes += 1;
        }
    }
    pub fn trace(&self) {
        tracing::trace!(target: "plico::skia", visited = self.visited, culled = self.culled,
            paragraphs = self.paragraphs, glyphs = self.glyphs, image_brushes = self.image_brushes,
            layers = self.layers, svgs = self.svgs, "replay.work");
    }
}
