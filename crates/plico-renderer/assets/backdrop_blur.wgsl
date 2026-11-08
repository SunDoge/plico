// Plico progressive backdrop blur. Inspired by MyGO's glass.ScrollEdge/Blur;
// this bounded single-pass kernel is independent of MyGO's blur pyramid.
// values[0]: radius, mode (0=uniform, 1=top, 2=bottom, 3=left, 4=right), tint amount.
// values[1].xyz: straight tint RGB.
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let resolution = max(plico.resolution_time_scale.xy, vec2<f32>(1.0));
    let pixel = vec2<f32>(0.5) / resolution;
    let original = plico_backdrop_at(clamp(uv, pixel, vec2<f32>(1.0) - pixel));
    let mode = plico.values[0].y;
    var fade = 1.0;
    if mode > 0.5 {
        var distance = uv.y;
        if mode > 3.5 {
            distance = 1.0 - uv.x;
        } else if mode > 2.5 {
            distance = uv.x;
        } else if mode > 1.5 {
            distance = 1.0 - uv.y;
        }
        fade = 1.0 - smoothstep(0.0, 1.0, distance);
    }
    let radius = plico.values[0].x * fade;
    var blurred = original;
    if radius > 0.01 {
        let spread = radius * max(plico.resolution_time_scale.w, 0.01) / resolution;
        var sum = vec4<f32>(0.0);
        var weight_sum = 0.0;
        for(var y = -2; y <= 2; y = y + 1) {
            for(var x = -2; x <= 2; x = x + 1) {
                let offset = vec2<f32>(f32(x), f32(y));
                let weight = exp(-dot(offset, offset) * 0.5);
                let sample_uv = clamp(uv + offset * spread, pixel, vec2<f32>(1.0) - pixel);
                sum += plico_backdrop_at(sample_uv) * weight;
                weight_sum += weight;
            }
        }
        blurred = sum / weight_sum;
    }
    let tinted = mix(blurred, vec4<f32>(plico.values[1].xyz, 1.0), plico.values[0].z);
    // Composite in the shader: transparent output would erase the framebuffer
    // in a backdrop replacement pass, rather than revealing the original scene.
    return mix(original, tinted, fade);
}
