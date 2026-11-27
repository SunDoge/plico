// MyGO ui/transition.go, MIT, revision f8f8a8df. See licenses/MyGo-MIT.txt.
// Snap and rasterize at the resting origin, then move that same coverage by
// the sampled displacement. The displacement includes its fractional part.

pub(crate) fn horizontal(position: f64, displacement: f64) -> (f64, u8) {
    let quarter = ((position - displacement) * 4.).round() as i64;
    (
        quarter.div_euclid(4) as f64 + displacement,
        quarter.rem_euclid(4) as u8,
    )
}

pub(crate) fn vertical(position: f64, displacement: f64) -> f64 {
    (position - displacement).round() + displacement
}

#[test]
fn translation_preserves_resting_phase_and_moves_between_pixels() {
    for rest in [-3.34, -0.25, 2.7, 100.3125] {
        let (base, phase) = horizontal(rest, 0.);
        for displacement in [-40.75, -0.55, 0.13, 2.4, 21.625] {
            let (moved, sampled_phase) = horizontal(rest + displacement, displacement);
            assert_eq!(sampled_phase, phase);
            assert_eq!(moved, base + displacement);
            assert_eq!(
                vertical(rest + displacement, displacement),
                vertical(rest, 0.) + displacement
            );
        }
    }
}
