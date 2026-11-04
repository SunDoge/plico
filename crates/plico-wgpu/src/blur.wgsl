struct Params { sigma:f32, axis:u32, edge:u32, padding:u32 }
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var destination:texture_storage_2d<rgba8unorm,write>;
@group(0) @binding(2) var<uniform> params:Params;
fn coordinate(x:i32,n:i32)->i32 {
    if params.edge==1u {return clamp(x,0,n-1);}
    if params.edge==2u {return ((x%n)+n)%n;}
    let v=((x%(2*n))+2*n)%(2*n);
    return min(v,2*n-1-v);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3u) {
    let size=vec2i(textureDimensions(source));
    if any(vec2i(id.xy)>=size) {return;}
    let radius=i32(ceil(4.*params.sigma));
    var sum=vec4f(0.);
    var weight=0.;
    for(var i=-radius;i<=radius;i++) {
        let w=exp(-0.5*f32(i*i)/(params.sigma*params.sigma));
        var p=vec2i(id.xy)+select(vec2i(0,i),vec2i(i,0),params.axis==0u);
        weight+=w;
        if params.edge==0u && (any(p<vec2i(0)) || any(p>=size)) {continue;}
        if params.edge!=0u {p=vec2i(coordinate(p.x,size.x),coordinate(p.y,size.y));}
        sum+=textureLoad(source,p,0)*w;
    }
    textureStore(destination,vec2i(id.xy),sum/weight);
}
