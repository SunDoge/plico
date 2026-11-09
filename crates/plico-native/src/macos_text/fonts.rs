//! Bounded, thread-confined native faces shared by measurement and shaping.
//! CoreText keeps shaping caches on CTFont; rebuilding it per line discards that
//! work. Inspired by GPUI's SizedFonts (see SOURCES.md), without rounding sizes.
use super::*;
use std::{cell::RefCell, collections::HashMap};

const MAX_FONTS: usize = 256;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum WeightKey {
    Regular,
    Bold,
    Normalized(u64),
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct FaceKey {
    size: u64,
    style: Option<(WeightKey, bool)>,
}
impl FaceKey {
    fn new(request: &Request<'_>) -> Self {
        Self {
            size: request.size.to_bits(),
            style: request.style.map(|style| {
                let weight = match style.weight {
                    FontWeight::Regular => WeightKey::Regular,
                    FontWeight::Bold => WeightKey::Bold,
                    FontWeight::Normalized(value) => WeightKey::Normalized(value.to_bits()),
                };
                (weight, style.italic)
            }),
        }
    }
}
#[derive(Default)]
struct Fonts {
    families: HashMap<String, HashMap<FaceKey, CFRetained<CTFont>>>,
    count: usize,
    registered: HashMap<String, Vec<RegisteredFace>>,
}
impl Fonts {
    fn resolve(&mut self, request: &Request<'_>) -> Result<CFRetained<CTFont>> {
        // Text/tracking do not identify a face. The caller validates text before
        // constructing attributes; do not scan that text again on a font hit.
        // Still reject invalid face properties, including on cached lookups.
        if !(Request {
            text: "",
            ..*request
        })
        .valid()
        {
            return Err(Error::InvalidRequest);
        }
        let family = if request.family.is_empty() {
            "system-ui"
        } else {
            request.family
        };
        let key = FaceKey::new(request);
        if let Some(font) = self.families.get(family).and_then(|faces| faces.get(&key)) {
            return Ok(font.clone());
        }
        let font = if let Some(faces) = self.registered.get(&family.to_lowercase()) {
            let style = request.style.unwrap_or_default();
            let weight = style.weight.native_value();
            let face = faces
                .iter()
                .min_by(|a, b| {
                    let score = |face: &RegisteredFace| {
                        (face.weight - weight).abs()
                            + if face.italic == style.italic { 0. } else { 10. }
                    };
                    score(a).total_cmp(&score(b))
                })
                .ok_or(Error::UnavailableStyle)?;
            // SAFETY: Collection retains the descriptor; no optional matrix.
            unsafe {
                CTFont::with_font_descriptor(&face.descriptor, request.size, std::ptr::null())
            }
        } else {
            create(request)?
        };
        // Continuous font-size animations must not keep every sampled size.
        // Live lines retain their own face, so eviction cannot invalidate them.
        if self.count >= MAX_FONTS {
            self.families.clear();
            self.count = 0;
        }
        self.families
            .entry(family.to_owned())
            .or_default()
            .insert(key, font.clone());
        self.count += 1;
        Ok(font)
    }
}
thread_local! {
    // Native lines are already confined to this thread. No locks, Send/Sync
    // casts or cross-thread CoreText lifetimes are introduced by this cache.
    static FONTS: RefCell<Fonts> = RefCell::new(Fonts::default());
}
pub(super) fn resolve(request: &Request<'_>) -> Result<CFRetained<CTFont>> {
    if let Some(collection) = request.fonts {
        collection.inner.borrow_mut().resolve(request)
    } else {
        FONTS.with_borrow_mut(|fonts| fonts.resolve(request))
    }
}
fn create(request: &Request<'_>) -> Result<CFRetained<CTFont>> {
    let system = matches!(request.family, "" | "system-ui" | "ui-monospace");
    let font = if system {
        system_font(request)?
    } else {
        let name = CFString::from_str(request.family);
        // SAFETY: Retained name and null optional transform are valid.
        unsafe { CTFont::with_name(&name, request.size, std::ptr::null()) }
    };
    let font = if let Some(style) = request.style {
        let mut traits = CTFontSymbolicTraits::empty();
        // NSFont already selected the native UI weight; do not clear it.
        if system && !style.italic {
            return Ok(font);
        }
        if !system && style.weight == FontWeight::Bold {
            traits |= CTFontSymbolicTraits::TraitBold;
        }
        if style.italic {
            traits |= CTFontSymbolicTraits::TraitItalic;
        }
        // SAFETY: The retained font is valid and the optional matrix is null.
        // Mask both bits so regular/upright can remove a named face's traits.
        let font = unsafe {
            font.copy_with_symbolic_traits(
                0.,
                std::ptr::null(),
                traits,
                if system {
                    CTFontSymbolicTraits::TraitItalic
                } else {
                    CTFontSymbolicTraits::TraitBold | CTFontSymbolicTraits::TraitItalic
                },
            )
        }
        .ok_or(Error::UnavailableStyle)?;
        if !system && let FontWeight::Normalized(weight) = style.weight {
            let number = CFNumber::new_f64(weight);
            // SAFETY: Framework constants are valid retained CFString keys;
            // dictionaries retain their values for the synchronous font copy.
            unsafe {
                let traits = CFDictionary::<CFString, CFType>::from_slices(
                    &[kCTFontWeightTrait],
                    &[&number],
                );
                let attributes = CFDictionary::<CFString, CFType>::from_slices(
                    &[kCTFontTraitsAttribute],
                    &[&traits],
                );
                let descriptor = CTFontDescriptor::with_attributes(attributes.as_ref());
                let selected = font.copy_with_attributes(0., std::ptr::null(), Some(&descriptor));
                if selected
                    .symbolic_traits()
                    .contains(CTFontSymbolicTraits::TraitItalic)
                    != style.italic
                {
                    return Err(Error::UnavailableStyle);
                }
                selected
            }
        } else {
            font
        }
    } else {
        font
    };
    Ok(font)
}

// Like MyGO's systemFont, scope AppKit temporary objects to this thread even
// outside an event loop. The returned CF owner survives pool drainage.
fn system_font(request: &Request<'_>) -> Result<CFRetained<CTFont>> {
    use objc2_app_kit::NSFont;
    objc2::rc::autoreleasepool(|_| {
        let weight = request.style.unwrap_or_default().weight.native_value();
        if request.family == "ui-monospace" && !objc2::available!(macos = 10.15) {
            return create(&Request {
                family: "Menlo",
                ..*request
            });
        }
        let native = if request.family == "ui-monospace" {
            NSFont::monospacedSystemFontOfSize_weight(request.size, weight)
        } else {
            NSFont::systemFontOfSize_weight(request.size, weight)
        };
        let font: &NSFont = &native;
        let font: &CTFont = font.as_ref();
        // SAFETY: objc2-app-kit supplies the public toll-free CTFont bridge;
        // retain before the NSFont owner and local autorelease pool are dropped.
        Ok(unsafe { CFRetained::retain(std::ptr::NonNull::from(font)) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_faces_match_appkit_selection_and_survive_the_local_pool() {
        use objc2_app_kit::NSFont;
        for family in ["system-ui", "ui-monospace"] {
            if family == "ui-monospace" && !objc2::available!(macos = 10.15) {
                continue;
            }
            for size in [11., 13., 17., 24.] {
                for css in [400., 500., 600., 700.] {
                    let weight = FontWeight::from_css(css).unwrap();
                    let line = Line::new(&Request {
                        family,
                        size,
                        style: Some(FontStyle {
                            weight,
                            italic: false,
                        }),
                        ..Request::new("AV office 0123")
                    })
                    .unwrap();
                    let expected = objc2::rc::autoreleasepool(|_| {
                        let native = if family == "ui-monospace" {
                            NSFont::monospacedSystemFontOfSize_weight(size, weight.native_value())
                        } else {
                            NSFont::systemFontOfSize_weight(size, weight.native_value())
                        };
                        native.fontName().to_string()
                    });
                    assert_eq!(
                        line.font_postscript_name(),
                        expected,
                        "{family} {size} {css}"
                    );
                    // system_font's local pool has drained before these calls.
                    assert!(line.caret_offsets(2).is_ok());
                    assert!(line.rasterize(2., [0.25, 0.], [255; 4]).unwrap().is_some());
                }
            }
        }
    }

    #[test]
    fn system_monospace_has_equal_advances_and_explicit_menlo_still_resolves() {
        let width = |text| {
            Line::new(&Request {
                family: "ui-monospace",
                ..Request::new(text)
            })
            .unwrap()
            .metrics()
            .width
        };
        assert!((width("iiii") - width("WWWW")).abs() < 1e-6);
        let named = Line::new(&Request {
            family: "Menlo",
            ..Request::new("iiii")
        })
        .unwrap();
        assert_eq!(named.font_family(), "Menlo");
    }

    #[test]
    fn changing_text_or_tracking_reuses_the_face_but_sizes_and_traits_do_not() {
        let mut fonts = Fonts::default();
        let request = Request::new("first line");
        let first = fonts.resolve(&request).unwrap();
        let other = fonts
            .resolve(&Request {
                text: "中文 second line",
                letter_spacing: 2.,
                ..request
            })
            .unwrap();
        assert!(std::ptr::eq(&*first, &*other));
        assert_eq!(fonts.count, 1);
        let next_size = fonts
            .resolve(&Request {
                size: request.size.next_up(),
                ..request
            })
            .unwrap();
        assert!(!std::ptr::eq(&*first, &*next_size));
        assert_eq!(fonts.count, 2, "do not round San Francisco optical sizes");
        let bold = fonts
            .resolve(&Request {
                style: Some(FontStyle {
                    weight: FontWeight::Bold,
                    italic: false,
                }),
                ..request
            })
            .unwrap();
        assert!(!std::ptr::eq(&*first, &*bold));
        assert_eq!(fonts.count, 3);
        assert!(
            fonts
                .resolve(&Request {
                    size: f64::NAN,
                    ..request
                })
                .is_err()
        );
        assert_eq!(fonts.count, 3);
    }

    #[test]
    fn animated_sizes_have_bounded_ownership_and_live_lines_survive_eviction() {
        FONTS.with_borrow_mut(|fonts| *fonts = Fonts::default());
        let line = Line::new(&Request::new("AV office 中文🙂")).unwrap();
        let before = line.rasterize(2., [0.25, 0.], [20, 30, 40, 255]).unwrap();
        for i in 0..MAX_FONTS + 20 {
            resolve(&Request {
                size: 20. + i as f64 / 16.,
                ..Request::new("")
            })
            .unwrap();
            FONTS.with_borrow(|fonts| assert!(fonts.count <= MAX_FONTS));
        }
        assert_eq!(
            line.rasterize(2., [0.25, 0.], [20, 30, 40, 255]).unwrap(),
            before
        );
    }

    #[test]
    fn cached_faces_preserve_native_measurement_carets_and_pixels() {
        let request = Request {
            size: 16.,
            ..Request::new("AVAWAY fi ffi 中文 日本語 e\u{301} 🙂")
        };
        let first = Line::new(&request).unwrap();
        let warm = Line::new(&request).unwrap();
        FONTS.with_borrow_mut(|fonts| *fonts = Fonts::default());
        let fresh = Line::new(&request).unwrap();
        for other in [&warm, &fresh] {
            assert_eq!(first.metrics().width, other.metrics().width);
            for (byte, _) in request.text.char_indices() {
                let a = first.caret_offsets(byte).unwrap();
                let b = other.caret_offsets(byte).unwrap();
                assert_eq!((a.primary, a.secondary), (b.primary, b.secondary));
            }
            assert_eq!(
                first.rasterize(1.25, [0.25, 0.], [255; 4]).unwrap(),
                other.rasterize(1.25, [0.25, 0.], [255; 4]).unwrap()
            );
        }
    }
}

struct RegisteredFace {
    descriptor: CFRetained<CTFontDescriptor>,
    weight: f64,
    italic: bool,
}
/// Thread-confined, privately owned application fonts and bounded face cache.
/// Based on MyGO's descriptor-based registration: aliases do not affect other
/// windows or applications, and existing lines retain their native fonts.
#[derive(Default)]
pub struct FontCollection {
    inner: RefCell<Fonts>,
    _thread: PhantomData<Rc<()>>,
    registrations: usize,
    font_bytes: usize,
}
impl std::fmt::Debug for FontCollection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontCollection").finish_non_exhaustive()
    }
}
impl FontCollection {
    /// Add TTF/OTF/TTC bytes under their native family and an optional alias.
    /// Invalid data leaves the collection and its existing face cache intact.
    /// Each collection accepts at most 64 registrations and 128 MiB of font data.
    pub fn register(&mut self, bytes: &[u8], alias: Option<&str>) -> Result<()> {
        use objc2_core_foundation::{CFArray, CFData};
        if self.registrations >= 64
            || bytes.len() > (128 * 1024 * 1024_usize).saturating_sub(self.font_bytes)
            || bytes.is_empty()
            || alias.is_some_and(|name| name.is_empty() || name.len() > 4096 || name.contains('\0'))
        {
            return Err(Error::InvalidRequest);
        }
        // The generated binding assumes a nonnull result, but CoreText can
        // return NULL for invalid data. Keep that failure observable as Result.
        unsafe extern "C-unwind" {
            #[link_name = "CTFontManagerCreateFontDescriptorsFromData"]
            fn descriptors(data: &CFData) -> Option<std::ptr::NonNull<CFArray<CTFontDescriptor>>>;
        }
        let data = CFData::from_bytes(bytes);
        // SAFETY: CFData lives through the synchronous call. CoreText returns
        // a create-rule array containing CTFontDescriptor objects or NULL.
        let descriptors =
            unsafe { CFRetained::from_raw(descriptors(&data).ok_or(Error::InvalidRequest)?) };
        let mut additions = Vec::new();
        for descriptor in descriptors.iter() {
            // SAFETY: The descriptor is retained by the array and valid.
            let font = unsafe { CTFont::with_font_descriptor(&descriptor, 12., std::ptr::null()) };
            let family = unsafe { font.family_name() }.to_string();
            let traits = unsafe { font.symbolic_traits() };
            let weight = unsafe {
                let dictionary =
                    CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(font.traits());
                dictionary.get(kCTFontWeightTrait)
            }
            .and_then(|value| value.downcast::<CFNumber>().ok())
            .and_then(|value| value.as_f64())
            .filter(|value| value.is_finite())
            .unwrap_or(0.);
            let mut names = vec![family.to_lowercase()];
            if let Some(alias) = alias {
                let alias = alias.to_lowercase();
                if !names.contains(&alias) {
                    names.push(alias);
                }
            }
            additions.push((
                names,
                RegisteredFace {
                    descriptor,
                    weight,
                    italic: traits.contains(CTFontSymbolicTraits::TraitItalic),
                },
            ));
        }
        if additions.is_empty() {
            return Err(Error::InvalidRequest);
        }
        self.registrations += 1;
        self.font_bytes += bytes.len();
        let fonts = self.inner.get_mut();
        for (names, face) in additions {
            for name in names {
                fonts
                    .registered
                    .entry(name)
                    .or_default()
                    .push(RegisteredFace {
                        descriptor: face.descriptor.clone(),
                        weight: face.weight,
                        italic: face.italic,
                    });
            }
        }
        fonts.families.clear();
        fonts.count = 0;
        Ok(())
    }
    /// Whether a private family or alias is available (case insensitive).
    pub fn contains_family(&self, family: &str) -> bool {
        self.inner
            .borrow()
            .registered
            .contains_key(&family.to_lowercase())
    }
}

#[cfg(test)]
mod application_font_tests {
    use super::*;
    const FONT: &[u8] =
        include_bytes!("../../../../backend/examples/web-preview/public/DejaVuSans.ttf");

    #[test]
    fn aliases_are_private_and_registration_invalidates_cached_substitution() {
        let mut fonts = FontCollection::default();
        let request = Request {
            family: "Plico Private Face",
            fonts: Some(&fonts),
            ..Request::new("iiii WWWW")
        };
        let fallback = Line::new(&request).unwrap();
        assert!(!fallback.font_family().eq_ignore_ascii_case("DejaVu Sans"));
        fonts.register(FONT, Some("Plico Private Face")).unwrap();
        for family in ["Plico Private Face", "plico private face", "DejaVu Sans"] {
            let line = Line::new(&Request {
                family,
                fonts: Some(&fonts),
                ..Request::new("iiii WWWW")
            })
            .unwrap();
            assert_eq!(line.font_family(), "DejaVu Sans");
            let again = Line::new(&Request {
                family,
                fonts: Some(&fonts),
                ..Request::new("iiii WWWW")
            })
            .unwrap();
            assert_eq!(line.metrics().width, again.metrics().width);
            assert_eq!(
                line.rasterize(2., [0., 0.], [255; 4]).unwrap(),
                again.rasterize(2., [0., 0.], [255; 4]).unwrap()
            );
        }
        let other = FontCollection::default();
        assert!(!other.contains_family("Plico Private Face"));
        let line = Line::new(&Request {
            family: "Plico Private Face",
            fonts: Some(&other),
            ..Request::new("iiii WWWW")
        })
        .unwrap();
        assert_eq!(line.font_family(), fallback.font_family());
    }

    #[test]
    fn invalid_registration_preserves_faces_and_live_lines_outlive_collection() {
        let line = {
            let mut fonts = FontCollection::default();
            fonts.register(FONT, Some("Private")).unwrap();
            for data in [&[][..], &[0, 1, 2][..]] {
                assert!(fonts.register(data, Some("Private")).is_err());
                assert!(fonts.contains_family("Private"));
            }
            Line::new(&Request {
                family: "Private",
                fonts: Some(&fonts),
                ..Request::new("AV office")
            })
            .unwrap()
        };
        assert_eq!(line.font_family(), "DejaVu Sans");
        assert!(line.rasterize(2., [0., 0.], [255; 4]).is_ok());
    }
}
