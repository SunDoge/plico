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
// One line tile plus a four-sigma halo: 544 RGBA values = 8704 bytes.
var<workgroup> samples:array<vec4f,544>;
@compute @workgroup_size(32)
fn main(@builtin(workgroup_id) group:vec3u,@builtin(local_invocation_index) lane:u32) {
    let size=vec2i(textureDimensions(source));
    let radius=i32(ceil(4.*params.sigma));
    let origin=select(vec2i(i32(group.x),i32(group.y)*32),vec2i(i32(group.x)*32,i32(group.y)),params.axis==0u);
    for(var k=lane;k<u32(32+2*radius);k+=32u) {
        let delta=i32(k)-radius;
        var p=origin+select(vec2i(0,delta),vec2i(delta,0),params.axis==0u);
        var pixel=vec4f(0.);
        if params.edge!=0u {p=vec2i(coordinate(p.x,size.x),coordinate(p.y,size.y));}
        if all(p>=vec2i(0)) && all(p<size) {pixel=textureLoad(source,p,0);}
        samples[k]=pixel;
    }
    workgroupBarrier();
    let p=origin+select(vec2i(0,i32(lane)),vec2i(i32(lane),0),params.axis==0u);
    if any(p>=size) {return;}
    var sum=vec4f(0.);
    var weight=0.;
    for(var i=-radius;i<=radius;i++) {
        let w=exp(-0.5*f32(i*i)/(params.sigma*params.sigma));
        weight+=w;
        sum+=samples[u32(i32(lane)+i+radius)]*w;
    }
    textureStore(destination,p,sum/weight);
}
