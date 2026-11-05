//! Canvas-backed ordinary text. Measurement and rasterization use one layout.
//! Caret geometry currently assumes LTR visual order; full bidi editing is pending.
use crate::{Error, Renderer, Result};
use kurbo::Rect;
use peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use plico_renderer::{
    TextContext,
    paragraph::{
        NativeParagraph, ParagraphEngine, ParagraphError, ParagraphMetrics, ParagraphRequest,
    },
    style::TextAlign,
    text::FontAsset,
};
use std::{cell::RefCell, num::NonZeroUsize, rc::Rc, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};
fn error(message: impl Into<String>) -> ParagraphError {
    ParagraphError {
        message: message.into(),
    }
}
fn context() -> std::result::Result<(HtmlCanvasElement, CanvasRenderingContext2d), ParagraphError> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| error("browser document unavailable"))?;
    let canvas = document
        .create_element("canvas")
        .map_err(|_| error("cannot create text canvas"))?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|_| error("text canvas unavailable"))?;
    let options = js_sys::Object::new();
    js_sys::Reflect::set(
        &options,
        &JsValue::from_str("willReadFrequently"),
        &JsValue::TRUE,
    )
    .map_err(|_| error("cannot configure text canvas"))?;
    let ctx = canvas
        .get_context_with_context_options("2d", &options)
        .map_err(|_| error("cannot create text context"))?
        .ok_or_else(|| error("2D canvas unavailable"))?
        .dyn_into()
        .map_err(|_| error("invalid text canvas context"))?;
    Ok((canvas, ctx))
}
fn font(
    ctx: &CanvasRenderingContext2d,
    r: &ParagraphRequest,
) -> std::result::Result<(), ParagraphError> {
    ctx.set_font(&format!(
        "{} {} {}px {}",
        if r.italic { "italic" } else { "normal" },
        r.font_weight,
        r.font_size,
        r.family
    ));
    let field = JsValue::from_str("letterSpacing");
    if js_sys::Reflect::has(ctx.as_ref(), &field).unwrap_or(false) {
        js_sys::Reflect::set(
            ctx.as_ref(),
            &field,
            &JsValue::from_str(&format!("{}px", r.letter_spacing)),
        )
        .map_err(|_| error("cannot set text letter spacing"))?;
    } else if r.letter_spacing != 0. {
        return Err(error("browser does not support Canvas letter spacing"));
    }
    Ok(())
}
fn width(ctx: &CanvasRenderingContext2d, text: &str) -> std::result::Result<f64, ParagraphError> {
    ctx.measure_text(text)
        .map(|m| m.width())
        .map_err(|_| error("Canvas text measurement failed"))
}
struct Line {
    start: usize,
    end: usize,
    draw: String,
    x: f32,
    y: f32,
    stops: Vec<usize>,
}
struct Paragraph {
    ctx: CanvasRenderingContext2d,
    request: Arc<ParagraphRequest>,
    lines: Vec<Line>,
    metrics: ParagraphMetrics,
    ink: Rect,
}
impl Paragraph {
    fn new(
        ctx: &CanvasRenderingContext2d,
        r: Arc<ParagraphRequest>,
    ) -> std::result::Result<Self, ParagraphError> {
        if !r.font_size.is_finite() || r.font_size <= 0. || r.text.len() > 262144 {
            return Err(error("invalid or oversized Canvas paragraph"));
        }
        if !r.runs.is_empty() {
            return Err(error("Canvas rich text spans are not implemented yet"));
        }
        font(ctx, &r)?;
        let m = ctx
            .measure_text("Mg国")
            .map_err(|_| error("cannot measure font metrics"))?;
        let ascent = m
            .actual_bounding_box_ascent()
            .max(f64::from(r.font_size) * 0.75) as f32;
        let descent = m
            .actual_bounding_box_descent()
            .max(f64::from(r.font_size) * 0.2) as f32;
        let lh = r
            .line_height
            .map_or(
                r.font_size * 1.35,
                |(v, m)| if m { v * r.font_size } else { v },
            )
            .max(1.);
        let baseline = (lh - ascent - descent) * 0.5 + ascent;
        let limit = if r.wrap || r.ellipsis {
            r.width.filter(|v| v.is_finite()).map(|v| v.max(0.) as f64)
        } else {
            None
        };
        let mut ranges = Vec::new();
        let mut base = 0;
        for raw in r.text.split('\n') {
            let hard = raw.trim_end_matches('\r');
            let boundaries: Vec<_> = hard
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .chain([hard.len()])
                .collect();
            let mut from = 0;
            if hard.is_empty() {
                ranges.push((base, base));
            }
            while from < hard.len() {
                let mut end = hard.len();
                if let Some(limit) = limit.filter(|_| r.wrap) {
                    let mut previous = from;
                    for &to in boundaries.iter().filter(|&&n| n > from) {
                        if width(ctx, &hard[from..to])? > limit && previous > from {
                            end = previous;
                            break;
                        }
                        previous = to;
                    }
                    if end < hard.len()
                        && let Some(boundary) = hard[from..]
                            .split_word_bound_indices()
                            .map(|(i, s)| from + i + s.len())
                            .rfind(|&n| n > from && n <= end)
                    {
                        end = boundary;
                    }
                }
                ranges.push((base + from, base + end));
                from = end;
            }
            base += raw.len() + 1;
        }
        let hidden = r.max_lines > 0 && ranges.len() > r.max_lines;
        if r.max_lines > 0 {
            ranges.truncate(r.max_lines);
        }
        let mut lines = Vec::new();
        let mut max_width = 0f32;
        let mut ink = Rect::ZERO;
        let mut has_ink = false;
        for (index, (start, mut end)) in ranges.into_iter().enumerate() {
            let mut draw = r.text[start..end].to_string();
            if r.ellipsis
                && ((hidden && index + 1 == r.max_lines)
                    || limit.is_some_and(|n| width(ctx, &draw).unwrap_or(f64::INFINITY) > n))
            {
                if let Some(limit) = limit {
                    while !draw.is_empty() && width(ctx, &format!("{draw}…"))? > limit {
                        let (last, _) = draw.grapheme_indices(true).next_back().unwrap();
                        draw.truncate(last);
                        end = start + last;
                    }
                }
                draw.push('…');
            }
            let advance = width(ctx, &draw)? as f32;
            max_width = max_width.max(advance);
            let available = r.width.unwrap_or(advance).max(advance);
            let x = match r.align {
                TextAlign::Center => (available - advance) * 0.5,
                TextAlign::End => available - advance,
                _ => 0.,
            };
            let y = index as f32 * lh + baseline;
            let mut stops = Vec::new();
            for byte in r.text[start..end]
                .grapheme_indices(true)
                .map(|(i, _)| start + i)
                .chain([end])
            {
                stops.push(byte);
            }
            let m = ctx
                .measure_text(&draw)
                .map_err(|_| error("cannot measure paragraph ink"))?;
            if m.actual_bounding_box_left() + m.actual_bounding_box_right() > 0. {
                let rect = Rect::new(
                    f64::from(x) - m.actual_bounding_box_left(),
                    f64::from(y) - m.actual_bounding_box_ascent(),
                    f64::from(x) + m.actual_bounding_box_right(),
                    f64::from(y) + m.actual_bounding_box_descent(),
                );
                ink = if has_ink { ink.union(rect) } else { rect };
                has_ink = true;
            }
            lines.push(Line {
                start,
                end,
                draw,
                x,
                y,
                stops,
            });
        }
        let intrinsic_min = r
            .text
            .split_word_bounds()
            .map(|s| width(ctx, s).unwrap_or(0.) as f32)
            .fold(0f32, f32::max);
        let intrinsic_max = r
            .text
            .split('\n')
            .map(|s| width(ctx, s).unwrap_or(0.) as f32)
            .fold(0f32, f32::max);
        let metrics = ParagraphMetrics {
            width: if r.wrap {
                r.width.map_or(max_width, |w| w.min(max_width))
            } else {
                max_width
            },
            height: lines.len() as f32 * lh,
            intrinsic: [intrinsic_min, intrinsic_max],
            baselines: [Some(baseline), lines.last().map(|l| l.y)],
            first_line_height: lh,
        };
        Ok(Self {
            ctx: ctx.clone(),
            request: r,
            lines,
            metrics,
            ink,
        })
    }
    // Measure only requested caret prefixes, rather than shaping every prefix
    // during layout (quadratic work for long unwrapped lines).
    fn x_at(&self, line: &Line, byte: usize) -> f32 {
        let stop = line.stops.partition_point(|&b| b <= byte).saturating_sub(1);
        let byte = line.stops[stop];
        let _ = font(&self.ctx, &self.request);
        line.x + width(&self.ctx, &self.request.text[line.start..byte]).unwrap_or(0.) as f32
    }
    fn bytes(&self) -> usize {
        self.request.text.len()
            + self
                .lines
                .iter()
                .map(|l| {
                    std::mem::size_of::<Line>()
                        + l.draw.capacity()
                        + l.stops.capacity() * std::mem::size_of::<usize>()
                })
                .sum::<usize>()
    }
}
impl NativeParagraph for Paragraph {
    fn metrics(&self) -> ParagraphMetrics {
        self.metrics
    }
    fn hit_test(&self, x: f32, y: f32) -> usize {
        let index = ((y / self.metrics.first_line_height).floor().max(0.) as usize)
            .min(self.lines.len().saturating_sub(1));
        let Some(line) = self.lines.get(index) else {
            return 0;
        };
        let mut low = 0;
        let mut high = line.stops.len();
        while low < high {
            let mid = (low + high) / 2;
            if self.x_at(line, line.stops[mid]) < x {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let right = low.min(line.stops.len() - 1);
        let left = right.saturating_sub(1);
        if (self.x_at(line, line.stops[left]) - x).abs()
            <= (self.x_at(line, line.stops[right]) - x).abs()
        {
            line.stops[left]
        } else {
            line.stops[right]
        }
    }

    fn caret_rect(&self, byte: usize) -> Option<[f32; 4]> {
        let (index, line) = self
            .lines
            .iter()
            .enumerate()
            .find(|(_, l)| byte >= l.start && byte <= l.end)
            .or_else(|| self.lines.iter().enumerate().next_back())?;
        let x = self.x_at(line, byte);
        let y = index as f32 * self.metrics.first_line_height;
        Some([x, y, x + 1., y + self.metrics.first_line_height])
    }
    fn selection_rects(&self, range: std::ops::Range<usize>) -> Vec<[f32; 4]> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let from = range.start.max(l.start);
                let to = range.end.min(l.end);
                if from >= to {
                    return None;
                }
                let at = |byte| self.x_at(l, byte);
                Some([
                    at(from),
                    i as f32 * self.metrics.first_line_height,
                    at(to),
                    (i + 1) as f32 * self.metrics.first_line_height,
                ])
            })
            .collect()
    }
}
type Key = (u64, u64);
pub(crate) struct Cache {
    canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    paragraphs: lru::LruCache<u64, Rc<Paragraph>>,
    images: lru::LruCache<Key, (Arc<ParagraphRequest>, ImageData, [f64; 2])>,
    bytes: usize,
    paragraph_bytes: usize,
}
impl Cache {
    fn new() -> std::result::Result<Self, ParagraphError> {
        let (canvas, ctx) = context()?;
        Ok(Self {
            canvas,
            ctx,
            paragraphs: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            images: lru::LruCache::new(NonZeroUsize::new(256).unwrap()),
            bytes: 0,
            paragraph_bytes: 0,
        })
    }
    fn layout(
        &mut self,
        r: Arc<ParagraphRequest>,
    ) -> std::result::Result<Rc<Paragraph>, ParagraphError> {
        let key = r.fingerprint();
        if let Some(p) = self.paragraphs.get(&key).filter(|p| p.request == r) {
            return Ok(p.clone());
        }
        let p = Rc::new(Paragraph::new(&self.ctx, r)?);
        let bytes = p.bytes();
        while self.paragraph_bytes + bytes > 8 * 1024 * 1024 || self.paragraphs.len() >= 256 {
            if let Some((_, old)) = self.paragraphs.pop_lru() {
                self.paragraph_bytes -= old.bytes();
            } else {
                break;
            }
        }
        if bytes <= 8 * 1024 * 1024 {
            if let Some(old) = self.paragraphs.put(key, p.clone()) {
                self.paragraph_bytes -= old.bytes();
            }
            self.paragraph_bytes += bytes;
        }
        Ok(p)
    }
    pub(crate) fn image(
        &mut self,
        r: Arc<ParagraphRequest>,
        scale: f64,
    ) -> std::result::Result<Option<(ImageData, [f64; 2])>, ParagraphError> {
        let key = (r.fingerprint(), scale.to_bits());
        if let Some((old, image, at)) = self.images.get(&key).filter(|(old, _, _)| *old == r) {
            let _ = old;
            return Ok(Some((image.clone(), *at)));
        }
        let p = self.layout(r.clone())?;
        if p.ink.is_zero_area() {
            return Ok(None);
        }
        let at = [
            (p.ink.x0 * scale).floor() - 1.,
            (p.ink.y0 * scale).floor() - 1.,
        ];
        let width = ((p.ink.x1 * scale).ceil() - at[0] + 1.).max(1.) as u32;
        let height = ((p.ink.y1 * scale).ceil() - at[1] + 1.).max(1.) as u32;
        let bytes = u64::from(width) * u64::from(height) * 4;
        if bytes > 32 * 1024 * 1024 || width > 8192 || height > 8192 {
            return Err(error("Canvas paragraph raster exceeds budget"));
        }
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        font(&self.ctx, &r)?;
        self.ctx
            .set_transform(scale, 0., 0., scale, -at[0], -at[1])
            .map_err(|_| error("invalid Canvas text transform"))?;
        self.ctx.set_text_baseline("alphabetic");
        self.ctx.set_fill_style_str(&format!(
            "rgba({},{},{},{})",
            r.color[0],
            r.color[1],
            r.color[2],
            f64::from(r.color[3]) / 255.
        ));
        for line in &p.lines {
            self.ctx
                .fill_text(&line.draw, f64::from(line.x), f64::from(line.y))
                .map_err(|_| error("Canvas text rasterization failed"))?;
        }
        let data = self
            .ctx
            .get_image_data(0., 0., f64::from(width), f64::from(height))
            .map_err(|_| error("Canvas text readback failed"))?
            .data()
            .0;
        while self.bytes + bytes as usize > 32 * 1024 * 1024 || self.images.len() >= 256 {
            if let Some((_, (_, old, _))) = self.images.pop_lru() {
                self.bytes -= (old.width * old.height * 4) as usize;
            } else {
                break;
            }
        }
        let image = ImageData {
            data: Blob::new(Arc::new(data)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        };
        if let Some((_, old, _)) = self.images.put(key, (r, image.clone(), at)) {
            self.bytes -= (old.width * old.height * 4) as usize;
        }
        self.bytes += bytes as usize;
        Ok(Some((image, at)))
    }
}
struct Engine(Rc<RefCell<Cache>>);
impl ParagraphEngine for Engine {
    fn name(&self) -> &'static str {
        "canvas2d-wgpu"
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn layout(
        &mut self,
        r: Arc<ParagraphRequest>,
    ) -> std::result::Result<Rc<dyn NativeParagraph>, ParagraphError> {
        self.0
            .borrow_mut()
            .layout(r)
            .map(|p| p as Rc<dyn NativeParagraph>)
    }
    fn register_font(&mut self, _: &FontAsset) {} // Browser fonts are supplied by CSS/FontFace.
    fn clear(&mut self) {
        let mut c = self.0.borrow_mut();
        c.paragraphs.clear();
        c.images.clear();
        c.bytes = 0;
        c.paragraph_bytes = 0;
    }
}
impl Renderer {
    pub(crate) fn configure_web_text(&mut self, text: &mut TextContext) -> Result<()> {
        if self.web_paragraphs.is_none() {
            self.web_paragraphs = Some(Rc::new(RefCell::new(
                Cache::new().map_err(|source| Error::Paragraph { source })?,
            )));
        }
        if !text.has_native_paragraphs() {
            text.set_paragraph_engine(Box::new(Engine(
                self.web_paragraphs.as_ref().unwrap().clone(),
            )));
        }
        Ok(())
    }
    pub(crate) fn web_paragraph_image(
        &mut self,
        r: Arc<ParagraphRequest>,
        scale: f64,
    ) -> Result<Option<(ImageData, [f64; 2])>> {
        if self.web_paragraphs.is_none() {
            self.web_paragraphs = Some(Rc::new(RefCell::new(
                Cache::new().map_err(|source| Error::Paragraph { source })?,
            )));
        }
        self.web_paragraphs
            .as_ref()
            .unwrap()
            .borrow_mut()
            .image(r, scale)
            .map_err(|source| Error::Paragraph { source })
    }
}
