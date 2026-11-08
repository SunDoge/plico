// Native-clock shimmer, shared with the Rust counterpart of the managed kit.
fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    let base = plico.values[0].xyz;
    let phase = uv.x - fract(plico.resolution_time_scale.z * 0.45) * 2.0 + 0.5;
    let shine = exp(-phase * phase * 18.0) * 0.09 * plico.values[0].w;
    return vec4<f32>(base + vec3<f32>(shine), 1.0);
}
