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
#[derive(Clone, Copy, Debug)]
pub(crate) struct InvalidBounds;
pub(crate) fn valid_clip([left, top, right, bottom]: [f64; 4]) -> bool {
    [left, top, right, bottom]
        .iter()
        .all(|n| n.is_finite() && n.abs() <= 1e9)
        && right >= left
        && bottom >= top
}
pub(crate) fn raster_bounds(
    [x0, y0, x1, y1]: [f64; 4],
    scale: f64,
    phase: [f64; 2],
    clip: Option<[f64; 4]>,
) -> Result<Option<RasterBounds>, InvalidBounds> {
    if !valid_sampling(scale, phase)
        || clip.is_some_and(|clip| !valid_clip(clip))
        || ![x0, y0, x1, y1].iter().all(|n| n.is_finite())
        || x1 < x0
        || y1 < y0
    {
        return Err(InvalidBounds);
    }
    if x0 == x1 || y0 == y1 {
        return Ok(None);
    }
    let mut left = (x0 * scale + phase[0]).floor() - 2.;
    let mut top = (-y1 * scale + phase[1]).floor() - 2.;
    let mut right = (x1 * scale + phase[0]).ceil() + 2.;
    let mut bottom = (-y0 * scale + phase[1]).ceil() + 2.;
    if ![left, top, right, bottom].iter().all(|n| n.is_finite()) {
        return Err(InvalidBounds);
    }
    if let Some([x0, y0, x1, y1]) = clip {
        if x0 == x1 || y0 == y1 {
            return Ok(None);
        }
        left = left.max(x0.floor());
        top = top.max(y0.floor());
        right = right.min(x1.ceil());
        bottom = bottom.min(y1.ceil());
    }
    if right <= left || bottom <= top {
        return Ok(None);
    }
    if ![left, top, right, bottom].iter().all(|n| n.abs() <= 1e9)
        || right - left > 16384.
        || bottom - top > 16384.
    {
        return Err(InvalidBounds);
    }
    let size = [(right - left) as u32, (bottom - top) as u32];
    let bytes = (size[0] as usize)
        .checked_mul(size[1] as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(InvalidBounds)?;
    if bytes > 64 * 1024 * 1024 {
        return Err(InvalidBounds);
    }
    Ok(Some(RasterBounds {
        origin: [left as i32, top as i32],
        size,
        bytes,
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_y_up_bounds_become_top_down_baseline_relative_pixels() {
        let bounds = raster_bounds([-1., -3., 10., 12.], 1.25, [0.25, 0.5], None)
            .unwrap()
            .unwrap();
        assert_eq!(bounds.origin, [-3, -17]);
        assert_eq!(bounds.size, [18, 24]);
        assert_eq!(bounds.bytes, 18 * 24 * 4);
    }
    #[test]
    fn invalid_sampling_and_excessive_allocations_are_rejected() {
        for scale in [0., -1., f64::NAN, f64::INFINITY, 8.1] {
            assert!(raster_bounds([0., 0., 10., 10.], scale, [0.; 2], None).is_err());
        }
        for phase in [[-0.1, 0.], [1., 0.], [0., f64::NAN]] {
            assert!(raster_bounds([0., 0., 10., 10.], 1., phase, None).is_err());
        }
        for rect in [
            [0., 0., f64::INFINITY, 10.],
            [0., 0., 16384., 1.],
            [0., 0., 5000., 5000.],
        ] {
            assert!(raster_bounds(rect, 1., [0.; 2], None).is_err());
        }
    }
    #[test]
    fn clipping_precedes_large_ink_allocation_and_rounds_outward() {
        let ink = [0., -5., 1_000_000., 20.];
        assert!(raster_bounds(ink, 2., [0.25, 0.], None).is_err());
        let clipped = raster_bounds(ink, 2., [0.25, 0.], Some([1000.25, -30.25, 1100.5, 8.75]))
            .unwrap()
            .unwrap();
        assert_eq!(clipped.origin, [1000, -31]);
        assert_eq!(clipped.size, [101, 40]);
        assert_eq!(clipped.bytes, 101 * 40 * 4);
        assert!(
            raster_bounds(
                [0., 0., 10., 10.],
                1.,
                [0.; 2],
                Some([100., 100., 110., 110.])
            )
            .unwrap()
            .is_none()
        );
        assert!(
            raster_bounds(ink, 1., [0.; 2], Some([0., 0., 0., 10.]))
                .unwrap()
                .is_none()
        );
        for clip in [
            [10., 0., 0., 10.],
            [0., 0., f64::NAN, 10.],
            [0., 0., 1e10, 10.],
        ] {
            assert!(raster_bounds(ink, 1., [0.; 2], Some(clip)).is_err());
        }
    }
}
