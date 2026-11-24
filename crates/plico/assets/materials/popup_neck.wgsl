// Concave liquid bridge across the gap between an anchor and its panel.
// The flat ends overlap both surfaces. The waist pinches off as they separate.
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let phase = plico.values[0].x;
    let tension = plico.values[0].y;
    let list = tension >= 0.;
    let separation = select(smoothstep(0.18, 0.82, phase), 1. - tension, list);
    let across = abs(uv.x * 2.0 - 1.0);
    let waist = sin(3.14159 * uv.y);
    let half_width = 1.0 - (0.35 + separation * 0.82) * waist;
    let d = half_width - across;
    let edge = max(fwidth(d), 0.005);
    let alpha = smoothstep(-edge, edge, d) * smoothstep(0.0, 0.12, phase) * (1.0 - smoothstep(0.76, 1.0, phase));
    let normal = vec2<f32>(sign(uv.x - 0.5), 0.3 * cos(3.14159 * uv.y));
    let scene = plico_backdrop_at(clamp(uv - normal * 0.08, vec2<f32>(0.0), vec2<f32>(1.0)));
    let rim = 0.36 * exp(-18.0 * max(d, 0.0));
    let tint = mix(scene.rgb, vec3<f32>(1.0), 0.09 + rim);
    if list {
        return mix(plico_backdrop_original_at(uv), vec4f(tint, 1.), alpha);
    }
    return vec4<f32>(tint * alpha, alpha);
}
