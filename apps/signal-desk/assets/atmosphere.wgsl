// Rounded-box Gaussian integration follows MyGO's boxShadow in
// internal/gpu/metal/shader.metal at 74c3f26d (MIT).
// Copyright (c) 2026 MyGo contributors. See ../../../crates/plico/third_party/mygo-LICENSE.
const SIGMA: f32 = 54.;
const CORNER: f32 = 130.;

fn normal_cdf(x: f32) -> f32 {
    let a = abs(x);
    let t = 1. / (1. + 0.2316419 * a);
    let tail = 0.39894228 * exp(-0.5 * a * a) * t
        * (0.31938153 + t * (-0.35656378 + t * (1.78147794 + t * (-1.82125598 + t * 1.33027443))));
    return select(tail, 1. - tail, x >= 0.);
}

fn glow(p: vec2f, origin: vec2f, size: vec2f) -> f32 {
    let half_size = size * 0.5;
    let local = abs(p - origin - half_size);
    if any(local >= half_size + 3. * SIGMA) {
        return 0.;
    }
    let lo = max(-3. * SIGMA, local.y - half_size.y);
    let hi = min(3. * SIGMA, local.y + half_size.y);
    let step = (hi - lo) * 0.25;
    var coverage = 0.;
    for(var tap = 0u; tap < 4u; tap++) {
        let y = lo + (f32(tap) + 0.5) * step;
        let corner_y = max(abs(local.y - y) - (half_size.y - CORNER), 0.);
        let half_width = half_size.x - CORNER + sqrt(max(CORNER * CORNER - corner_y * corner_y, 0.));
        let across = normal_cdf((half_width - local.x) / SIGMA) - normal_cdf((-half_width - local.x) / SIGMA);
        let z = y / SIGMA;
        coverage += across * exp(-0.5 * z * z);
    }
    return clamp(coverage * step * 0.39894228 / SIGMA, 0., 1.);
}

fn plico_effect(uv: vec2f) -> vec4f {
    let size = plico.resolution_time_scale.xy / plico.resolution_time_scale.w;
    let p = uv * size;
    let top = vec4f(vec3f(36., 46., 62.) / 255., 1.) * glow(p, vec2f(size.x * 0.37, -155.), vec2f(510., 295.));
    let side = mix(top, vec4f(vec3f(30., 77., 84.) / 255., 1.), glow(p, vec2f(-190., size.y * 0.4), vec2f(400., 380.)));
    return mix(
            side,
            vec4f(vec3f(54., 50., 86.) / 255., 1.),
            glow(p, vec2f(size.x * 0.68, size.y - 205.), vec2f(420., 320.)),
        );
}
