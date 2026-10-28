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
