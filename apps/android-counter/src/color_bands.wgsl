fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    if uv.x < 1.0 / 3.0 {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0);
    }
    if uv.x < 2.0 / 3.0 {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);
    }
    return vec4<f32>(0.0, 0.0, 1.0, 1.0);
}
