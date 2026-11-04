// Area prefilter a single mip step, including odd dimensions. Views expose
// exactly one source/destination level; no frame or application uniforms.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    let output_size=textureDimensions(destination);
    if any(id.xy>=output_size) {return;}
    let input_size=textureDimensions(source);
    let start=id.xy*input_size;
    let end=(id.xy+vec2u(1))*input_size;
    let first=start/output_size;
    let last=(end+output_size-vec2u(1))/output_size;
    var sum=vec4f(0.);
    for(var y=first.y;y<last.y;y++) {
        let wy=min((y+1u)*output_size.y,end.y)-max(y*output_size.y,start.y);
        for(var x=first.x;x<last.x;x++) {
            let wx=min((x+1u)*output_size.x,end.x)-max(x*output_size.x,start.x);
            sum+=textureLoad(source,vec2i(i32(x),i32(y)),0)*f32(wx)*f32(wy);
        }
    }
    let average=sum/(f32(input_size.x)*f32(input_size.y));
    // Match CPU half-up quantization as closely as f32 precision permits.
    textureStore(destination,vec2i(id.xy),floor(clamp(average,vec4f(0.),vec4f(1.))*255.+0.5)/255.);
}
