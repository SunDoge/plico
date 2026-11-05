// The assistant's orb from MyGO's chat-glow example (orb.kage): a glass
// sphere of colored light whose blobs drift and fold faster while it thinks,
// lit from the top left with a colored rim and a soft halo. values[0] is
// energy from 0 (at rest) to 1 (thinking). Straight-alpha in, premultiplied
// out.
fn palette(x: f32) -> vec3<f32> {
    let h = fract(x) * 6.0;
    if h < 1.0 {
        return mix(vec3<f32>(0.74, 0.51, 0.95), vec3<f32>(0.96, 0.73, 0.92), h);
    } else if h < 2.0 {
        return mix(vec3<f32>(0.96, 0.73, 0.92), vec3<f32>(0.55, 0.62, 1.0), h - 1.0);
    } else if h < 3.0 {
        return mix(vec3<f32>(0.55, 0.62, 1.0), vec3<f32>(0.67, 0.43, 0.93), h - 2.0);
    } else if h < 4.0 {
        return mix(vec3<f32>(0.67, 0.43, 0.93), vec3<f32>(1.0, 0.4, 0.47), h - 3.0);
    } else if h < 5.0 {
        return mix(vec3<f32>(1.0, 0.4, 0.47), vec3<f32>(1.0, 0.73, 0.44), h - 4.0);
    }
    return mix(vec3<f32>(1.0, 0.73, 0.44), vec3<f32>(0.74, 0.51, 0.95), h - 5.0);
}

fn around(p: vec2<f32>) -> f32 {
    return atan2(p.y, p.x) / 6.2831853 + 0.5;
}

fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = plico.resolution_time_scale.xy;
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let time = plico.resolution_time_scale.z;
    let energy = clamp(plico.values[0].x, 0.0, 1.0);
    let size = resolution / scale;
    let inner = min(size.x, size.y) * 0.5 / 1.25;
    let p = (uv - vec2<f32>(0.5)) * size;
    let q = p / max(inner, 0.001);
    let r = length(q);
    let t = time;
    if r > 1.25 {
        return vec4<f32>(0.0);
    }
    if r > 1.0 {
        let halo = exp(-(r - 1.0) * 5.0) * (0.25 + 0.2 * energy) * (1.0 - smoothstep(1.05, 1.25, r));
        let c = palette(t * 0.05 + around(q));
        return vec4<f32>(c * halo, halo);
    }
    let z = sqrt(max(1.0 - r * r, 0.0));
    let normal = vec3<f32>(q.x, q.y, z);
    var w = q * (1.1 + 0.15 * z);
    w = w + (0.22 + 0.25 * energy) * vec2<f32>(sin(w.y * 2.7 + t * 1.1), cos(w.x * 2.3 - t * 0.9));
    w = w + 0.12 * vec2<f32>(sin(w.y * 5.1 - t * 1.7), cos(w.x * 4.7 + t * 1.3));
    var color = vec3<f32>(0.07, 0.04, 0.18);
    for (var i = 0; i < 5; i = i + 1) {
        let f = f32(i);
        let blob = 0.6 * vec2<f32>(
            sin(t * (0.5 + 0.13 * f) + f * 2.1),
            cos(t * (0.43 + 0.11 * f) + f * 1.3),
        );
        let k = exp(-dot(w - blob, w - blob) * 3.4);
        color = color + palette(f / 5.0 + t * 0.03) * k * 0.85;
    }
    // Glass: darker toward the rim inside, a colored rim, a highlight.
    let rim = pow(1.0 - z, 2.6);
    color = color * (0.7 + 0.4 * z);
    color = color + palette(around(q) + t * 0.05) * rim * 0.8;
    let spec = pow(max(dot(normal, normalize(vec3<f32>(-0.45, -0.6, 0.66))), 0.0), 60.0);
    color = color + vec3<f32>(1.0) * spec * 0.7;
    color = color + vec3<f32>(1.0) * 0.06 * smoothstep(0.3, 1.0, -q.y * z);
    let aa = 1.5 / max(inner * scale, 1.0);
    let alpha = 1.0 - smoothstep(1.0 - aa, 1.0, r);
    return vec4<f32>(min(color, vec3<f32>(1.0)) * alpha, alpha);
}
