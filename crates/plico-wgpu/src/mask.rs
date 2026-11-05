//! Coverage-mask intersection for nonrectangular clip stacks.
use crate::{Error, Result};
use kurbo::Rect;
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use std::{num::NonZeroUsize, rc::Rc, sync::Arc};
/// A retained physical-pixel alpha mask. Rect must be integer aligned, with
/// dimensions equal to the source image. Drawing outside it has zero coverage.
pub struct CoverageMask {
    pub image: ImageData,
    pub rect: [f32; 4],
}
impl CoverageMask {
    pub(crate) fn bounds(&self) -> Rect {
        let [x, y, w, h] = self.rect;
        Rect::new(x as f64, y as f64, (x + w) as f64, (y + h) as f64)
    }
}
type Key = (u64, u64, i64, i64);
pub(crate) struct MaskCache {
    entries: lru::LruCache<Key, (ImageData, usize)>,
    bytes: usize,
    empty: Option<ImageData>,
}
impl Default for MaskCache {
    fn default() -> Self {
        Self {
            entries: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            bytes: 0,
            empty: None,
        }
    }
}
impl MaskCache {
    pub fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.bytes)
    }
    pub fn intersect(
        &mut self,
        image: ImageData,
        bounds: Rect,
        parent: Option<&CoverageMask>,
    ) -> Result<Rc<CoverageMask>> {
        let Some(parent) = parent else {
            // The child raster already covers its own bounds; keep whatever
            // fractional edge the clip geometry produced clamped to it.
            let [x0, y0, x1, y1] = [
                bounds.x0.floor(),
                bounds.y0.floor(),
                (bounds.x1.ceil() as i64).min((bounds.x0 as i64) + image.width as i64) as f64,
                (bounds.y1.ceil() as i64).min((bounds.y0 as i64) + image.height as i64) as f64,
            ];
            return Ok(Rc::new(CoverageMask {
                image,
                rect: [x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32],
            }));
        };
        let outer = parent.bounds();
        // Align outward to whole pixels: CoverageMask rects must be integer
        // (the renderer validates it), and HiDPI blur halos inflate clips to
        // fractional bounds. Ceil keeps the mask covering everything the
        // floating-point intersection did, clamped back into the child image
        // so the rasterized coverage stays the source of the dimensions.
        let rect = bounds.intersect(outer);
        let rect = Rect::new(
            rect.x0.floor(),
            rect.y0.floor(),
            (rect.x1.ceil() as i64)
                .min((bounds.x0 as i64) + image.width as i64)
                .max(rect.x0.floor() as i64) as f64,
            (rect.y1.ceil() as i64)
                .min((bounds.y0 as i64) + image.height as i64)
                .max(rect.y0.floor() as i64) as f64,
        );
        // An empty intersection is represented by an all-zero one-pixel mask;
        // the independent clip rectangle prevents it from drawing anything.
        if rect.width() <= 0. || rect.height() <= 0. {
            return Ok(Rc::new(CoverageMask {
                image: self
                    .empty
                    .get_or_insert_with(|| ImageData {
                        data: Blob::new(Arc::new(vec![0; 4])),
                        format: ImageFormat::Rgba8,
                        alpha_type: ImageAlphaType::AlphaPremultiplied,
                        width: 1,
                        height: 1,
                    })
                    .clone(),
                rect: [0., 0., 1., 1.],
            }));
        }
        // Float world-coordinate precision must not turn coverage indexing into
        // an out-of-bounds read when very large translated content is clipped.
        let child_x = (rect.x0 - bounds.x0) as usize;
        let child_y = (rect.y0 - bounds.y0) as usize;
        let parent_x = (rect.x0 - outer.x0) as usize;
        let parent_y = (rect.y0 - outer.y0) as usize;
        let width = rect.width() as usize;
        let height = rect.height() as usize;
        if child_x + width > image.width as usize
            || child_y + height > image.height as usize
            || parent_x + width > parent.image.width as usize
            || parent_y + height > parent.image.height as usize
        {
            return Err(Error::InvalidInput {
                reason: "clip bounds exceed mask pixel dimensions".into(),
            });
        }
        let key = (
            image.data.id(),
            parent.image.data.id(),
            (bounds.x0 - outer.x0) as i64,
            (bounds.y0 - outer.y0) as i64,
        );
        let data = if let Some((image, _)) = self.entries.get(&key) {
            image.clone()
        } else {
            let (w, h) = (rect.width() as u32, rect.height() as u32);
            let bytes = w as usize * h as usize * 4;
            if bytes > 64 * 1024 * 1024 {
                return Err(Error::InvalidInput {
                    reason: "clip intersection exceeds 64 MiB".into(),
                });
            }
            let mut pixels = vec![0; bytes];
            for y in 0..h {
                for x in 0..w {
                    let child = ((y as usize + (rect.y0 - bounds.y0) as usize)
                        * image.width as usize
                        + x as usize
                        + (rect.x0 - bounds.x0) as usize)
                        * 4
                        + 3;
                    let parent_index = ((y as usize + (rect.y0 - outer.y0) as usize)
                        * parent.image.width as usize
                        + x as usize
                        + (rect.x0 - outer.x0) as usize)
                        * 4
                        + 3;
                    let alpha = ((u16::from(image.data.data()[child])
                        * u16::from(parent.image.data.data()[parent_index])
                        + 127)
                        / 255) as u8;
                    let i = (y as usize * w as usize + x as usize) * 4;
                    pixels[i..i + 4].fill(alpha);
                }
            }
            let image = ImageData {
                data: Blob::new(Arc::new(pixels)),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::AlphaPremultiplied,
                width: w,
                height: h,
            };
            while self.bytes + bytes > 64 * 1024 * 1024
                || self.entries.len() == self.entries.cap().get()
            {
                let Some((_, (_, bytes))) = self.entries.pop_lru() else {
                    break;
                };
                self.bytes -= bytes;
            }
            self.entries.put(key, (image.clone(), bytes));
            self.bytes += bytes;
            image
        };
        Ok(Rc::new(CoverageMask {
            image: data,
            rect: [
                rect.x0 as f32,
                rect.y0 as f32,
                rect.width() as f32,
                rect.height() as f32,
            ],
        }))
    }
}
