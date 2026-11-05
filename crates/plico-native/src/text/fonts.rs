//! Private Fontconfig collection. Anonymous font files live as long as glyphs.
use super::Error;
type Result<T> = std::result::Result<T, Error>;
use fontconfig_sys as fc;
use pango::glib::translate::ToGlibPtr;
use std::{
    ffi::{CStr, CString},
    fs::File,
    io::Write,
    os::fd::AsRawFd,
    ptr::NonNull,
};

#[link(name = "pangoft2-1.0")]
unsafe extern "C" {
    fn pango_fc_font_map_set_config(map: *mut std::ffi::c_void, config: *mut fc::FcConfig);
    fn pango_fc_font_map_config_changed(map: *mut std::ffi::c_void);
}

#[derive(Debug)]
pub(super) struct Fonts {
    config: Option<NonNull<fc::FcConfig>>,
    files: Vec<File>,
    bytes: usize,
}
impl Fonts {
    pub fn new() -> Self {
        Self {
            config: None,
            files: Vec::new(),
            bytes: 0,
        }
    }
    pub fn add(&mut self, map: &pango::FontMap, bytes: &[u8], alias: Option<&str>) -> Result<()> {
        let rejected = |reason| Error::FontCollection { reason };
        let next_bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| rejected("font byte budget"))?;
        if bytes.is_empty() || self.files.len() >= 64 || next_bytes > 128 * 1024 * 1024 {
            return Err(rejected("font count or byte budget"));
        }
        let alias = alias
            .map(CString::new)
            .transpose()
            .map_err(|_| rejected("invalid family alias"))?;
        let mut file = tempfile::tempfile().map_err(|source| Error::FontIo { source })?;
        file.write_all(bytes)
            .map_err(|source| Error::FontIo { source })?;
        let path = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
        if self.config.is_none() {
            // SAFETY: creates an owned, independent config with the system fallback fonts.
            self.config = NonNull::new(unsafe { fc::FcInitLoadConfigAndFonts() });
        }
        let config = self
            .config
            .ok_or_else(|| rejected("could not initialize Fontconfig"))?;
        // SAFETY: config is owned; path is valid for the call and the open anonymous
        // file is retained below. Fontconfig copies the path into its font patterns.
        if unsafe { fc::FcConfigAppFontAddFile(config.as_ptr(), path.as_ptr().cast()) } == 0 {
            return Err(rejected("invalid application font"));
        }
        // Retain immediately: even a later alias error must not leave dangling paths
        // in a configuration that another previously created layout still owns.
        self.files.push(file);
        self.bytes = next_bytes;
        if let Some(alias) = alias {
            // SAFETY: Fontconfig owns this set and its patterns. Only patterns for
            // the just-added private path are modified; no process-global set changes.
            unsafe {
                let set = fc::FcConfigGetFonts(config.as_ptr(), fc::FcSetApplication);
                if set.is_null() || (*set).nfont <= 0 || (*set).fonts.is_null() {
                    return Err(rejected("missing application font set"));
                }
                let patterns = std::slice::from_raw_parts((*set).fonts, (*set).nfont as usize);
                let mut matched = false;
                for &pattern in patterns {
                    let mut value = std::ptr::null_mut();
                    if fc::FcPatternGetString(pattern, c"file".as_ptr(), 0, &mut value)
                        == fc::FcResultMatch
                        && !value.is_null()
                        && CStr::from_ptr(value.cast()) == path.as_c_str()
                    {
                        matched = true;
                        if fc::FcPatternAddString(
                            pattern,
                            c"family".as_ptr(),
                            alias.as_ptr().cast(),
                        ) == 0
                        {
                            return Err(rejected("could not register family alias"));
                        }
                    }
                }
                if !matched {
                    return Err(rejected("application font path not found"));
                }
            }
        }
        // SAFETY: on Linux this is the Cairo FreeType (PangoFc) font map. Pango
        // retains its own config reference. Fonts is additionally retained by
        // every returned layout/glyph, keeping anonymous files valid for rasterization.
        unsafe {
            let pointer: *mut pango::ffi::PangoFontMap = map.to_glib_none().0;
            pango_fc_font_map_set_config(pointer.cast(), config.as_ptr());
            pango_fc_font_map_config_changed(pointer.cast());
        }
        Ok(())
    }
}
impl Drop for Fonts {
    fn drop(&mut self) {
        if let Some(config) = self.config {
            // SAFETY: release exactly the config reference returned by init.
            unsafe {
                fc::FcConfigDestroy(config.as_ptr());
            }
        }
    }
}
