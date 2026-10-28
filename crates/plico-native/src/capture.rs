//! User-authorized screenshots, independent of the window and rendering backend.
//!
//! Linux uses the desktop Screenshot portal. It returns a URI owned by the
//! portal rather than pixels; applications should load it promptly. Portal
//! implementations choose what can be captured and whether to show a dialog.
//! This API is for occasional screenshots, not continuous screen capture.

use snafu::Snafu;

/// The desktop's response to a screenshot request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screenshot {
    /// URI of the captured image, normally a local `file://` URI on Linux.
    /// Its lifetime and format are controlled by the desktop portal.
    pub uri: String,
}

/// Failure to request a screenshot from the desktop.
#[derive(Debug, Snafu)]
pub enum Error {
    /// Screen capture has not yet been implemented for this platform.
    #[snafu(display("screen capture is not supported on this platform"))]
    Unsupported,
    /// The portal request failed, was cancelled, or was refused.
    #[snafu(display("screenshot portal: {message}"))]
    Portal {
        /// Portal-provided error details.
        message: String,
    },
}

/// Request a screenshot through the desktop's interactive picker.
///
/// On Linux this requires an async runtime and a working XDG desktop portal.
/// The portal controls the selection UI and can reject the request. On other
/// platforms the function returns [`Error::Unsupported`].
pub async fn screenshot() -> Result<Screenshot, Error> {
    #[cfg(target_os = "linux")]
    {
        let request = ashpd::desktop::screenshot::Screenshot::request()
            .interactive(true)
            .send()
            .await
            .map_err(|error| Error::Portal {
                message: error.to_string(),
            })?;
        let screenshot = request.response().map_err(|error| Error::Portal {
            message: error.to_string(),
        })?;
        Ok(Screenshot {
            uri: screenshot.uri().to_string(),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(Error::Unsupported)
    }
}
