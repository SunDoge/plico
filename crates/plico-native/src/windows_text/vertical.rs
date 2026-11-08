//! DirectWrite vertical reading/flow and native oriented glyph-run rasterization.
use super::*;
use crate::vertical::{Bitmap, Request as VerticalRequest};

pub(crate) fn rasterize(context: &Context, request: &VerticalRequest<'_>) -> Result<Bitmap> {
    let native = Request {
        text: request.text,
        family: if request.family.is_empty() {
            "Segoe UI"
        } else {
            request.family
        },
        size: request.size as f32,
        weight: request.weight,
        ..Request::new(request.text)
    };
    let paragraph = context.layout(&native)?;
    // SAFETY: The retained native layout is local to this call; no other owner
    // observes its metrics/editing geometry while vertical properties change.
    unsafe {
        paragraph
            .layout
            .SetReadingDirection(DWRITE_READING_DIRECTION_TOP_TO_BOTTOM)
            .context(NativeSnafu)?;
        paragraph
            .layout
            .SetFlowDirection(DWRITE_FLOW_DIRECTION_RIGHT_TO_LEFT)
            .context(NativeSnafu)?;
        paragraph
            .layout
            .SetMaxWidth(request.bounds[0] as f32)
            .context(NativeSnafu)?;
        paragraph
            .layout
            .SetMaxHeight(request.bounds[1] as f32)
            .context(NativeSnafu)?;
        paragraph
            .layout
            .SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)
            .context(NativeSnafu)?;
        let layout: IDWriteTextLayout2 = paragraph.layout.cast().context(NativeSnafu)?;
        layout
            .SetVerticalGlyphOrientation(DWRITE_VERTICAL_GLYPH_ORIENTATION_DEFAULT)
            .context(NativeSnafu)?;
    }
    // This path uses IDWriteTextRenderer1, including the orientation supplied
    // by DirectWrite. Horizontal Paragraph metrics are never exposed as vertical.
    let raster = paragraph.rasterize(request.scale as f32, [0., 0.])?;
    let masks = raster.colorize(request.color.map(|v| f32::from(v) / 255.), false)?;
    let mut bounds = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for mask in &masks {
        bounds[0] = bounds[0].min(mask.origin[0]);
        bounds[1] = bounds[1].min(mask.origin[1]);
        bounds[2] = bounds[2].max(mask.origin[0] + mask.size[0] as i32);
        bounds[3] = bounds[3].max(mask.origin[1] + mask.size[1] as i32);
    }
    if masks.is_empty() {
        return Ok(Bitmap {
            size: [1, 1],
            rgba: vec![0; 4],
            fits: true,
            missing_glyphs: None,
        });
    }
    let width = (bounds[2] - bounds[0]) as u32;
    let height = (bounds[3] - bounds[1]) as u32;
    if u64::from(width) * u64::from(height) * 4 > 64 * 1024 * 1024 {
        return Err(Error::InvalidRequest);
    }
    let mut rgba = vec![0_u8; width as usize * height as usize * 4];
    for mask in masks {
        for y in 0..mask.size[1] as usize {
            for x in 0..mask.size[0] as usize {
                let source = &mask.rgba[(y * mask.size[0] as usize + x) * 4..][..4];
                let dx = (mask.origin[0] - bounds[0]) as usize + x;
                let dy = (mask.origin[1] - bounds[1]) as usize + y;
                let target = &mut rgba[(dy * width as usize + dx) * 4..][..4];
                let alpha = u32::from(source[3]);
                for c in 0..4 {
                    target[c] = (u32::from(source[c])
                        + (u32::from(target[c]) * (255 - alpha) + 127) / 255)
                        .min(255) as u8;
                }
            }
        }
    }
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(pixel[3]);
        if a > 0 && a < 255 {
            for c in &mut pixel[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Ok(Bitmap {
        size: [width, height],
        rgba,
        fits: true, // All runs are retained; common code checks native ink bounds.
        missing_glyphs: None,
    })
}
