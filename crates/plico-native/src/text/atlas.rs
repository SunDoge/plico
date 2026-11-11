//! System-shaped glyphs and a fixed-budget, phase-quantized raster atlas.
use cairo::{Antialias, Context, FontOptions, Format, HintMetrics, HintStyle, ImageSurface};
use pango::prelude::*;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Fixed atlas side in physical texels.
pub const ATLAS_SIZE: u32 = 2048;
const UNIT: f64 = pango::SCALE as f64;
/// Errors shared with the native text service.
pub type Result<T> = std::result::Result<T, super::Error>;

#[derive(Clone, Debug)]
/// A system-resolved glyph retaining its font and physical position.
pub struct Glyph {
    /// Explicit span foreground; None uses the draw foreground.
    pub foreground: Option<[u8; 4]>,
    font: pango::Font,
    id: u32,
    /// Physical horizontal glyph origin.
    pub x: f64,
    /// Physical baseline, including shaping offset.
    pub y: f64,
    /// Physical ink x/y/width/height relative to the glyph baseline origin.
    pub ink_bounds: [f64; 4],
    // Drop after the native font.
    _fonts: Rc<RefCell<super::fonts::Fonts>>,
}
#[derive(Clone, Copy)]
/// Raster resource and placement relative to a glyph origin.
pub struct Entry {
    /// Atlas x, y, width and height.
    pub rect: [u32; 4],
    /// Raster offset from the baseline origin.
    pub origin: [i32; 2],
    /// Whether the glyph contains intrinsic color independent of foreground tint.
    pub colored: bool,
}
/// Positions are physical pixels; metrics are logical pixels from the same layout.
#[derive(Clone, Debug)]
pub struct GlyphLayout {
    clip: Option<LineClip>,
    native: pango::Layout,
    source: String,
    /// Font-resolved glyph positions.
    pub glyphs: Vec<Glyph>,
    /// Logical width and height, including whitespace.
    pub logical_size: [f64; 2],
    /// Logical first-line baseline.
    pub baseline: f64,
    /// Count of missing glyphs after system fallback.
    pub missing_glyphs: u32,
    /// Device scale used for shaping and rasterization.
    pub scale: f64,
    // Drop after the native layout and its glyphs, including empty paragraphs.
    _fonts: Rc<RefCell<super::fonts::Fonts>>,
}
#[derive(Clone, Debug)]
struct LineClip {
    end: usize,
    lines: usize,
    caret: [f64; 4],
}
/// Fixed-budget atlas. The caller may recycle storage at a frame boundary.
/// Four horizontal raster phases; vertical raster origins are pixel-aligned.
pub struct Atlas {
    shaper: LayoutContext,
    entries: HashMap<(pango::Font, u32, u8), Entry>,
    /// Premultiplied RGBA8 atlas storage.
    pub pixels: Vec<u8>,
    cursor: [u32; 2],
    row_height: u32,
    dirty: Vec<[u32; 4]>,
    /// Monotonic raster/cache generation counter.
    pub revision: usize,
    /// Cumulative glyph rectangle bytes rasterized.
    pub rasterized_bytes: usize,
}
impl Atlas {
    /// Create a private system-font context with grayscale AA and medium hinting.
    pub fn new() -> Result<Self> {
        let shaper = LayoutContext::new()?;
        Ok(Self {
            shaper,
            entries: HashMap::new(),
            pixels: vec![0; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize],
            cursor: [2, 2],
            row_height: 0,
            dirty: Vec::new(),
            revision: 0,
            rasterized_bytes: 0,
        })
    }
    /// Convenience shaping with the comparison font family and unit scale.
    pub fn shape(&self, text: &str, size: f64, weight: u16) -> Result<Vec<Glyph>> {
        let mut request = super::TextRequest::new(text);
        request.family = "Noto Sans, Noto Sans CJK SC, sans-serif";
        request.size = size;
        request.weight = weight;
        Ok(self.layout(&request)?.glyphs)
    }
    /// Shape once; retaining the layout does not retain a second atlas allocation.
    pub fn layout(&self, request: &super::TextRequest<'_>) -> Result<GlyphLayout> {
        self.shaper.layout(request)
    }
    /// Areas changed since the previous upload; each rectangle occurs only on a cache miss.
    pub fn take_dirty(&mut self) -> Vec<[u32; 4]> {
        std::mem::take(&mut self.dirty)
    }
    /// Rasterize or retrieve a glyph at one of four horizontal subpixel phases.
    pub fn get(&mut self, glyph: &Glyph, phase: u8) -> Result<Entry> {
        if phase >= 4 {
            return Err(super::Error::InvalidRequest);
        }
        let key = (glyph.font.clone(), glyph.id, phase);
        if let Some(entry) = self.entries.get(&key) {
            return Ok(*entry);
        }
        let mut string = pango::GlyphString::new();
        string.set_size(1);
        string.glyph_info_mut()[0].set_glyph(glyph.id);
        let geometry = string.glyph_info_mut()[0].geometry_mut();
        geometry.set_width(0);
        geometry.set_x_offset(0);
        geometry.set_y_offset(0);
        let (ink, _) = string.extents(&glyph.font);
        let left = (ink.x() as f64 / UNIT).floor() as i32 - 2;
        let top = (ink.y() as f64 / UNIT).floor() as i32 - 2;
        let width = (ink.width() as f64 / UNIT).ceil() as u32 + 6;
        let height = (ink.height() as f64 / UNIT).ceil() as u32 + 6;
        if self.cursor[0] + width + 2 > ATLAS_SIZE {
            self.cursor = [2, self.cursor[1] + self.row_height + 2];
            self.row_height = 0;
        }
        if width + 4 > ATLAS_SIZE || self.cursor[1] + height + 2 > ATLAS_SIZE {
            return Err(super::Error::AtlasFull);
        }
        let mut surface = ImageSurface::create(Format::ARgb32, width as i32, height as i32)
            .map_err(|source| super::Error::Cairo { source })?;
        {
            let cr = Context::new(&surface).map_err(|source| super::Error::Cairo { source })?;
            cr.set_source_rgba(1., 1., 1., 1.);
            cr.move_to(-left as f64 + phase as f64 / 4., -top as f64);
            pangocairo::functions::show_glyph_string(&cr, &glyph.font, &mut string);
        }
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface
            .data()
            .map_err(|source| super::Error::Pixels { source })?;
        let mut colored = false;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = y * stride + x * 4;
                let argb = u32::from_ne_bytes(
                    data[offset..offset + 4]
                        .try_into()
                        .expect("four-byte pixel"),
                );
                let a = (argb >> 24) as u8;
                let r = (argb >> 16) as u8;
                let g = (argb >> 8) as u8;
                let b = argb as u8;
                colored |= r != g || g != b;
                let dest = (((self.cursor[1] as usize + y) * ATLAS_SIZE as usize)
                    + self.cursor[0] as usize
                    + x)
                    * 4;
                // Keep premultiplied Cairo pixels, including color-font glyphs.
                self.pixels[dest..dest + 4].copy_from_slice(&[r, g, b, a]);
            }
        }
        // A color glyph can be entirely grayscale (e.g. a white/black symbol).
        // RGB equality alone cannot distinguish it from a tintable mask. Probe
        // with black foreground: fixed-color pixels stay unchanged, while a
        // monochrome glyph follows the foreground. This is cold-cache work only.
        if !colored {
            let mut black = ImageSurface::create(Format::ARgb32, width as i32, height as i32)
                .map_err(|source| super::Error::Cairo { source })?;
            {
                let cr = Context::new(&black).map_err(|source| super::Error::Cairo { source })?;
                cr.set_source_rgba(0., 0., 0., 1.);
                cr.move_to(-left as f64 + phase as f64 / 4., -top as f64);
                pangocairo::functions::show_glyph_string(&cr, &glyph.font, &mut string);
            }
            black.flush();
            let black_stride = black.stride() as usize;
            let black_data = black
                .data()
                .map_err(|source| super::Error::Pixels { source })?;
            'rows: for y in 0..height as usize {
                for x in 0..width as usize {
                    let offset = y * stride + x * 4;
                    if data[offset..offset + 4] != [0; 4]
                        && data[offset..offset + 4]
                            == black_data[y * black_stride + x * 4..y * black_stride + x * 4 + 4]
                    {
                        colored = true;
                        break 'rows;
                    }
                }
            }
        }
        let entry = Entry {
            rect: [self.cursor[0], self.cursor[1], width, height],
            origin: [left, top],
            colored,
        };
        self.cursor[0] += width + 2;
        self.row_height = self.row_height.max(height);
        self.revision += 1;
        self.rasterized_bytes += (width * height * 4) as usize;
        self.dirty.push(entry.rect);
        self.entries.insert(key, entry);
        Ok(entry)
    }
}
impl Atlas {
    /// Recycle storage at a frame boundary. Previously generated atlas UVs are invalid;
    /// positioned layouts remain usable and will regenerate their glyph rectangles.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.pixels.fill(0);
        self.cursor = [2, 2];
        self.row_height = 0;
        self.dirty.clear();
        self.dirty.push([0, 0, ATLAS_SIZE, ATLAS_SIZE]);
        self.revision += 1;
    }
    /// Current number of cached font/glyph/phase entries.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

