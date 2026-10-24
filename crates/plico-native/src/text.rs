//! Pango/Cairo text layout and rasterization with physical-pixel output.
//!
//! Inspired by MyGO's Linux text backend; see SOURCES.md. Contexts stay on their
//! creating thread; returned bitmaps contain no platform objects and can be sent
//! to the UI. Grayscale AA works on transparent surfaces without LCD fringes.
use cairo::{Antialias, Context, FontOptions, Format, HintMetrics, HintStyle, ImageSurface};
use lru::LruCache;
use pango::prelude::*;
use snafu::{ResultExt, Snafu};
use std::{num::NonZeroUsize, sync::Arc};

/// A complete raster request. Sizes and wrapping width are in logical pixels.
#[derive(Clone, Debug)]
pub struct TextRequest<'a> {
    /// Plain UTF-8, never interpreted as markup.
    pub text: &'a str,
    /// Fontconfig family list, including optional fallback families.
    pub family: &'a str,
    /// Logical pixel size, not points.
    pub size: f64,
    /// CSS/OpenType weight, 1 through 1000.
    pub weight: u16,
    /// Request the italic face.
    pub italic: bool,
    /// Wrap by word, falling back to character breaks.
    pub width: Option<f64>,
    /// Physical pixels per logical pixel.
    pub scale: f64,
    /// Straight-alpha sRGB text color. Color emoji retain their own colors.
    pub color: [u8; 4],
}
impl<'a> TextRequest<'a> {
    /// A 14px system sans-serif request at 1x, opaque black.
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            family: "sans-serif",
            size: 14.,
            weight: 400,
            italic: false,
            width: None,
            scale: 1.,
            color: [0, 0, 0, 255],
        }
    }
}

/// Owned platform-neutral pixels and the metrics that produced them.
#[derive(Debug)]
pub struct TextBitmap {
    /// Physical pixel dimensions, including ink overhangs and logical extents.
    pub pixels: [u32; 2],
    /// Physical offset from the layout's top-left to this bitmap's top-left.
    /// Apply this offset when drawing; italic/combining ink can extend left/up.
    pub pixel_origin: [i32; 2],
    /// Logical layout size, including spaces and line advances, excluding overhangs.
    pub logical_size: [f64; 2],
    /// Logical first-line baseline from the layout's top-left.
    pub baseline: f64,
    /// Full layout line count, including wrapped lines.
    pub lines: u32,
    /// Missing glyphs after system fallback; useful for font diagnostics.
    pub missing_glyphs: u32,
    /// Straight-alpha RGBA8 in row-major order, with no stride padding.
    pub rgba: Vec<u8>,
}

/// Errors are explicit; a caller may fall back to its ordinary text backend.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Invalid sizes, excessive input or excessive output allocation.
    #[snafu(display("Invalid or oversized system text request"))]
    InvalidRequest,
    /// Cairo rejected an operation.
    #[snafu(display("System text rasterization failed: {source}"))]
    Cairo {
        /// Native Cairo error.
        source: cairo::Error,
    },
    /// Cairo surface could not be borrowed for reading.
    #[snafu(display("System text pixel access failed: {source}"))]
    Pixels {
        /// Surface borrow error.
        source: cairo::BorrowError,
    },
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct Key {
    text: String,
    family: String,
    size: u64,
    weight: u16,
    italic: bool,
    width: Option<u64>,
    scale: u64,
    color: [u8; 4],
}

