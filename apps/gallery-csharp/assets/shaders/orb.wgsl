fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = plico.values[0].x;
    let t = plico.resolution_time_scale.z;
    let p = (uv - vec2<f32>(0.5)) * vec2<f32>(1.6, 1.0);
    let radius = length(p);
    let halo = exp(-radius * radius * 15.0);
    let ring = exp(-abs(radius - 0.28) * 65.0);
    let swirl = 0.5 + 0.5 * sin(atan2(p.y, p.x) * 3.0 + radius * 18.0 - t);
    let color = mix(vec3<f32>(0.15, 0.45, 0.92), vec3<f32>(0.9, 0.35, 0.55), swirl);
    return vec4<f32>(strength * (vec3<f32>(0.025, 0.035, 0.07) + color * (halo * 0.65 + ring * 0.5)), 1.0);
}
