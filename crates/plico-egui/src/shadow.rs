//! Box-shadow replay: epaint has no blur primitive, so the shadow's Gaussian
//! coverage is rasterized once per (size, radius, std_dev) into an alpha
//! texture - the same cached-coverage approach as glyphs.
use egui::{Color32, Pos2, Rect as ERect, Vec2};

use crate::glyph::GlyphCache;

/// Draw one blurred rounded-rect shadow quad. The texture is keyed by the
/// rounded-up blur extents so equal-geometry shadows share coverage.
pub(crate) fn draw_box_shadow(
    painter: &egui::Painter,
    cache: &mut GlyphCache,
    transform: vello_common::kurbo::Affine,
    rect: vello_common::kurbo::Rect,
    color: Color32,
    radius: f64,
    std_dev: f64,
) {
    if !(std_dev.is_finite() && std_dev > 0.) {
        // A zero-blur shadow is the solid rounded rect itself.
        return;
    }
    // Blur extent: ~3 sigma each side, matching visual_bounds' accounting.
    let extent = (std_dev * 3.).ceil();
    let outer = [
        rect.x0 - extent,
        rect.y0 - extent,
        rect.x1 + extent,
        rect.y1 + extent,
    ];
    let outer = transform.transform_rect_bbox(vello_common::kurbo::Rect::new(
        outer[0], outer[1], outer[2], outer[3],
    ));
    let (width, height) = (
        (outer.x1 - outer.x0).ceil() as usize,
        (outer.y1 - outer.y0).ceil() as usize,
    );
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return;
    }
    let key = (
        0x05EA_D001,
        0,
        (width * 4 + height) as u32,
        ((radius * 8.) as u32) << 16 | std_dev.round() as u32,
    );
    let texture = cache.shadow_texture(painter.ctx(), width, height, radius, std_dev, key);
    let rect = ERect::from_min_size(
        Pos2::new(outer.x0 as f32, outer.y0 as f32),
        Vec2::new(width as f32, height as f32),
    );
    let uv = ERect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
    let mesh = std::sync::Arc::new(egui::epaint::Mesh {
        texture_id: texture,
        indices: vec![0, 1, 2, 2, 1, 3],
        vertices: vec![
            vertex(rect.left_top(), uv.left_top(), color),
            vertex(rect.right_top(), uv.right_top(), color),
            vertex(rect.left_bottom(), uv.left_bottom(), color),
            vertex(rect.right_bottom(), uv.right_bottom(), color),
        ],
    });
    painter.add(egui::Shape::Mesh(mesh));
}

fn vertex(pos: Pos2, uv: Pos2, color: Color32) -> egui::epaint::Vertex {
    egui::epaint::Vertex { pos, uv, color }
}

impl GlyphCache {
    /// Rasterize the shadow coverage (rounded rect dilated by the blur) and
    /// upload it once per geometry key. Three box-blur passes approximate
    /// the Gaussian closely enough for UI shadows.
    pub(crate) fn shadow_texture(
        &mut self,
        ctx: &egui::Context,
        width: usize,
        height: usize,
        radius: f64,
        std_dev: f64,
        key: (u64, u32, u32, u32),
    ) -> egui::TextureId {
        if let Some(id) = self.shadow_textures.get(&key) {
            return *id;
        }
        let mut coverage = vec![0f32; width * height];
        // 1. Hard rounded-rect coverage.
        let eps = 0.5;
        for y in 0..height {
            for x in 0..width {
                coverage[y * width + x] = rounded_rect_coverage(
                    x as f64 + eps,
                    y as f64 + eps,
                    width as f64,
                    height as f64,
                    radius,
                );
            }
        }
        // 2. Separable box blur, three passes each axis, each pass sized
        //    std_dev * sqrt(12/9) / 3 for the sum to approximate a Gaussian.
        let pass = ((std_dev * (12f64 / 9.).sqrt() / 3.).ceil() as usize).max(1);
        let mut scratch = vec![0f32; width * height];
        for _ in 0..3 {
            box_blur_h(&coverage, &mut scratch, width, height, pass);
            box_blur_v(&scratch, &mut coverage, width, height, pass);
        }
        // 3. Normalize to the peak so the brush alpha modulates the quad.
        let peak = coverage.iter().cloned().fold(0f32, f32::max);
        let mut pixels = Vec::with_capacity(width * height);
        for value in &coverage {
            let alpha = if peak > 0. { value / peak } else { 0. };
            pixels.push(Color32::from_rgba_unmultiplied(
                255,
                255,
                255,
                (alpha * 255.) as u8,
            ));
        }
        let texture = ctx.load_texture(
            format!("plico-shadow-{key:?}"),
            egui::ColorImage {
                size: [width, height],
                source_size: egui::Vec2::new(width as f32, height as f32),
                pixels,
            },
            egui::TextureOptions::LINEAR,
        );
        let id = texture.id();
        self.shadow_textures.insert(key, id);
        id
    }
}

fn rounded_rect_coverage(px: f64, py: f64, width: f64, height: f64, radius: f64) -> f32 {
    // Point-in-rounded-rect with a smooth 1px edge via signed distance.
    let radius = radius.min(width / 2.).min(height / 2.);
    let cx = (width / 2. - radius).max(px.min(width - px).abs());
    let _ = cx;
    let dx = (px - width / 2.).abs() - (width / 2. - radius);
    let dy = (py - height / 2.).abs() - (height / 2. - radius);
    let dist = if dx > 0. && dy > 0. {
        (dx * dx + dy * dy).sqrt() - radius
    } else {
        dx.max(dy)
    };
    (1. - dist).clamp(0., 1.) as f32
}

fn box_blur_h(src: &[f32], dst: &mut [f32], width: usize, height: usize, pass: usize) {
    let window = pass * 2 + 1;
    for y in 0..height {
        let row = y * width;
        let mut sum = 0f32;
        for x in 0..window.min(width) {
            sum += src[row + x.min(width - 1)];
        }
        for x in 0..width {
            dst[row + x] = sum / window as f32;
            let add = (x + pass + 1).min(width - 1);
            let sub = x.saturating_sub(pass);
            sum += src[row + add] - src[row + sub];
        }
    }
}

fn box_blur_v(src: &[f32], dst: &mut [f32], width: usize, height: usize, pass: usize) {
    let window = pass * 2 + 1;
    for x in 0..width {
        let mut sum = 0f32;
        for y in 0..window.min(height) {
            sum += src[y.min(height - 1) * width + x];
        }
        for y in 0..height {
            dst[y * width + x] = sum / window as f32;
            let add = (y + pass + 1).min(height - 1);
            let sub = y.saturating_sub(pass);
            sum += src[add * width + x] - src[sub * width + x];
        }
    }
}
