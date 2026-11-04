//! Raster pre-shaped glyph IDs with their original font bytes and positions.
//! This does not perform fallback, shaping or advance calculation.
use super::Error;
type Result<T> = std::result::Result<T, Error>;
use cairo::{Antialias, Context, FontOptions, Format, HintMetrics, HintStyle, ImageSurface};
use snafu::ResultExt;
use std::{borrow::Borrow, rc::Rc};
#[derive(Clone)]
struct SharedBytes(Rc<dyn AsRef<[u8]>>);
impl Borrow<[u8]> for SharedBytes {
    fn borrow(&self) -> &[u8] {
        self.0.as_ref().as_ref()
    }
}
static FONT_BYTES: cairo::UserDataKey<cairo::freetype::Face<SharedBytes>> =
    cairo::UserDataKey::new();

/// A physical-pixel baseline position in a previously shaped font face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// Font-specific glyph index, not a Unicode codepoint.
    pub id: u32,
    /// Physical horizontal baseline origin.
    pub x: f64,
    /// Physical vertical baseline origin.
    pub y: f64,
}
/// Premultiplied RGBA8 coverage at the original requested positions.
pub struct Bitmap {
    /// Integer offset relative to the caller's glyph coordinate system.
    pub origin: [i32; 2],
    /// Width and height in physical pixels.
    pub size: [u32; 2],
    /// Tightly packed premultiplied RGBA8, with no row padding.
    pub rgba: Vec<u8>,
}
/// Thread-confined native face owning its in-memory font data. Collection face
/// indices are explicit. Variable coordinates and synthetic styles are not part
/// of this initial interface; callers must not silently discard those settings.
pub struct Font {
    face: cairo::FontFace,
    native: cairo::freetype::Face<SharedBytes>,
    _library: cairo::freetype::Library,
}
impl Font {
    /// Open owned font bytes without writing an asset to a temporary file.
    pub fn from_bytes(bytes: Vec<u8>, index: u32) -> Result<Self> {
        Self::from_shared(Rc::new(bytes), index)
    }
    /// Retain a shared font allocation or mapping without copying its contents.
    /// Collection sources are limited to 1 GiB; raster output remains 64 MiB.
    pub fn from_shared(bytes: Rc<dyn AsRef<[u8]>>, index: u32) -> Result<Self> {
        let length = bytes.as_ref().as_ref().len();
        if length == 0 || length > 1024 * 1024 * 1024 {
            return Err(Error::InvalidRequest);
        }
        let library = cairo::freetype::Library::init().context(super::FreeTypeSnafu)?;
        let mut native = library
            .new_memory_face2(
                SharedBytes(bytes),
                isize::try_from(index).map_err(|_| Error::InvalidRequest)?,
            )
            .context(super::FreeTypeSnafu)?;
        // SAFETY: FreeType opened this live face. The user-data owner below
        // retains a cloned Face (and shared bytes/library) for Cairo's entire
        // face lifetime, including references held by Cairo's own caches.
        // This mirrors cairo-rs create_from_ft, generalized to shared byte owners.
        let face = unsafe {
            cairo::FontFace::from_raw_full(cairo::ffi::cairo_ft_font_face_create_for_ft_face(
                native.raw_mut() as cairo::freetype::ffi::FT_Face as *mut _,
                0,
            ))
        };
        face.status().context(super::CairoSnafu)?;
        face.set_user_data(&FONT_BYTES, Rc::new(native.clone()))
            .context(super::CairoSnafu)?;
        Ok(Self {
            face,
            native,
            _library: library,
        })
    }
    /// Rasterize exact glyph positions, preserving the source font and IDs.
    /// Size/positions are physical pixels; color is straight sRGB RGBA.
    /// Empty/whitespace-only runs return None. Output is limited to 64 MiB.
    pub fn raster(
        &self,
        glyphs: &[Glyph],
        size: f64,
        hint: bool,
        color: [f32; 4],
    ) -> Result<Option<Bitmap>> {
        if !size.is_finite()
            || size <= 0.
            || size > 4096.
            || glyphs.len() > 100_000
            || color
                .iter()
                .any(|c| !c.is_finite() || !(0. ..=1.).contains(c))
            || glyphs.iter().any(|g| {
                !g.x.is_finite()
                    || !g.y.is_finite()
                    || g.x.abs() > 1e7
                    || g.y.abs() > 1e7
                    || u64::from(g.id) >= self.native.num_glyphs() as u64
            })
        {
            return Err(Error::InvalidRequest);
        }
        if glyphs.is_empty() {
            return Ok(None);
        }
        let glyphs: Vec<_> = glyphs
            .iter()
            .map(|g| cairo::Glyph::new(g.id.into(), g.x, g.y))
            .collect();
        let surface = ImageSurface::create(Format::ARgb32, 1, 1).context(super::CairoSnafu)?;
        let context = Context::new(&surface).context(super::CairoSnafu)?;
        self.configure(&context, size, hint)?;
        let extents = context.glyph_extents(&glyphs).context(super::CairoSnafu)?;
        if extents.width() <= 0. || extents.height() <= 0. {
            return Ok(None);
        }
        let x = (extents.x_bearing() + glyphs[0].x()).floor() - 1.;
        let y = (extents.y_bearing() + glyphs[0].y()).floor() - 1.;
        let w = (glyphs[0].x() + extents.x_bearing() + extents.width()).ceil() + 1. - x;
        let h = (glyphs[0].y() + extents.y_bearing() + extents.height()).ceil() + 1. - y;
        if w > 16384. || h > 16384. || w * h * 4. > 64. * 1024. * 1024. {
            return Err(Error::InvalidRequest);
        }
        let mut surface =
            ImageSurface::create(Format::ARgb32, w as i32, h as i32).context(super::CairoSnafu)?;
        {
            let context = Context::new(&surface).context(super::CairoSnafu)?;
            self.configure(&context, size, hint)?;
            context.translate(-x, -y);
            context.set_source_rgba(
                color[0] as f64,
                color[1] as f64,
                color[2] as f64,
                color[3] as f64,
            );
            context.show_glyphs(&glyphs).context(super::CairoSnafu)?;
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        {
            let data = surface.data().context(super::PixelsSnafu)?;
            for row in data.chunks_exact(stride).take(h as usize) {
                for pixel in row[..w as usize * 4].as_chunks::<4>().0 {
                    let p = u32::from_ne_bytes(*pixel);
                    rgba.extend_from_slice(&[
                        (p >> 16) as u8,
                        (p >> 8) as u8,
                        p as u8,
                        (p >> 24) as u8,
                    ]);
                }
            }
        }
        Ok(Some(Bitmap {
            origin: [x as i32, y as i32],
            size: [w as u32, h as u32],
            rgba,
        }))
    }
    fn configure(&self, context: &Context, size: f64, hint: bool) -> Result<()> {
        let mut options = FontOptions::new().context(super::CairoSnafu)?;
        options.set_antialias(Antialias::Gray);
        options.set_hint_metrics(HintMetrics::Off);
        options.set_hint_style(if hint {
            HintStyle::Medium
        } else {
            HintStyle::None
        });
        context.set_font_options(&options);
        context.set_font_face(&self.face);
        context.set_font_size(size);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires Fontconfig and a system sans-serif font"]
    fn in_memory_face_keeps_positions_and_premultiplied_pixels() {
        let path = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "sans-serif"])
            .output()
            .unwrap();
        assert!(path.status.success());
        let path = std::str::from_utf8(&path.stdout).unwrap();
        let font = Font::from_bytes(std::fs::read(path).unwrap(), 0).unwrap();
        let id = font.native.get_char_index('A' as usize).unwrap();
        let a = font
            .raster(&[Glyph { id, x: 0., y: 20. }], 18., true, [1., 0., 0., 0.5])
            .unwrap()
            .unwrap();
        let b = font
            .raster(
                &[Glyph { id, x: 30., y: 40. }],
                18.,
                true,
                [1., 0., 0., 0.5],
            )
            .unwrap()
            .unwrap();
        assert_eq!(a.size, b.size);
        assert_eq!(a.rgba, b.rgba);
        assert_eq!([a.origin[0] + 30, a.origin[1] + 20], b.origin);
        assert!(a.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 80));
        for pixel in a.rgba.as_chunks::<4>().0 {
            assert_eq!(pixel[0], pixel[3]);
            assert_eq!(pixel[1], 0);
            assert_eq!(pixel[2], 0);
            assert!(pixel[3] <= 128);
        }
        assert!(
            font.raster(
                &[Glyph {
                    id: u32::MAX,
                    x: 0.,
                    y: 0.
                }],
                18.,
                true,
                [1.; 4]
            )
            .is_err()
        );
        assert!(font.raster(&[], 18., true, [1.; 4]).unwrap().is_none());
        assert!(Font::from_bytes(vec![0; 32], 0).is_err());
    }
}
