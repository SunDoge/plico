fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = plico.values[0].x;
    let t = plico.resolution_time_scale.z;
    let d = length(uv - vec2<f32>(0.5));
    let rings = 0.5 + 0.5 * sin(d * 42.0 - t * 2.0);
    let fade = exp(-d * 3.0);
    let color = mix(vec3<f32>(0.08, 0.15, 0.3), vec3<f32>(0.32, 0.65, 0.85), rings * fade);
    return vec4<f32>(strength * (color), 1.0);
}
