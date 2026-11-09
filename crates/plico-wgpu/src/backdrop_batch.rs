//! Same-stage, equal-size snapshots. Each array slice preserves private UVs/mips.
use kurbo::Rect;
#[derive(Clone, Copy, Debug)]
pub(crate) struct Request {
    pub instance: usize,
    pub region: [u32; 4],
    pub levels: u32,
}
pub(crate) struct Plan {
    pub groups: Vec<Vec<Request>>,
    pub members: Vec<Option<(usize, usize)>>,
}
impl Plan {
    pub fn new(requests: &[Request], writes: &[Option<Rect>], count: usize) -> Self {
        let mut result = Self {
            groups: Vec::new(),
            members: vec![None; count],
        };
        for (start, a) in requests.iter().enumerate() {
            if result.members[a.instance].is_some() {
                continue;
            }
            let mut group = vec![*a];
            let mut levels = a.levels;
            for b in &requests[start + 1..] {
                if b.instance - a.instance > 512 || group.len() == 16 {
                    break;
                }
                if result.members[b.instance].is_some() || a.region[2..] != b.region[2..] {
                    continue;
                }
                let input = Rect::new(
                    f64::from(b.region[0]),
                    f64::from(b.region[1]),
                    f64::from(b.region[0] + b.region[2]),
                    f64::from(b.region[1] + b.region[3]),
                );
                // Every write since the snapshot point, including earlier
                // effects and foreground labels, must miss this reader's input.
                if writes[a.instance..b.instance]
                    .iter()
                    .flatten()
                    .any(|r| r.intersect(input).area() > 0.)
                {
                    continue;
                }
                let next = levels.max(b.levels);
                let bytes = super::backdrop_prefilter::bytes(a.region[2], a.region[3], next)
                    * (group.len() + 1) as u64;
                if bytes > 32 * 1024 * 1024 {
                    continue;
                }
                group.push(*b);
                levels = next;
            }
            if group.len() > 1 {
                let index = result.groups.len();
                for (slot, request) in group.iter().enumerate() {
                    result.members[request.instance] = Some((index, slot));
                }
                result.groups.push(group);
            }
        }
        result
    }
}

pub(crate) struct Prefilter {
    pipeline: wgpu::ComputePipeline,
}
impl Prefilter {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("batched backdrop prefilter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop_batch.wgsl").into()),
        });
        Self {
            pipeline: device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Plico batched backdrop prefilter"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            }),
        }
    }
    fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        let layout = self.pipeline.get_bind_group_layout(0);
        for level in 1..texture.mip_level_count() {
            let view = |mip| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            };
            let input = view(level - 1);
            let output = view(level);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("batched backdrop mip"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&output),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("batched backdrop mip"),
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(
                (texture.width() >> level).max(1).div_ceil(8),
                (texture.height() >> level).max(1).div_ceil(8),
                texture.depth_or_array_layers(),
            );
        }
    }
}
pub(crate) fn capture(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
    requests: &[Request],
    prefilter: &mut Option<Prefilter>,
) -> Vec<wgpu::TextureView> {
    let [_, _, width, height] = requests[0].region;
    let levels = requests.iter().map(|r| r.levels).max().unwrap();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Plico same-stage backdrop array"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: requests.len() as u32,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (slot, r) in requests.iter().enumerate() {
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: r.region[0],
                    y: r.region[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: slot as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    if levels > 1 {
        prefilter
            .get_or_insert_with(|| Prefilter::new(device))
            .encode(device, encoder, &texture);
    }
    requests
        .iter()
        .enumerate()
        .map(|(slot, r)| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: slot as u32,
                array_layer_count: Some(1),
                mip_level_count: Some(r.levels),
                ..Default::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stages_ignore_unrelated_writes_but_split_dependent_or_different_size_inputs() {
        let requests = [
            Request {
                instance: 0,
                region: [0, 0, 16, 16],
                levels: 3,
            },
            Request {
                instance: 2,
                region: [24, 0, 16, 16],
                levels: 3,
            },
            Request {
                instance: 4,
                region: [48, 0, 16, 16],
                levels: 3,
            },
        ];
        let writes = [
            Some(Rect::new(0., 0., 16., 16.)),
            Some(Rect::new(24., 0., 40., 16.)),
            Some(Rect::new(24., 0., 40., 16.)),
            None,
            Some(Rect::new(48., 0., 64., 16.)),
        ];
        let plan = Plan::new(&requests, &writes, 5);
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].len(), 2);
        assert!(plan.members[0].is_some() && plan.members[4].is_some());
        assert!(plan.members[2].is_none());
        let mut different = requests;
        different[2].region[2] = 17;
        assert!(Plan::new(&different, &writes, 5).groups.is_empty());
    }
    #[test]
    fn batches_have_bounded_layers_and_payload() {
        let requests: Vec<_> = (0..40)
            .map(|i| Request {
                instance: i,
                region: [i as u32 * 20, 0, 16, 16],
                levels: 3,
            })
            .collect();
        let writes: Vec<_> = requests
            .iter()
            .map(|r| {
                Some(Rect::new(
                    f64::from(r.region[0]),
                    0.,
                    f64::from(r.region[0] + 16),
                    16.,
                ))
            })
            .collect();
        let plan = Plan::new(&requests, &writes, 40);
        assert_eq!(
            plan.groups.iter().map(Vec::len).collect::<Vec<_>>(),
            [16, 16, 8]
        );
        let huge = [
            Request {
                instance: 0,
                region: [0, 0, 4096, 4096],
                levels: 3,
            },
            Request {
                instance: 1,
                region: [5000, 0, 4096, 4096],
                levels: 3,
            },
        ];
        assert!(Plan::new(&huge, &[None, None], 2).groups.is_empty());
    }
}
