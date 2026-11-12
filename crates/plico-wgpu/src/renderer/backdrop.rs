use super::*;

fn backdrop_region(
    rect: [f32; 4],
    effect: &plico_renderer::ShaderEffect,
    size: [u32; 2],
) -> [u32; 4] {
    let [x, y, width, height] = rect;
    // Match MyGO's backdrop dependency: a Gaussian can read three sigma
    // beyond the output, plus one texel for area reduction/interpolation.
    let gaussian = effect.backdrop_gaussian_plan();
    let reach = gaussian
        .map(|plan| plan.reach)
        .or_else(|| {
            effect
                .prefilter_backdrop()
                .map(|sigma| (3. * sigma * effect.device_scale()).ceil() + 1.)
        })
        .unwrap_or_default();
    let desired_left = (x - reach).floor() as i64;
    let desired_top = (y - reach).floor() as i64;
    let mut desired_width = ((x + width + reach).ceil() as i64 - desired_left).max(1) as u32;
    let mut desired_height = ((y + height + reach).ceil() as i64 - desired_top).max(1) as u32;
    if let Some(plan) = gaussian {
        desired_width = desired_width.div_ceil(plan.down) * plan.down;
        desired_height = desired_height.div_ceil(plan.down) * plan.down;
    }
    // Slide an edge-clipped window inward instead of shrinking it. Equal-size
    // effects then retain equal-size snapshots and can share an array capture;
    // the mapping still addresses their original physical coordinates.
    let width = desired_width.min(size[0].max(1));
    let height = desired_height.min(size[1].max(1));
    let left = desired_left.clamp(0, i64::from(size[0].saturating_sub(width))) as u32;
    let top = desired_top.clamp(0, i64::from(size[1].saturating_sub(height))) as u32;
    [left, top, width, height]
}

fn output_scissor(
    rect: [f32; 4],
    clip: [f32; 4],
    mask: Option<[f32; 4]>,
    origin: [f32; 2],
    size: [u32; 2],
) -> Option<[u32; 4]> {
    let [x, y, width, height] = rect;
    let [cx, cy, cw, ch] = clip;
    let mut left = x.max(cx - origin[0]);
    let mut top = y.max(cy - origin[1]);
    let mut right = (x + width).min(cx + cw - origin[0]);
    let mut bottom = (y + height).min(cy + ch - origin[1]);
    if let Some([mx, my, mw, mh]) = mask {
        left = left.max(mx - origin[0]);
        top = top.max(my - origin[1]);
        right = right.min(mx + mw - origin[0]);
        bottom = bottom.min(my + mh - origin[1]);
    }
    left = left.floor().max(0.);
    top = top.floor().max(0.);
    right = right.ceil().min(size[0] as f32);
    bottom = bottom.ceil().min(size[1] as f32);
    (right > left && bottom > top).then(|| {
        let left = left as u32;
        let top = top as u32;
        [left, top, right as u32 - left, bottom as u32 - top]
    })
}

