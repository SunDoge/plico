// Scene-backed glass. MyGO's quartic lens, rounded-rectangle normal and
// tone curve adapted to WGSL from plugins/glass/glass.glsl (MIT).
// Original Plico scene sampling / coverage retained (Apache-2.0).
// Framework implementation: Apache-2.0; MyGO-derived formulas: MIT.
// See ../../third_party/README.md and ../../third_party/mygo-LICENSE.
fn glass_backdrop(uv: vec2<f32>) -> vec4<f32> {
    let pixel = 0.5 / max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    return plico_backdrop_original_at(clamp(uv, pixel, vec2<f32>(1.0) - pixel));
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
    if smoothing <= 0.0 || radius >= min(size.x, size.y) * 0.5 {
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
    if clarity > 0.5 {
        value = mix(mid, clear, (clarity - 0.5) * 2.0);
    }
    return mix(frost, value, pane);
}
// Smooth union of the retracting capsule and the anchored action droplet.
// Adapted from liquid-glass engine.ts/bodyShape and glass-layer.tsx (MIT).
fn action_contour(p: vec2f, size: vec2f, t: f32) -> f32 {
    let r = size.y * 0.5;
    let scale_k = plico.values[6].y;
    let pull = 60. * t * scale_k;
    let body_size = vec2f(max(size.x - pull, size.y), size.y);
    let body = glass_contour_field(p + vec2f(pull * 0.5, 0.), body_size, r, 0.);
    if t <= 0.001 {
        return body;
    }
    let base_half = plico.resolution_time_scale.x / max(plico.resolution_time_scale.w, 0.01) * 0.5 - 44.;
    let grow_x = size.x * 0.5 - base_half * scale_k;
    let grow_y = r - 24. * scale_k;
    let center = (base_half - 24.) * scale_k + mix(grow_x - grow_y, grow_x * 0.5, smoothstep(0., 0.15, t));
    let drop = length(p - vec2f(center, 0.)) - r;
    let bridge = max(0.001, 48. * 0.34 * smoothstep(0., 0.15, t) * (1. - smoothstep(0.8, 1., t)));
    let dip = length(vec2f(r, pull * 0.5)) - r;
    let k = max(0.001, mix(bridge, min(bridge, 4. * dip), plico.values[6].z));
    let h = clamp(0.5 + 0.5 * (drop - body) / k, 0., 1.);
    return mix(drop, body, h) - k * h * (1. - h);
}
// Button and panel are the same contour, painted in complementary slices.
fn select_field(p: vec2f) -> vec2f {
    let anchor = plico.values[7];
    let panel = plico.values[8];
    let motion = plico.values[9];
    let body = glass_contour_field(p - anchor.xy, anchor.zw, anchor.w * 0.5, 0.);
    let blob = glass_contour_field(p - panel.xy, panel.zw, motion.x, 0.);
    let n = (p.x - anchor.x) / max(motion.z, 1.);
    let k = max(motion.y * exp(0.2 - sqrt(n * n + 0.04)), 0.001);
    let h = clamp(0.5 + 0.5 * (body - blob) / k, 0., 1.);
    return vec2f(mix(body, blob, h) - k * h * (1. - h), h);
}

fn select_glass(p: vec2f, uv: vec2f, resolution: vec2f, scale: f32) -> vec4f {
    let anchor = plico.values[7];
    let panel = plico.values[8];
    let motion = plico.values[9];
    let original = glass_backdrop(uv);
    let direction = sign(panel.y - anchor.y);
    let edge = anchor.y + direction * anchor.w * 0.5;
    if (p.y - edge) * motion.w < 0. {
        return original;
    }
    let field = select_field(p);
    let distance = field.x;
    let e = 0.25 / scale;
    let gradient = vec2f(
            select_field(p + vec2f(e, 0.)).x - select_field(p - vec2f(e, 0.)).x,
            select_field(p + vec2f(0., e)).x - select_field(p - vec2f(0., e)).x,
        );
    let normal = gradient / max(length(gradient), 0.00001);
    let depth = max(-distance, 0.);
    let own = smoothstep(0.12, 0.92, field.y) * smoothstep(-6., 14., (p.y - edge) * direction);
    let bend_edge = 1. - smoothstep(0., mix(8.5, 24., own), depth);
    let bend = pow(bend_edge, mix(1.6, 9.75, own)) * mix(8., 15., own);
    let at = uv - normal * bend * scale / resolution;
    let sampled = mix(glass_sample(at), plico_backdrop_at(at).rgb, own);
    let dark = plico.values[3].w > 0.5;
    let tint = vec3f(select(1., 0., dark));
    let cover = mix(0.6 * (1. - bend_edge * 0.72), 0.7, own);
    let line = 1. - smoothstep(1.3 - 0.5 / scale, 1.3 + 0.5 / scale, depth);
    let light = pow(max(dot(normal, normalize(vec2f(0.55, -1.))), 0.), 1.6);
    let rim = mix(0.25 + 0.45 * light, select(0.6, 0.25, dark), own) * line;
    let color = mix(mix(sampled, tint, cover), vec3f(1.), rim);
    let coverage = 1. - smoothstep(-0.5 / scale, 0.5 / scale, distance);
    // Cast the soft contact shadow from the joined silhouette so it stays
    // attached to the glass while the list is still stretching.
    let near = 1. - smoothstep(-3., 9., select_field(p - vec2f(0., 5.)).x);
    let far = 1. - smoothstep(-4., 18., select_field(p - vec2f(0., 10.)).x);
    let outside = smoothstep(-1.5, 6., distance);
    let fade = smoothstep(0., 4., min(uv.x, 1. - uv.x) * resolution.x / scale)
        * smoothstep(0., 4., min(uv.y, 1. - uv.y) * resolution.y / scale);
    let shadow = (near * 0.035 + far * 0.06) * outside * fade;
    let halo = (1. - smoothstep(0., 11., max(distance, 0.))) * smoothstep(0., 3., distance) * fade
        * select(0.07, 0., dark);
    let behind = vec4f(mix(original.rgb * (1. - shadow), vec3f(1.), halo), original.a);
    return mix(behind, vec4f(color, 1.), coverage);
}

fn card_bend(p: vec2f, distance: f32, half_short: f32) -> vec2f {
    let rim = pow(clamp(1. - abs(distance) / (1.35 * half_short), 0., 1.), 9.75);
    let mask = 1. - smoothstep(-0.5, 1.5, distance);
    return -p / max(length(p), 0.0001) * plico.values[6].yz * (rim * mask);
}

// At the rim, frost the refracted image and reject taps outside the card.
fn card_frost(p: vec2f, uv: vec2f, dip_to_uv: vec2f, size: vec2f, radius: f32, distance: f32) -> vec3f {
    let half_short = min(size.x, size.y) * 0.5;
    if -distance > half_short * 0.7 {
        return plico_backdrop_at(uv + card_bend(p, distance, half_short) * dip_to_uv).rgb;
    }
    var total = 0.;
    var color = vec3f(0.);
    for(var i = 0u; i < 32u; i++) {
        let f = f32(i) + 0.5;
        let r = sqrt(f / 32.) * 12.5;
        let a = f * 2.39996323;
        let tap = p + r * vec2f(cos(a), sin(a));
        let d = glass_contour_field(tap, size, radius, 0.);
        if d <= 0. {
            let weight = exp(-0.02 * r * r);
            let at = uv + (tap - p + card_bend(tap, d, half_short)) * dip_to_uv;
            color += glass_sample(at) * weight;
            total += weight;
        }
    }
    if total > 0.0001 {
        return color / total;
    }
    return glass_sample(uv + card_bend(p, distance, half_short) * dip_to_uv);
}

fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let size = resolution / scale;
    let p = (uv - vec2<f32>(0.5)) * size;
    if abs(plico.values[9].w) > 0.5 {
        return select_glass(p, uv, resolution, scale);
    }
    let hover = clamp(plico.values[4].x, 0.0, 1.0);
    let pressed = clamp(plico.values[4].y, 0.0, 1.0);
    let clear_button = plico.values[3].y > 5.5 && plico.values[3].y < 6.5;
    let control_lens = plico.values[3].y > 6.5;
    let group = clear_button && plico.values[5].w < -0.5;
    let grow = select(
            vec2f(1.1, 0.45) * pressed,
            vec2f(3.9 * hover + 4.5 * pressed, 4.4 * hover + 2.56 * pressed),
            clear_button,
        );
    let inset = plico.values[4].zw - grow;
    let pane_size = size - 2.0 * inset;
    let morph = plico.values[5].z > 1.5;
    if morph && plico.values[5].w < 0.5 {
        return glass_backdrop(uv);
    }
    let face_radius = select(plico.values[0].x, plico.values[9].x, morph && plico.values[9].x > 0.);
    let radius = min(face_radius + grow.y, min(pane_size.x, pane_size.y) * 0.5);
    let smoothing = clamp(plico.values[3].x, 0.0, 1.0);
    var distance = glass_contour_field(p, pane_size, radius, smoothing);
    var normal = glass_normal(p, pane_size * 0.5, radius);
    // Only curved continuous corners need the field gradient. Flat edges retain
    // the inexpensive analytic path and the lens never affects the center.
    let nearest = pane_size * 0.5 - abs(p);
    if smoothing > 0.0 && radius < min(pane_size.x, pane_size.y) * 0.5 && nearest.x < radius * 1.528665
        && nearest.y < radius * 1.528665 {
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
    let separation = plico.values[2].w;
    if plico.values[6].x > 0.5 {
        distance = action_contour(p, pane_size, separation);
        let e = 0.25 / scale;
        let gradient = vec2f(
                action_contour(p + vec2f(e, 0.), pane_size, separation)
                - action_contour(p - vec2f(e, 0.), pane_size, separation),
                action_contour(p + vec2f(0., e), pane_size, separation)
                - action_contour(p - vec2f(0., e), pane_size, separation),
            );
        normal = gradient / max(length(gradient), 0.00001);
    }
    let coverage = 1.0 - smoothstep(-0.5 / scale, 0.5 / scale, distance);
    let depth = max(-distance, 0.0);
    let material = plico.values[3].y;
    let clarity = clamp(plico.values[3].z, 0.0, 1.0);
    let dark = plico.values[3].w > 0.5;
    let short_side = min(pane_size.x, pane_size.y);
    let pane = smoothstep(100.0, 200.0, short_side);
    // Optical lens adapted from bilalgursen/liquid-glass, engine.ts (MIT).
    // Keep the scene in logical coordinates until the final backdrop lookup.
    // The popup uses the upstream Card face and a separate clear trigger.
    if material > 3.5 {
        let panel = material > 4.5 && !clear_button && !control_lens;
        let panel_mix = select(select(0., 1., panel), clarity, morph);
        let frost = clamp(plico.values[0].z, 0., 1.);
        let clear = 1. - frost;
        let face_bevel = select(mix(8.5, 24., panel_mix), mix(9.41, 11.57, pressed), clear_button);
        let bevel = select(face_bevel, plico.values[1].w * 0.35, control_lens);
        let edge = 1. - smoothstep(0., bevel, depth);
        let refraction = select(mix(8., 15., panel_mix), mix(23.47, 19.73, pressed), clear_button);
        let control_bend = plico.values[5].w;
        let bend = pow(edge, mix(1.6, 9.75, panel_mix)) * select(refraction, control_bend, control_lens);
        let card = plico.values[5].z > 0.5 && !morph;
        let at = uv - normal * bend * scale / resolution;
        let original = glass_backdrop(uv);
        var sampled: vec3f;
        if card && coverage > 0. {
            sampled = card_frost(p, uv, scale / resolution, pane_size, radius, distance);
        } else if morph {
            sampled = mix(glass_sample(at), plico_backdrop_at(at).rgb, panel_mix);
        } else if panel {
            sampled = plico_backdrop_at(at).rgb;
        } else {
            let split = select(0.042, 0.042 * clear, control_lens);
            let dispersion = normal * select(0.6 * edge, split * bend, clear_button || control_lens) * scale
                / resolution;
            sampled = vec3f(glass_sample(at - dispersion).r, glass_sample(at).g, glass_sample(at + dispersion).b);
        }
        let tint = mix(vec3f(select(1., 0., dark)), plico.values[2].xyz, select(0., frost * (1. - panel_mix), morph));
        var color = mix(
                sampled,
                tint,
                select(mix(0.6 * (1. - edge * 0.72), 0.7, panel_mix), 0., clear_button || control_lens),
            );
        let line = 1. - smoothstep(1.3 - 0.5 / scale, 1.3 + 0.5 / scale, depth);
        let highlight = pow(max(dot(normal, normalize(vec2f(0.55, -1.))), 0.), 1.6);
        let face_opening = plico.values[3].z;
        let rim = mix(
                (0.25 + 0.45 * highlight) * line + 0.09 * highlight * edge,
                select(0.6, 0.25, dark) * line * mix(0.35, 1., smoothstep(0.45, 0.95, face_opening)),
                panel_mix,
            );
        if group {
            color = mix(color, plico.values[2].xyz, frost * (1. - 0.2 * edge));
        }
        if clear_button || control_lens {
            // The clear face still absorbs light on the inner bevel, even with zero tint.
            let up = max(-normal.y, 0.);
            let down = max(normal.y, 0.);
            let left = max(-normal.x, 0.);
            let right = max(normal.x, 0.);
            let ny = p.y / (pane_size.y * 0.5);
            let nx = p.x / (pane_size.x * 0.5);
            let lighting = select(1., clear, control_lens);
            let top = (0.18 * (1. - smoothstep(-0.05, 0.75, ny)) * (1. - nx * 0.12) + 0.02) * lighting;
            let physical_bevel = bevel / 0.35;
            let lip = smoothstep(-physical_bevel * 0.19, -physical_bevel * 0.024, distance);
            let deep = smoothstep(-physical_bevel * 0.73, -physical_bevel * 0.13, distance);
            let ring_at = (distance + physical_bevel * 0.45) / (physical_bevel * 0.26);
            let inner = 0.22 * lip * (up + left * 0.6 + right * 0.3 + down * 0.15);
            let ring = 0.15 * exp(-ring_at * ring_at) * (right * 0.9 + up * 0.5 + left * 0.4 + down * 0.3);
            color *= pow(
                    (vec3f(1.) - vec3f(1., 1., 0.72) * top) * (1. - inner * lighting)
                    * (1. - 0.2 * left * deep * lighting) * (1. - ring * lighting),
                    vec3f(1. / 2.2),
                );

            let studio = 0.9 * right * right * exp(-pow(p.y / 16., 2.)) + 0.55 * left * left + 0.2 * down * down;
            let rim_width = max(1.1 / scale, 64. / 120.);
            let bright_rim = 1. - smoothstep(0., 1.6 * rim_width, abs(depth - 1.8 * rim_width));
            if control_lens {
                let t = pow(clamp(depth / physical_bevel, 0., 1.), 1. / 1.4);
                let strength = pow(1. - sqrt(max(0., 1. - (1. - t) * (1. - t))), 1.6);
                color = mix(color, mix(vec3f(1.), vec3f(0.914, 0.922, 0.937), strength), frost);
            }
            let rim_strength = select(0.55, plico.values[1].z, control_lens);
            color += vec3f(rim_strength * studio * bright_rim);
        } else {
            color = mix(color, vec3f(1.), rim);
        }
        var outside = original;
        if morph {
            let contact = glass_contour_field(p - vec2f(0., 3.), pane_size, radius, 0.);
            let ambient = glass_contour_field(p - vec2f(0., select(20., 22., dark)), pane_size, radius, 0.);
            let card_shadow = select(0.045, 0., dark) * (1. - smoothstep(-8., 8., contact))
                + select(0.105, 0.34, dark) * (1. - smoothstep(-32., 32., ambient));
            let controlled = glass_contour_field(p - vec2f(0., 4.4), pane_size, radius, 0.);
            let trigger_shadow = 0.1 * (1. - smoothstep(-3., 10., controlled)) * smoothstep(-1.6, 5.6, distance);
            let shadow = mix(trigger_shadow, card_shadow, panel_mix);
            let fade = smoothstep(0., 6., min(size.x * 0.5 - abs(p.x), size.y * 0.5 - abs(p.y)));
            outside = vec4f(original.rgb * (1. - shadow * fade), original.a);
        } else if !panel && !control_lens {
            let shadow_scale = select(1., 0.35, group && plico.values[5].w < -1.5);
            let offset = mix(4.4, 8.8, pressed) * shadow_scale;
            let softness = mix(8.16, 13.6, pressed) * shadow_scale;
            let near = glass_contour_field(p - vec2f(0., offset), pane_size, radius, 0.);
            let far = glass_contour_field(p - vec2f(0., offset * 1.6), pane_size, radius, 0.);
            let shadow = (1. - smoothstep(-2.4, softness, near)) + (1. - smoothstep(-4., softness * 1.5, far));
            let fade = smoothstep(0., 5.6, min(size.x * 0.5 - abs(p.x), size.y * 0.5 - abs(p.y)));
            let side = select(1., 0.08 + 0.92 * smoothstep(-0.1, 0.9, p.y / (pane_size.y * 0.5)), clear_button);
            let shade = shadow * mix(0.045, 0.105, pressed) * fade * side * smoothstep(-1.6, 5.6, distance);
            outside = vec4f(original.rgb * (1. - shade), original.a);
        }
        return mix(outside, vec4f(min(color, vec3f(1.)), 1.), coverage);
    }
    if material > 2.5 {
        let band = max(min(plico.values[1].w, short_side * 0.5), 0.001);
        let edge = 1.0 - smoothstep(0.0, band, depth);
        let strength = plico.values[1].x * (1.0 + 0.16 * pressed);
        let height = sqrt(max(1.0 - edge * edge, 0.0));
        let at = vec2f(0.5) + (uv - vec2f(0.5)) / (1.0 + 0.12 * strength * height);
        let bend = -normal * band * strength * pow(edge, 1.6) * scale / resolution;
        let split = 0.11;
        var color = vec3f(
                plico_backdrop_at(at + bend * (1.0 + split)).r,
                plico_backdrop_at(at + bend).g,
                plico_backdrop_at(at + bend * (1.0 - split)).b,
            );
        let luma = dot(color, vec3f(0.2126, 0.7152, 0.0722));
        color = mix(vec3f(luma), color, plico.values[1].y);
        color = mix(color, plico.values[2].xyz, clamp(plico.values[0].z, 0.0, 1.0));
        let facing = dot(normal, normalize(vec2f(-0.45, -1.0)));
        let rim_width = max(1.1 / scale, 1.0);
        let line = 1.0 - smoothstep(0.0, 1.6 * rim_width, abs(depth - 1.8 * rim_width));
        let sheen = clamp(plico.values[1].z + 0.18 * hover, 0.0, 1.0);
        let facing_light = max(facing, 0.0);
        let rim = line * (0.9 * facing_light * facing_light + 0.45 * max(-facing, 0.0));
        color *= 1.0 - 0.12 * edge * sheen;
        color += vec3f((rim + 0.14 * pow(edge, 6.0) * facing_light) * sheen);
        let mirror = plico.values[5].w;
        if mirror > 0. {
            // Reflect the actual surrounding scene. A clear thumb at the top
            // of the alarm track can pick up the Snooze button above it.
            let look = normal * (8. + 24. * pow(edge, 1.5)) * scale / resolution;
            let seen = glass_sample(uv + look);
            let hi = max(max(seen.r, seen.g), seen.b);
            let lo = min(min(seen.r, seen.g), seen.b);
            let saturation = smoothstep(0.10, 0.45, hi - lo);
            let reflection = mirror * saturation * max(-normal.y, 0.) * (0.60 * line + 0.24 * edge);
            color = mix(color, seen, reflection);
        }
        var backdrop = glass_backdrop(uv);
        if plico.values[6].x > 0.5 {
            let down = clamp(plico.values[6].w, 0., 1.);
            let bevel = mix(20.16, 24.8, down);
            let dir = normal;
            let right = max(dir.x, 0.);
            let left = max(-dir.x, 0.);
            let up = max(-dir.y, 0.);
            let bottom = max(dir.y, 0.);
            let edge_mix = 1. - smoothstep(0., bevel * 0.35, depth);
            color = mix(plico.values[2].xyz, backdrop.rgb, edge_mix * 0.2);
            let top =
                (1. - smoothstep(-0.05, 0.75, p.y / (pane_size.y * 0.5)))
                * (1. - p.x / max(1., pane_size.x * 0.5) * 0.12) * 0.18 + 0.02;
            color *= pow(clamp(vec3f(1.) - vec3f(1., 1., 0.72) * top, vec3f(0.), vec3f(1.)), vec3f(1. / 2.2));
            let inner =
                smoothstep(-bevel * 0.19, -bevel * 0.024, distance) * (up + left * 0.6 + right * 0.3 + bottom * 0.15)
                * 0.22;
            let deep = smoothstep(-bevel * 0.73, -bevel * 0.13, distance) * left * 0.2;
            let hx = (distance + bevel * 0.45) / (bevel * 0.26);
            let ring = exp(-hx * hx) * (right * 0.9 + up * 0.5 + left * 0.4 + bottom * 0.3) * 0.15;
            color *= pow(max(0., (1. - inner) * (1. - deep) * (1. - ring)), 1. / 2.2);
            let studio = right * right * exp(-p.y * p.y / 144.) * 0.9 + bottom * bottom * 0.2 + left * left * 0.55;
            color += vec3f(line * studio * 0.55);
            let t = pow(clamp(depth / bevel, 0., 1.), 1. / 1.4);
            let surface_depth = 1. - sqrt(max(0., 1. - (1. - t) * (1. - t)));
            color += vec3f(1., 0.92, 0.86) * smoothstep(0.72, 0.98, surface_depth) * bottom * 0.08;
            let softness = 13.6 * mix(0.6, 1., down);
            let offset = 8.8 * mix(0.5, 1., down);
            let near = 1. - smoothstep(-2.4, softness, action_contour(p - vec2f(0., offset), pane_size, separation));
            let far = 1.
                - smoothstep(-4., softness * 1.5, action_contour(p - vec2f(0., offset * 1.6), pane_size, separation));
            let shade =
                (near + far) * 0.12 * mix(0.55, 1., down)
                * (smoothstep(-0.1, 0.9, p.y / (pane_size.y * 0.5)) * 0.92 + 0.08) * smoothstep(-1.6, 5.6, distance);
            backdrop = vec4f(backdrop.rgb * pow(1. - shade, 1. / 2.2), backdrop.a);
        }
        return mix(backdrop, vec4f(clamp(color, vec3f(0.), vec3f(1.)), 1.), coverage);
    }
    var bezel_width = plico.values[1].w;
    if material > 0.5 {
        bezel_width *= glass_parameter(36.0, 48.0, 48.0, clarity, pane) / 36.0;
    }
    let bezel = min(bezel_width, short_side * 0.5);
    let refraction = bezel * plico.values[1].x * glass_lens(depth, bezel);
    let lens = uv - normal * refraction * scale / resolution;
    let frost = clamp(plico.values[0].y, 0.0, 1.0);
    // The host resolves MyGO's size-responsive material sigma once per
    // surface. The renderer and shader therefore share one fixed backdrop
    // contract instead of recomputing a dynamic blur at every fragment.
    var sigma = max(plico.values[5].x, 0.0);
    // Raw users of the exported WGSL do not carry the builtin metadata. Keep
    // that compatibility path while framework-authored glass uses the single
    // host-resolved value above.
    if sigma == 0.0 && plico.values[5].y <= 0.5 {
        sigma = 0.6 + frost * 5.0;
        if material > 0.5 {
            sigma = min(max(short_side * 0.035, 1.0), glass_parameter(10.0, 5.0, 2.0, clarity, pane));
            if material > 1.5 {
                sigma = 1.0;
            }
            sigma *= frost / 0.42;
        }
    }
    var blurred = vec3f(0.);
    if plico.values[5].y > 0.5 {
        blurred = plico_backdrop_at(lens).rgb;
    } else {
        blurred = plico_backdrop_blur(lens, sigma * scale).rgb;
    }
    let luminance = dot(blurred, vec3<f32>(0.2126, 0.7152, 0.0722));
    // A clearer MyGO-style light material: the scene remains legible through
    // broad panes, while surface text is painted separately and stays sharp.
    var low = mix(0.125, 0.35, frost);
    var high = 1.0;
    var curve = 1.2;
    var saturation = plico.values[1].y;
    if material > 1.5 {
        low = 0.125;
        high = 1.082;
        curve = 1.0;
    } else if material > 0.5 {
        low = glass_parameter(0.541, 0.35, 0.2, clarity, pane);
        if dark {
            low = glass_parameter(0.15, 0.15, 0.1, clarity, pane);
            high = glass_parameter(0.51, 0.6, 0.85, clarity, pane);
            curve = 2.0;
            saturation *= 2.2;
        }
    }
    low = min(low + 0.02 * hover, 1.0);
    let toned = low + (high - low) * (1.0 - pow(max(1.0 - luminance, 0.0), curve));
    var color = clamp(
            vec3<f32>(toned) + (blurred - vec3<f32>(luminance)) * (high - low) * saturation,
            vec3<f32>(0.0),
            vec3<f32>(1.0),
        );
    let tint_amount = clamp(plico.values[0].z, 0.0, 1.0);
    // Prominent controls need strong color to keep on-accent text readable.
    // Ordinary translucent panes retain the historical 0.88 tint response.
    let tint_alpha = min(tint_amount * (0.88 + 0.08 * smoothstep(0.88, 1.0, tint_amount)) * (1.0 + 0.1 * hover), 1.0);
    color = mix(color, plico.values[2].xyz, tint_alpha);
    // MyGO's one-pixel lit rim, with a wider, softer inner reflection.
    let light = abs(dot(normal, normalize(vec2<f32>(-0.25, -1.0))));
    var rim_strength = plico.values[1].z;
    if material > 0.5 {
        rim_strength *= glass_parameter(0.35, 0.5, 0.5, clarity, pane) / 0.35;
    }
    let rim = min(rim_strength + 0.18 * hover, 1.0) * exp(-depth * depth);
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
