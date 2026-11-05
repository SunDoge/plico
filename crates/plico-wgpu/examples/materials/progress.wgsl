// Runtime-loaded WGSL: no Rust build is needed to edit this material.
// Material provides UV, physical size, two straight sRGB colors, injected time,
// and one per-instance parameter. Return straight RGBA, without geometry AA.
fn plico_material(m: Material) -> vec4f {
    let uv = clamp(m.uv, vec2f(0), vec2f(1));
    let background = vec3f(0.90, 0.93, 0.97);
    let gradient = mix(m.color.rgb, m.color2.rgb, uv.y);
    let inside = 1. - smoothstep(m.parameter - 1. / m.size.x, m.parameter + 1. / m.size.x, uv.x);
    let stripes = 0.5 + 0.5 * sin((uv.x * m.size.x + uv.y * m.size.y - m.time * 45.) * 0.15);
    let sheen = 0.12 * pow(1. - uv.y, 3.);
    let foreground = gradient * (0.90 + 0.10 * stripes) + vec3f(sheen);
    return vec4f(mix(background, foreground, inside), 1.);
}
