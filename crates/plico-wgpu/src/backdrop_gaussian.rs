//! MyGO-style fixed backdrop Gaussian: area reduction, then two separable axes.
use crate::blur::{Blur, Workspace};

#[derive(Clone, Copy, PartialEq)]
struct SnapshotKey {
    effect: plico_renderer::ShaderEffectId,
    command: usize,
    region: [u32; 4],
    plan: plico_renderer::BackdropGaussian,
}

struct Slot {
    size: [u32; 2],
    down: u32,
    used: bool,
    snapshot: wgpu::Texture,
    snapshot_view: wgpu::TextureView,
    reduced: Option<wgpu::Texture>,
    down_bind: Option<wgpu::BindGroup>,
    blur: Option<Workspace>,
    snapshot_key: Option<SnapshotKey>,
}

pub(crate) struct GaussianBackdrops {
    reduction_shader: wgpu::ShaderModule,
    downsample: [Option<wgpu::ComputePipeline>; 3],
    blur: Blur,
    slots: Vec<Slot>,
    allocations: u64,
    reuses: u64,
    committed: bool,
}

impl GaussianBackdrops {
    pub fn supported(device: &wgpu::Device) -> bool {
        crate::mipmap::MipGenerator::supported(device)
    }

    pub fn new(device: &wgpu::Device) -> Self {
        let reduction_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Plico fixed backdrop area reduction"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop_gaussian.wgsl").into()),
        });
        Self {
            reduction_shader,
            downsample: [None, None, None],
            blur: Blur::new(device),
            slots: Vec::new(),
            allocations: 0,
            reuses: 0,
            committed: false,
        }
    }

    pub fn begin_frame(&mut self) {
        for slot in &mut self.slots {
            // The prefix comparison is relative to the preceding scene.
            // A skipped reader or an abandoned encoder has no such snapshot.
            if !self.committed || !slot.used {
                slot.snapshot_key = None;
            }
            slot.used = false;
        }
        self.committed = false;
    }

    pub fn commit_frame(&mut self) {
        self.committed = true;
    }

    fn slot(&mut self, device: &wgpu::Device, size: [u32; 2], down: u32, blurred: bool) -> usize {
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| !slot.used && slot.size == size && slot.down == down)
        {
            let slot = &mut self.slots[index];
            slot.used = true;
            let allocated_blur = blurred && slot.blur.is_none();
            if allocated_blur {
                let source = slot.reduced.as_ref().unwrap_or(&slot.snapshot);
                slot.blur = Some(self.blur.workspace(
                    device,
                    source,
                    [size[0].div_ceil(down), size[1].div_ceil(down)],
                ));
                self.allocations += 2;
            }
            self.reuses += 1
                + u64::from(slot.reduced.is_some())
                + if blurred && !allocated_blur { 2 } else { 0 };
            return index;
        }

        let index = self
            .slots
            .iter()
            .position(|slot| !slot.used)
            .unwrap_or(self.slots.len());
        {
            let snapshot = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Plico fixed backdrop snapshot"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let snapshot_view = snapshot.create_view(&Default::default());
            let reduced_size = [size[0].div_ceil(down), size[1].div_ceil(down)];
            let (reduced, down_bind) = if down > 1 {
                let reduced = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Plico fixed backdrop reduction"),
                    size: wgpu::Extent3d {
                        width: reduced_size[0],
                        height: reduced_size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::STORAGE_BINDING,
                    view_formats: &[],
                });
                let pipeline =
                    self.downsample[down.ilog2() as usize - 1].get_or_insert_with(|| {
                        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                            label: Some("Plico fixed backdrop area reduction"),
                            layout: None,
                            module: &self.reduction_shader,
                            entry_point: Some("main"),
                            compilation_options: wgpu::PipelineCompilationOptions {
                                constants: &[("DOWN", f64::from(down))],
                                ..Default::default()
                            },
                            cache: None,
                        })
                    });
                let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Plico fixed backdrop reduction"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&snapshot_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &reduced.create_view(&Default::default()),
                            ),
                        },
                    ],
                });
                (Some(reduced), Some(bind))
            } else {
                (None, None)
            };
            let source = reduced.as_ref().unwrap_or(&snapshot);
            let blur = blurred.then(|| self.blur.workspace(device, source, reduced_size));
            let slot = Slot {
                size,
                down,
                used: true,
                snapshot,
                snapshot_view,
                reduced,
                down_bind,
                blur,
                snapshot_key: None,
            };
            self.allocations += 1 + u64::from(down > 1) + if blurred { 2 } else { 0 };
            if index == self.slots.len() {
                self.slots.push(slot);
            } else {
                self.slots[index] = slot;
            }
        }
        index
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        region: [u32; 4],
        plan: plico_renderer::BackdropGaussian,
        key: Option<(plico_renderer::ShaderEffectId, usize, bool)>,
    ) -> (wgpu::TextureView, wgpu::TextureView, u64, u64, bool) {
        let [left, top, width, height] = region;
        debug_assert!(matches!(plan.down, 1 | 2 | 4 | 8));
        debug_assert!(width.is_multiple_of(plan.down) && height.is_multiple_of(plan.down));
        let snapshot_key = key.map(|(effect, command, _)| SnapshotKey {
            effect,
            command,
            region,
            plan,
        });
        let matching = snapshot_key.and_then(|key| {
            self.slots
                .iter()
                .position(|slot| !slot.used && slot.snapshot_key == Some(key))
        });
        let index = matching.unwrap_or_else(|| {
            self.slot(device, [width, height], plan.down, plan.reduced_sigma > 0.)
        });
        if matching.is_some() {
            self.reuses += 1 + u64::from(plan.down > 1) + 2 * u64::from(plan.reduced_sigma > 0.);
        }
        let slot = &mut self.slots[index];
        slot.used = true;
        slot.snapshot_key = snapshot_key;
        let copied = matching.is_none() || !key.is_some_and(|(_, _, clean)| clean);
        let reduced_size = [width.div_ceil(plan.down), height.div_ceil(plan.down)];
        let base = u64::from(width) * u64::from(height) * 4;
        let reduced = u64::from(reduced_size[0]) * u64::from(reduced_size[1]) * 4;
        let bytes = base
            + u64::from(plan.down > 1) * reduced
            + u64::from(plan.reduced_sigma > 0.) * reduced * 2;
        let original = slot.snapshot_view.clone();
        let workspace = slot.blur.as_mut().filter(|_| plan.reduced_sigma > 0.);
        let view = workspace
            .as_ref()
            .map_or_else(|| original.clone(), |work| work.output_view().clone());
        if !copied {
            return (view, original, 0, bytes, false);
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: left,
                    y: top,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &slot.snapshot,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let passes = u64::from(plan.down > 1) + 2 * u64::from(workspace.is_some());
        if passes != 0 {
            // Writes to the reduction and horizontal textures become visible to
            // the next dispatch through wgpu's per-dispatch usage tracking.
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Plico fixed backdrop Gaussian"),
                ..Default::default()
            });
            if plan.down > 1 {
                pass.set_pipeline(
                    self.downsample[plan.down.ilog2() as usize - 1]
                        .as_ref()
                        .unwrap(),
                );
                pass.set_bind_group(0, slot.down_bind.as_ref().unwrap(), &[]);
                pass.dispatch_workgroups(
                    reduced_size[0].div_ceil(8),
                    reduced_size[1].div_ceil(8),
                    1,
                );
            }
            if let Some(workspace) = workspace {
                self.blur.dispatch_backdrop_axes(
                    queue,
                    &mut pass,
                    workspace,
                    plan.reduced_sigma,
                    plan.radius,
                );
            }
        }
        (view, original, passes, bytes, true)
    }

    pub fn clear(&mut self) {
        self.slots.clear();
    }

    pub fn stats(&self) -> (usize, u64, u64, u64) {
        let mut entries = 0;
        let mut bytes = 0_u64;
        for slot in &self.slots {
            entries += 1;
            bytes += u64::from(slot.size[0]) * u64::from(slot.size[1]) * 4;
            if let Some(reduced) = &slot.reduced {
                entries += 1;
                bytes += u64::from(reduced.width()) * u64::from(reduced.height()) * 4;
            }
            if let Some(blur) = &slot.blur {
                entries += 2;
                bytes += blur.bytes();
            }
        }
        (entries, bytes, self.allocations, self.reuses)
    }
}
