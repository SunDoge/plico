// A gravitational lens over whatever the page paints behind it: the
// event-horizon silhouette, a photon ring and the Einstein-arc stretching of
// the surrounding content. The effect reads its backdrop through
// plico.backdrop_at and follows the pointer via plico.values.
// values[0..1] = pointer (x, y) in surface-local uv-free fractions; the
// surface is the whole window when used as an overlay.
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = plico.resolution_time_scale.xy;
    let pointer = vec2<f32>(plico.values[0].x, plico.values[0].y);
    // Pointer in surface pixels; default center before the first event.
    let center = select(resolution * 0.5, pointer, all(pointer >= vec2<f32>(0.0)));
    let p = uv * resolution - center;
    let r = length(p);
    // Horizon scales with the window.
    let horizon = min(resolution.x, resolution.y) * 0.06;
    let photon = horizon * 1.5;

    // First-order lens deflection: sample the backdrop pulled toward the
    // mass. Sources at radius r appear displaced outward.
    let deflection = horizon * horizon / max(r, horizon * 0.25);
    let sample_p = p + normalize(select(vec2<f32>(1.0, 0.0), p, r > 0.5)) * deflection;
    let sample_uv = (sample_p + center) / resolution;

    let backdrop = plico_backdrop_at(sample_uv);

    // Photon ring: light orbiting the hole glows amber-white.
    let ring = exp(-pow((r - photon) / (horizon * 0.06), 2.0));
    // The hole itself: nothing escapes inside the horizon.
    let inside = smoothstep(horizon * 0.98, horizon * 1.02, r);
    var color = backdrop.rgb * inside + vec3<f32>(1.0, 0.85, 0.6) * ring * 1.6;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), inside);
}
