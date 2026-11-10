use super::*;

pub(super) fn plan_backdrops(
    instances: &[Instance],
    effects: &[plico_renderer::ShaderEffect],
    origin: [f32; 2],
    size: [u32; 2],
    enabled: bool,
) -> crate::backdrop_batch::Plan {
    if !enabled || effects.len() < 2 {
        return crate::backdrop_batch::Plan {
            groups: Vec::new(),
            members: Vec::new(),
        };
    }
    let mut requests = Vec::new();
    let mut writes = Vec::with_capacity(instances.len());
    for (index, i) in instances.iter().enumerate() {
        let [x, y, w, h] = i.rect;
        let [cx, cy, cw, ch] = i.clip;
        let mut output = kurbo::Rect::new(
            f64::from(x - origin[0] - 1.),
            f64::from(y - origin[1] - 1.),
            f64::from(x + w - origin[0] + 1.),
            f64::from(y + h - origin[1] + 1.),
        )
        .intersect(kurbo::Rect::new(
            f64::from(cx - origin[0] - 1.),
            f64::from(cy - origin[1] - 1.),
            f64::from(cx + cw - origin[0] + 1.),
            f64::from(cy + ch - origin[1] + 1.),
        ));
        if i.clip_params[2] > 0. {
            let [mx, my, mw, mh] = i.mask_rect;
            output = output.intersect(kurbo::Rect::new(
                f64::from(mx - origin[0]),
                f64::from(my - origin[1]),
                f64::from(mx + mw - origin[0]),
                f64::from(my + mh - origin[1]),
            ));
        }
        writes.push((output.width() > 0. && output.height() > 0.).then_some(output));
        if !enabled || i.color[3] != 2. || output.width() <= 0. || output.height() <= 0. {
            continue;
        }
        // Match the actual marker's visibility test, not the AA-inflated
        // write bound: an offscreen future member must never be copied early.
        if x.max(cx).max(origin[0]) >= (x + w).min(cx + cw).min(origin[0] + size[0] as f32)
            || y.max(cy).max(origin[1]) >= (y + h).min(cy + ch).min(origin[1] + size[1] as f32)
        {
            continue;
        }
        let effect = &effects[i.config[3] as usize];
        let left = (x - origin[0]).floor().max(0.) as u32;
        let top = (y - origin[1]).floor().max(0.) as u32;
        let width =
            (((x + w - origin[0]).ceil().min(size[0] as f32) as u32).saturating_sub(left)).max(1);
        let height =
            (((y + h - origin[1]).ceil().min(size[1] as f32) as u32).saturating_sub(top)).max(1);
        requests.push(crate::backdrop_batch::Request {
            instance: index,
            region: [left, top, width, height],
            levels: crate::backdrop_prefilter::levels(
                width,
                height,
                effect
                    .prefilter_backdrop()
                    .map(|sigma| sigma * effect.device_scale()),
            ),
        });
    }
    crate::backdrop_batch::Plan::new(&requests, &writes, instances.len())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn encode_backdrop_step<'p>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: Option<&Target>,
    effects: &mut Option<plico_renderer::shader::ShaderRenderer>,
    compositor: &mut Option<crate::backdrop_composite::Compositor>,
    prefilter: &mut Option<crate::backdrop_prefilter::Prefilter>,
    stats: &mut FrameBatchStats,
    shared: Option<&wgpu::TextureView>,
    mask_layout: &wgpu::BindGroupLayout,
    mask: &wgpu::BindGroup,
    instance: Instance,
    origin: [f32; 2],
    encoder: &'p mut wgpu::CommandEncoder,
    frame_view: &wgpu::TextureView,
    rect: [f32; 4],
    effect: &plico_renderer::ShaderEffect,
    size: [u32; 2],
) -> wgpu::RenderPass<'p> {
    let [x, y, w, h] = rect;
    let target = target.expect("backdrop steps only run with a live target");
    let left = x.floor().max(0.) as u32;
    let top = y.floor().max(0.) as u32;
    let region = wgpu::Extent3d {
        width: ((x + w).ceil().min(size[0] as f32) as u32)
            .saturating_sub(left)
            .max(1),
        height: ((y + h).ceil().min(size[1] as f32) as u32)
            .saturating_sub(top)
            .max(1),
        depth_or_array_layers: 1,
    };
    let backdrop_view = if let Some(view) = shared {
        stats.backdrop_shared_reads += 1;
        view.clone()
    } else {
        let mip_levels = crate::backdrop_prefilter::levels(
            region.width,
            region.height,
            effect
                .prefilter_backdrop()
                .map(|sigma| sigma * effect.device_scale()),
        );
        stats.backdrop_copies += 1;
        stats.backdrop_textures += 1;
        stats.backdrop_mip_passes += u64::from(mip_levels - 1);
        stats.backdrop_bytes +=
            crate::backdrop_prefilter::bytes(region.width, region.height, mip_levels);
        let backdrop = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Plico effect backdrop copy"),
            size: region,
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: left,
                    y: top,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &backdrop,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            region,
        );
        if mip_levels > 1 {
            prefilter
                .get_or_insert_with(|| crate::backdrop_prefilter::Prefilter::new(device))
                .encode(device, encoder, &backdrop);
        }
        backdrop.create_view(&wgpu::TextureViewDescriptor::default())
    };
    let effects = effects.get_or_insert_with(|| {
        let mut effects = plico_renderer::shader::ShaderRenderer::new(device);
        effects.begin_frame();
        effects
    });
    let output = effects.render_view_with_backdrop_mapping(
        effect,
        &backdrop_view,
        [
            (x - left as f32) / region.width as f32,
            (y - top as f32) / region.height as f32,
            w / region.width as f32,
            h / region.height as f32,
        ],
        device,
        queue,
        encoder,
    );
    let compositor = compositor
        .get_or_insert_with(|| crate::backdrop_composite::Compositor::new(device, mask_layout));
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Plico backdrop effect composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_scissor_rect(left, top, region.width, region.height);
        // Keep the effect's original rectangle when an isolated target crops
        // it. The compositor applies the inherited world-space clip and mask;
        // shrinking its viewport instead would stretch offscreen content in.
        pass.set_viewport(0., 0., size[0] as f32, size[1] as f32, 0., 1.);
        compositor.encode(
            device,
            &output,
            &backdrop_view,
            mask,
            [
                [x, y, w, h],
                instance.clip,
                instance.mask_rect,
                [
                    origin[0],
                    origin[1],
                    instance.clip_params[2],
                    instance.clip_params[0],
                ],
            ],
            pass,
        );
    }
    // Continue the frame in a fresh pass over the same target.
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Plico quad draw (continued)"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: frame_view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    })
}