pub(super) fn effect_output_scissor(
    instance: Instance,
    origin: [f32; 2],
    size: [u32; 2],
) -> Option<[u32; 4]> {
    output_scissor(
        [
            instance.rect[0] - origin[0],
            instance.rect[1] - origin[1],
            instance.rect[2],
            instance.rect[3],
        ],
        instance.clip,
        (instance.clip_params[2] > 0.).then_some(instance.mask_rect),
        origin,
        size,
    )
}

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
        // Fixed Gaussians own a separable prefilter workspace rather than a
        // same-stage texture array. Keep their painter-order barrier explicit.
        if effect.backdrop_gaussian_plan().is_some() {
            continue;
        }
        let [left, top, width, height] =
            backdrop_region([x - origin[0], y - origin[1], w, h], effect, size);
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
    effects: &mut Option<crate::shader::ShaderRenderer>,
    prefilter: &mut Option<crate::backdrop_prefilter::Prefilter>,
    gaussian: &mut Option<crate::backdrop_gaussian::GaussianBackdrops>,
    textures: &mut crate::backdrop_pool::Pool,
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
    force_generic: bool,
    size: [u32; 2],
    scissor: [u32; 4],
) -> wgpu::RenderPass<'p> {
    let [x, y, w, h] = rect;
    let target = target.expect("backdrop steps only run with a live target");
    let [left, top, width, height] = backdrop_region(rect, effect, size);
    let region = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let mut fixed_gaussian = false;
    let (backdrop_view, original_view) = if let Some(view) = shared {
        stats.backdrop_shared_reads += 1;
        (view.clone(), view.clone())
    } else if let Some(plan) = effect.backdrop_gaussian_plan().filter(|plan| {
        !force_generic
            && width.is_multiple_of(plan.down)
            && height.is_multiple_of(plan.down)
            && crate::backdrop_gaussian::GaussianBackdrops::supported(device)
    }) {
        let (view, original, passes, bytes) = gaussian
            .get_or_insert_with(|| crate::backdrop_gaussian::GaussianBackdrops::new(device))
            .encode(
                device,
                queue,
                encoder,
                &target.texture,
                [left, top, width, height],
                plan,
            );
        fixed_gaussian = true;
        stats.backdrop_copies += 1;
        stats.backdrop_textures +=
            1 + u64::from(plan.down > 1) + u64::from(plan.reduced_sigma > 0.) * 2;
        stats.backdrop_mip_passes += passes;
        stats.backdrop_bytes += bytes;
        (view, original)
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
        let backdrop = textures.acquire(
            device,
            [region.width, region.height],
            1,
            mip_levels,
            crate::backdrop_pool::Kind::Single,
        );
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
        let view = backdrop.create_view(&wgpu::TextureViewDescriptor::default());
        (view.clone(), view)
    };
    let mapping = [
        (x - left as f32) / region.width as f32,
        (y - top as f32) / region.height as f32,
        w / region.width as f32,
        h / region.height as f32,
    ];
    let composite = [
        [x, y, w, h],
        instance.clip,
        instance.mask_rect,
        [
            origin[0],
            origin[1],
            instance.clip_params[2],
            instance.clip_params[0],
        ],
    ];
    let effects = effects.get_or_insert_with(|| {
        let mut effects = crate::shader::ShaderRenderer::new(device);
        effects.begin_frame();
        effects
    });
    effects.render_backdrop_direct(
        effect,
        &backdrop_view,
        &original_view,
        mapping,
        composite,
        [1., f32::from(fixed_gaussian), 0., 0.],
        mask_layout,
        mask,
        device,
        queue,
        encoder,
        frame_view,
        size,
        scissor,
    );
    stats.backdrop_effect_pixels += u64::from(scissor[2]) * u64::from(scissor[3]);
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

#[cfg(test)]
mod tests {
    use super::output_scissor;

    #[test]
    fn effect_output_scissor_intersects_canvas_clip_mask_origin_and_target() {
        assert_eq!(
            output_scissor(
                [0., 0., 100., 100.],
                [40., 25., 20., 10.],
                None,
                [10., 5.],
                [80, 60],
            ),
            Some([30, 20, 20, 10])
        );
        assert_eq!(
            output_scissor(
                [0., 0., 100., 100.],
                [40., 25., 20., 10.],
                Some([42.25, 26.5, 4., 3.]),
                [10., 5.],
                [80, 60],
            ),
            Some([32, 21, 5, 4])
        );
        assert_eq!(
            output_scissor(
                [-20., -10., 100., 100.],
                [-100., -100., 400., 400.],
                None,
                [0., 0.],
                [40, 30],
            ),
            Some([0, 0, 40, 30])
        );
        assert_eq!(
            output_scissor(
                [0., 0., 10., 10.],
                [20., 20., 5., 5.],
                None,
                [0., 0.],
                [100, 100],
            ),
            None
        );
    }
}
