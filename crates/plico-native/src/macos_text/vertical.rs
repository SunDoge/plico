//! CoreText vertical forms plus native CTFramesetter vertical-rl line breaking.
use super::*;
use crate::vertical::{Bitmap, Request as VerticalRequest};
use objc2_core_foundation::{CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo, CGPath,
};
use objc2_core_text::{
    CTFrameProgression, CTFramesetter, kCTFrameProgressionAttributeName,
    kCTVerticalFormsAttributeName,
};

pub(crate) fn rasterize(request: &VerticalRequest<'_>) -> Result<Bitmap> {
    let native = Request {
        text: request.text,
        family: if request.family.is_empty() {
            "system-ui"
        } else {
            request.family
        },
        fonts: None,
        size: request.size,
        letter_spacing: 0.,
        style: Some(FontStyle {
            weight: FontWeight::from_css(f64::from(request.weight))?,
            italic: false,
        }),
    };
    let (_font, attributed) = Line::attributed_text(&native, &[], true)?;
    let units = request.text.encode_utf16().count();
    // SAFETY: The mutable string is newly owned. Native attributes are retained,
    // cover the validated UTF-16 range, and remain immutable during framesetting.
    let frame = unsafe {
        CFMutableAttributedString::set_attribute(
            Some(&attributed),
            CFRange::new(0, units as isize),
            Some(kCTVerticalFormsAttributeName),
            Some(CFBoolean::new(true)),
        );
        let setter = CTFramesetter::with_attributed_string(&attributed);
        let path = CGPath::with_rect(
            CGRect {
                origin: CGPoint { x: 0., y: 0. },
                size: CGSize {
                    width: request.bounds[0],
                    height: request.bounds[1],
                },
            },
            std::ptr::null(),
        );
        let progression = CFNumber::new_i32(CTFrameProgression::RightToLeft.0 as i32);
        let attrs =
            CFDictionary::from_slices(&[kCTFrameProgressionAttributeName], &[&*progression]);
        setter.frame(
            CFRange::new(0, units as isize),
            &path,
            Some(attrs.as_opaque()),
        )
    };
    // CTFrame reports truncation directly; never silently export the visible prefix.
    let visible = unsafe { frame.visible_string_range() };
    let fits = visible.location == 0 && visible.length == units as isize;
    let width = (request.bounds[0] * request.scale).ceil() as u32;
    let height = (request.bounds[1] * request.scale).ceil() as u32;
    let mut rgba = vec![0_u8; width as usize * height as usize * 4];
    // SAFETY: Storage is exactly width*height*4 and not touched until CGContext
    // is released. The frame owns its fonts, strings and paths while drawing.
    let context = super::drawing::with_srgb(|space| {
        unsafe {
            CGBitmapContextCreate(
                rgba.as_mut_ptr().cast(),
                width as usize,
                height as usize,
                8,
                width as usize * 4,
                Some(space),
                CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
            )
        }
        .ok_or(Error::Allocation)
    })?;
    let ctx = Some(&*context);
    CGContext::set_allows_antialiasing(ctx, true);
    CGContext::set_should_antialias(ctx, true);
    CGContext::set_allows_font_smoothing(ctx, false);
    CGContext::set_should_smooth_fonts(ctx, false);
    CGContext::scale_ctm(ctx, request.scale, request.scale);
    let c = request.color.map(|n| f64::from(n) / 255.);
    CGContext::set_rgb_fill_color(ctx, c[0], c[1], c[2], c[3]);
    unsafe {
        frame.draw(&context);
    }
    CGContext::flush(ctx);
    drop(context);
    for p in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for channel in &mut p[..3] {
                *channel = ((u32::from(*channel) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok(Bitmap {
        size: [width, height],
        rgba,
        fits,
        missing_glyphs: None,
    })
}
