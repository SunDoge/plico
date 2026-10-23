fn wabou_effect(uv: vec2<f32>) -> vec4<f32> {
    let time = wabou.resolution_time_scale.z;
    let strength = wabou.values[0].x;
    let wave = 0.5 + 0.5 * sin(uv.x * 8.0 - uv.y * 4.0 + time);
    let glow = exp(-abs(uv.y - 0.5 - 0.18 * sin(uv.x * 6.0 + time * 0.6)) * 9.0);
    let base = mix(vec3<f32>(0.035, 0.06, 0.14), vec3<f32>(0.12, 0.2, 0.36), uv.x);
    let ribbon = mix(vec3<f32>(0.16, 0.48, 0.92), vec3<f32>(0.32, 0.8, 0.68), wave);
    return vec4<f32>(base + ribbon * glow * strength, 1.0);
}
