fn plico_effect(uv: vec2f) -> vec4f {
    let x = uv.x * plico.resolution_time_scale.x / max(plico.resolution_time_scale.w, 0.01);
    let stripe = u32(floor(x / 4.)) % 2u;
    return vec4f(vec3f(f32(stripe)), 1.);
}
