//! Thread-local immutable CoreGraphics resources, like MyGO's retained sRGB space.
use super::{Error, Result};
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use std::cell::OnceCell;

thread_local! {
    // No cross-thread native ownership or process-global teardown ordering.
    // Failed creation leaves the cell empty, permitting a later retry.
    static SRGB: OnceCell<CFRetained<CGColorSpace>> = const { OnceCell::new() };
}

pub(super) fn with_srgb<T>(draw: impl FnOnce(&CGColorSpace) -> Result<T>) -> Result<T> {
    SRGB.with(|slot| {
        if slot.get().is_none() {
            // SAFETY: CoreGraphics exports an immutable standard-space name.
            let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
                .ok_or(Error::Allocation)?;
            // This thread has exclusive initialization; creation has no callback.
            let _ = slot.set(space);
        }
        // Borrow instead of retaining/releasing a local native owner per raster.
        // A created CGContext independently retains its color space.
        draw(slot.get().expect("initialized sRGB color space"))
    })
}
