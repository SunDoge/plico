use crate::mesh::{Mesh, PlaneVertex, SolidVertex};
use base64::Engine as _;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    view_projection: [f32; 16],
    eye: [f32; 4],
    light: [f32; 4],
    sphere: [f32; 4],
}

pub(crate) struct SceneRenderer {
    uniforms: wgpu::Buffer,
    bind_groups: [wgpu::BindGroup; 2],
    pool_pipeline: wgpu::RenderPipeline,
    sphere_pipeline: wgpu::RenderPipeline,
    water_above_pipeline: wgpu::RenderPipeline,
    water_below_pipeline: wgpu::RenderPipeline,
    pool: Mesh,
    sphere: Mesh,
    water: Mesh,
    depth: wgpu::Texture,
    depth_size: [u32; 2],
    _tiles: wgpu::Texture,
    _sky: wgpu::Texture,
}

#[derive(Clone, Copy)]
pub(crate) struct SceneState {
    pub angle_x: f32,
    pub angle_y: f32,
    pub sphere: [f32; 4],
    pub light: [f32; 3],
}

pub(crate) struct SceneFrame<'a> {
    pub output: &'a wgpu::TextureView,
    pub water_index: usize,
    pub size: [u32; 2],
    pub state: SceneState,
}

pub(crate) fn camera_ray(
    angle_x: f32,
    angle_y: f32,
    size: [f32; 2],
    point: [f64; 2],
) -> Option<(Vec3, Vec3)> {
    if size[0] <= 0.0 || size[1] <= 0.0 {
        return None;
    }
    let (view, projection, eye) = camera(angle_x, angle_y, size[0] / size[1]);
    let inverse = (projection * view).inverse();
    let ndc = Vec3::new(
        point[0] as f32 / size[0] * 2.0 - 1.0,
        1.0 - point[1] as f32 / size[1] * 2.0,
        0.0,
    );
    let near = inverse.project_point3(ndc);
    let far = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
    Some((eye, (far - near).normalize()))
}

fn camera(angle_x: f32, angle_y: f32, aspect: f32) -> (Mat4, Mat4, Vec3) {
    let view = Mat4::from_translation(Vec3::new(0.0, 0.0, -4.0))
        * Mat4::from_rotation_x((-angle_x).to_radians())
        * Mat4::from_rotation_y((-angle_y).to_radians())
        * Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0));
    let projection = Mat4::perspective_rh(45_f32.to_radians(), aspect, 0.01, 100.0);
    let eye = view.inverse().transform_point3(Vec3::ZERO);
    (view, projection, eye)
}