impl GlyphLayout {
    /// Rust-owned source and glyph-vector capacity; excludes opaque Pango/font data.
    pub fn rust_payload_bytes(&self) -> usize {
        self.source.capacity() + self.glyphs.capacity() * std::mem::size_of::<Glyph>()
    }

    /// Original UTF-8 text used by both glyph drawing and editing geometry.
    pub fn text(&self) -> &str {
        &self.source
    }
    /// Convert a native-platform UTF-16 offset. Reject half-surrogate positions.
    pub fn byte_from_utf16(&self, offset: usize) -> Option<usize> {
        let mut units = 0;
        for (byte, ch) in self.source.char_indices() {
            if units == offset {
                return Some(byte);
            }
            units += ch.len_utf16();
            if units > offset {
                return None;
            }
        }
        (units == offset).then_some(self.source.len())
    }
    /// Convert a valid document UTF-8 byte boundary to native UTF-16 units.
    pub fn utf16_from_byte(&self, byte: usize) -> Option<usize> {
        self.source.get(..byte).map(|s| s.encode_utf16().count())
    }
    /// Logical caret x/y/width/height from the same native layout as glyphs.
    /// Use a one-physical-pixel caret and apply the widget's window transform once.
    pub fn caret_rect(&self, byte: usize) -> Option<[f64; 4]> {
        if !self.source.is_char_boundary(byte) {
            return None;
        }
        if let Some(clip) = &self.clip {
            if byte > clip.end {
                return None;
            }
            if byte == clip.end {
                return Some(clip.caret);
            }
        }
        let (strong, _) = self.native.cursor_pos(byte as i32);
        Some([
            strong.x() as f64 / UNIT / self.scale,
            strong.y() as f64 / UNIT / self.scale,
            1. / self.scale,
            strong.height() as f64 / UNIT / self.scale,
        ])
    }
    /// Original UTF-8 range of the visual line nearest a logical y coordinate.
    pub fn line_range_at(&self, y: f64) -> Option<std::ops::Range<usize>> {
        if !y.is_finite() {
            return None;
        }
        let mut iter = self.native.iter();
        let mut last = None;
        let mut count = 0;
        loop {
            if self.clip.as_ref().is_some_and(|clip| count >= clip.lines) {
                break;
            }
            if let Some(line) = iter.line_readonly() {
                let (_, bounds) = iter.line_extents();
                let start = line.start_index().max(0) as usize;
                let end = start + line.length().max(0) as usize;
                let end = self.clip.as_ref().map_or(end, |clip| end.min(clip.end));
                last = Some(start.min(end)..end);
                if y < f64::from(bounds.y() + bounds.height()) / UNIT / self.scale {
                    break;
                }
            }
            count += 1;
            if !iter.next_line() {
                break;
            }
        }
        last
    }
    /// Logical-position hit test, returning a UTF-8 byte boundary (not glyph index).
    pub fn hit_test(&self, x: f64, y: f64) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let (_, index, trailing) = self.native.xy_to_index(
            (x * self.scale * UNIT) as i32,
            (y * self.scale * UNIT) as i32,
        );
        let index = index.max(0) as usize;
        let rest = self.source.get(index..)?;
        let byte = index
            + rest
                .char_indices()
                .nth(trailing.max(0) as usize)
                .map_or(rest.len(), |(i, _)| i);
        Some(self.clip.as_ref().map_or(byte, |clip| byte.min(clip.end)))
    }
    /// Native bidi-aware selection fragments in logical coordinates. Input uses
    /// UTF-8 bytes; platform adapters convert UTF-16 only at their boundary.
    pub fn selection_rects(&self, range: std::ops::Range<usize>) -> Option<Vec<[f64; 4]>> {
        if range.start > range.end || self.source.get(range.clone()).is_none() {
            return None;
        }
        if range.is_empty() {
            return Some(Vec::new());
        }
        let mut iter = self.native.iter();
        let mut rects = Vec::new();
        loop {
            if let Some(line) = iter.line_readonly() {
                let start = line.start_index() as usize;
                if self.clip.as_ref().is_some_and(|clip| start >= clip.end) {
                    break;
                }
                let end = start + line.length() as usize;
                if range.start < end && range.end > start {
                    let (_, bounds) = iter.line_extents();
                    let ranges =
                        line.x_ranges(range.start.max(start) as i32, range.end.min(end) as i32);
                    for pair in ranges.as_chunks::<2>().0 {
                        rects.push([
                            pair[0].min(pair[1]) as f64 / UNIT / self.scale,
                            bounds.y() as f64 / UNIT / self.scale,
                            (pair[1] - pair[0]).abs() as f64 / UNIT / self.scale,
                            bounds.height() as f64 / UNIT / self.scale,
                        ]);
                    }
                }
            }
            if !iter.next_line() {
                break;
            }
        }
        Some(rects)
    }
}
impl GlyphLayout {
    /// Candidate-window anchor in logical layout coordinates. During composition,
    /// anchor to the marked range's start on the caret's visual line; otherwise
    /// use the active caret. Input offsets are UTF-8 bytes, validated before use.
    pub fn ime_candidate_rect(
        &self,
        caret: usize,
        marked: Option<std::ops::Range<usize>>,
    ) -> Option<[f64; 4]> {
        self.caret_rect(caret)?;
        let anchor = if let Some(marked) = marked {
            if self.source.get(marked.clone()).is_none()
                || !marked.contains(&caret) && caret != marked.end
            {
                return None;
            }
            let (line, _) = self.native.index_to_line_x(caret as i32, false);
            let start = self.native.line_readonly(line)?.start_index() as usize;
            marked.start.max(start)
        } else {
            caret
        };
        self.caret_rect(anchor)
    }
}

