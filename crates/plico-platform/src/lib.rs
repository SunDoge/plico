//! Platform contracts shared by Plico hosts.
pub mod app_dirs;
pub mod effect;
mod effect_protocol;
mod resource;
pub use resource::ResourceKey;
pub mod window;
pub mod window_lifecycle;

/// Parse links that a Plico host may hand to a system browser.
pub fn allowed_external_url(raw: &str) -> Option<url::Url> {
    let url = url::Url::parse(raw).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}
