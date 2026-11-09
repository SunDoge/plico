// Adapted from aaaa-zhen/glsl-playground siri-wave.html (MIT), resolved wave.
fn spectral(s: f32) -> vec3<f32> {
    return clamp(vec3<f32>(abs(s - 3.0) - 1.0, 2.0 - abs(s - 2.0), 2.0 - abs(s - 4.0)), vec3<f32>(0.0), vec3<f32>(1.0));
}
fn plico_voice_color(uv: vec2<f32>, res: vec2<f32>, t: f32, energy: f32) -> vec3<f32> {
    let aspect = res.x / res.y;
    let p = (uv * 2.0 - 1.0) * vec2<f32>(aspect, -1.0) / 0.6;
    let low = 0.45 + 0.45 * sin(t * 0.8) * sin(t * 0.37 + 1.0);
    let mid = 0.40 + 0.40 * sin(t * 1.7 + 2.0) * sin(t * 0.53);
    let high = 0.30 + 0.30 * sin(t * 2.9 + 4.0) * sin(t * 0.71 + 2.0);
    let xn = p.x / max(aspect, 1.0);
    let env = pow(cos(1.5707963 * min(abs(0.9 * xn), 1.0)), 2.0);
    let a = (0.32 + 0.06 * low) * mix(0.2, 1.4, energy);
    let drift = (t % 62.831853) * 2.4;
    let main_y = a * env * sin(p.x * 1.1 + drift);
    let intensity = 0.01 * (2.0 + low * 1.5);
    let soft = 0.01 * (2.5 + mid * 0.4);
    var num = vec3<f32>(0.0);
    var den = vec3<f32>(0.0);
    for(var i = 0; i < 4; i++) {
        let hue = spectral(f32(i));
        let ab = mix(-1.0, 1.0, f32(i) / 3.0) * (2.6 + mid * 0.8 + high * 0.5);
        let y = (a + mid * 0.05 + high * 0.06) * env * sin(p.x + drift + ab);
        let d = abs(p.y - y);
        let line = intensity / (sqrt(d * d + soft * soft) + 0.03);
        let band_d = max(0.0, max(p.y - max(main_y, y), min(main_y, y) - p.y));
        num += hue * (line + 3.0 * intensity / (band_d + 0.08));
        den += hue;
    }
    let dm = abs(p.y - main_y);
    var col = num / max(den, vec3<f32>(0.001)) + vec3<f32>(0.5 * intensity / (sqrt(dm * dm + soft * soft) + 0.03));
    col = pow(max(col, vec3<f32>(0.0)), vec3<f32>(1.5));
    let edge = 1.0 - smoothstep(0.6, 1.0, abs(uv.y * 2.0 - 1.0));
    return min(col * edge * exp(-pow(xn * 1.7, 2.0)), vec3<f32>(1.0));
}
