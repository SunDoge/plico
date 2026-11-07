// Local scene-backed glass. Parameters: radius, frost, tint, animation.
// The exterior returns the untouched scene, preserving rounded pane corners.
fn glass_sample(uv: vec2<f32>) -> vec3<f32> {
    let pixel = 0.5 / max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    return plico_backdrop_at(clamp(uv, pixel, vec2<f32>(1.0) - pixel)).rgb;
}
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let size = resolution / scale;
    let p = (uv - vec2<f32>(0.5)) * size;
    let radius = min(plico.values[0].x, min(size.x, size.y) * 0.5);
    let q = abs(p) - size * 0.5 + vec2<f32>(radius + 0.5);
    let distance = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    let coverage = 1.0 - smoothstep(-0.5 / scale, 0.5 / scale, distance);
    // Analytic gradient of the rounded-box SDF. On straight edges q is
    // negative: normalizing max(q,0)+epsilon incorrectly gives a diagonal
    // everywhere, producing flat white bands instead of a curved lens.
    let outside = max(q, vec2<f32>(0.0));
    var normal = vec2<f32>(0.0, sign(p.y));
    if q.x > q.y {
        normal = vec2<f32>(sign(p.x), 0.0);
    }
    if dot(outside, outside) > 0.000001 {
        normal = normalize(outside) * sign(p);
    }
    let bevel = clamp(radius * 1.1, 10.0, 28.0);
    let depth = max(-distance, 0.0);
    let rim = exp(-pow(depth / bevel, 2.0) * 2.0);
    let lens = uv - normal * rim * plico.values[1].x * scale / resolution;
    let frost = clamp(plico.values[0].y, 0.0, 1.0);
    let spread = frost * 12.0 * scale / resolution;
    var blurred = vec3<f32>(0.0);
    var weight_sum = 0.0;
    for(var y = -2; y <= 2; y = y + 1) {
        for(var x = -2; x <= 2; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y));
            let weight = exp(-dot(offset, offset) * 0.5);
            blurred += glass_sample(lens + offset * spread) * weight;
            weight_sum += weight;
        }
    }
    blurred /= weight_sum;
    // Restrained dispersion is confined to the bevel, leaving the center clear.
    let dispersion = normal * rim * 0.4 * scale / resolution;
    blurred.r = mix(blurred.r, glass_sample(lens + dispersion).r, rim * 0.35);
    blurred.b = mix(blurred.b, glass_sample(lens - dispersion).b, rim * 0.35);
    let luminance = dot(blurred, vec3<f32>(0.2126, 0.7152, 0.0722));
    let saturated = mix(vec3<f32>(luminance), blurred, plico.values[1].y);
    var color = mix(saturated, plico.values[2].xyz, clamp(plico.values[0].z, 0.0, 1.0));
    let light = clamp(dot(normal, normalize(vec2<f32>(-0.5, -0.85))), 0.0, 1.0);
    let edge = exp(-abs(distance + 0.8) * 1.9);
    color += vec3<f32>(edge * (0.06 + light * 0.18) * plico.values[1].z);
    let time = plico.resolution_time_scale.z * plico.values[0].w;
    let specular = exp(-pow((uv.x * 0.7 + uv.y - 0.28 - sin(time * 0.55) * 0.12) * 7.0, 2.0));
    color += vec3<f32>(specular * rim * 0.045 * plico.values[1].z);
    // Stable grain: no full-window animation or temporal noise when idle.
    let grain = fract(sin(dot(floor(uv * resolution), vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
    color += vec3<f32>(grain / 1600.0);
    return vec4<f32>(mix(glass_sample(uv), clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), coverage), 1.0);
}
