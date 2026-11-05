// A small dark center inside a broad, smoothly decaying gravitational lens.
// Pointer values are window-logical pixels; texture coordinates are physical.
fn plico_effect(uv: vec2f) -> vec4f {
    let resolution = plico.resolution_time_scale.xy;
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let pointer = plico.values[0].xy;
    let original = plico_backdrop_at(uv);
    if any(pointer < vec2f(0.0)) {
        return original;
    }
    let p = (uv * resolution - pointer * scale) / scale;
    let r = length(p);
    if r > 300.0 {
        return original;
    }
    let direction = p / max(r, 0.001);
    // Keep the horizon small independently of the lens strength and radius.
    // The outer taper has zero slope, so there is no visible circular seam.
    let taper = 1.0 - smoothstep(220.0, 300.0, r);
    let distortion = 95.0 * r / (r + 36.0) * exp(-pow(r / 180.0, 2.0)) * taper;
    let sample_uv = clamp(
            uv + direction * distortion * scale / resolution,
            vec2f(0.5) / resolution,
            (resolution - 0.5) / resolution,
        );
    let backdrop = plico_backdrop_at(sample_uv);
    // The interface itself forms the lens arcs. A restrained rim defines
    // the small horizon without drawing a separate decorative orbit.
    let rim = exp(-pow((r - 11.2) / 0.8, 2.0));
    var color = mix(backdrop.rgb, vec3f(0.84, 0.86, 0.9), rim * 0.28);
    let core = 1.0 - smoothstep(9.5, 10.5, r);
    color = mix(color, vec3f(0.015, 0.018, 0.025), core);
    return vec4f(color, 1.0);
}