/// Native font/layout service without bitmap atlas allocation. Reuse it for
/// measurement and retain its layouts for drawing and editing geometry.
pub struct LayoutContext {
    context: pango::Context,
    fonts: Rc<RefCell<super::fonts::Fonts>>,
}
impl LayoutContext {
    /// Create an isolated system-font context with the same raster policy as Atlas.
    pub fn new() -> Result<Self> {
        let map = pangocairo::FontMap::for_font_type(cairo::FontType::FontTypeFt).ok_or(
            super::Error::FontCollection {
                reason: "FreeType Pango font map unavailable",
            },
        )?;
        let context = map.create_context();
        let mut options = FontOptions::new().map_err(|source| super::Error::Cairo { source })?;
        options.set_antialias(Antialias::Gray);
        options.set_hint_style(HintStyle::Medium);
        options.set_hint_metrics(HintMetrics::On);
        pangocairo::functions::context_set_font_options(&context, Some(&options));
        pangocairo::functions::context_set_resolution(&context, 96.);
        Ok(Self {
            context,
            fonts: Rc::new(RefCell::new(super::fonts::Fonts::new())),
        })
    }
    /// Register an in-memory application font in this context only.
    /// Existing layouts/glyphs keep the backing font alive when this context drops.
    pub fn register_font(&mut self, bytes: &[u8], alias: Option<&str>) -> Result<()> {
        let map = self
            .context
            .font_map()
            .ok_or(super::Error::InvalidRequest)?;
        self.fonts.borrow_mut().add(&map, bytes, alias)?;
        self.context.changed();
        Ok(())
    }
    /// Shape once using the default start alignment and no line truncation.
    pub fn layout(&self, request: &super::TextRequest<'_>) -> Result<GlyphLayout> {
        self.layout_styled(request, LayoutOptions::default())
    }
    /// Shape with native alignment, ellipsis and visible-line clipping.
    pub fn layout_styled(
        &self,
        request: &super::TextRequest<'_>,
        options: LayoutOptions,
    ) -> Result<GlyphLayout> {
        self.layout_rich(request, options, &[])
    }
    /// Shape UTF-8 range overrides in the same paragraph used for editing geometry.
    pub fn layout_rich(
        &self,
        request: &super::TextRequest<'_>,
        options: LayoutOptions,
        spans: &[LayoutSpan],
    ) -> Result<GlyphLayout> {
        let mut validation = request.clone();
        if validation.width == Some(0.) {
            validation.width = None;
        }
        super::validate(&validation)?;
        let layout = pango::Layout::new(&self.context);
        let mut description = pango::FontDescription::new();
        description.set_family(request.family);
        description.set_absolute_size(request.size * request.scale * UNIT);
        // Pango accepts the full validated numeric OpenType weight range.
        use pango::glib::translate::from_glib;
        description.set_weight(unsafe { from_glib(i32::from(request.weight)) });
        description.set_style(if request.italic {
            pango::Style::Italic
        } else {
            pango::Style::Normal
        });
        layout.set_font_description(Some(&description));
        layout.set_text(request.text);
        layout.set_wrap(pango::WrapMode::WordChar);
        layout.set_alignment(match options.alignment {
            Alignment::Start => pango::Alignment::Left,
            Alignment::Center => pango::Alignment::Center,
            Alignment::End => pango::Alignment::Right,
            Alignment::Justify => pango::Alignment::Left,
        });
        layout.set_justify(matches!(options.alignment, Alignment::Justify));
        if !options.letter_spacing.is_finite()
            || options.letter_spacing.abs() > 16384.
            || options
                .line_height
                .is_some_and(|v| !v.is_finite() || v <= 0. || v > 16384.)
        {
            return Err(super::Error::InvalidRequest);
        }
        let attrs = pango::AttrList::new();
        if options.letter_spacing != 0. {
            attrs.insert(pango::AttrInt::new_letter_spacing(
                (options.letter_spacing * request.scale * UNIT).round() as i32,
            ));
        }
        if let Some(height) = options.line_height {
            attrs.insert(pango::AttrInt::new_line_height_absolute(
                (height * request.scale * UNIT).round() as i32,
            ));
        }
        for span in spans {
            if span.range.start > span.range.end
                || span.range.end > request.text.len()
                || !request.text.is_char_boundary(span.range.start)
                || !request.text.is_char_boundary(span.range.end)
                || span.range.end > u32::MAX as usize
                || !span.size.is_finite()
                || span.size <= 0.
                || span.size > 4096.
                || !(1..=1000).contains(&span.weight)
                || span
                    .line_height
                    .is_some_and(|h| !h.is_finite() || h <= 0. || h > 16384.)
            {
                return Err(super::Error::InvalidRequest);
            }
        }
        // Resolve overlaps once. Pango's absolute/relative line-height attributes
        // are distinct types, so inserting overlapping overrides directly would
        // leave stale absolute heights underneath later natural-height spans.
        let mut events = Vec::with_capacity(spans.len() * 2);
        for (index, span) in spans
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.range.is_empty())
        {
            events.push((span.range.start, true, index));
            events.push((span.range.end, false, index));
        }
        events.sort_unstable();
        let mut active = std::collections::BTreeSet::new();
        let mut resolved = Vec::new();
        let mut cursor = 0;
        while cursor < events.len() {
            let start = events[cursor].0;
            while cursor < events.len() && events[cursor].0 == start {
                let (_, entering, index) = events[cursor];
                if entering {
                    active.insert(index);
                } else {
                    active.remove(&index);
                }
                cursor += 1;
            }
            if let (Some(&index), Some(&(end, _, _))) = (active.last(), events.get(cursor)) {
                let mut span = spans[index].clone();
                span.range = start..end;
                resolved.push(span);
            }
        }
        for span in &resolved {
            let insert = |mut attr: pango::Attribute| {
                attr.set_start_index(span.range.start as u32);
                attr.set_end_index(span.range.end as u32);
                attrs.change(attr);
            };
            insert(
                pango::AttrSize::new_size_absolute(
                    (span.size * request.scale * UNIT).round() as i32
                )
                .into(),
            );
            insert(pango::AttrInt::new_weight(unsafe { from_glib(i32::from(span.weight)) }).into());
            insert(
                pango::AttrInt::new_style(if span.italic {
                    pango::Style::Italic
                } else {
                    pango::Style::Normal
                })
                .into(),
            );
            insert(
                pango::AttrColor::new_foreground(
                    u16::from(span.color[0]) * 257,
                    u16::from(span.color[1]) * 257,
                    u16::from(span.color[2]) * 257,
                )
                .into(),
            );
            insert(pango::AttrInt::new_foreground_alpha(u16::from(span.color[3]) * 257).into());
            if let Some(height) = span.line_height.or(options.line_height) {
                insert(
                    pango::AttrInt::new_line_height_absolute(
                        (height * request.scale * UNIT).round() as i32,
                    )
                    .into(),
                );
            } else {
                insert(pango::AttrFloat::new_line_height(1.).into());
            }
        }
        layout.set_attributes(Some(&attrs));
        if options.ellipsis {
            layout.set_ellipsize(pango::EllipsizeMode::End);
            layout.set_height(-(options.max_lines.max(1).min(i32::MAX as usize) as i32));
        }
        if let Some(width) = request.width {
            layout.set_width((width * request.scale * UNIT).round() as i32);
        }
        let mut clip = if !options.ellipsis && options.max_lines > 0 {
            i32::try_from(options.max_lines)
                .ok()
                .and_then(|n| layout.line_readonly(n))
                .map(|line| LineClip {
                    end: line.start_index() as usize,
                    lines: options.max_lines,
                    caret: [0.; 4],
                })
        } else {
            None
        };
        let mut clipped_size = [0_f64; 2];
        let mut iter = layout.iter();
        let mut glyphs = Vec::new();
        loop {
            if let Some(clip) = &mut clip {
                let line = iter.line_readonly().unwrap();
                if line.start_index() as usize >= clip.end {
                    break;
                }
                let (_, bounds) = iter.line_extents();
                clipped_size[0] = clipped_size[0].max(bounds.width() as f64 / UNIT / request.scale);
                clipped_size[1] = (bounds.y() + bounds.height()) as f64 / UNIT / request.scale;
                clip.caret = [
                    line.index_to_x(clip.end as i32, false) as f64 / UNIT / request.scale,
                    bounds.y() as f64 / UNIT / request.scale,
                    1. / request.scale,
                    bounds.height() as f64 / UNIT / request.scale,
                ];
            }
            if let Some(run) = iter.run_readonly() {
                let (_, logical) = iter.run_extents();
                let baseline = iter.baseline() as f64 / UNIT;
                let font = run.item().analysis().font();
                let mut x = logical.x() as f64 / UNIT;
                let string = run.glyph_string();
                for (info, cluster) in string.glyph_info().iter().zip(string.log_clusters()) {
                    let byte = (run.item().offset() + cluster) as usize;
                    let g = info.geometry();
                    let (ink, _) = font.glyph_extents(info.glyph());
                    glyphs.push(Glyph {
                        _fonts: self.fonts.clone(),
                        foreground: resolved
                            .get(resolved.partition_point(|s| s.range.end <= byte))
                            .filter(|s| s.range.contains(&byte))
                            .map(|s| s.color),
                        font: font.clone(),
                        id: info.glyph(),
                        ink_bounds: [
                            ink.x() as f64 / UNIT,
                            ink.y() as f64 / UNIT,
                            ink.width() as f64 / UNIT,
                            ink.height() as f64 / UNIT,
                        ],
                        x: x + g.x_offset() as f64 / UNIT,
                        y: baseline + g.y_offset() as f64 / UNIT,
                    });
                    x += g.width() as f64 / UNIT;
                }
            }
            if !iter.next_run() {
                break;
            }
        }
        let (width, height) = layout.size();
        let logical_size = if clip.is_some() {
            clipped_size
        } else {
            [
                width as f64 / UNIT / request.scale,
                height as f64 / UNIT / request.scale,
            ]
        };
        Ok(GlyphLayout {
            clip,
            _fonts: self.fonts.clone(),
            native: layout.clone(),
            source: request.text.into(),
            glyphs,
            logical_size,
            baseline: layout.baseline() as f64 / UNIT / request.scale,
            missing_glyphs: layout.unknown_glyphs_count() as u32,
            scale: request.scale,
        })
    }
}
impl GlyphLayout {
    /// Logical line rectangles and baseline: x, y, width, height, baseline.
    pub fn line_metrics(&self) -> Vec<[f64; 5]> {
        let mut iter = self.native.iter();
        let mut lines = Vec::new();
        loop {
            let (_, r) = iter.line_extents();
            lines.push([
                r.x() as f64 / UNIT / self.scale,
                r.y() as f64 / UNIT / self.scale,
                r.width() as f64 / UNIT / self.scale,
                r.height() as f64 / UNIT / self.scale,
                iter.baseline() as f64 / UNIT / self.scale,
            ]);
            if self
                .clip
                .as_ref()
                .is_some_and(|clip| lines.len() >= clip.lines)
                || !iter.next_line()
            {
                break;
            }
        }
        lines
    }
    /// Word-wrap minimum and unwrapped maximum intrinsic widths, in logical pixels.
    pub fn intrinsic_widths(&self) -> [f64; 2] {
        let minimum = self.native.copy();
        minimum.set_ellipsize(pango::EllipsizeMode::None);
        minimum.set_wrap(pango::WrapMode::Word);
        minimum.set_width(0);
        let maximum = self.native.copy();
        maximum.set_ellipsize(pango::EllipsizeMode::None);
        maximum.set_width(-1);
        [
            minimum.size().0 as f64 / UNIT / self.scale,
            maximum.size().0 as f64 / UNIT / self.scale,
        ]
    }
}

