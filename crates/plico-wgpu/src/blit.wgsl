@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
    let vertices=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));
    return vec4f(vertices[index],0,1);
}
@fragment fn fs(@builtin(position) position:vec4f)->@location(0) vec4f {
    let encoded=textureLoad(source,vec2i(position.xy),0);
    if TARGET_SRGB {
        // Preserve encoded-space composition on an sRGB swapchain. Decode the
        // stored premultiplied channels once; attachment encoding reverses it.
        let linear=select(pow((encoded.rgb+0.055)/1.055,vec3f(2.4)),encoded.rgb/12.92,encoded.rgb<=vec3f(0.04045));
        return vec4f(linear,encoded.a);
    }
    return encoded;
}
