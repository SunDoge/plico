//! Pre-shaped glyph rasterization: skrifa extracts the outline, ab_glyph
//! rasterizes it into 8-bit alpha coverage, epaint draws it as a textured
//! quad. Shaping (which glyphs, where, at what size) always comes from the
//! command stream - this module never measures or shapes text.
use std::collections::HashMap;

use ab_glyph_rasterizer::Rasterizer;
use egui::{Color32, Pos2, Rect as ERect, Vec2};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
};

/// One rasterized glyph: alpha coverage plus its placement relative to the
/// glyph origin in the command stream.
struct Coverage {
    /// Row-major 8-bit alpha, `size.0 * size.1` bytes.
    pixels: Vec<u8>,
    size: (usize, usize),
    /// Offset of the coverage's top-left corner from the glyph origin, in
    /// logical pixels (skrifa's y-up bearings converted to screen y-down).
    offset: [f32; 2],
}

/// Cache keyed by (font blob id, font index, rounded pixel size, glyph id).
/// The texture is uploaded once per unique glyph; egui keeps the GPU copy.
#[derive(Default)]
pub struct GlyphCache {
    coverage: HashMap<(u64, u32, u32, u32), Option<Coverage>>,
    textures: HashMap<(u64, u32, u32, u32), egui::TextureId>,
    /// Shadow coverage textures keyed by their geometry tuple.
    pub(crate) shadow_textures: HashMap<(u64, u32, u32, u32), egui::TextureId>,
}

impl GlyphCache {
    /// Draw one pre-shaped glyph run quad. `origin` is the glyph's (x, y)
    /// baseline position from the command; `font` carries the bytes and
    /// collection index Parley selected.
    pub(crate) fn draw_glyph(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        font: &peniko::FontData,
        font_size: f32,
        glyph: &plico_renderer::Glyph,
        color: Color32,
    ) {
        let size = font_size.round().max(1.) as u32;
        let key = (font.data.id(), font.index, size, glyph.id);
        let Some(coverage) = self.ensure(ctx, font, size, glyph.id, key) else {
            return; // empty coverage (space, .notdef) draws nothing
        };
        let (width, height) = coverage.size;
        let rect = ERect::from_min_size(
            Pos2::new(glyph.x + coverage.offset[0], glyph.y + coverage.offset[1]),
            Vec2::new(width as f32, height as f32),
        );
        let texture = self
            .textures
            .get(&key)
            .copied()
            .unwrap_or(egui::TextureId::Managed(0));
        let uv = ERect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.));
        let mesh = std::sync::Arc::new(egui::epaint::Mesh {
            texture_id: texture,
            indices: vec![0, 1, 2, 2, 1, 3],
            vertices: vec![
                uv_vertex(rect.left_top(), uv.left_top(), color),
                uv_vertex(rect.right_top(), uv.right_top(), color),
                uv_vertex(rect.left_bottom(), uv.left_bottom(), color),
                uv_vertex(rect.right_bottom(), uv.right_bottom(), color),
            ],
        });
        painter.add(egui::Shape::Mesh(mesh));
    }

    fn ensure(
        &mut self,
        ctx: &egui::Context,
        font: &peniko::FontData,
        size: u32,
        glyph: u32,
        key: (u64, u32, u32, u32),
    ) -> Option<&Coverage> {
        if !self.coverage.contains_key(&key) {
            let coverage = rasterize(font, size, glyph);
            if let Some(coverage) = &coverage {
                let (width, height) = coverage.size;
                let mut pixels = Vec::with_capacity(width * height * 4);
                for alpha in &coverage.pixels {
                    pixels.push(Color32::from_rgba_unmultiplied(255, 255, 255, *alpha));
                }
                let image = egui::ColorImage {
                    size: [width, height],
                    source_size: egui::Vec2::new(width as f32, height as f32),
                    pixels,
                };
                let texture = ctx.load_texture(
                    format!("plico-glyph-{key:?}"),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.textures.insert(key, texture.id());
            }
            self.coverage.insert(key, coverage);
        }
        self.coverage.get(&key).and_then(|entry| entry.as_ref())
    }
}

fn uv_vertex(pos: Pos2, uv: Pos2, color: Color32) -> egui::epaint::Vertex {
    egui::epaint::Vertex { pos, uv, color }
}

