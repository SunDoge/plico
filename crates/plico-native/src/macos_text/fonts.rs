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
        let font = create(request)?;
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
    FONTS.with_borrow_mut(|fonts| fonts.resolve(request))
}
fn create(request: &Request<'_>) -> Result<CFRetained<CTFont>> {
    let name = CFString::from_str(request.family);
    // SAFETY: CFString values and the null optional transform remain valid
    // throughout each synchronous call. Returned create-rule objects are owned.
    let font = unsafe {
        if matches!(request.family, "" | "system-ui") {
            CTFont::new_ui_font_for_language(CTFontUIFontType::System, request.size, None)
                .ok_or(Error::Allocation)?
        } else {
            CTFont::with_name(&name, request.size, std::ptr::null())
        }
    };
    let font = if let Some(style) = request.style {
        let mut traits = CTFontSymbolicTraits::empty();
        if style.weight == FontWeight::Bold {
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
                CTFontSymbolicTraits::TraitBold | CTFontSymbolicTraits::TraitItalic,
            )
        }
        .ok_or(Error::UnavailableStyle)?;
        if let FontWeight::Normalized(weight) = style.weight {
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

#[cfg(test)]
mod tests {
    use super::*;

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
