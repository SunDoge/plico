//! Bounded physical raster geometry, independently testable without Apple APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RasterBounds {
    pub origin: [i32; 2],
    pub size: [u32; 2],
    pub bytes: usize,
}
pub(crate) fn valid_sampling(scale: f64, phase: [f64; 2]) -> bool {
    scale.is_finite()
        && scale > 0.
        && scale <= 8.
        && phase.iter().all(|p| p.is_finite() && (0. ..1.).contains(p))
}
pub(crate) fn raster_bounds(
    [x0, y0, x1, y1]: [f64; 4],
    scale: f64,
    phase: [f64; 2],
) -> Option<RasterBounds> {
    if !valid_sampling(scale, phase)
        || ![x0, y0, x1, y1].iter().all(|n| n.is_finite())
        || x1 <= x0
        || y1 <= y0
    {
        return None;
    }
    let left = (x0 * scale + phase[0]).floor() - 2.;
    let top = (-y1 * scale + phase[1]).floor() - 2.;
    let right = (x1 * scale + phase[0]).ceil() + 2.;
    let bottom = (-y0 * scale + phase[1]).ceil() + 2.;
    if ![left, top, right, bottom]
        .iter()
        .all(|n| n.is_finite() && n.abs() <= 1e9)
        || right - left > 16384.
        || bottom - top > 16384.
    {
        return None;
    }
    let size = [(right - left) as u32, (bottom - top) as u32];
    let bytes = (size[0] as usize)
        .checked_mul(size[1] as usize)?
        .checked_mul(4)?;
    (bytes <= 64 * 1024 * 1024).then_some(RasterBounds {
        origin: [left as i32, top as i32],
        size,
        bytes,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_y_up_bounds_become_top_down_baseline_relative_pixels() {
        let bounds = raster_bounds([-1., -3., 10., 12.], 1.25, [0.25, 0.5]).unwrap();
        assert_eq!(bounds.origin, [-3, -17]);
        assert_eq!(bounds.size, [18, 24]);
        assert_eq!(bounds.bytes, 18 * 24 * 4);
    }
    #[test]
    fn invalid_sampling_and_excessive_allocations_are_rejected() {
        for scale in [0., -1., f64::NAN, f64::INFINITY, 8.1] {
            assert!(raster_bounds([0., 0., 10., 10.], scale, [0.; 2]).is_none());
        }
        for phase in [[-0.1, 0.], [1., 0.], [0., f64::NAN]] {
            assert!(raster_bounds([0., 0., 10., 10.], 1., phase).is_none());
        }
        for rect in [
            [0., 0., 0., 10.],
            [0., 0., f64::INFINITY, 10.],
            [0., 0., 16384., 1.],
            [0., 0., 5000., 5000.],
        ] {
            assert!(raster_bounds(rect, 1., [0.; 2]).is_none());
        }
    }
}
