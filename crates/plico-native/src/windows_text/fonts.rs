//! Private memory font collections. Each loader is retained through every layout
//! using its collection, including layouts outliving their originating context.
use super::*;
use windows::core::IUnknown;

pub(super) struct Loader {
    factory: IDWriteFactory5,
    loader: IDWriteInMemoryFontFileLoader,
}
impl Drop for Loader {
    fn drop(&mut self) {
        // SAFETY: Context/Paragraph field order drops native collections/layouts
        // before the last registry guard. No borrowed application buffer exists.
        let _ = unsafe { self.factory.UnregisterFontFileLoader(&self.loader) };
    }
}
#[derive(Default)]
pub(super) struct Registry {
    loaders: Vec<Rc<Loader>>,
    bytes: usize,
}
impl Context {
    /// Register a private memory font (Windows 10 build 14393+). DirectWrite
    /// copies input bytes once. No system font installation or temporary file.
    /// An optional alias must refer to a file containing exactly one family.
    /// Registered data is limited to 64 files and 128 MiB per context.
    pub fn register_font(&mut self, bytes: &[u8], alias: Option<&str>) -> Result<()> {
        const LIMIT: usize = 128 * 1024 * 1024;
        if bytes.is_empty()
            || bytes.len() > LIMIT - self.font_registry.bytes
            || self.font_registry.loaders.len() >= 64
            || alias.is_some_and(|a| a.trim().is_empty() || a.len() > 4096 || a.contains('\0'))
        {
            return Err(Error::InvalidRequest);
        }
        let factory: IDWriteFactory5 = self.factory.cast().map_err(|_| Error::Unsupported {
            reason: "memory fonts require DirectWrite factory 5 (Windows 10 build 14393+)",
        })?;
        // SAFETY: typed factory interfaces, native APIs retain their COM inputs.
        let loader = unsafe { factory.CreateInMemoryFontFileLoader() }.context(NativeSnafu)?;
        unsafe { factory.RegisterFontFileLoader(&loader) }.context(NativeSnafu)?;
        // Declare guard before font/collection objects so failed attempts drop
        // those native references before unregistering this attempt's loader.
        let guard = Rc::new(Loader {
            factory: factory.clone(),
            loader,
        });
        // SAFETY: null owner requests a native copy before this call returns.
        let file = unsafe {
            guard.loader.CreateInMemoryFontFileReference(
                &factory,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                None::<&IUnknown>,
            )
        }
        .context(NativeSnafu)?;
        let mut supported = BOOL::default();
        let mut kind = DWRITE_FONT_FILE_TYPE_UNKNOWN;
        let mut faces = 0;
        unsafe { file.Analyze(&mut supported, &mut kind, None, &mut faces) }
            .context(NativeSnafu)?;
        if !supported.as_bool() || faces == 0 || faces > 64 {
            return Err(Error::InvalidRequest);
        }
        let builder = unsafe { factory.CreateFontSetBuilder() }.context(NativeSnafu)?;
        unsafe { builder.AddFontFile(&file) }.context(NativeSnafu)?;
        let private = unsafe { builder.CreateFontSet() }.context(NativeSnafu)?;
        let collection =
            unsafe { factory.CreateFontCollectionFromFontSet(&private) }.context(NativeSnafu)?;
        let canonical = if alias.is_some() {
            if unsafe { collection.GetFontFamilyCount() } != 1 {
                return Err(Error::Unsupported {
                    reason: "font aliases require exactly one family per file",
                });
            }
            let family = unsafe { collection.GetFontFamily(0) }.context(NativeSnafu)?;
            let names = unsafe { family.GetFamilyNames() }.context(NativeSnafu)?;
            let length = unsafe { names.GetStringLength(0) }.context(NativeSnafu)? as usize;
            if length == 0 || length > 4096 {
                return Err(Error::InvalidRequest);
            }
            let mut name = vec![0; length + 1];
            unsafe { names.GetString(0, &mut name) }.context(NativeSnafu)?;
            let name = String::from_utf16(&name[..length]).map_err(|_| Error::InvalidRequest)?;
            if name.trim().is_empty() || name.contains('\0') {
                return Err(Error::InvalidRequest);
            }
            Some(name)
        } else {
            None
        };
        let builder = unsafe { factory.CreateFontSetBuilder() }.context(NativeSnafu)?;
        // New private faces precede existing/system faces when names overlap.
        unsafe { builder.AddFontSet(&private) }.context(NativeSnafu)?;
        let previous = match &self.font_set {
            Some(set) => set.clone(),
            None => unsafe { factory.GetSystemFontSet() }.context(NativeSnafu)?,
        };
        unsafe { builder.AddFontSet(&previous) }.context(NativeSnafu)?;
        let set = unsafe { builder.CreateFontSet() }.context(NativeSnafu)?;
        let collection = unsafe { factory.CreateFontCollectionFromFontSet(&set) }
            .context(NativeSnafu)?
            .cast()
            .context(NativeSnafu)?;
        let mut loaders = self.font_registry.loaders.clone();
        loaders.push(guard);
        let registry = Rc::new(Registry {
            loaders,
            bytes: self.font_registry.bytes + bytes.len(),
        });
        // Commit only after every native construction/validation succeeded.
        self.fonts = collection;
        self.font_set = Some(set);
        self.font_registry = registry;
        if let (Some(alias), Some(canonical)) = (alias, canonical) {
            self.aliases.insert(alias.trim().to_lowercase(), canonical);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FONT: &[u8] =
        include_bytes!("../../../../backend/examples/web-preview/public/DejaVuSans.ttf");
    #[test]
    fn private_font_alias_and_loader_survive_the_originating_context() {
        let mut context = Context::new().unwrap();
        context
            .register_font(FONT, Some("Plico Private Font"))
            .unwrap();
        assert!(
            context
                .resolve_family(&["plico private font"])
                .unwrap()
                .is_some()
        );
        assert!(
            Context::new()
                .unwrap()
                .resolve_family(&["Plico Private Font"])
                .unwrap()
                .is_none()
        );
        let mut request = Request::new("Private ffi 0123");
        request.family = "Plico Private Font";
        let paragraph = context.layout(&request).unwrap();
        let registry = Rc::downgrade(&context.font_registry);
        // A second registration must not invalidate the old collection/guard.
        context
            .register_font(FONT, Some("Another Private Font"))
            .unwrap();
        drop(context);
        assert!(registry.upgrade().is_some());
        assert!(!paragraph.rasterize(1., [0., 0.]).unwrap().masks.is_empty());
        assert!(paragraph.caret_rect(3, 1.).is_ok());
        drop(paragraph);
        assert!(registry.upgrade().is_none());
    }
    #[test]
    fn failed_font_candidates_leave_the_current_collection_usable() {
        let mut context = Context::new().unwrap();
        context
            .register_font(FONT, Some("Plico Kept Font"))
            .unwrap();
        for _ in 0..4 {
            assert!(
                context
                    .register_font(b"not a font", Some("Invalid"))
                    .is_err()
            );
        }
        assert_eq!(context.font_registry.loaders.len(), 1);
        assert_eq!(context.font_registry.bytes, FONT.len());
        assert!(
            context
                .resolve_family(&["Plico Kept Font"])
                .unwrap()
                .is_some()
        );
        assert!(context.register_font(FONT, Some("bad\0name")).is_err());
        assert!(context.register_font(&[], None).is_err());
    }
}