/// Logical paragraph alignment, honoring Pango's automatic base direction.
#[derive(Clone, Copy, Debug, Default)]
pub enum Alignment {
    /// Start of the paragraph's writing direction.
    #[default]
    Start,
    /// Center within the supplied layout width.
    Center,
    /// End of the paragraph's writing direction.
    End,
    /// Distribute spacing to justify wrapped lines, excluding the final line.
    Justify,
}
/// Native paragraph presentation options beyond the basic glyph request.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutOptions {
    /// Alignment inside the available width.
    pub alignment: Alignment,
    /// Ellipsize the final visible line; currently one native paragraph.
    pub ellipsis: bool,
    /// Maximum visible lines; zero is unlimited without ellipsis and one with it.
    pub max_lines: usize,
    /// Extra logical pixels between characters.
    pub letter_spacing: f64,
    /// Absolute logical line-box height. None uses native font metrics.
    pub line_height: Option<f64>,
}

/// Complete style override for one UTF-8 range. Later spans take precedence.
#[derive(Clone, Debug)]
pub struct LayoutSpan {
    /// Byte range in the paragraph, on UTF-8 boundaries.
    pub range: std::ops::Range<usize>,
    /// Logical font size.
    pub size: f64,
    /// Numeric OpenType weight.
    pub weight: u16,
    /// Italic face selection.
    pub italic: bool,
    /// Absolute logical line height, if overridden.
    pub line_height: Option<f64>,
    /// Straight RGBA foreground.
    pub color: [u8; 4],
}

