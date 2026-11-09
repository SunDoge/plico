fn plico_effect(uv: vec2f) -> vec4f {
    return plico_backdrop_blur(uv, 8. * plico.resolution_time_scale.w);
}
