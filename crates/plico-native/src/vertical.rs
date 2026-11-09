//! Vertical document text. Native engines own shaping, fallback and orientation;
//! returned pixels own no OS objects and can be uploaded by any GPU renderer.
use snafu::Snafu;

/// A vertical-rl text box, in logical document pixels.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    /// Plain text, not markup.
    pub text: &'a str,
    /// Native family name or a platform default when empty.
    pub family: &'a str,
    /// Logical font size.
    pub size: f64,
    /// Available width across columns and height along columns.
    pub bounds: [f64; 2],
    /// Physical output pixels per logical document pixel.
    pub scale: f64,
    /// CSS weight, 1–1000.
    pub weight: u16,
    /// Straight RGBA foreground.
    pub color: [u8; 4],
}
impl<'a> Request<'a> {
    /// Black regular text, 1x, using the system font fallback chain.
    pub fn new(text: &'a str, bounds: [f64; 2], size: f64) -> Self {
        Self {
            text,
            family: "",
            size,
            bounds,
            scale: 1.,
            weight: 400,
            color: [0, 0, 0, 255],
        }
    }
    fn valid(&self) -> bool {
        self.text.len() <= 64 * 1024
            && !self.text.contains('\0')
            && self.family.len() <= 4096
            && !self.family.contains('\0')
            && self.size.is_finite()
            && (0. ..=1024.).contains(&self.size)
            && self.size > 0.
            && self.scale.is_finite()
            && self.scale > 0.
            && self.scale <= 8.
            && (1..=1000).contains(&self.weight)
            && self
                .bounds
                .iter()
                .all(|n| n.is_finite() && *n > 0. && *n <= 16384.)
            && self
                .bounds
                .iter()
                .map(|n| (n * self.scale).ceil())
                .product::<f64>()
                * 4.
                <= 64. * 1024. * 1024.
    }
}
/// Owned straight-alpha pixels. `fits=false` must never be silently exported.
pub struct Bitmap {
    /// Physical width and height.
    pub size: [u32; 2],
    /// Straight RGBA8 in top-down row order.
    pub rgba: Vec<u8>,
    /// Whether the entire input fits the requested text box.
    pub fits: bool,
    /// Native missing-glyph count, or None when the backend does not report it.
    pub missing_glyphs: Option<u32>,
}
/// Native API errors are retained as readable diagnostics, never blank output.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Resource limits or invalid values.
    #[snafu(display("invalid vertical text request"))]
    InvalidRequest,
    /// The current target has no native vertical backend.
    #[snafu(display("native vertical text is unavailable on this target"))]
    Unsupported,
    /// System layout/raster failure.
    #[snafu(display("native vertical text failed: {message}"))]
    Native {
        /// Original platform diagnostic.
        message: String,
    },
}
/// Thread-confined service. Construct it on the worker that uses it.
pub struct Engine {
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    #[cfg(target_os = "linux")]
    linux: crate::text::SystemText,
    #[cfg(target_os = "windows")]
    windows: crate::windows_text::Context,
}
impl Engine {
    /// Native font/layout caches are bounded by the underlying backend.
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            _thread: std::marker::PhantomData,
            #[cfg(target_os = "linux")]
            linux: crate::text::SystemText::new(16 * 1024 * 1024).map_err(native)?,
            #[cfg(target_os = "windows")]
            windows: crate::windows_text::Context::new().map_err(native)?,
        })
    }
    /// Use native line breaking and mixed-script vertical glyph orientation.
    pub fn rasterize(&mut self, request: &Request<'_>) -> Result<Bitmap, Error> {
        if !request.valid() {
            return Err(Error::InvalidRequest);
        }
        let bitmap: Bitmap = {
            #[cfg(target_os = "linux")]
            {
                let mut r = crate::text::TextRequest::new(request.text);
                r.family = if request.family.is_empty() {
                    "sans-serif"
                } else {
                    request.family
                };
                r.size = request.size;
                r.weight = request.weight;
                r.width = Some(request.bounds[1]);
                r.scale = request.scale;
                r.color = request.color;
                let bitmap = self.linux.rasterize_vertical(&r).map_err(native)?;
                Ok(Bitmap {
                    size: bitmap.pixels,
                    rgba: bitmap.rgba.clone(),
                    fits: true, // Pango returns all columns; ink fit is checked below.
                    missing_glyphs: Some(bitmap.missing_glyphs),
                })
            }
            #[cfg(target_os = "macos")]
            {
                crate::macos_text::vertical::rasterize(request).map_err(native)
            }
            #[cfg(target_os = "windows")]
            {
                crate::windows_text::vertical::rasterize(&self.windows, request).map_err(native)
            }
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            {
                Err(Error::Unsupported)
            }
        }?;
        let mut bitmap = trim_ink(bitmap);
        bitmap.fits &= f64::from(bitmap.size[0]) <= request.bounds[0] * request.scale + 0.01
            && f64::from(bitmap.size[1]) <= request.bounds[1] * request.scale + 0.01;
        Ok(bitmap)
    }
}
// Fit/center native painted ink without treating logical leading as a glyph.
fn trim_ink(mut bitmap: Bitmap) -> Bitmap {
    let [w, h] = bitmap.size;
    let mut bounds = [w, h, 0, 0];
    for y in 0..h {
        for x in 0..w {
            if bitmap.rgba[((y * w + x) * 4 + 3) as usize] != 0 {
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x + 1);
                bounds[3] = bounds[3].max(y + 1);
            }
        }
    }
    if bounds[2] == 0 {
        bitmap.size = [1, 1];
        bitmap.rgba = vec![0; 4];
        return bitmap;
    }
    let width = bounds[2] - bounds[0];
    let height = bounds[3] - bounds[1];
    if width == w && height == h {
        return bitmap;
    }
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in bounds[1]..bounds[3] {
        let start = ((y * w + bounds[0]) * 4) as usize;
        rgba.extend_from_slice(&bitmap.rgba[start..start + (width * 4) as usize]);
    }
    bitmap.size = [width, height];
    bitmap.rgba = rgba;
    bitmap
}
fn native(error: impl std::fmt::Display) -> Error {
    Error::Native {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_scales_and_oversized_buffers_before_native_work() {
        let mut r = Request::new("中文", [100., 200.], 20.);
        assert!(r.valid());
        r.scale = f64::NAN;
        assert!(!r.valid());
        r.scale = 8.;
        r.bounds = [16384.; 2];
        assert!(!r.valid());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn coretext_horizontal_and_vertical_rasters_keep_independent_color_contexts() {
        let mut engine = Engine::new().unwrap();
        for scale in [1., 1.25, 2.] {
            let mut request = Request::new("日本語（縦書き）ABC", [160., 320.], 20.);
            request.scale = scale;
            request.color = [180, 30, 20, 128];
            let first = engine.rasterize(&request).unwrap();
            assert!(first.fits);
            assert!(
                first
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[0] > p[1])
            );
            let horizontal =
                crate::macos_text::Line::new(&crate::macos_text::Request::new("office 日本語"))
                    .unwrap()
                    .rasterize(scale, [0.25, 0.], [20, 180, 30, 255])
                    .unwrap()
                    .unwrap();
            assert!(
                horizontal
                    .pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[1] > p[0])
            );
            let next = engine.rasterize(&request).unwrap();
            assert_eq!(next.size, first.size);
            assert_eq!(
                next.rgba, first.rgba,
                "sharing immutable native resources must not share mutable draw state"
            );
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn pango_mixed_vertical_text_wraps_and_does_not_poison_horizontal_cache() {
        let mut engine = Engine::new().unwrap();
        let r = Request::new("中文（测试）ABC 123\n第二列", [160., 180.], 20.);
        let bitmap = engine.rasterize(&r).unwrap();
        assert!(bitmap.fits);
        assert_eq!(bitmap.missing_glyphs, Some(0));
        assert!(bitmap.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
        let short = engine
            .rasterize(&Request::new("中文测试", [50., 180.], 20.))
            .unwrap();
        assert!(short.size[1] > short.size[0] * 2);
        let small = engine
            .rasterize(&Request::new("中文测试中文测试", [8., 30.], 20.))
            .unwrap();
        assert!(!small.fits);
        let narrow = engine
            .rasterize(&Request::new("……", [10., 70.], 8.))
            .unwrap();
        assert!(
            narrow.fits,
            "logical font leading must not reject fitting painted ink"
        );
        let ordinary = engine
            .linux
            .rasterize(&crate::text::TextRequest::new("中文测试"))
            .unwrap();
        assert!(ordinary.pixels[0] > ordinary.pixels[1]);
    }
}
