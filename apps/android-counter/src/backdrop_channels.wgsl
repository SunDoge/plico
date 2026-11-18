fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let color = plico_backdrop_at(uv);
    return vec4<f32>(color.gbr, color.a);
}
