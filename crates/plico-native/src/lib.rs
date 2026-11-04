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

#[cfg(all(feature = "windows-text", any(target_os = "windows", test)))]
mod text_direction;
#[cfg(all(feature = "windows-text", any(target_os = "windows", test)))]
mod text_indices;
/// DirectWrite layout and UTF-8 editing geometry (experimental Windows service).
#[cfg(all(target_os = "windows", feature = "windows-text"))]
pub mod windows_text;

/// Microsoft Terminal-compatible grayscale coverage correction (experimental).
#[cfg(feature = "windows-text")]
pub mod directwrite_correction;
