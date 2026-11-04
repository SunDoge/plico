//! Capture the retained layout's native glyph runs; no reshaping or bitmap scaling.
use super::{Error, Paragraph, Result};
use snafu::ResultExt;
use std::{cell::RefCell, ffi::c_void, ops::Range, rc::Rc};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER},
        Graphics::DirectWrite::*,
    },
    core::{BOOL, IUnknown, Interface, Ref, implement},
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
#[derive(Default)]
struct Output {
    masks: Vec<GlyphMask>,
    bytes: usize,
    glyphs: usize,
}
// The collector owns thread-local Rc/RefCell state; do not advertise COM agility.
#[implement(IDWriteTextRenderer, Agile = false)]
struct Collector {
    factory: IDWriteFactory2,
    params: IDWriteRenderingParams,
    scale: f32,
    phase: [f32; 2],
    units: u32,
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
        _effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        if run.is_null() || description.is_null() {
            return Err(native_error(E_POINTER));
        }
        // SAFETY: the native Draw callback borrows both structures for this call.
        // All analysis and pixel copying complete before their pointers expire.
        let (run, description) = unsafe { (&*run, &*description) };
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
        let face = run
            .fontFace
            .as_ref()
            .ok_or_else(|| native_error(E_INVALIDARG))?;
        let face2: IDWriteFontFace2 = face.cast()?;
        // SAFETY: typed COM font interface remains borrowed from the live run.
        if unsafe { face2.IsColorFont() }.as_bool() {
            return Err(native_error(E_NOTIMPL));
        }
        let transform = DWRITE_MATRIX {
            m11: self.scale,
            m22: self.scale,
            ..Default::default()
        };
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
            utf16_range: description.textPosition..end,
            origin: [bounds.left, bounds.top],
            size: [width as u32, height as u32],
            coverage,
        });
        Ok(())
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
        _x: f32,
        _y: f32,
        _object: Ref<IDWriteInlineObject>,
        _sideways: BOOL,
        _rtl: BOOL,
        _effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        Err(native_error(E_NOTIMPL))
    }
}
impl Paragraph {
    /// Rasterize this exact native layout into ordered raw grayscale masks.
    /// Scale is physical pixels per DIP; phase is a [0,1) physical-pixel offset.
    /// Color fonts, decorations and inline objects currently return Unsupported.
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
        let renderer: IDWriteTextRenderer = Collector {
            factory: self._factory.clone(),
            params,
            scale,
            phase,
            units: self.source.encode_utf16().count() as u32,
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
                    reason: "color glyphs, decorations or inline objects",
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
        let renderer: IDWriteTextRenderer = Collector {
            factory: context.factory.clone(),
            // SAFETY: a live factory returns an owned rendering-parameter interface.
            params: unsafe { context.factory.CreateRenderingParams() }.unwrap(),
            scale: 1.,
            phase: [0.; 2],
            units: 0,
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
}
