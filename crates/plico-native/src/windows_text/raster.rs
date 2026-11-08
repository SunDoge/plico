//! Capture the retained layout's native glyph runs; no reshaping or bitmap scaling.
use super::{Error, Paragraph, Result};
use snafu::ResultExt;
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ops::Range,
    rc::Rc,
};
use windows::{
    Win32::{
        Foundation::{DWRITE_E_NOCOLOR, E_FAIL, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER},
        Graphics::DirectWrite::*,
    },
    core::{BOOL, IUnknown, IUnknownImpl, Interface, Ref, implement},
};
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Raw single-channel native coverage for one resolved glyph run.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphMask {
    /// Original layout range in UTF-16 units, not glyph indices.
    pub utf16_range: Range<u32>,
    /// Physical top-left offset from the paragraph origin.
    pub origin: [i32; 2],
    /// Physical width/height.
    pub size: [u32; 2],
    /// Raw grayscale coverage, tightly packed; native gamma is not applied.
    pub coverage: Vec<u8>,
    /// Straight palette RGBA for a color layer; None uses the authored foreground.
    pub palette: Option<[f32; 4]>,
    /// Range foreground override captured from a native drawing effect.
    pub foreground: Option<[u8; 4]>,
}
/// Corrected premultiplied sRGB RGBA8 for one run; no native resources retained.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorMask {
    /// Original source UTF-16 range.
    pub utf16_range: Range<u32>,
    /// Physical top-left offset from the paragraph origin.
    pub origin: [i32; 2],
    /// Physical bitmap width/height.
    pub size: [u32; 2],
    /// Corrected, tightly packed premultiplied sRGB pixels.
    pub rgba: Vec<u8>,
}
/// Monochrome run masks and native parameters for a future compositor.
pub struct Rasterized {
    /// Ordered native glyph-run masks; no borrowed COM/font pointers.
    pub masks: Vec<GlyphMask>,
    /// Native rendering gamma. This metadata is not a pow(alpha, gamma) recipe.
    pub gamma: f32,
    /// Native grayscale enhanced contrast, not a synthetic font weight.
    pub grayscale_contrast: f32,
}
impl Rasterized {
    /// Correct and tint raw masks without reshaping/rasterizing. Color/opacity
    /// changes can reuse the same coverage. `thin_font` explicitly selects the
    /// legacy-family policy; automatic per-resolved-font detection is not provided.
    /// Output is bounded to 64 MiB, independently of the retained raw coverage.
    pub fn colorize(&self, foreground: [f32; 4], thin_font: bool) -> Result<Vec<ColorMask>> {
        let correction = crate::directwrite_correction::GrayscaleCorrection::new(
            self.gamma,
            self.grayscale_contrast,
        )
        .context(super::CorrectionSnafu)?;
        let foreground_correction = correction
            .prepare(foreground, thin_font)
            .context(super::CorrectionSnafu)?;
        if self.masks.len() > 4096 {
            return Err(Error::InvalidRequest);
        }
        let mut total = 0usize;
        for mask in &self.masks {
            let pixels = (mask.size[0] as usize)
                .checked_mul(mask.size[1] as usize)
                .ok_or(Error::InvalidRequest)?;
            if mask.size.iter().any(|v| *v > 16384)
                || pixels != mask.coverage.len()
                || mask.utf16_range.start > mask.utf16_range.end
                || mask
                    .palette
                    .is_some_and(|c| !c.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v)))
            {
                return Err(Error::InvalidRequest);
            }
            total = total
                .checked_add(pixels.checked_mul(4).ok_or(Error::InvalidRequest)?)
                .filter(|v| *v <= MAX_BYTES)
                .ok_or(Error::InvalidRequest)?;
        }
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.masks.len())
            .map_err(|_| Error::InvalidRequest)?;
        for mask in &self.masks {
            if mask.coverage.is_empty() {
                continue;
            }
            let brush = mask
                .foreground
                .map(|c| c.map(|v| v as f32 / 255.))
                .unwrap_or(foreground);
            let prepared = if let Some(mut color) = mask.palette {
                color[3] *= brush[3];
                correction
                    .prepare(color, false)
                    .context(super::CorrectionSnafu)?
            } else if mask.foreground.is_some() {
                correction
                    .prepare(brush, thin_font)
                    .context(super::CorrectionSnafu)?
            } else {
                foreground_correction
            };
            let mut rgba = Vec::new();
            rgba.try_reserve_exact(mask.coverage.len() * 4)
                .map_err(|_| Error::InvalidRequest)?;
            for coverage in &mask.coverage {
                rgba.extend_from_slice(&prepared.blend_u8(*coverage));
            }
            result.push(ColorMask {
                utf16_range: mask.utf16_range.clone(),
                origin: mask.origin,
                size: mask.size,
                rgba,
            });
        }
        Ok(result)
    }
}
#[derive(Default)]
struct Output {
    masks: Vec<GlyphMask>,
    bytes: usize,
    glyphs: usize,
}
// The collector owns thread-local Rc/RefCell state; do not advertise COM agility.
#[implement(IDWriteTextRenderer1, Agile = false)]
struct Collector {
    factory: IDWriteFactory2,
    params: IDWriteRenderingParams,
    scale: f32,
    phase: [f32; 2],
    units: u32,
    visible_units: Option<u32>,
    ellipsis: Option<IDWriteInlineObject>,
    in_ellipsis: Cell<bool>,
    orientation: Cell<DWRITE_GLYPH_ORIENTATION_ANGLE>,
    output: Rc<RefCell<Output>>,
}
fn native_error(code: windows::core::HRESULT) -> windows::core::Error {
    windows::core::Error::from_hresult(code)
}
#[allow(non_snake_case)]
impl IDWritePixelSnapping_Impl for Collector_Impl {
    fn IsPixelSnappingDisabled(&self, _context: *const c_void) -> windows::core::Result<BOOL> {
        Ok(BOOL(1))
    }
    fn GetCurrentTransform(
        &self,
        _context: *const c_void,
        transform: *mut DWRITE_MATRIX,
    ) -> windows::core::Result<()> {
        if transform.is_null() {
            return Err(native_error(E_POINTER));
        }
        // SAFETY: DirectWrite supplies writable transform output storage.
        unsafe {
            *transform = DWRITE_MATRIX {
                m11: 1.,
                m22: 1.,
                ..Default::default()
            };
        }
        Ok(())
    }
    fn GetPixelsPerDip(&self, _context: *const c_void) -> windows::core::Result<f32> {
        Ok(self.scale)
    }
}
#[allow(non_snake_case)]
impl IDWriteTextRenderer_Impl for Collector_Impl {
    fn DrawGlyphRun(
        &self,
        _context: *const c_void,
        x: f32,
        y: f32,
        measuring: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        description: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        let foreground = super::color::read(effect)?;
        if run.is_null() {
            return Err(native_error(E_POINTER));
        }
        let ellipsis_description = DWRITE_GLYPH_RUN_DESCRIPTION {
            textPosition: self.units,
            ..Default::default()
        };
        let description = if self.in_ellipsis.get() {
            &ellipsis_description
        } else {
            // SAFETY: native callback description lives through this call.
            unsafe { description.as_ref() }.ok_or_else(|| native_error(E_POINTER))?
        };
        // SAFETY: native run and all arrays live through this synchronous call.
        let run = unsafe { &*run };
        if run.glyphCount == 0 {
            return Ok(());
        }
        if run.glyphCount > 100_000
            || run.glyphIndices.is_null()
            || !run.fontEmSize.is_finite()
            || run.fontEmSize <= 0.
            || !x.is_finite()
            || !y.is_finite()
        {
            return Err(native_error(E_INVALIDARG));
        }
        let end = description
            .textPosition
            .checked_add(description.stringLength)
            .filter(|n| *n <= self.units)
            .ok_or_else(|| native_error(E_INVALIDARG))?;
        if let Some(limit) = self.visible_units {
            if description.textPosition >= limit {
                return Ok(());
            }
            if end > limit {
                return Err(native_error(E_NOTIMPL));
            }
        }
        let face = run
            .fontFace
            .as_ref()
            .ok_or_else(|| native_error(E_INVALIDARG))?;
        let face2: IDWriteFontFace2 = face.cast()?;
        let transform = self.run_transform(x, y, run.isSideways.as_bool())?;
        // A color-capable face can also contain ordinary monochrome glyphs.
        // Only the documented NOCOLOR result permits the original run to use
        // this grayscale path. Otherwise preserve the enumerated layer order.
        // SAFETY: native run arrays and description remain live for this call.
        if unsafe { face2.IsColorFont() }.as_bool() {
            match unsafe {
                self.factory.TranslateColorGlyphRun(
                    x,
                    y,
                    run,
                    Some(description),
                    measuring,
                    Some(&transform),
                    0,
                )
            } {
                Ok(layers) => {
                    let mut count = 0;
                    // SAFETY: the enumerator owns each layer until MoveNext;
                    // capture_run copies coverage before advancing it.
                    while unsafe { layers.MoveNext() }?.as_bool() {
                        count += 1;
                        if count > 4096 {
                            return Err(native_error(E_INVALIDARG));
                        }
                        let pointer = unsafe { layers.GetCurrentRun() }?;
                        let layer =
                            unsafe { pointer.as_ref() }.ok_or_else(|| native_error(E_POINTER))?;
                        let c = layer.runColor;
                        let palette =
                            if layer.paletteIndex == u16::MAX || [c.r, c.g, c.b, c.a] == [0.; 4] {
                                None
                            } else {
                                Some([c.r, c.g, c.b, c.a])
                            };
                        self.capture_run(
                            &layer.glyphRun,
                            [layer.baselineOriginX, layer.baselineOriginY],
                            measuring,
                            description.textPosition..end,
                            palette,
                            foreground,
                        )?;
                    }
                    return Ok(());
                }
                Err(error) if error.code() == DWRITE_E_NOCOLOR => {}
                Err(error) => return Err(error),
            }
        }
        self.capture_run(
            run,
            [x, y],
            measuring,
            description.textPosition..end,
            None,
            foreground,
        )
    }

