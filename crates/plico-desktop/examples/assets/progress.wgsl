// Effect surfaces return premultiplied RGBA. Opaque colors need no conversion.
fn plico_effect(uv: vec2f) -> vec4f {
    let progress = clamp(plico.values[0].x, 0., 1.);
    let time = plico.resolution_time_scale.z;
    let pulse = 0.92 + 0.08 * sin(time * 3.);
    let background = vec3f(0.90, 0.92, 0.96);
    let accent = vec3f(0.15, 0.33, 0.89) * pulse;
    return vec4f(mix(background, accent, step(uv.x, progress)), 1.);
}