/// Rasterize one glyph outline into alpha coverage, or None when the font
/// has no outline for it.
fn rasterize(font: &peniko::FontData, size: u32, glyph: u32) -> Option<Coverage> {
    let font = FontRef::new(font.data.as_ref()).ok()?;
    let outline_glyphs = font.outline_glyphs();
    let outline = outline_glyphs.get(skrifa::GlyphId::new(glyph))?;
    // Skrifa draws y-up; collect the outline and its bounds first.
    let mut pen = BoundsPen::default();
    let settings = DrawSettings::unhinted(Size::new(size as f32), LocationRef::default());
    let metrics = outline.draw(settings, &mut pen).ok()?;
    let _ = metrics;
    if pen.min_x >= pen.max_x || pen.min_y >= pen.max_y {
        return None; // empty outline (space, blank glyph)
    }
    // Add one pixel of padding and flip y for the rasterizer.
    let width = (pen.max_x - pen.min_x).ceil() as usize + 2;
    let height = (pen.max_y - pen.min_y).ceil() as usize + 2;
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return None;
    }
    let mut rasterizer = Rasterizer::new(width, height);
    let min_x = pen.min_x;
    let max_y = pen.max_y;
    let flip = move |x: f32, y: f32| ab_glyph_rasterizer::Point {
        x: x - min_x + 1.,
        y: max_y - y + 1.,
    };
    let mut stream = PenStream {
        rasterizer: &mut rasterizer,
        flip,
        cursor: (0., 0.),
    };
    let settings = DrawSettings::unhinted(Size::new(size as f32), LocationRef::default());
    outline.draw(settings, &mut stream).ok()?;
    let mut pixels = vec![0u8; width * height];
    rasterizer.for_each_pixel_2d(|x, y, alpha| {
        if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y))
            && let Some(slot) = pixels.get_mut(y * width + x)
        {
            *slot = (alpha * 255.).round().clamp(0., 255.) as u8;
        }
    });
    Some(Coverage {
        pixels,
        size: (width, height),
        // The rasterizer's top-left is (min_x - 1, max_y + 1) in the y-up
        // font space; relative to the baseline origin that is (-left + 1)
        // horizontally and -(1 + top) vertically in screen space.
        offset: [-(pen.min_x - 1.), -(pen.max_y + 1.)],
    })
}

#[derive(Default)]
struct BoundsPen {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    seen: bool,
}

impl OutlinePen for BoundsPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.absorb(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.absorb(x, y);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.absorb(cx, cy);
        self.absorb(x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.absorb(cx0, cy0);
        self.absorb(cx1, cy1);
        self.absorb(x, y);
    }
    fn close(&mut self) {}
}

impl BoundsPen {
    fn absorb(&mut self, x: f32, y: f32) {
        if !self.seen {
            self.min_x = x;
            self.max_x = x;
            self.min_y = y;
            self.max_y = y;
            self.seen = true;
            return;
        }
        self.min_x = self.min_x.min(x);
        self.max_x = self.max_x.max(x);
        self.min_y = self.min_y.min(y);
        self.max_y = self.max_y.max(y);
    }
}

/// Feeds one outline's segments into the rasterizer with y flipped.
struct PenStream<'a, F>
where
    F: Fn(f32, f32) -> ab_glyph_rasterizer::Point,
{
    rasterizer: &'a mut Rasterizer,
    flip: F,
    cursor: (f32, f32),
}

impl<F> OutlinePen for PenStream<'_, F>
where
    F: Fn(f32, f32) -> ab_glyph_rasterizer::Point,
{
    fn move_to(&mut self, x: f32, y: f32) {
        self.cursor = (x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p0 = (self.flip)(self.cursor.0, self.cursor.1);
        let p1 = (self.flip)(x, y);
        self.rasterizer.draw_line(p0, p1);
        self.cursor = (x, y);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let p0 = (self.flip)(self.cursor.0, self.cursor.1);
        let p1 = (self.flip)(cx, cy);
        let p2 = (self.flip)(x, y);
        self.rasterizer.draw_quad(p0, p1, p2);
        self.cursor = (x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let p0 = (self.flip)(self.cursor.0, self.cursor.1);
        let p1 = (self.flip)(cx0, cy0);
        let p2 = (self.flip)(cx1, cy1);
        let p3 = (self.flip)(x, y);
        self.rasterizer.draw_cubic(p0, p1, p2, p3);
        self.cursor = (x, y);
    }
    fn close(&mut self) {}
}