impl SceneRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        water_views: &[wgpu::TextureView; 2],
        caustic_view: &wgpu::TextureView,
    ) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("scene.wgsl"));
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Plico water scene bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Plico water scene pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Plico water scene uniforms"),
            contents: bytemuck::bytes_of(&SceneUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Plico water linear repeat sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let tiles = create_texture_2d(
            device,
            queue,
            "Plico water original tile texture",
            include_str!("../assets/tiles.jpg.b64"),
        );
        let tile_view = tiles.create_view(&Default::default());
        let sky = create_sky_texture(device, queue);
        let sky_view = sky.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let bind_groups = std::array::from_fn(|index| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico water scene bind group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&water_views[index]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(caustic_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&tile_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&sky_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            })
        });
        let depth_stencil = || wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let pipeline = |label: &'static str,
                        vertex_entry: &'static str,
                        fragment_entry: &'static str,
                        vertex_layout: wgpu::VertexBufferLayout<'static>,
                        cull_mode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vertex_entry),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout)],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fragment_entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil()),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let pool_pipeline = pipeline(
            "Plico water pool",
            "pool_vertex",
            "pool_fragment",
            SolidVertex::layout(),
            Some(wgpu::Face::Back),
        );
        let sphere_pipeline = pipeline(
            "Plico water sphere",
            "sphere_vertex",
            "sphere_fragment",
            SolidVertex::layout(),
            None,
        );
        let water_above_pipeline = pipeline(
            "Plico water surface above",
            "water_vertex",
            "water_above",
            PlaneVertex::layout(),
            Some(wgpu::Face::Front),
        );
        let water_below_pipeline = pipeline(
            "Plico water surface below",
            "water_vertex",
            "water_below",
            PlaneVertex::layout(),
            Some(wgpu::Face::Back),
        );
        let depth_size = [1, 1];
        Self {
            uniforms,
            bind_groups,
            pool_pipeline,
            sphere_pipeline,
            water_above_pipeline,
            water_below_pipeline,
            pool: Mesh::pool(device),
            sphere: Mesh::sphere(device, 10),
            water: Mesh::water(device, 200),
            depth: create_depth(device, depth_size),
            depth_size,
            _tiles: tiles,
            _sky: sky,
        }
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        frame: SceneFrame<'_>,
    ) {
        let SceneFrame {
            output,
            water_index,
            size,
            state,
        } = frame;
        if size != self.depth_size {
            self.depth = create_depth(device, size);
            self.depth_size = size;
        }
        let (model_view, projection, eye) = camera(
            state.angle_x,
            state.angle_y,
            size[0] as f32 / size[1].max(1) as f32,
        );
        let light = Vec3::from_array(state.light).normalize();
        let uniforms = SceneUniforms {
            view_projection: (projection * model_view).to_cols_array(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            light: [light.x, light.y, light.z, 0.0],
            sphere: state.sphere,
        };
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&uniforms));
        let depth_view = self.depth.create_view(&Default::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Plico water exact scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &self.bind_groups[water_index], &[]);
        draw_mesh(&mut pass, &self.pool_pipeline, &self.pool);
        draw_mesh(&mut pass, &self.water_above_pipeline, &self.water);
        draw_mesh(&mut pass, &self.water_below_pipeline, &self.water);
        draw_mesh(&mut pass, &self.sphere_pipeline, &self.sphere);
    }
}

fn draw_mesh<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a wgpu::RenderPipeline,
    mesh: &'a Mesh,
) {
    pass.set_pipeline(pipeline);
    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
}

fn create_depth(device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Plico water scene depth"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn decode_jpeg(source: &str) -> image::RgbaImage {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(source.lines().collect::<String>())
        .expect("embedded WebGL Water asset must be valid base64");
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
        .expect("embedded WebGL Water asset must be a valid JPEG")
        .to_rgba8();
    image::imageops::flip_vertical(&image)
}

fn create_texture_2d(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &'static str,
    source: &str,
) -> wgpu::Texture {
    let image = decode_jpeg(source);
    let size = wgpu::Extent3d {
        width: image.width(),
        height: image.height(),
        depth_or_array_layers: 1,
    };
    let mip_level_count = image.width().max(image.height()).ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut level_image = image;
    for level in 0..mip_level_count {
        let width = level_image.width();
        let height = level_image.height();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            level_image.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        if level + 1 < mip_level_count {
            level_image = image::imageops::resize(
                &level_image,
                (width / 2).max(1),
                (height / 2).max(1),
                image::imageops::FilterType::Triangle,
            );
        }
    }
    texture
}

fn create_sky_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let faces = [
        include_str!("../assets/xpos.jpg.b64"),
        include_str!("../assets/xneg.jpg.b64"),
        include_str!("../assets/ypos.jpg.b64"),
        include_str!("../assets/ypos.jpg.b64"),
        include_str!("../assets/zpos.jpg.b64"),
        include_str!("../assets/zneg.jpg.b64"),
    ]
    .map(decode_jpeg);
    let width = faces[0].width();
    let height = faces[0].height();
    assert!(
        faces
            .iter()
            .all(|face| face.dimensions() == (width, height)),
        "embedded cubemap faces must have equal dimensions"
    );
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Plico water original sky cubemap"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, face) in faces.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            face.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    texture
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_original_assets_decode_at_their_source_dimensions() {
        let tiles = decode_jpeg(include_str!("../assets/tiles.jpg.b64"));
        let sky = decode_jpeg(include_str!("../assets/xpos.jpg.b64"));
        assert_eq!(tiles.dimensions(), (512, 512));
        assert_eq!(sky.dimensions(), (256, 256));
    }
}
