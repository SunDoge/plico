//! Reusable WGSL studies adapted from glsl-playground (MIT).
//! Use these assets with `widgets::shader`; shader clocks, pause and reduced
//! motion are handled by the runtime. No window or GPU objects live here.
use crate::ShaderAsset;

/// Four dispersed voice ribbons. Uniform scalar 0 is energy in 0..=1.
pub const fn voice_wave() -> ShaderAsset {
    ShaderAsset::new(concat!(
        include_str!("../assets/effects/voice_core.wgsl"),
        include_str!("../assets/effects/voice_wave.wgsl")
    ))
    .with_source_path("effects/voice_wave.wgsl")
}
/// Six spring-driven, smoothly merging dots; no custom uniforms.
pub const fn fluid_dots() -> ShaderAsset {
    crate::shader_asset!("../assets/effects/fluid_dots.wgsl")
}
/// Premultiplied edge-light overlay. Scalars 0/1 are intensity and corner
/// radius as a fraction of half the width. Uses procedural noise, no textures.
pub const fn edge_light() -> ShaderAsset {
    crate::shader_asset!("../assets/effects/edge_light.wgsl")
}
/// Scene-backed lens and gradient-aware liquid union. Center and half-size are fractions of the shader surface; remaining distances are logical pixels:
/// `[center_x, center_y, half_width, half_height, top_edge, fusion, refraction,
/// blur, dark_amount, aberration, zoom, corner_radius]`. A negative top edge
/// disables the union. Optional scalars 12/13 select dark capsule mode and
/// equatorial wave strength. Blur is bounded to 32 px; foreground stays separate.
pub const fn liquid_lens() -> ShaderAsset {
    ShaderAsset::new(concat!(
        include_str!("../assets/effects/voice_core.wgsl"),
        include_str!("../assets/effects/liquid_lens.wgsl")
    ))
    .with_source_path("effects/liquid_lens.wgsl")
    .prefilter_backdrop(32.)
}
