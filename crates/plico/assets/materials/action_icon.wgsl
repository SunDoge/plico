// Action glyphs adapted from glass-action-button.tsx (MIT), in its 24-unit view box.
fn segment(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    let v = b - a;
    return length(p - a - v * clamp(dot(p - a, v) / max(dot(v, v), 0.00001), 0., 1.));
}
fn plico_effect(uv: vec2f) -> vec4f {
    let p = uv * 24.;
    let state = plico.values[0].x;
    let phase = plico.values[0].y;
    var distance = 0.;
    if state < 0.5 {
        distance = min(
                segment(p, vec2f(4., 12.), vec2f(18., 12.)),
                min(segment(p, vec2f(11., 5.), vec2f(18., 12.)), segment(p, vec2f(18., 12.), vec2f(11., 19.))),
            ) - 1.;
    } else if state < 1.5 {
        let v = p - vec2f(12.);
        let angle = fract(atan2(v.y, v.x) / 6.2831853 - phase + 2.);
        let end = 0.806;
        let a = phase * 6.2831853;
        let b = (phase + end) * 6.2831853;
        if angle <= end {
            distance = abs(length(v) - 9.);
        } else {
            distance = min(length(v - 9. * vec2f(cos(a), sin(a))), length(v - 9. * vec2f(cos(b), sin(b))));
        }
        distance -= 1.25;
    } else {
        if phase <= 0. {
            return vec4f(0.);
        }
        let box = state > 2.5;
        let a = select(vec2f(4.5, 12.75), vec2f(0.23, 0.52) * 24., box);
        let b = select(vec2f(10.5, 18.75), vec2f(0.42, 0.7) * 24., box);
        let c = select(vec2f(19.5, 5.25), vec2f(0.79, 0.3) * 24., box);
        let l = length(b - a);
        let m = length(c - b);
        let progress = (l + m) * phase;
        distance = segment(p, a, mix(a, b, min(progress / l, 1.)));
        if progress > l {
            distance = min(distance, segment(p, b, mix(b, c, (progress - l) / m)));
        }
        distance -= select(1.25, 1.56, box);
    }
    let aa = max(fwidth(distance) * 0.5, 0.01);
    let alpha = 1. - smoothstep(-aa, aa, distance);
    return vec4f(vec3f(plico.values[0].zw, plico.values[1].x) * alpha, alpha);
}
