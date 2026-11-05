// Frosted glass pane over whatever was painted behind it (MyGO's liquid-glass
// example): a nine-tap blur, a rim lens that bends the backdrop, chromatic
// fringing, a bright inner rim and a slow specular sheen. Requires
// read_backdrop; values are [frost, refraction in px, sheen].
fn sd_round_box(p: vec2<f32>, half: vec2<f32>, radius: f32) -> f32 {
    let q = abs(p) - half + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn glass_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn glass_blur(uv: vec2<f32>, spread: vec2<f32>) -> vec4<f32> {
    var sum = plico_backdrop_at(uv) * 0.24;
    sum = sum + plico_backdrop_at(uv + vec2<f32>(spread.x, 0.0)) * 0.11;
    sum = sum + plico_backdrop_at(uv - vec2<f32>(spread.x, 0.0)) * 0.11;
    sum = sum + plico_backdrop_at(uv + vec2<f32>(0.0, spread.y)) * 0.11;
    sum = sum + plico_backdrop_at(uv - vec2<f32>(0.0, spread.y)) * 0.11;
    let diag = spread * 0.7071;
    sum = sum + plico_backdrop_at(uv + diag) * 0.06;
    sum = sum + plico_backdrop_at(uv + vec2<f32>(diag.x, -diag.y)) * 0.06;
    sum = sum + plico_backdrop_at(uv + vec2<f32>(-diag.x, diag.y)) * 0.06;
    sum = sum + plico_backdrop_at(uv - diag) * 0.06;
    return sum + plico_backdrop_at(uv) * 0.08;
}

fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = plico.resolution_time_scale.xy;
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let time = plico.resolution_time_scale.z;
    let frost = clamp(plico.values[0].x, 0.0, 1.0);
    let bend = max(plico.values[1].x, 0.0);
    let sheen_strength = clamp(plico.values[2].x, 0.0, 1.0);
    let size = resolution / scale;
    let half = size * 0.5;
    let p = (uv - vec2<f32>(0.5)) * size;
    let sd = sd_round_box(p, half - vec2<f32>(1.0), min(half.x, half.y) * 0.3);
    let inside = smoothstep(0.0, min(half.x, half.y) * 0.55, -sd);
    let direction = normalize(p + vec2<f32>(0.0001, 0.0002));
    // The rim acts as a weak lens: it pulls the backdrop inward near the edge.
    let displacement = direction * bend * (1.0 - inside) * (1.0 - inside) * scale;
    let lens_uv = clamp(
        uv + displacement / resolution,
        vec2<f32>(0.5) / resolution,
        (resolution - vec2<f32>(0.5)) / resolution,
    );
    let spread = vec2<f32>(frost * 22.0 * scale, frost * 22.0 * scale) / resolution;
    let glass = glass_blur(lens_uv, spread);
    // Chromatic fringing grows toward the rim and with frost.
    let fringe = (0.002 + 0.004 * frost) * smoothstep(0.35, 1.0, 1.0 - inside);
    let red = plico_backdrop_at(clamp(lens_uv + direction * fringe, vec2<f32>(0.0), vec2<f32>(1.0))).r;
    let blue = plico_backdrop_at(clamp(lens_uv - direction * fringe, vec2<f32>(0.0), vec2<f32>(1.0))).b;
    var color = vec3<f32>(red, glass.g, blue);
    // A cool tint keeps the pane readable over busy content.
    color = mix(color, vec3<f32>(0.78, 0.86, 0.98), 0.06 + 0.1 * frost);
    let grain = glass_hash(uv * resolution + vec2<f32>(time * 4.0, time * 2.0)) - 0.5;
    color = color + vec3<f32>(grain) * (0.009 + 0.012 * frost);
    // Lit top-left inner rim, dimmer opposite edge.
    let rim = smoothstep(2.5, 0.0, abs(sd));
    let light_side = clamp(dot(direction, normalize(vec2<f32>(-0.6, -0.8))) * 0.5 + 0.5, 0.0, 1.0);
    color = color + vec3<f32>(0.82, 0.9, 1.0) * rim * (0.2 + 0.35 * light_side);
    // A soft band crossing the pane every few seconds.
    let sweep = fract(time * 0.08) * 2.6 - 0.8;
    let sheen = exp(-pow((uv.x + uv.y * 0.55 - sweep) * 5.0, 2.0));
    color = color + vec3<f32>(0.95, 0.98, 1.0) * sheen * 0.16 * sheen_strength;
    color = color * (0.94 + 0.06 * smoothstep(-1.0, 1.0, uv.y));
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
