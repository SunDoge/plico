// The two colour rows from the Liquid Glass Card example (MIT).
fn ribbon_color(index: u32) -> vec3f {
    let colors = array<vec3f, 6>(
            vec3f(10., 132., 255.),
            vec3f(255., 107., 61.),
            vec3f(48., 209., 88.),
            vec3f(255., 55., 95.),
            vec3f(255., 214., 10.),
            vec3f(191., 90., 242.),
        );
    return colors[index % 6u] / 255.;
}

fn plico_effect(uv: vec2f) -> vec4f {
    let scale = max(plico.resolution_time_scale.w, 0.01);
    let size = plico.resolution_time_scale.xy / scale;
    let at = uv * size;
    let lower = uv.y > 0.5;
    let seconds = plico.resolution_time_scale.z;
    let shift = 1296. * select(fract(seconds / 40.), -fract(seconds / 48.), lower);
    let x = at.x + shift + 1296.;
    let index = u32(floor(x / 108.)) + select(0u, 3u, lower);
    let y = at.y - size.y * 0.5 + select(30., -30., lower);
    let local = vec2f(x % 108. - 48., y);
    let distance = length(vec2f(max(abs(local.x) - 24., 0.), local.y)) - 24.;
    let side = min(at.x, size.x - at.x);
    let blur = mix(3.5, 0.7, smoothstep(0., 150., side));
    let fade = smoothstep(0., 102., side);
    let alpha = (1. - smoothstep(-blur, blur, distance)) * fade;
    return vec4f(ribbon_color(index) * alpha, alpha);
}
