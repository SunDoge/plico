//! GPU-only overlay after Bevy's camera submissions and before presentation.
use crate::PlicoOutput;
use bevy::{
    camera::NormalizedRenderTarget,
    prelude::*,
    render::{
        camera::ExtractedCamera,
        renderer::{RenderDevice, RenderQueue},
        view::ViewTarget,
    },
};

#[derive(Resource, Default)]
pub(crate) struct Compositor {
    pipeline: Option<(
        wgpu::TextureFormat,
        wgpu::RenderPipeline,
        wgpu::BindGroupLayout,
    )>,
    source: Option<(wgpu::TextureView, wgpu::BindGroup)>,
}
impl Compositor {
    fn encode(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        destination: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        if self
            .pipeline
            .as_ref()
            .is_none_or(|(old, ..)| *old != format)
        {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Plico Bevy texture layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Plico Bevy premultiplied overlay"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "const TARGET_SRGB: bool = {};\n{}",
                        format.is_srgb(),
                        include_str!("composite.wgsl")
                    )
                    .into(),
                ),
            });
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Plico Bevy overlay"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            self.pipeline = Some((format, pipeline, layout));
            self.source = None;
        }
        if self.source.as_ref().is_none_or(|(old, _)| old != source) {
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Plico Bevy texture"),
                layout: &self.pipeline.as_ref().unwrap().2,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                }],
            });
            self.source = Some((source.clone(), bind));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Plico on Bevy"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline.as_ref().unwrap().1);
        pass.set_bind_group(0, &self.source.as_ref().unwrap().1, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub(crate) fn composite(
    output: Option<Res<PlicoOutput>>,
    views: Query<(&ViewTarget, &ExtractedCamera)>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    mut compositor: ResMut<Compositor>,
) {
    let Some(output) = output.as_ref().and_then(|output| output.0.as_ref()) else {
        return;
    };
    // Several cameras can share one window. Composite once, after all cameras.
    let Some((target, _)) = views.iter().find(|(_, camera)| {
        matches!(camera.target, Some(NormalizedRenderTarget::Window(window)) if window.entity() == output.window)
            && camera.physical_target_size.is_some_and(|size| size.to_array() == output.size)
    }) else { return; };
    let (Some(view), Some(format)) = (target.out_texture(), target.out_texture_view_format())
    else {
        return;
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Plico Bevy composite"),
    });
    compositor.encode(
        device.wgpu_device(),
        &mut encoder,
        &output.view,
        view,
        format,
    );
    queue.submit([encoder.finish()]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico_renderer::{PaintScene, Scene};

    #[test]
    #[ignore = "requires a GPU; validates shared-device rendering and final overlay pixels"]
    fn shared_device_overlay_preserves_game_pixels_and_premultiplied_alpha() {
        pollster::block_on(async {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    ..Default::default()
                })
                .await
                .unwrap();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .unwrap();
            let mut renderer =
                plico_wgpu::Renderer::from_device(adapter, device.clone(), queue.clone())
                    .await
                    .unwrap();
            let mut compositor = Compositor::default();
            // Changing dimensions and format exercises texture replacement and
            // cached bind-group/pipeline invalidation, not just the first frame.
            for (width, format, half) in [
                (16, wgpu::TextureFormat::Rgba8Unorm, 128_u8),
                (32, wgpu::TextureFormat::Rgba8UnormSrgb, 188),
                (16, wgpu::TextureFormat::Rgba8Unorm, 128),
            ] {
                let extent = wgpu::Extent3d {
                    width,
                    height: 4,
                    depth_or_array_layers: 1,
                };
                let texture = |format| {
                    device.create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size: extent,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING
                            | wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::COPY_SRC,
                        view_formats: &[],
                    })
                };
                let ui = texture(wgpu::TextureFormat::Rgba8Unorm);
                let target = texture(format);
                let view = target.create_view(&Default::default());
                let mut scene = Scene::new();
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    peniko::Color::from_rgba8(255, 0, 0, 128),
                    None,
                    &kurbo::Rect::new(0., 0., 4., 4.),
                );
                renderer
                    .render_scene_to_view(
                        &scene,
                        [width, 4],
                        [0.; 4],
                        false,
                        &ui.create_view(&Default::default()),
                        wgpu::TextureFormat::Rgba8Unorm,
                    )
                    .unwrap();
                let mut encoder = device.create_command_encoder(&Default::default());
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.,
                                    g: 0.,
                                    b: 1.,
                                    a: 1.,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                }
                compositor.encode(
                    &device,
                    &mut encoder,
                    &ui.create_view(&Default::default()),
                    &view,
                    format,
                );
                let readback = device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 256 * 4,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: &target,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &readback,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(256),
                            rows_per_image: Some(4),
                        },
                    },
                    extent,
                );
                queue.submit([encoder.finish()]);
                let (tx, rx) = std::sync::mpsc::channel();
                readback
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        tx.send(result).unwrap();
                    });
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                rx.recv().unwrap().unwrap();
                let pixels = readback.slice(..).get_mapped_range().unwrap();
                for row in pixels.as_chunks::<256>().0.iter() {
                    for (x, actual) in row[..width as usize * 4]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .enumerate()
                    {
                        let expected = if x < 4 {
                            [half, 0, half, 255]
                        } else {
                            [0, 0, 255, 255]
                        };
                        for (actual, expected) in actual.iter().zip(expected) {
                            assert!(
                                actual.abs_diff(expected) <= 2,
                                "{format:?} x={x}: {actual} != {expected}"
                            );
                        }
                    }
                }
            }
        });
    }
}
