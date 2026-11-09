// Adapted from aaaa-zhen/glsl-playground siri-edge-light.html (MIT).
// Procedural noise replaces its external 3D texture. values[0]: intensity, radius.
fn screen_sdf(np: vec2<f32>, ratio: f32, rr: f32) -> f32 {
    let p = vec2<f32>(np.x, np.y * ratio);
    let r = max(rr, 0.001) * 1.19086;
    let q = abs(p) - vec2<f32>(1.0, ratio) + vec2<f32>(r);
    if min(q.x, q.y) < 0.0 {
        return max(q.x, q.y) - r;
    }
    let w = vec2<f32>(abs(q.x - q.y), q.x + q.y - r) / r;
    let y = (w.y + 0.5) * 0.6665;
    let t = pow(sqrt(abs(y * y * y + w.x * w.x)) + w.x, 1.0 / 3.0) + 0.0000001;
    let c = t - y / t;
    let d = (1.0 - c * c) * 0.5;
    return rr * 0.8409 * sign(w.y - d) * length(w - vec2<f32>(c, d));
}
fn soft_clamp(x: f32) -> f32 {
    return abs(x) / sqrt(x * x + 1.0);
}
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let res = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let t = plico.resolution_time_scale.z;
    let ratio = res.y / res.x;
    let rr = clamp(plico.values[0].y, 0.01, min(0.4, ratio * 0.5));
    let sd = screen_sdf(uv * 2.0 - 1.0, ratio, rr);
    let dist = max(0.0, -sd * 1.7);
    let n = clamp(
            0.5 + 0.22 * sin(uv.x * 19.0 + uv.y * 23.0 + t * 1.4) + 0.18 * sin(uv.x * 37.0 - uv.y * 17.0 - t * 0.9),
            0.0,
            1.0,
        );
    let a = n * 0.8;
    let lobe1 = clamp(pow(max(0.0001, 4.0 - 2.0 * a), -60.0 * (dist - 0.05 * a)), 0.0, 1.0);
    let lobe2 = max(0.0, exp2(-81.0 * dist) - 0.045 * max(0.0, 1.0 - 4.0 * dist));
    let mask = (n + 0.7) * soft_clamp(lobe1 * 0.8) + n * 2.0 * soft_clamp(lobe2);
    let palette = array<vec3<f32>, 4>(
            vec3<f32>(0.7334, 0.3921, 1.0),
            vec3<f32>(0.0, 0.5884, 1.0),
            vec3<f32>(1.0, 0.1765, 0.3333),
            vec3<f32>(1.0, 0.5767, 0.0),
        );
    var col = palette[1];
    for(var i = 0; i < 11; i++) {
        let f = f32(i);
        let pos = 0.5 + 0.65 * vec2<f32>(cos(f * 2.399 + t * 0.17), sin(f * 2.399 + t * 0.13));
        let blend = clamp(1.0 - distance(pos, uv), 0.0, 1.0);
        col = mix(col, palette[i % 4], blend * blend * (3.0 - 2.0 * blend));
    }
    let alpha = clamp(mask * plico.values[0].x, 0.0, 1.0) * (1.0 - smoothstep(-0.002, 0.002, sd));
    return vec4<f32>(col * alpha, alpha);
}
