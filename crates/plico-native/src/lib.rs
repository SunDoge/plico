//! Optional desktop services, independent of Plico's window and GPU renderer.
//!
//! Nothing is enabled by default. Linux system text requires `linux-text` and
//! Pango/Cairo development libraries. The default Parley pipeline is unchanged.
#![warn(missing_docs)]

#[cfg(all(target_os = "linux", feature = "linux-text"))]
pub mod text;

#[cfg(all(
    feature = "global-hotkeys",
    any(target_os = "linux", target_os = "windows", target_os = "macos")
))]
pub mod hotkeys;

/// Optional, user-authorized system screenshot capture.
#[cfg(feature = "screen-capture")]
pub mod capture;

#[cfg(any(
    all(feature = "windows-text", any(target_os = "windows", test)),
    all(feature = "macos-text", any(target_os = "macos", test))
))]
mod text_direction;
#[cfg(any(
    all(target_os = "windows", feature = "windows-text"),
    all(target_os = "macos", feature = "macos-text"),
    all(test, any(feature = "windows-text", feature = "macos-text"))
))]
mod text_indices;
/// DirectWrite layout and UTF-8 editing geometry (experimental Windows service).
#[cfg(all(target_os = "windows", feature = "windows-text"))]
pub mod windows_text;

/// Microsoft Terminal-compatible grayscale coverage correction (experimental).
#[cfg(feature = "windows-text")]
pub mod directwrite_correction;

/// Experimental CoreText line/paragraph metrics, rasterization and UTF-8 geometry.
#[cfg(all(target_os = "macos", feature = "macos-text"))]
pub mod macos_text;

#[cfg(all(feature = "macos-text", any(target_os = "macos", test)))]
mod raster_bounds;

#[cfg(all(feature = "macos-text", any(target_os = "macos", test)))]
mod paragraph_geometry;

/// System-owned vertical-rl layout/rasterization for bounded document text.
#[cfg(feature = "vertical-text")]
pub mod vertical;
