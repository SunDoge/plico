//! Stable, bounded bitmap windows; the draw retains the exact viewport clip.
//! Reusing text pixels across scrolling follows MyGO's raster/clip separation.
const SMALL_BYTES: f64 = 64. * 1024.;
const MAX_BYTES: f64 = 64. * 1024. * 1024.;
const HALO: f64 = 2.;
const GRID: [f64; 2] = [128., 32.];

/// Inputs are outward-rounded physical ink and its nonempty visible intersection,
/// relative to an integer baseline. The result contains all visible pixels.
pub(crate) fn cache_window(ink: [f64; 4], visible: [f64; 4]) -> [f64; 4] {
    let fits = |rect: [f64; 4], bytes: f64| {
        let width = rect[2] - rect[0];
        let height = rect[3] - rect[1];
        rect.iter().all(|v| v.is_finite() && v.abs() <= 1e9)
            && width > 0.
            && height > 0.
            && width <= 16384.
            && height <= 16384.
            && width * height * 4. <= bytes
    };
    if fits(ink, SMALL_BYTES) {
        return ink;
    }
    // Anchor to line space, independent of the moving viewport. A very long
    // line is still rasterized only near the viewport, never in full.
    let expanded = [
        ink[0].max(((visible[0] - HALO) / GRID[0]).floor() * GRID[0]),
        ink[1].max(((visible[1] - HALO) / GRID[1]).floor() * GRID[1]),
        ink[2].min(((visible[2] + HALO) / GRID[0]).ceil() * GRID[0]),
        ink[3].min(((visible[3] + HALO) / GRID[1]).ceil() * GRID[1]),
    ];
    if fits(expanded, MAX_BYTES) {
        expanded
    } else {
        visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_line_pixels_are_independent_of_a_scrolling_clip() {
        let ink = [-3., -20., 400., 6.];
        for y in -19..6 {
            assert_eq!(cache_window(ink, [0., y as f64, 120., 6.]), ink);
        }
    }
    #[test]
    fn long_line_windows_are_stable_without_allocating_the_entire_line() {
        let ink = [-3., -20., 1_000_000., 6.];
        let first = cache_window(ink, [10., -15., 110., 4.]);
        assert_eq!(first, [0., -20., 128., 6.]);
        for x in 10..26 {
            assert_eq!(
                cache_window(ink, [x as f64, -14., x as f64 + 100., 4.]),
                first
            );
        }
        let next = cache_window(ink, [29., -14., 129., 4.]);
        assert_eq!(next, [0., -20., 256., 6.]);
    }
    #[test]
    fn expansion_contains_visible_pixels_with_bounded_overdraw_in_all_quadrants() {
        let ink = [-1e8, -1e8, 1e8, 1e8];
        for x in [-1001., -128., -1., 0., 1., 127., 128., 999.] {
            for y in [-1001., -32., -1., 0., 1., 31., 32., 999.] {
                let visible = [x, y, x + 201., y + 57.];
                let result = cache_window(ink, visible);
                assert!(result[0] <= visible[0] && result[1] <= visible[1]);
                assert!(result[2] >= visible[2] && result[3] >= visible[3]);
                assert!(
                    visible[0] - result[0] < GRID[0] + HALO
                        && result[2] - visible[2] < GRID[0] + HALO
                );
                assert!(
                    visible[1] - result[1] < GRID[1] + HALO
                        && result[3] - visible[3] < GRID[1] + HALO
                );
            }
        }
    }
    #[test]
    fn scrolling_a_large_line_does_not_create_a_cache_key_for_every_pixel() {
        use std::collections::HashSet;
        let ink = [-2., -2., 10000., 10000.];
        let mut exact = HashSet::new();
        let mut stable = HashSet::new();
        for y in 0..120 {
            let visible = [0., y as f64, 800., y as f64 + 200.];
            exact.insert(visible.map(f64::to_bits));
            let result = cache_window(ink, visible);
            assert!((result[2] - result[0]) * (result[3] - result[1]) * 4. < 1024. * 1024.);
            stable.insert(result.map(f64::to_bits));
        }
        assert_eq!(exact.len(), 120);
        assert!(stable.len() <= 10, "stable windows: {}", stable.len());
    }
    #[test]
    fn near_budget_and_dimension_limits_keep_the_existing_exact_window() {
        let ink = [-1e8, -1e8, 1e8, 1e8];
        for visible in [[1., 1., 16385., 2.], [1., 1., 4097., 4097.]] {
            assert_eq!(cache_window(ink, visible), visible);
        }
    }
}
