fn plico_effect(uv: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(plico_voice_color(uv, max(plico.resolution_time_scale.xy, vec2<f32>(1.0)),
        plico.resolution_time_scale.z, clamp(plico.values[0].x,0.0,1.0)), 1.0);
}
