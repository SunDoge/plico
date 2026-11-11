use super::*;

// MyGO charges roughly one 4096-pixel block per extra draw call. A split
// needs up to four strips, so omit a transparent core only when it is both
// substantial and at least a quarter of the quad it replaces.
const RING_SPLIT_MIN_PIXELS: u64 = 4096;

fn target_scissor(bounds: [f32; 4], origin: [f32; 2], size: [u32; 2]) -> Option<[u32; 4]> {
    let left = (bounds[0] - origin[0]).floor().max(0.) as u32;
    let top = (bounds[1] - origin[1]).floor().max(0.) as u32;
    let right = (bounds[2] - origin[0]).ceil().max(0.).min(size[0] as f32) as u32;
    let bottom = (bounds[3] - origin[1]).ceil().max(0.).min(size[1] as f32) as u32;
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

pub(super) fn intersect_scissor(a: [u32; 4], b: [u32; 4]) -> Option<[u32; 4]> {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

/// Up to four target-relative strips around a transparent ring core.
pub(super) struct ScissorStrips {
    strips: [[u32; 4]; 4],
    len: usize,
}

impl ScissorStrips {
    pub(super) fn as_slice(&self) -> &[[u32; 4]] {
        &self.strips[..self.len]
    }
}

/// Plan strips where the fill/border shader may produce visible pixels.
/// Circular corner coverage is conservative: the omitted rectangle lies
/// inside the inner rounded shape.
pub(super) fn ring_scissors(
    quad: &Quad,
    size: [u32; 2],
    origin: [f32; 2],
) -> Option<ScissorStrips> {
    if quad.material
        || quad.shadow_hole.is_some()
        || quad.shadow_sigma != 0.
        || quad.shadow_smoothing != 0.
        || quad.border_width <= 0.
        || quad.border_color[3] <= 0.
        || quad.colors[0][3] != 0.
        || quad.colors[1][3] != 0.
    {
        return None;
    }
    let [x, y, width, height] = quad.rect;
    let radii = shrink_radii([width, height], quad.radii);
    let inner_radius = radii
        .into_iter()
        .map(|radius| (radius - quad.border_width).max(0.))
        .fold(0_f32, f32::max);
    // The pixel-area coverage fringe reaches half a pixel. Two pixels match
    // MyGO's conservative bandMargin and leave slack for fractional edges.
    let inset = quad.border_width + inner_radius + 2.;
    if width <= 2. * inset || height <= 2. * inset {
        return None;
    }
    let mut whole = [x - 1., y - 1., x + width + 1., y + height + 1.];
    if let Some([cx, cy, cw, ch]) = quad.clip {
        whole = [
            whole[0].max(cx),
            whole[1].max(cy),
            whole[2].min(cx + cw),
            whole[3].min(cy + ch),
        ];
    }
    let whole = target_scissor(whole, origin, size)?;
    let core = target_scissor(
        [x + inset, y + inset, x + width - inset, y + height - inset],
        origin,
        size,
    )
    .and_then(|core| intersect_scissor(core, whole))?;
    let area = u64::from(core[2]) * u64::from(core[3]);
    let whole_area = u64::from(whole[2]) * u64::from(whole[3]);
    if area < RING_SPLIT_MIN_PIXELS || 4 * area < whole_area {
        return None;
    }
    let [wx, wy, ww, wh] = whole;
    let [cx, cy, cw, ch] = core;
    let right = wx + ww;
    let bottom = wy + wh;
    let core_right = cx + cw;
    let core_bottom = cy + ch;
    let mut strips = ScissorStrips {
        strips: [[0; 4]; 4],
        len: 0,
    };
    for strip in [
        [wx, wy, ww, cy - wy],
        [wx, core_bottom, ww, bottom - core_bottom],
        [wx, cy, cx - wx, ch],
        [core_right, cy, right - core_right, ch],
    ] {
        if strip[2] > 0 && strip[3] > 0 {
            strips.strips[strips.len] = strip;
            strips.len += 1;
        }
    }
    (strips.len > 1).then_some(strips)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_border_only_quads_split_around_a_conservative_core() {
        let mut ring = Quad::new([10., 20., 1000., 700.], [0.; 4]);
        ring.radii = [40.; 4];
        ring.border_width = 2.;
        ring.border_color = [1.; 4];
        let strips = ring_scissors(&ring, [1200, 900], [0.; 2]).unwrap();
        assert_eq!(strips.len, 4);
        let shaded: u64 = strips
            .as_slice()
            .iter()
            .map(|strip| u64::from(strip[2]) * u64::from(strip[3]))
            .sum();
        assert!(shaded < 200_000, "split shades {shaded} pixels");
        assert!(ring_scissors(&ring, [64, 64], [0.; 2]).is_none());
    }

    #[test]
    fn filled_or_small_borders_do_not_pay_for_split_draws() {
        let mut ring = Quad::new([0., 0., 64., 64.], [0.; 4]);
        ring.border_width = 2.;
        ring.border_color = [1.; 4];
        assert!(ring_scissors(&ring, [64, 64], [0.; 2]).is_none());
        ring.rect = [0., 0., 1000., 700.];
        ring.colors = [[1., 0., 0., 0.01]; 2];
        assert!(ring_scissors(&ring, [1000, 700], [0.; 2]).is_none());
    }
}
