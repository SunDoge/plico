fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = plico.values[0].x;
    let t = plico.resolution_time_scale.z;
    let wave = 0.5 + 0.5 * sin(uv.x * 5.0 - t * 0.45);
    let ribbon = exp(-abs(uv.y - 0.48 - 0.17 * sin(uv.x * 6.0 + t * 0.7)) * 12.0);
    let ribbon2 = exp(-abs(uv.y - 0.64 - 0.15 * sin(uv.x * 5.0 - t * 0.5)) * 14.0);
    let base = mix(vec3<f32>(0.025, 0.04, 0.12), vec3<f32>(0.05, 0.12, 0.23), uv.y);
    let light = mix(vec3<f32>(0.12, 0.42, 0.95), vec3<f32>(0.2, 0.85, 0.65), wave);
    return vec4<f32>(strength * (base + light * ribbon * 0.8 + vec3<f32>(0.33, 0.2, 0.65) * ribbon2), 1.0);
}
