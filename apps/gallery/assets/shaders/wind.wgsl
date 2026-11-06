// A flowing wind field after MyGO's wind example (wind.kage): domain-warped
// value noise bends luminous ribbons across a dark sky, with drifting
// sparkles and a vignette. values are [glow, hue shift].
fn wind_hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx + vec3<f32>(33.33));
    return fract((p3.x + p3.y) * p3.z);
}

fn wind_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = wind_hash(i);
    let b = wind_hash(i + vec2<f32>(1.0, 0.0));
    let c = wind_hash(i + vec2<f32>(0.0, 1.0));
    let d = wind_hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn wind_fbm(p0: vec2<f32>) -> f32 {
    var p = p0;
    var value = 0.0;
    var amplitude = 0.5;
    for(var i = 0; i < 4; i = i + 1) {
        value = value + amplitude * wind_noise(p);
        p = p * 2.03 + vec2<f32>(1.7, 9.2);
        amplitude = amplitude * 0.5;
    }
    return value;
}

fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = plico.resolution_time_scale.xy;
    let time = plico.resolution_time_scale.z;
    let glow = clamp(plico.values[0].x, 0.0, 1.0);
    let hue = plico.values[1].x;
    let aspect = resolution.x / max(resolution.y, 1.0);
    let q = vec2<f32>(uv.x * aspect, uv.y);
    // Two slow warps fold the noise into long, breathing gusts.
    let flow = q * vec2<f32>(2.4, 3.0) + vec2<f32>(time * 0.12, -time * 0.08);
    let warp = vec2<f32>(
            wind_fbm(flow + vec2<f32>(0.0, time * 0.15)),
            wind_fbm(flow + vec2<f32>(4.7, 2.3) - time * 0.1),
        );
    let n = wind_fbm(flow + warp * 1.8);
    let bands = sin((q.y * 7.0 + n * 4.0 - time * 0.6) * 3.14159265);
    let ribbon = pow(abs(bands), 6.0);
    let streak = pow(0.5 + 0.5 * sin(q.x * 18.0 + n * 10.0 - time * 1.2), 12.0);
    let light = smoothstep(0.2, 1.0, n * 0.7 + ribbon * 0.6 + streak * 0.35);
    let base = mix(vec3<f32>(0.02, 0.03, 0.08), vec3<f32>(0.05, 0.12, 0.22), uv.y);
    let tint = mix(vec3<f32>(0.15, 0.75, 0.85), vec3<f32>(0.62, 0.35, 0.95), clamp(n + hue, 0.0, 1.0));
    var color = base + tint * light * (0.45 + 0.75 * glow);
    // Sparse sparkles drift with the flow.
    let cell = floor(q * vec2<f32>(120.0, 90.0));
    let star = wind_hash(cell + floor(time * 3.0));
    let twinkle = step(0.994, star) * (0.5 + 0.5 * sin(time * 6.0 + star * 30.0));
    color = color + vec3<f32>(0.7, 0.9, 1.0) * twinkle * 0.8;
    color = color * (1.0 - 0.45 * length(uv - vec2<f32>(0.5)));
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
