// Area reduction of premultiplied RGBA, including odd and one-pixel axes.
@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.,-1.),vec2f(3.,-1.),vec2f(-1.,3.));
    return vec4f(p[i],0.,1.);
}
@fragment fn fs(@builtin(position) p:vec4f)->@location(0) vec4f {
    let input_size=textureDimensions(source);
    let output_size=max(input_size/2u,vec2u(1u));
    let xy=vec2u(p.xy);
    let start=xy*input_size;
    let end=(xy+1u)*input_size;
    let first=start/output_size;
    let last=(end+output_size-1u)/output_size;
    var sum=vec4f(0.);
    for(var y=first.y; y<last.y; y++) {
        let wy=min((y+1u)*output_size.y,end.y)-max(y*output_size.y,start.y);
        for(var x=first.x; x<last.x; x++) {
            let wx=min((x+1u)*output_size.x,end.x)-max(x*output_size.x,start.x);
            sum+=textureLoad(source,vec2i(i32(x),i32(y)),0)*f32(wx)*f32(wy);
        }
    }
    let average=sum/(f32(input_size.x)*f32(input_size.y));
    return floor(clamp(average,vec4f(0.),vec4f(1.))*255.+0.5)/255.;
}
