// Scene-backed glass. MyGO's quartic lens, rounded-rectangle normal and
// tone curve adapted to WGSL from plugins/glass/glass.glsl (MIT).
// Original Plico scene sampling / coverage retained (Apache-2.0).
// Framework implementation: Apache-2.0; MyGO-derived formulas: MIT.
// See ../../third_party/README.md and ../../third_party/mygo-LICENSE.
fn glass_backdrop(uv: vec2<f32>) -> vec4<f32> {
    let pixel = 0.5 / max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    return plico_backdrop_at(clamp(uv, pixel, vec2<f32>(1.0) - pixel));
}
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
    let n = max(sqrt(a * a + b * b), 0.00001);
    let s = b / n;
    let c = a / n;
    let st = s / 1.5;
    let ct = sqrt(max(1.0 - st * st, 0.0));
    return (s * ct - c * st) / max(c * ct + s * st, 0.00001) / 1.1180340;
}
fn glass_normal(p: vec2<f32>, half_size: vec2<f32>, radius: f32) -> vec2<f32> {
    let r = min(radius * 1.5, min(half_size.x, half_size.y));
    let a = abs(p) - (half_size - vec2<f32>(r));
    if a.x > 0.0 || a.y > 0.0 {
        let m = max(a, vec2<f32>(0.0));
        return sign(p) * m / max(length(m), 0.00001);
    }
    if a.x > a.y {
        return vec2<f32>(sign(p.x), 0.0);
    }
    return vec2<f32>(0.0, sign(p.y));
}
// Same implicit continuous-corner field as plico-renderer/corner_shape.rs.
// Uniform radii: the nearest corner is the maximal corner field.
fn glass_contour_field(p: vec2<f32>, size: vec2<f32>, radius: f32, smoothing: f32) -> f32 {
    let u = size * 0.5 - abs(p);
    let edge = -min(u.x, u.y);
    if radius <= 0.0 {
        return edge;
    }
    var circular = edge;
    if u.x < radius && u.y < radius {
        circular = max(circular, length(max(vec2<f32>(radius) - u, vec2<f32>(0.0))) - radius);
    }
    if smoothing <= 0.0 || radius >= min(size.x,size.y)*0.5 {
        // Fully rounded controls remain true circles/capsules.
        return circular;
    }
    let extent = 1.528665 * radius;
    var continuous = edge;
    if u.x < extent && u.y < extent {
        let a = max(vec2<f32>(1.0) - u / extent, vec2<f32>(0.0));
        let len = length(a);
        let high = max(a.x, a.y);
        var rho = 0.0;
        if high > 0.0 {
            rho = min(min(a.x, a.y) / high, 1.0);
        }
        let polynomial = (((-0.926054 * rho + 3.15601) * rho - 3.64122) * rho + 1.26803) * rho + 0.268531;
        let curve = len + 1.0 - 1.0 / (1.0 - rho * rho * min(len, 1.0) * polynomial);
        let q = max(a * 1.528665 - vec2<f32>(0.528665), vec2<f32>(0.0));
        let circle = length(q) * 0.654166 + 0.345834;
        let clamp_xy = clamp((vec2<f32>(1.528665) - size / (2.0 * radius)) / 0.528665, vec2<f32>(0.0), vec2<f32>(1.0));
        let effective = mix(extent, radius, max(clamp_xy.x, clamp_xy.y));
        var direction = -1.0;
        if a.y > a.x {
            direction = 1.0;
        }
        let weight = clamp(0.5 - direction + direction * rho, 0.0, 1.0);
        let fx = mix(curve, circle, clamp_xy.x);
        let fy = mix(curve, circle, clamp_xy.y);
        continuous = max(edge, min(max(effective - u.x, effective - u.y), 0.0) + extent * (mix(fx, fy, weight) - 1.0));
    }
    return mix(circular, continuous, smoothing);
}
// MyGO paneTone: clarity changes larger panes, while controls keep their material.
fn glass_parameter(frost: f32, mid: f32, clear: f32, clarity: f32, pane: f32) -> f32 {
    var value = mix(frost, mid, min(clarity * 2.0, 1.0));
    if clarity > 0.5 { value = mix(mid, clear, (clarity - 0.5) * 2.0); }
    return mix(frost, value, pane);
}
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let size = resolution / scale;
    let p = (uv - vec2<f32>(0.5)) * size;
    let hover = clamp(plico.values[4].x,0.0,1.0);
    let pressed = clamp(plico.values[4].y,0.0,1.0);
    let inset = plico.values[4].zw - vec2f(1.1,0.45)*pressed;
    let pane_size = size - 2.0*inset;
    let radius = min(plico.values[0].x + 0.45*pressed, min(pane_size.x,pane_size.y)*0.5);
    let smoothing = clamp(plico.values[3].x, 0.0, 1.0);
    var distance = glass_contour_field(p, pane_size, radius, smoothing);
    var normal = glass_normal(p, pane_size * 0.5, radius);
    // Only curved continuous corners need the field gradient. Flat edges retain
    // the inexpensive analytic path and the lens never affects the center.
    let nearest = pane_size * 0.5 - abs(p);
    if smoothing > 0.0 && radius < min(pane_size.x,pane_size.y)*0.5
        && nearest.x < radius * 1.528665 && nearest.y < radius * 1.528665 {
        let epsilon = 0.25 / scale;
        let dx =
            (glass_contour_field(p + vec2<f32>(epsilon, 0.0), pane_size, radius, smoothing)
                - glass_contour_field(p - vec2<f32>(epsilon, 0.0), pane_size, radius, smoothing)) / (2.0 * epsilon);
        let dy =
            (glass_contour_field(p + vec2<f32>(0.0, epsilon), pane_size, radius, smoothing)
                - glass_contour_field(p - vec2<f32>(0.0, epsilon), pane_size, radius, smoothing)) / (2.0 * epsilon);
        let gradient = vec2<f32>(dx, dy);
        let magnitude = max(length(gradient), 0.00001);
        normal = gradient / magnitude;
        distance /= magnitude;
    }
    let coverage = 1.0 - smoothstep(-0.5 / scale, 0.5 / scale, distance);
    let depth = max(-distance, 0.0);
    let material = plico.values[3].y;
    let clarity = clamp(plico.values[3].z, 0.0, 1.0);
    let dark = plico.values[3].w > 0.5;
    let short_side = min(pane_size.x, pane_size.y);
    let pane = smoothstep(100.0, 200.0, short_side);
    var bezel_width = plico.values[1].w;
    if material > 0.5 { bezel_width *= glass_parameter(36.0,48.0,48.0,clarity,pane)/36.0; }
    let bezel = min(bezel_width, short_side * 0.5);
    let refraction = bezel * plico.values[1].x * glass_lens(depth, bezel);
    let lens = uv - normal * refraction * scale / resolution;
    let frost = clamp(plico.values[0].y, 0.0, 1.0);
    var sigma = 0.6 + frost * 5.0;
    if material > 0.5 {
        sigma = min(max(short_side * 0.035,1.0),glass_parameter(10.0,5.0,2.0,clarity,pane));
        if material > 1.5 { sigma = 1.0; }
        sigma *= frost / 0.42;
    }
    let spread = sigma * scale / resolution;
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
    let luminance = dot(blurred, vec3<f32>(0.2126, 0.7152, 0.0722));
    // A clearer MyGO-style light material: the scene remains legible through
    // broad panes, while surface text is painted separately and stays sharp.
    var low = mix(0.125, 0.35, frost);
    var high = 1.0;
    var curve = 1.2;
    var saturation = plico.values[1].y;
    if material > 1.5 {
        low = 0.125; high = 1.082; curve = 1.0;
    } else if material > 0.5 {
        low = glass_parameter(0.541,0.35,0.2,clarity,pane);
        if dark {
            low = glass_parameter(0.15,0.15,0.1,clarity,pane);
            high = glass_parameter(0.51,0.6,0.85,clarity,pane);
            curve = 2.0; saturation *= 2.2;
        }
    }
    low = min(low + 0.02*hover,1.0);
    let toned = low + (high - low) * (1.0 - pow(max(1.0 - luminance, 0.0), curve));
    var color = clamp(vec3<f32>(toned) + (blurred - vec3<f32>(luminance)) * (high - low) * saturation,
        vec3<f32>(0.0),vec3<f32>(1.0));
    let tint_amount = clamp(plico.values[0].z, 0.0, 1.0);
    // Prominent controls need strong color to keep on-accent text readable.
    // Ordinary translucent panes retain the historical 0.88 tint response.
    let tint_alpha = min(tint_amount * (0.88 + 0.08 * smoothstep(0.88,1.0,tint_amount))*(1.0+0.1*hover),1.0);
    color = mix(color, plico.values[2].xyz, tint_alpha);
    // MyGO's one-pixel lit rim, with a wider, softer inner reflection.
    let light = abs(dot(normal, normalize(vec2<f32>(-0.25, -1.0))));
    var rim_strength = plico.values[1].z;
    if material > 0.5 { rim_strength *= glass_parameter(0.35,0.5,0.5,clarity,pane)/0.35; }
    let rim = min(rim_strength+0.18*hover,1.0) * exp(-depth * depth);
    color = mix(color, vec3<f32>(light), rim);
    let inner = exp(-pow((depth - 2.0) / 1.25, 2.0));
    color += vec3<f32>(inner * light * 0.055);
    let time = plico.resolution_time_scale.z * plico.values[0].w;
    let specular = exp(-pow((uv.x * 0.7 + uv.y - 0.28 - sin(time * 0.55) * 0.12) * 7.0, 2.0));
    color += vec3<f32>(specular * 0.018);
    let grain = fract(sin(dot(floor(uv * resolution), vec2<f32>(12.9898, 78.233))) * 43758.5453) - 0.5;
    color += vec3<f32>(grain / 900.0);
    return mix(glass_backdrop(uv), vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0), coverage);
}