    fn DrawUnderline(
        &self,
        _context: *const c_void,
        _x: f32,
        _y: f32,
        _underline: *const DWRITE_UNDERLINE,
        _effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        Err(native_error(E_NOTIMPL))
    }
    fn DrawStrikethrough(
        &self,
        _context: *const c_void,
        _x: f32,
        _y: f32,
        _strike: *const DWRITE_STRIKETHROUGH,
        _effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        Err(native_error(E_NOTIMPL))
    }
    fn DrawInlineObject(
        &self,
        _context: *const c_void,
        x: f32,
        y: f32,
        object: Ref<IDWriteInlineObject>,
        sideways: BOOL,
        rtl: BOOL,
        effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        let object = object.ok()?;
        let allowed = self
            .ellipsis
            .as_ref()
            .ok_or_else(|| native_error(E_NOTIMPL))?;
        if object.cast::<IUnknown>()? != allowed.cast::<IUnknown>()? {
            return Err(native_error(E_NOTIMPL));
        }
        if self.in_ellipsis.replace(true) {
            return Err(native_error(E_FAIL));
        }
        struct Guard<'a>(&'a Cell<bool>);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _guard = Guard(&self.in_ellipsis);
        let renderer: IDWriteTextRenderer1 = self.to_interface();
        // SAFETY: only our factory-created ellipsis may reenter the renderer;
        // callbacks synchronously copy glyph pixels and no RefCell borrow is held.
        unsafe {
            object.Draw(
                None,
                &renderer,
                x,
                y,
                sideways.as_bool(),
                rtl.as_bool(),
                effect.as_ref(),
            )
        }
    }
}
#[allow(non_snake_case)]
impl IDWriteTextRenderer1_Impl for Collector_Impl {
    fn DrawGlyphRun(
        &self,
        context: *const c_void,
        x: f32,
        y: f32,
        angle: DWRITE_GLYPH_ORIENTATION_ANGLE,
        measuring: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        description: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        let old = self.orientation.replace(angle);
        let result = IDWriteTextRenderer_Impl::DrawGlyphRun(
            self,
            context,
            x,
            y,
            measuring,
            run,
            description,
            effect,
        );
        self.orientation.set(old);
        result
    }
    fn DrawUnderline(
        &self,
        context: *const c_void,
        x: f32,
        y: f32,
        angle: DWRITE_GLYPH_ORIENTATION_ANGLE,
        underline: *const DWRITE_UNDERLINE,
        effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        if angle != DWRITE_GLYPH_ORIENTATION_ANGLE_0_DEGREES {
            return Err(native_error(E_NOTIMPL));
        }
        IDWriteTextRenderer_Impl::DrawUnderline(self, context, x, y, underline, effect)
    }
    fn DrawStrikethrough(
        &self,
        context: *const c_void,
        x: f32,
        y: f32,
        angle: DWRITE_GLYPH_ORIENTATION_ANGLE,
        strike: *const DWRITE_STRIKETHROUGH,
        effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        if angle != DWRITE_GLYPH_ORIENTATION_ANGLE_0_DEGREES {
            return Err(native_error(E_NOTIMPL));
        }
        IDWriteTextRenderer_Impl::DrawStrikethrough(self, context, x, y, strike, effect)
    }
    fn DrawInlineObject(
        &self,
        context: *const c_void,
        x: f32,
        y: f32,
        angle: DWRITE_GLYPH_ORIENTATION_ANGLE,
        object: Ref<IDWriteInlineObject>,
        sideways: BOOL,
        rtl: BOOL,
        effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        if angle != DWRITE_GLYPH_ORIENTATION_ANGLE_0_DEGREES {
            return Err(native_error(E_NOTIMPL));
        }
        IDWriteTextRenderer_Impl::DrawInlineObject(
            self, context, x, y, object, sideways, rtl, effect,
        )
    }
}
impl Collector {
    fn run_transform(
        &self,
        x: f32,
        y: f32,
        sideways: bool,
    ) -> windows::core::Result<DWRITE_MATRIX> {
        let angle = self.orientation.get();
        if angle == DWRITE_GLYPH_ORIENTATION_ANGLE_0_DEGREES && !sideways {
            return Ok(DWRITE_MATRIX {
                m11: self.scale,
                m22: self.scale,
                ..Default::default()
            });
        }
        // Let DirectWrite choose the per-run orientation around its own baseline;
        // do not infer CJK/Latin orientation or manually rotate character bitmaps.
        let analyzer: IDWriteTextAnalyzer2 =
            unsafe { self.factory.CreateTextAnalyzer() }?.cast()?;
        let mut transform = DWRITE_MATRIX::default();
        unsafe { analyzer.GetGlyphOrientationTransform(angle, sideways, x, y, &mut transform) }?;
        transform.m11 *= self.scale;
        transform.m12 *= self.scale;
        transform.m21 *= self.scale;
        transform.m22 *= self.scale;
        transform.dx *= self.scale;
        transform.dy *= self.scale;
        Ok(transform)
    }
    fn capture_run(
        &self,
        run: &DWRITE_GLYPH_RUN,
        origin: [f32; 2],
        measuring: DWRITE_MEASURING_MODE,
        source_range: Range<u32>,
        palette: Option<[f32; 4]>,
        foreground: Option<[u8; 4]>,
    ) -> windows::core::Result<()> {
        let [x, y] = origin;
        if run.glyphCount == 0 {
            return Ok(());
        }
        if run.glyphCount > 100_000
            || run.glyphIndices.is_null()
            || !run.fontEmSize.is_finite()
            || run.fontEmSize <= 0.
            || !x.is_finite()
            || !y.is_finite()
        {
            return Err(native_error(E_INVALIDARG));
        }
        let face = run
            .fontFace
            .as_ref()
            .ok_or_else(|| native_error(E_INVALIDARG))?;
        let face2: IDWriteFontFace2 = face.cast()?;
        let transform = self.run_transform(x, y, run.isSideways.as_bool())?;
        let mut rendering = DWRITE_RENDERING_MODE_DEFAULT;
        let mut grid = DWRITE_GRID_FIT_MODE_DEFAULT;
        // SAFETY: typed live font/parameter interfaces and writable enum outputs.
        unsafe {
            face2.GetRecommendedRenderingMode(
                run.fontEmSize,
                96.,
                96.,
                Some(&transform),
                run.isSideways.as_bool(),
                DWRITE_OUTLINE_THRESHOLD_ANTIALIASED,
                measuring,
                &self.params,
                &mut rendering,
                &mut grid,
            )
        }?;
        if rendering == DWRITE_RENDERING_MODE_OUTLINE || rendering == DWRITE_RENDERING_MODE_DEFAULT
        {
            rendering = DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC;
        }
        // SAFETY: run arrays and font remain valid throughout this callback.
        let analysis = unsafe {
            self.factory.CreateGlyphRunAnalysis(
                run,
                Some(&transform),
                rendering,
                measuring,
                grid,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                x + self.phase[0] / self.scale,
                y + self.phase[1] / self.scale,
            )
        }?;
        // ALIASED_1x1 names the single-channel texture format; the analysis above
        // explicitly requests grayscale antialiasing, not aliased glyph edges.
        // SAFETY: analysis owns its native raster state and returns initialized bounds.
        let bounds = unsafe { analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1) }?;
        let width = i64::from(bounds.right) - i64::from(bounds.left);
        let height = i64::from(bounds.bottom) - i64::from(bounds.top);
        if width <= 0 || height <= 0 {
            return Ok(());
        }
        if width > 16384 || height > 16384 {
            return Err(native_error(E_INVALIDARG));
        }
        let bytes = (width * height) as usize;
        let mut output = self
            .output
            .try_borrow_mut()
            .map_err(|_| native_error(E_FAIL))?;
        if output.masks.len() >= 4096
            || bytes > MAX_BYTES - output.bytes
            || run.glyphCount as usize > 100_000 - output.glyphs
        {
            return Err(native_error(E_INVALIDARG));
        }
        let mut coverage = Vec::new();
        coverage
            .try_reserve_exact(bytes)
            .map_err(|_| native_error(E_OUTOFMEMORY))?;
        coverage.resize(bytes, 0);
        // SAFETY: bounded allocation matches the requested 1-byte-per-pixel rectangle.
        unsafe { analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &bounds, &mut coverage) }?;
        output
            .masks
            .try_reserve(1)
            .map_err(|_| native_error(E_OUTOFMEMORY))?;
        output.bytes += bytes;
        output.glyphs += run.glyphCount as usize;
        output.masks.push(GlyphMask {
            utf16_range: source_range,
            origin: [bounds.left, bounds.top],
            size: [width as u32, height as u32],
            coverage,
            palette,
            foreground,
        });
        Ok(())
    }
}