#[cfg(test)]
mod rich_tests {
    use super::*;
    #[test]
    #[ignore = "requires system fonts"]
    fn overlapping_spans_equal_explicit_disjoint_styles_and_validate_utf8() {
        let context = LayoutContext::new().unwrap();
        let request = crate::text::TextRequest::new("AB日本CD");
        let outer = LayoutSpan {
            range: 0..10,
            size: 18.,
            weight: 700,
            italic: true,
            line_height: Some(48.),
            color: [255, 0, 0, 255],
        };
        let inner = LayoutSpan {
            range: 2..8,
            size: 24.,
            weight: 400,
            italic: false,
            line_height: None,
            color: [0, 0, 255, 128],
        };
        let nested = context
            .layout_rich(
                &request,
                LayoutOptions::default(),
                &[outer.clone(), inner.clone()],
            )
            .unwrap();
        let mut left = outer.clone();
        left.range = 0..2;
        let mut right = outer;
        right.range = 8..10;
        let flat = context
            .layout_rich(
                &request,
                LayoutOptions::default(),
                &[left, inner.clone(), right],
            )
            .unwrap();
        assert_eq!(nested.logical_size, flat.logical_size);
        assert_eq!(nested.line_metrics(), flat.line_metrics());
        assert_eq!(nested.glyphs.len(), flat.glyphs.len());
        for (a, b) in nested.glyphs.iter().zip(&flat.glyphs) {
            assert_eq!(
                (a.id, a.x, a.y, a.foreground),
                (b.id, b.x, b.y, b.foreground)
            );
        }
        assert!(
            nested
                .glyphs
                .iter()
                .any(|g| g.foreground == Some(inner.color))
        );
        let mut invalid = inner;
        invalid.range.start = 3;
        assert!(
            context
                .layout_rich(&request, LayoutOptions::default(), &[invalid])
                .is_err()
        );
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;
    const FONT: &[u8] =
        include_bytes!("../../../../backend/examples/web-preview/public/DejaVuSans.ttf");
    #[test]
    #[ignore = "requires system fonts"]
    fn private_memory_font_alias_isolated_and_survives_context_drop() {
        let mut request = crate::text::TextRequest::new("WiABC");
        request.family = "Plico Private Native Font Test";
        request.size = 20.;
        let unrelated = LayoutContext::new().unwrap();
        let before = unrelated.layout(&request).unwrap();
        // Observe the actual resolved file, rather than relying on a family-name
        // match that might also succeed through system fallback.
        #[link(name = "pangoft2-1.0")]
        unsafe extern "C" {
            fn pango_fc_font_get_pattern(
                font: *mut std::ffi::c_void,
            ) -> *mut fontconfig_sys::FcPattern;
        }
        let layout = {
            let mut context = LayoutContext::new().unwrap();
            context.register_font(FONT, Some(request.family)).unwrap();
            let layout = context.layout(&request).unwrap();
            use pango::glib::translate::ToGlibPtr;
            // SAFETY: this context explicitly creates FreeType Pango fonts; the
            // borrowed pattern and its file string are valid while the font lives.
            unsafe {
                let font: *mut pango::ffi::PangoFont = layout.glyphs[0].font.to_glib_none().0;
                let pattern = pango_fc_font_get_pattern(font.cast());
                let mut path = std::ptr::null_mut();
                assert_eq!(
                    fontconfig_sys::FcPatternGetString(pattern, c"file".as_ptr(), 0, &mut path),
                    fontconfig_sys::FcResultMatch
                );
                assert!(
                    std::ffi::CStr::from_ptr(path.cast())
                        .to_bytes()
                        .starts_with(b"/proc/self/fd/")
                );
            }
            assert!(
                layout.glyphs.iter().all(|g| g
                    .font
                    .describe()
                    .family()
                    .unwrap()
                    .contains("DejaVu"))
            );
            layout
        };
        let after = unrelated.layout(&request).unwrap();
        assert_eq!(before.logical_size, after.logical_size);
        assert_eq!(
            before.glyphs[0].font.describe(),
            after.glyphs[0].font.describe()
        );
        let mut atlas = Atlas::new().unwrap();
        assert!(
            layout
                .glyphs
                .iter()
                .any(|g| atlas.get(g, 0).unwrap().rect[2] > 0)
        );
        let mut invalid = LayoutContext::new().unwrap();
        assert!(invalid.register_font(&[0, 1, 2, 3], None).is_err());
    }
}

#[cfg(test)]
mod line_clip_tests {
    use super::*;
    #[test]
    #[ignore = "requires system fonts"]
    fn line_clipping_preserves_shaped_prefix_and_bounds_editing_geometry() {
        let context = LayoutContext::new().unwrap();
        for content in [
            "\nsecond line after an empty first line 日本語",
            "alpha beta gamma delta epsilon zeta eta theta",
            "first\n\nthird 日本語\nfourth",
            "مرحبا بالعالم هذا نص طويل لاختبار الأسطر",
        ] {
            for scale in [1., 1.25, 2.] {
                let mut request = crate::text::TextRequest::new(content);
                request.width = Some(110.);
                request.size = 16.;
                request.scale = scale;
                let full = context.layout(&request).unwrap();
                let lines = full.line_metrics();
                assert!(lines.len() > 1);
                let mut iter = full.native.iter();
                for bounds in &lines {
                    let line = iter.line_readonly().unwrap();
                    let start = line.start_index() as usize;
                    let end = start + line.length() as usize;
                    assert_eq!(
                        full.line_range_at(bounds[1] + bounds[3] / 2.),
                        Some(start..end)
                    );
                    assert!(content.is_char_boundary(start) && content.is_char_boundary(end));
                    iter.next_line();
                }
                assert!(full.line_range_at(f64::NAN).is_none());
                assert_eq!(full.line_range_at(-100.), full.line_range_at(lines[0][1]));
                let last = lines.last().unwrap();
                assert_eq!(
                    full.line_range_at(1e6),
                    full.line_range_at(last[1] + last[3] / 2.)
                );
                for limit in [1, 2, 99] {
                    let clipped = context
                        .layout_styled(
                            &request,
                            LayoutOptions {
                                max_lines: limit,
                                ..Default::default()
                            },
                        )
                        .unwrap();
                    assert_eq!(clipped.line_metrics(), lines[..limit.min(lines.len())]);
                    for (a, b) in clipped.glyphs.iter().zip(&full.glyphs) {
                        assert_eq!((a.id, a.x, a.y, &a.font), (b.id, b.x, b.y, &b.font));
                    }
                    if let Some(clip) = &clipped.clip {
                        let last = clipped.line_metrics().last().copied().unwrap();
                        assert_eq!(clipped.logical_size[1], last[1] + last[3]);
                        let caret = clipped.caret_rect(clip.end).unwrap();
                        assert!(caret[1] < clipped.logical_size[1]);
                        assert_eq!(clipped.hit_test(0., 1e6), Some(clip.end));
                        assert!(
                            clipped
                                .selection_rects(0..content.len())
                                .unwrap()
                                .iter()
                                .all(|r| r[1] + r[3] <= clipped.logical_size[1] + 0.001)
                        );
                        for offset in content
                            .char_indices()
                            .map(|(i, _)| i)
                            .filter(|i| *i > clip.end)
                        {
                            assert!(clipped.caret_rect(offset).is_none());
                        }
                        let expected_count = {
                            let mut iter = full.native.iter();
                            let mut count = 0;
                            loop {
                                if iter.line_readonly().unwrap().start_index() as usize >= clip.end
                                {
                                    break;
                                }
                                if let Some(run) = iter.run_readonly() {
                                    count += run.glyph_string().glyph_info().len();
                                }
                                if !iter.next_run() {
                                    break;
                                }
                            }
                            count
                        };
                        assert_eq!(clipped.glyphs.len(), expected_count);
                    } else {
                        assert_eq!(clipped.glyphs.len(), full.glyphs.len());
                    }
                }
            }
        }
    }
}
