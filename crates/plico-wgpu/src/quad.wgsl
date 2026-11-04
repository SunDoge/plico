struct Frame { size_time_scale: vec4f }
@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var glyph_atlas: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;
@group(2) @binding(0) var clip_mask: texture_2d<f32>;
@group(2) @binding(1) var clip_sampler: sampler;
// PLICO_INTERFACE
// PLICO_MATERIAL
struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) local: vec2f,
    @location(1) @interpolate(flat) size: vec2f,
    @location(2) @interpolate(flat) color: vec4f,
    @location(3) @interpolate(flat) color2: vec4f,
    @location(4) @interpolate(flat) border: vec4f,
    @location(5) @interpolate(flat) config: vec4f,
    @location(6) @interpolate(flat) clip: vec4f,
    @location(7) uv:vec2f,
    @location(8) @interpolate(flat) gradient:vec4f,
    @location(9) @interpolate(flat) clip_params:vec4f,
    @location(10) @interpolate(flat) mask_rect:vec4f,
}
@vertex fn vs_main(@builtin(vertex_index) index: u32,
    @location(0) rect: vec4f, @location(1) color: vec4f,
    @location(2) color2: vec4f, @location(3) border: vec4f,
    @location(4) config: vec4f, @location(5) clip: vec4f, @location(6) uv:vec4f, @location(7) gradient:vec4f, @location(8) clip_params:vec4f, @location(9) mask_rect:vec4f) -> Vertex {
    let corners=array<vec2f,6>(vec2f(0,0),vec2f(1,0),vec2f(0,1),vec2f(0,1),vec2f(1,0),vec2f(1,1));
    // Include the 1px antialiasing fringe, rather than clipping it at the quad.
    let local=corners[index]*(rect.zw+2.)-1.;
    let world=rect.xy+local;
    var out: Vertex;
    out.position=vec4f(world/frame.size_time_scale.xy*vec2f(2,-2)+vec2f(-1,1),0,1);
    out.local=local;out.size=rect.zw;out.color=color;out.color2=color2;
    out.clip_params=clip_params;
    out.mask_rect=mask_rect;
    out.gradient=gradient;
    out.uv=uv.xy+local/rect.zw*uv.zw;
    out.border=border;out.config=config;out.clip=clip;
    return out;
}
fn distance_to_round_rect(p: vec2f,size: vec2f,radius: f32) -> f32 {
    let r=clamp(radius,0.,min(size.x,size.y)*0.5);
    let q=abs(p-size*0.5)-size*0.5+r;
    return length(max(q,vec2f(0)))+min(max(q.x,q.y),0.)-r;
}
fn coverage(p: vec2f,size: vec2f,radius: f32) -> f32 {
    if radius<=0. {
        // Exact pixel-area coverage for an axis-aligned rectangle, including corners.
        let c=clamp(min(size,p+0.5)-max(vec2f(0),p-0.5),vec2f(0),vec2f(1));
        return c.x*c.y;
    }
    return clamp(0.5-distance_to_round_rect(p,size,radius),0.,1.);
}
@fragment fn fs_main(input: Vertex) -> @location(0) vec4f {
    let world=input.position.xy;
    let clip=input.clip;
    // Clip rectangle is a hard scissor equivalent, in physical pixels.
    var clip_alpha=1.;
    if input.clip_params.x>0.5 {
        clip_alpha=coverage(world-clip.xy,clip.zw,0.);
        if clip_alpha<=0. { discard; }
    } else if any(world<clip.xy) || any(world>=clip.xy+clip.zw) { discard; }
    if input.clip_params.z>0. {
        let m=input.mask_rect;
        if any(world<m.xy) || any(world>=m.xy+m.zw) {discard;}
        clip_alpha*=textureSampleLevel(clip_mask,clip_sampler,(world-m.xy)/m.zw,0.).a;
        if clip_alpha<=0. {discard;}
    }
    if input.config.z>=4. {
        let pixel=textureSampleLevel(glyph_atlas,glyph_sampler,input.uv,0.);
        return pixel*input.color.a*coverage(input.local,input.size,input.config.x)*clip_alpha;
    }
    if input.config.z>=2. {
        let glyph=textureSampleLevel(glyph_atlas,glyph_sampler,input.uv,0.);
        if input.config.z>=3. {return glyph*input.color.a*clip_alpha;}
        return vec4f(input.color.rgb*glyph.a*input.color.a,glyph.a*input.color.a)*clip_alpha;
    }
    let outer=coverage(input.local,input.size,input.config.x);
    let width=min(input.config.y,min(input.size.x,input.size.y)*0.5);
    let inner_size=max(input.size-2.*width,vec2f(0));
    var inner=0.;
    if all(inner_size>vec2f(0)) {
        inner=coverage(input.local-vec2f(width),inner_size,max(input.config.x-width,0.));
    }
    let border_coverage=max(outer-inner,0.);
    let uv=input.local/input.size;
    let start=input.gradient.xy*input.size;
    let direction=(input.gradient.zw-input.gradient.xy)*input.size;
    let t=clamp(dot(input.local-start,direction)/max(dot(direction,direction),0.000001),0.,1.);
    var fill=mix(input.color,input.color2,t);
    if input.clip_params.y>0.5 {
        let a=vec4f(input.color.rgb*input.color.a,input.color.a);
        let b=vec4f(input.color2.rgb*input.color2.a,input.color2.a);
        let premultiplied=mix(a,b,t);
        fill=vec4f(premultiplied.rgb/max(premultiplied.a,0.000001),premultiplied.a);
    }
    if input.config.z>0.5 {
        fill=plico_material(Material(uv,input.size,input.color,input.color2,frame.size_time_scale.z,input.config.w));
    }
    let fill_coverage=select(outer,inner,width>0.);
    let alpha=fill.a*fill_coverage;
    let border_alpha=input.border.a*border_coverage;
    return vec4f(fill.rgb*alpha+input.border.rgb*border_alpha,alpha+border_alpha)*clip_alpha;
}