/// Thread-confined system layout engine with a byte-bounded LRU bitmap cache.
/// Cache eviction does not invalidate images already held by callers.
pub struct SystemText {
    context: pango::Context,
    cache: LruCache<Key, Arc<TextBitmap>>,
    retained: usize,
    budget: usize,
}
impl SystemText {
    /// A private font map avoids sharing Pango's process-global default map.
    pub fn new(cache_bytes: usize) -> Result<Self, Error> {
        let map = pangocairo::FontMap::new();
        let context = map.create_context();
        let mut options = FontOptions::new().context(CairoSnafu)?;
        options.set_antialias(Antialias::Gray);
        options.set_hint_style(HintStyle::Medium);
        options.set_hint_metrics(HintMetrics::On);
        pangocairo::functions::context_set_font_options(&context, Some(&options));
        pangocairo::functions::context_set_resolution(&context, 96.);
        Ok(Self {
            context,
            cache: LruCache::new(NonZeroUsize::new(256).unwrap()),
            retained: 0,
            budget: cache_bytes,
        })
    }
    /// Available system family names, sorted and deduplicated.
    pub fn font_families(&self) -> Vec<String> {
        let mut families: Vec<_> = self
            .context
            .list_families()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        families.sort();
        families.dedup();
        families
    }
    /// Forget derived bitmaps, for example after a desktop font-setting change.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.retained = 0;
    }
    /// Bytes held by this cache, excluding caller-held outputs and platform caches.
    pub fn cached_bytes(&self) -> usize {
        self.retained
    }
    /// Shape, measure and rasterize using the same native layout. Repeated requests
    /// return the same resource. Decode/rasterize on the host, not every UI declaration.
    pub fn rasterize(&mut self, request: &TextRequest<'_>) -> Result<Arc<TextBitmap>, Error> {
        validate(request)?;
        let key = Key {
            text: request.text.into(),
            family: request.family.into(),
            size: request.size.to_bits(),
            weight: request.weight,
            italic: request.italic,
            width: request.width.map(f64::to_bits),
            scale: request.scale.to_bits(),
            color: request.color,
        };
        if let Some(bitmap) = self.cache.get(&key) {
            return Ok(bitmap.clone());
        }
        let bitmap = Arc::new(self.render(request)?);
        let bytes = bitmap.rgba.len() + key.text.len() + key.family.len();
        if bytes <= self.budget {
            while self.retained > self.budget.saturating_sub(bytes)
                || self.cache.len() == self.cache.cap().get()
            {
                if let Some((key, old)) = self.cache.pop_lru() {
                    self.retained -= old.rgba.len() + key.text.len() + key.family.len();
                } else {
                    break;
                }
            }
            self.retained += bytes;
            self.cache.put(key, bitmap.clone());
        }
        Ok(bitmap)
    }
    fn render(&self, request: &TextRequest<'_>) -> Result<TextBitmap, Error> {
        let layout = pango::Layout::new(&self.context);
        let mut font = pango::FontDescription::new();
        font.set_family(request.family);
        font.set_absolute_size(request.size * request.scale * f64::from(pango::SCALE));
        use pango::glib::translate::from_glib;
        // SAFETY: gtk-rs represents unrecognized PangoWeight values with its
        // unknown variant. Pango permits numeric OpenType weights, validated above.
        font.set_weight(unsafe { from_glib(i32::from(request.weight)) });
        font.set_style(if request.italic {
            pango::Style::Italic
        } else {
            pango::Style::Normal
        });
        layout.set_font_description(Some(&font));
        layout.set_text(request.text);
        layout.set_wrap(pango::WrapMode::WordChar);
        if let Some(width) = request.width {
            layout.set_width((width * request.scale * f64::from(pango::SCALE)).round() as i32);
        }
        let (ink, logical) = layout.pixel_extents();
        let x = ink.x().min(logical.x());
        let y = ink.y().min(logical.y());
        let right = (ink.x() + ink.width()).max(logical.x() + logical.width());
        let bottom = (ink.y() + ink.height()).max(logical.y() + logical.height());
        let width = (right - x).max(1);
        let height = (bottom - y).max(1);
        if width > 16384
            || height > 16384
            || i64::from(width) * i64::from(height) * 4 > 64 * 1024 * 1024
        {
            return Err(Error::InvalidRequest);
        }
        let mut surface =
            ImageSurface::create(Format::ARgb32, width, height).context(CairoSnafu)?;
        {
            let cr = Context::new(&surface).context(CairoSnafu)?;
            cr.translate(f64::from(-x), f64::from(-y));
            let [r, g, b, a] = request.color.map(|v| f64::from(v) / 255.);
            cr.set_source_rgba(r, g, b, a);
            pangocairo::functions::show_layout(&cr, &layout);
            cr.status().context(CairoSnafu)?;
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        {
            let data = surface.data().context(PixelsSnafu)?;
            for row in data.chunks_exact(stride).take(height as usize) {
                for pixel in row[..width as usize * 4].as_chunks::<4>().0 {
                    // Cairo ARGB32 is native-endian and premultiplied. Peniko's
                    // ordinary image path expects straight RGBA, not darkened RGB.
                    let argb = u32::from_ne_bytes(*pixel);
                    let a = (argb >> 24) as u8;
                    let straight = |v: u32| {
                        if a == 0 {
                            0
                        } else {
                            ((v * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                        }
                    };
                    rgba.extend_from_slice(&[
                        straight((argb >> 16) & 255),
                        straight((argb >> 8) & 255),
                        straight(argb & 255),
                        a,
                    ]);
                }
            }
        }
        let (logical_w, logical_h) = layout.size();
        Ok(TextBitmap {
            pixels: [width as u32, height as u32],
            pixel_origin: [x, y],
            logical_size: [
                f64::from(logical_w) / f64::from(pango::SCALE) / request.scale,
                f64::from(logical_h) / f64::from(pango::SCALE) / request.scale,
            ],
            baseline: f64::from(layout.baseline()) / f64::from(pango::SCALE) / request.scale,
            lines: layout.line_count() as u32,
            missing_glyphs: layout.unknown_glyphs_count() as u32,
            rgba,
        })
    }
}
fn validate(r: &TextRequest<'_>) -> Result<(), Error> {
    let good = |x: f64, max: f64| x.is_finite() && x > 0. && x <= max;
    if r.text.len() > 64 * 1024
        || r.text.contains('\0')
        || r.family.len() > 4096
        || r.family.contains('\0')
        || !good(r.size, 1024.)
        || !good(r.scale, 8.)
        || !(1..=1000).contains(&r.weight)
        || r.width.is_some_and(|w| {
            !good(w, 16384.) || w * r.scale * f64::from(pango::SCALE) > f64::from(i32::MAX)
        })
    {
        return Err(Error::InvalidRequest);
    }
    Ok(())
}
