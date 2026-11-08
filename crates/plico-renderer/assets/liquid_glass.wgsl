// The lens profile and smoothed normals adapt MyGO plugins/glass/glass.glsl
// and effect.go, Copyright (c) 2026 MyGo contributors (MIT).
// See ../third_party/mygo-LICENSE and ../third_party/README.md.
// Local scene-backed glass. Parameters: radius, frost, tint, animation.
// The exterior returns the untouched scene, preserving rounded pane corners.
fn glass_backdrop(uv: vec2<f32>) -> vec4<f32> {
    let pixel = 0.5 / max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    return plico_backdrop_at(clamp(uv, pixel, vec2<f32>(1.0) - pixel));
}
// Snell refraction through a quartic/squircle bezel, normalized to 0..1.
fn glass_sample(uv: vec2<f32>) -> vec3<f32> {
    return glass_backdrop(uv).rgb;
}
fn glass_lens(depth: f32, bezel: f32) -> f32 {
    if bezel <= 0.0 || depth >= bezel {
        return 0.0;
    }
    let u = 1.0 - max(depth, 0.0) / bezel;
    let v = max(1.0 - u * u * u * u, 0.0);
    let a = sqrt(v * sqrt(v));
    let b = u * u * u;
    let length = max(sqrt(a * a + b * b), 0.000001);
    let sine = b / length;
    let cosine = a / length;
    let refracted_sine = sine / 1.5;
    let refracted_cosine = sqrt(1.0 - refracted_sine * refracted_sine);
    return (sine * refracted_cosine - cosine * refracted_sine)
        / max(cosine * refracted_cosine + sine * refracted_sine, 0.000001) / 1.1180340;
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
    let normal_radius = min(radius * 1.5, min(size.x, size.y) * 0.5);
    let normal_q = abs(p) - size * 0.5 + vec2<f32>(normal_radius);
    let outside = max(normal_q, vec2<f32>(0.0));
    var normal = vec2<f32>(0.0, sign(p.y));
    if normal_q.x > normal_q.y {
        normal = vec2<f32>(sign(p.x), 0.0);
    }
    if dot(outside, outside) > 0.000001 {
        normal = normalize(outside) * sign(p);
    }
    let bevel = min(plico.values[1].w, min(size.x, size.y) * 0.5);
    let depth = max(-distance, 0.0);
    let rim = glass_lens(depth, bevel);
    let lens = uv - normal * rim * min(plico.values[1].x, bevel * 1.6) * scale / resolution;
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
    // Opposite edges face the same overhead light, as a polished glass rim.
    let light = abs(normal.y);
    let edge = exp(-depth * depth / 1.44);
    color = mix(color, vec3<f32>(light), edge * 0.35 * plico.values[1].z);
    let time = plico.resolution_time_scale.z * plico.values[0].w;
    let specular = exp(-pow((uv.x * 0.7 + uv.y - 0.28 - sin(time * 0.55) * 0.12) * 7.0, 2.0));
    color += vec3<f32>(specular * rim * 0.045 * plico.values[1].z);
    // Stable grain: no full-window animation or temporal noise when idle.
    let grain = fract(sin(dot(floor(uv * resolution), vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
    color += vec3<f32>(grain / 1600.0);
    return mix(glass_backdrop(uv), vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0), coverage);
}
