use super::{Error, Line, Result};
use crate::raster_bounds::{raster_bounds, valid_sampling};
use objc2_core_foundation::CGAffineTransform;
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGTextDrawingMode, kCGColorSpaceSRGB,
};

/// A top-down premultiplied sRGB RGBA image of one native line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rasterized {
    /// Physical pixel offset relative to the line baseline, including phase.
    pub origin: [i32; 2],
    /// Physical dimensions of the image.
    pub size: [u32; 2],
    /// Tightly packed RGBA pixels, four bytes per pixel.
    pub pixels: Vec<u8>,
}
impl Line {
    /// Draw the retained line at a physical DPI and subpixel baseline phase.
    /// Scale must be in `(0, 8]`, each phase in `[0, 1)`. Foreground is straight
    /// RGBA; native color glyphs keep their colors and receive global alpha.
    /// Empty ink/zero alpha returns None. This uses native grayscale AA without
    /// synthetic weight, font smoothing or LCD color fringes.
    pub fn rasterize(
        &self,
        scale: f64,
        phase: [f64; 2],
        foreground: [u8; 4],
    ) -> Result<Option<Rasterized>> {
        if !valid_sampling(scale, phase) {
            return Err(Error::InvalidRequest);
        }
        if self.metrics.glyphs == 0 || foreground[3] == 0 {
            return Ok(None);
        }
        // SAFETY: The native line is retained and immutable. Null context asks
        // for bounds relative to its baseline rather than another draw position.
        let ink = unsafe { self.native.image_bounds(None) };
        if ink.size.width == 0. || ink.size.height == 0. {
            return Ok(None);
        }
        let bounds = raster_bounds(
            [
                ink.origin.x,
                ink.origin.y,
                ink.origin.x + ink.size.width,
                ink.origin.y + ink.size.height,
            ],
            scale,
            phase,
        )
        .ok_or(Error::InvalidRequest)?;
        let [width, height] = bounds.size;
        let mut pixels = vec![0u8; bounds.bytes];
        // SAFETY: A framework-owned constant denotes the standard sRGB space.
        let space =
            CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or(Error::Allocation)?;
        // SAFETY: The initialized byte allocation is exactly width*height*4;
        // RGBA8 big-endian component order matches the stride. It is not moved,
        // accessed, or freed while the CGContext owns its temporary raw pointer.
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width as usize,
                height as usize,
                8,
                width as usize * 4,
                Some(&space),
                CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
            )
        }
        .ok_or(Error::Allocation)?;
        let ctx = Some(&*context);
        CGContext::set_allows_antialiasing(ctx, true);
        CGContext::set_should_antialias(ctx, true);
        CGContext::set_allows_font_smoothing(ctx, false);
        CGContext::set_should_smooth_fonts(ctx, false);
        CGContext::set_allows_font_subpixel_positioning(ctx, true);
        CGContext::set_should_subpixel_position_fonts(ctx, true);
        CGContext::set_allows_font_subpixel_quantization(ctx, false);
        CGContext::set_should_subpixel_quantize_fonts(ctx, false);
        CGContext::set_text_drawing_mode(ctx, CGTextDrawingMode::Fill);
        CGContext::set_rgb_fill_color(
            ctx,
            f64::from(foreground[0]) / 255.,
            f64::from(foreground[1]) / 255.,
            f64::from(foreground[2]) / 255.,
            1.,
        );
        CGContext::set_alpha(ctx, f64::from(foreground[3]) / 255.);
        // Quartz draws y-up. Baseline-relative top-down bounds locate its
        // baseline at the image's bottom - phase_y, matching native glyph draws.
        CGContext::translate_ctm(
            ctx,
            -f64::from(bounds.origin[0]) + phase[0],
            f64::from(height) + f64::from(bounds.origin[1]) - phase[1],
        );
        CGContext::scale_ctm(ctx, scale, scale);
        CGContext::set_text_matrix(
            ctx,
            CGAffineTransform {
                a: 1.,
                b: 0.,
                c: 0.,
                d: 1.,
                tx: 0.,
                ty: 0.,
            },
        );
        CGContext::set_text_position(ctx, 0., 0.);
        // SAFETY: Both retained objects are valid for this synchronous draw;
        // buffer access resumes only after the bitmap context is released.
        unsafe {
            self.native.draw(&context);
        }
        CGContext::flush(ctx);
        drop(context);
        Ok(Some(Rasterized {
            origin: bounds.origin,
            size: bounds.size,
            pixels,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macos_text::Request;
    #[test]
    fn native_rasters_are_repeatable_dpi_aware_and_premultiplied() {
        let line = Line::new(&Request {
            size: 24.,
            ..Request::new("S 中文 日本語 🙂")
        })
        .unwrap();
        let metrics = line.metrics();
        for scale in [1., 1.25, 1.5, 2.] {
            for phase in [0., 0.25, 0.5, 0.75] {
                let image = line
                    .rasterize(scale, [phase, 0.], [200, 90, 20, 128])
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    image.pixels.len(),
                    image.size[0] as usize * image.size[1] as usize * 4
                );
                assert!(image.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
                for pixel in image.pixels.as_chunks::<4>().0 {
                    assert!(pixel[3] <= 128);
                    assert!(pixel[..3].iter().all(|c| *c <= pixel[3]));
                }
                assert_eq!(
                    line.rasterize(scale, [phase, 0.], [200, 90, 20, 128])
                        .unwrap()
                        .unwrap(),
                    image
                );
            }
        }
        assert_eq!(line.metrics().width, metrics.width);
        assert!(line.rasterize(1., [0.; 2], [0; 4]).unwrap().is_none());
        assert!(line.rasterize(0., [0.; 2], [255; 4]).is_err());
        assert!(
            Line::new(&Request::new("   "))
                .unwrap()
                .rasterize(1., [0.; 2], [255; 4])
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn foreground_and_asymmetric_glyph_rows_are_not_ignored_or_flipped() {
        let line = Line::new(&Request {
            size: 40.,
            ..Request::new("F")
        })
        .unwrap();
        let image = line
            .rasterize(1., [0.; 2], [255, 0, 0, 255])
            .unwrap()
            .unwrap();
        assert!(image.origin[1] < 0);
        assert!(image.pixels.as_chunks::<4>().0.iter().any(|p| p[0] > 0));
        assert!(
            image
                .pixels
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[1] == 0 && p[2] == 0)
        );
        let stride = image.size[0] as usize * 4;
        let rows: Vec<u64> = image
            .pixels
            .chunks_exact(stride)
            .map(|row| row.as_chunks::<4>().0.iter().map(|p| u64::from(p[3])).sum())
            .collect();
        let third = (rows.len() / 3).max(1);
        assert!(
            rows[..third].iter().sum::<u64>() > rows[rows.len() - third..].iter().sum::<u64>(),
            "F's top bar is below its stem: bitmap row convention must be checked"
        );
    }
}