impl Paragraph {
    /// Rasterize this exact native layout into ordered raw grayscale masks.
    /// Scale is physical pixels per DIP; phase is a [0,1) physical-pixel offset.
    /// DirectWrite outline color layers preserve their palette and painter order.
    /// The retained native ellipsis object is replayed synchronously. Other
    /// inline objects and decorations return Unsupported; bitmap/SVG color glyph
    /// formats are not implemented by this outline-layer interface.
    /// No partial mask list is returned on native failure. Coverage correction and
    /// foreground compositing remain the consumer's responsibility.
    pub fn rasterize(&self, scale: f32, phase: [f32; 2]) -> Result<Rasterized> {
        if !scale.is_finite()
            || scale <= 0.
            || scale > 8.
            || !phase.iter().all(|p| p.is_finite() && (0. ..1.).contains(p))
        {
            return Err(Error::InvalidRequest);
        }
        // SAFETY: these are typed native rendering-parameter interfaces. Their
        // values are metadata; raw mask coverage is not modified or emboldened.
        let (params, gamma, grayscale_contrast) = unsafe {
            let params = self
                ._factory
                .CreateRenderingParams()
                .context(super::NativeSnafu)?;
            let grayscale: IDWriteRenderingParams1 = params.cast().context(super::NativeSnafu)?;
            (
                params.clone(),
                params.GetGamma(),
                grayscale.GetGrayscaleEnhancedContrast(),
            )
        };
        if !gamma.is_finite()
            || gamma <= 0.
            || !grayscale_contrast.is_finite()
            || grayscale_contrast < 0.
        {
            return Err(Error::InvalidRequest);
        }
        let output = Rc::new(RefCell::new(Output::default()));
        let renderer: IDWriteTextRenderer1 = Collector {
            factory: self._factory.clone(),
            params,
            scale,
            phase,
            units: self.source.encode_utf16().count() as u32,
            visible_units: self.visible_units,
            ellipsis: self.ellipsis.clone(),
            in_ellipsis: Cell::new(false),
            orientation: Cell::new(DWRITE_GLYPH_ORIENTATION_ANGLE_0_DEGREES),
            output: output.clone(),
        }
        .into();
        // SAFETY: Draw is synchronous and the collector owns all callback state.
        // No client-context pointer or borrowed Rust closure crosses the COM boundary.
        let result = unsafe { self.layout.Draw(None, &renderer, 0., 0.) };
        drop(renderer);
        if let Err(error) = result {
            if error.code() == E_NOTIMPL {
                return Err(Error::Unsupported {
                    reason: "glyph raster format, decoration, inline object or cross-line clip boundary",
                });
            }
            return Err(Error::Native { source: error });
        }
        let masks = std::mem::take(
            &mut output
                .try_borrow_mut()
                .map_err(|_| Error::InvalidRequest)?
                .masks,
        );
        Ok(Rasterized {
            masks,
            gamma,
            grayscale_contrast,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_text::{Context, Request};
    #[test]
    fn palette_layers_preserve_color_order_and_apply_authored_opacity() {
        let mut raster = Rasterized {
            gamma: 1.,
            grayscale_contrast: 0.,
            masks: vec![
                GlyphMask {
                    utf16_range: 0..1,
                    origin: [3, 4],
                    size: [1, 1],
                    coverage: vec![255],
                    palette: Some([1., 0., 0., 0.5]),
                    foreground: None,
                },
                GlyphMask {
                    utf16_range: 0..1,
                    origin: [5, 6],
                    size: [1, 1],
                    coverage: vec![255],
                    palette: None,
                    foreground: None,
                },
            ],
        };
        let green = raster.colorize([0., 1., 0., 0.5], false).unwrap();
        assert_eq!(green[0].origin, [3, 4]);
        assert_eq!(green[1].origin, [5, 6]);
        assert_eq!(green[0].rgba, [64, 0, 0, 64]);
        assert_eq!(green[1].rgba, [0, 128, 0, 128]);
        let blue = raster.colorize([0., 0., 1., 0.5], false).unwrap();
        assert_eq!(green[0], blue[0]);
        assert_ne!(green[1], blue[1]);
        assert!(
            raster
                .colorize([0.; 4], false)
                .unwrap()
                .iter()
                .all(|m| m.rgba.iter().all(|v| *v == 0))
        );
        raster.masks[1].foreground = Some([0, 0, 255, 128]);
        let overridden = raster.colorize([0., 1., 0., 1.], false).unwrap();
        assert_eq!(overridden[1].rgba, [0, 0, 128, 128]);
        raster.masks[0].palette = Some([f32::NAN; 4]);
        assert!(matches!(
            raster.colorize([1.; 4], false),
            Err(Error::InvalidRequest)
        ));
    }
    #[test]
    fn color_capable_family_keeps_monochrome_runs_usable() {
        let context = Context::new().unwrap();
        let Some(family) = context.resolve_family(&["Segoe UI Emoji"]).unwrap() else {
            eprintln!("Segoe UI Emoji is not installed; color-face fixture skipped");
            return;
        };
        let mut request = Request::new("123");
        request.family = &family;
        let paragraph = context.layout(&request).unwrap();
        let raster = paragraph.rasterize(1., [0., 0.]).unwrap();
        assert!(!raster.masks.is_empty());
        request.text = "😀";
        let emoji = context.layout(&request).unwrap();
        let color = emoji.rasterize(1., [0., 0.]).unwrap();
        assert!(color.masks.iter().any(|mask| mask.palette.is_some()));
        let [left, top, right, bottom] = emoji.ink_bounds();
        for mask in &color.masks {
            assert!(mask.origin[0] as f32 >= left - 4.);
            assert!(mask.origin[1] as f32 >= top - 4.);
            assert!(mask.origin[0] as f32 + mask.size[0] as f32 <= right + 4.);
            assert!(mask.origin[1] as f32 + mask.size[1] as f32 <= bottom + 4.);
        }
        assert!(!color.colorize([0., 0., 0., 1.], false).unwrap().is_empty());
    }
    #[test]
    fn retained_layout_raster_is_repeatable_bounded_and_scale_specific() {
        let paragraph = {
            let context = Context::new().unwrap();
            context.layout(&Request::new("Native S ffi 123")).unwrap()
        };
        let mut base_width = 0;
        for scale in [1., 1.25, 1.5, 2.] {
            let first = paragraph.rasterize(scale, [0., 0.]).unwrap();
            let repeat = paragraph.rasterize(scale, [0., 0.]).unwrap();
            assert!(!first.masks.is_empty());
            assert_eq!(first.masks, repeat.masks);
            let corrected = first.colorize([0.1, 0.3, 0.8, 0.5], false).unwrap();
            assert_eq!(
                corrected,
                first.colorize([0.1, 0.3, 0.8, 0.5], false).unwrap()
            );
            for mask in corrected {
                assert_eq!(
                    mask.rgba.len(),
                    mask.size[0] as usize * mask.size[1] as usize * 4
                );
                assert!(
                    mask.rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[3] <= 128 && p[..3].iter().all(|c| *c <= p[3]))
                );
            }
            assert_eq!(
                first.masks, repeat.masks,
                "colorization changed raw coverage"
            );
            let left = first.masks.iter().map(|m| m.origin[0]).min().unwrap();
            let right = first
                .masks
                .iter()
                .map(|m| i64::from(m.origin[0]) + i64::from(m.size[0]))
                .max()
                .unwrap();
            let width = right - i64::from(left);
            if scale == 1. {
                base_width = width;
            }
            if scale == 2. {
                assert!(
                    width * 10 > base_width * 17 && width * 10 < base_width * 23,
                    "native raster scale was omitted or doubled"
                );
            }
            assert!(first.gamma.is_finite() && first.gamma > 0.);
            assert!(first.grayscale_contrast.is_finite() && first.grayscale_contrast >= 0.);
            let mut bytes = 0;
            for mask in &first.masks {
                assert_eq!(
                    mask.coverage.len(),
                    mask.size[0] as usize * mask.size[1] as usize
                );
                assert!(mask.utf16_range.end <= "Native S ffi 123".encode_utf16().count() as u32);
                assert!(mask.coverage.iter().any(|a| *a > 0));
                bytes += mask.coverage.len();
            }
            assert!(bytes <= MAX_BYTES);
            let phase = paragraph.rasterize(scale, [0.5, 0.]).unwrap();
            // Native grid fitting may quantize a phase; require deterministic
            // output rather than assuming one desktop antialiasing policy.
            assert_eq!(
                phase.masks,
                paragraph.rasterize(scale, [0.5, 0.]).unwrap().masks
            );
        }
        assert!(matches!(
            paragraph.rasterize(0., [0.; 2]),
            Err(Error::InvalidRequest)
        ));
        assert!(matches!(
            paragraph.rasterize(1., [f32::NAN, 0.]),
            Err(Error::InvalidRequest)
        ));
        assert!(matches!(
            paragraph.rasterize(1., [1., 0.]),
            Err(Error::InvalidRequest)
        ));
        let empty = Context::new().unwrap().layout(&Request::new("  ")).unwrap();
        assert!(empty.rasterize(1., [0.; 2]).unwrap().masks.is_empty());
    }
    #[test]
    fn wrapped_and_rtl_runs_retain_original_source_ranges() {
        let context = Context::new().unwrap();
        for (content, rtl) in [
            (
                "Wrapped words with multiple lines and native glyph runs",
                false,
            ),
            ("مرحبا بالعالم هذا نص عربي", true),
        ] {
            let mut request = Request::new(content);
            request.width = Some(100.);
            request.rtl = rtl;
            let paragraph = context.layout(&request).unwrap();
            let raster = paragraph.rasterize(1.25, [0., 0.]).unwrap();
            assert!(raster.masks.len() > 1);
            for mask in raster.masks {
                assert!(paragraph.byte_from_utf16(mask.utf16_range.start).is_some());
                assert!(paragraph.byte_from_utf16(mask.utf16_range.end).is_some());
            }
        }
    }
    #[test]
    fn collector_does_not_expose_agile_object_for_thread_local_state() {
        let context = Context::new().unwrap();
        let renderer: IDWriteTextRenderer1 = Collector {
            factory: context.factory.clone(),
            // SAFETY: a live factory returns an owned rendering-parameter interface.
            params: unsafe { context.factory.CreateRenderingParams() }.unwrap(),
            scale: 1.,
            phase: [0.; 2],
            units: 0,
            visible_units: None,
            ellipsis: None,
            in_ellipsis: Cell::new(false),
            orientation: Cell::new(DWRITE_GLYPH_ORIENTATION_ANGLE_0_DEGREES),
            output: Rc::new(RefCell::new(Output::default())),
        }
        .into();
        // IAgileObject's well-known IID. Querying it must fail rather than
        // permitting the private Rc/RefCell collector to be marshaled freely.
        let iid = windows::core::GUID::from_u128(0x94ea2b94_e9cc_49e0_c0ff_ee64ca8f5b90);
        let mut object = std::ptr::null_mut();
        // SAFETY: valid IID and COM output slot; query does not retain on failure.
        let result = unsafe { renderer.query(&iid, &mut object) };
        assert!(result.is_err());
        assert!(object.is_null());
    }
    #[test]
    fn public_mask_colorization_validates_geometry_before_allocating_output() {
        let malformed = Rasterized {
            gamma: 1.8,
            grayscale_contrast: 1.,
            masks: vec![GlyphMask {
                utf16_range: 0..1,
                origin: [0; 2],
                size: [2, 2],
                coverage: vec![255],
                palette: None,
                foreground: None,
            }],
        };
        assert!(matches!(
            malformed.colorize([1.; 4], false),
            Err(Error::InvalidRequest)
        ));
        let empty = Rasterized {
            gamma: 1.8,
            grayscale_contrast: 1.,
            masks: Vec::new(),
        };
        assert!(empty.colorize([1.; 4], false).unwrap().is_empty());
        assert!(matches!(
            empty.colorize([f32::NAN; 4], false),
            Err(Error::Correction { .. })
        ));
    }
}
