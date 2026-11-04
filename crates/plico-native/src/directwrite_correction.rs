// Copyright (c) Microsoft Corporation. All rights reserved.
// SPDX-License-Identifier: MIT
// Adapted from Microsoft Terminal, commit 1283c0f5b99a2961673249fa77c6b986efb5086c:
// src/renderer/atlas/dwrite.cpp and dwrite.hlsl.
// License: ../licenses/microsoft-terminal-MIT.txt. See SOURCES.md.
//! Foreground-dependent correction for raw DirectWrite grayscale coverage.
//! This is not outline emboldening, an LCD renderer or display color management.
use snafu::Snafu;
/// Nonfinite or out-of-contract correction inputs.
#[derive(Clone, Copy, Debug, Snafu)]
#[snafu(display("Invalid DirectWrite grayscale correction parameters"))]
pub struct InvalidCorrection;
/// Native gamma/contrast prepared for grayscale compositing.
#[derive(Clone, Copy, Debug)]
pub struct GrayscaleCorrection {
    ratios: [f32; 4],
    contrast: f32,
}
/// Prepared foreground and contrast terms; reusable across every pixel in a run.
#[derive(Clone, Copy, Debug)]
pub struct PreparedGrayscale {
    ratios: [f32; 4],
    contrast: f32,
    intensity: f32,
    premultiplied: [f32; 4],
}
impl GrayscaleCorrection {
    /// Prepare native rendering parameters. Gamma rounds to the upstream 1.0–2.2
    /// tenth-step table; positive finite values outside it clamp to its endpoints.
    /// Grayscale contrast must be finite and within DirectWrite's 0–10 range.
    pub fn new(gamma: f32, contrast: f32) -> Result<Self, InvalidCorrection> {
        if !gamma.is_finite()
            || gamma <= 0.
            || !contrast.is_finite()
            || !(0. ..=10.).contains(&contrast)
        {
            return Err(InvalidCorrection);
        }
        // Same coefficients as the original table; its /4 and normalization *4
        // cancel exactly (power-of-two scaling), leaving these simpler factors.
        const TABLE: [[f32; 4]; 13] = [
            [0., 0., 0., 0.],
            [0.0166, -0.0807, 0.2227, -0.0751],
            [0.0350, -0.1760, 0.4325, -0.1370],
            [0.0543, -0.2821, 0.6302, -0.1876],
            [0.0739, -0.3963, 0.8167, -0.2287],
            [0.0933, -0.5161, 0.9926, -0.2616],
            [0.1121, -0.6395, 1.1588, -0.2877],
            [0.1300, -0.7649, 1.3159, -0.3080],
            [0.1469, -0.8911, 1.4644, -0.3234],
            [0.1627, -1.0170, 1.6051, -0.3347],
            [0.1773, -1.1420, 1.7385, -0.3426],
            [0.1908, -1.2652, 1.8650, -0.3476],
            [0.2031, -1.3864, 1.9851, -0.3501],
        ];
        let index = ((gamma.clamp(1., 2.2) * 10.).round() as usize) - 10;
        let t = TABLE[index];
        let n13 = (65536_f64 / (255. * 255.)) as f32;
        let n24 = (256_f64 / 255.) as f32;
        Ok(Self {
            ratios: [t[0] * n13, t[1] * n24, t[2] * n13, t[3] * n24],
            contrast,
        })
    }
    /// Prepare a straight sRGB foreground. `thin_font` is the upstream special
    /// contrast policy for known legacy thin families, not a font-weight setting.
    /// Family detection is the caller's responsibility.
    pub fn prepare(
        &self,
        foreground: [f32; 4],
        thin_font: bool,
    ) -> Result<PreparedGrayscale, InvalidCorrection> {
        if !foreground
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v))
        {
            return Err(InvalidCorrection);
        }
        let [r, g, b, alpha] = foreground;
        let lightness = 0.30 * r + 0.59 * g + 0.11 * b;
        let contrast = self.contrast * (4. * (0.75 - lightness)).clamp(0., 1.)
            + if thin_font { 0.5 } else { 0. };
        Ok(PreparedGrayscale {
            ratios: self.ratios,
            contrast,
            intensity: 0.25 * r + 0.5 * g + 0.25 * b,
            premultiplied: [r * alpha, g * alpha, b * alpha, alpha],
        })
    }
}
impl PreparedGrayscale {
    fn corrected(&self, coverage: f32) -> [f32; 4] {
        let a = coverage * (self.contrast + 1.) / (coverage * self.contrast + 1.);
        let g = self.ratios;
        let f = self.intensity;
        let corrected =
            (a + a * (1. - a) * ((g[0] * f + g[1]) * a + (g[2] * f + g[3]))).clamp(0., 1.);
        self.premultiplied.map(|v| v * corrected)
    }
    /// Blend a validated normalized sample to premultiplied sRGB RGBA.
    pub fn blend(&self, coverage: f32) -> Result<[f32; 4], InvalidCorrection> {
        if !coverage.is_finite() || !(0. ..=1.).contains(&coverage) {
            return Err(InvalidCorrection);
        }
        Ok(self.corrected(coverage))
    }
    /// Blend an R8 coverage sample to premultiplied RGBA8, without validation or
    /// allocation in the pixel loop. Foreground/parameters were checked up front.
    pub fn blend_u8(&self, coverage: u8) -> [u8; 4] {
        self.corrected(f32::from(coverage) / 255.)
            .map(|v| (v * 255.).round() as u8)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_ratios_and_identity_match_upstream_contract() {
        let c = GrayscaleCorrection::new(1.8, 1.).unwrap();
        for (a, b) in
            c.ratios
                .into_iter()
                .zip([0.148054421_f64, -0.894594550, 1.47590804, -0.324668258])
        {
            assert!((f64::from(a) - b).abs() < 1e-6);
        }
        let identity = GrayscaleCorrection::new(1., 0.)
            .unwrap()
            .prepare([0.2, 0.4, 0.8, 0.5], false)
            .unwrap();
        let pixel = identity.blend(0.5).unwrap();
        for (a, b) in pixel.into_iter().zip([0.05, 0.1, 0.2, 0.25]) {
            assert!((a - b).abs() < 1e-6);
        }
        assert_eq!(
            GrayscaleCorrection::new(0.5, 0.).unwrap().ratios,
            GrayscaleCorrection::new(1., 0.).unwrap().ratios
        );
        assert_eq!(
            GrayscaleCorrection::new(3., 0.).unwrap().ratios,
            GrayscaleCorrection::new(2.2, 0.).unwrap().ratios
        );
    }
    #[test]
    fn correction_is_monotonic_premultiplied_and_preserves_endpoints() {
        for step in 10..=22 {
            for contrast in [0., 1., 10.] {
                let correction = GrayscaleCorrection::new(step as f32 / 10., contrast).unwrap();
                for color in [
                    [0., 0., 0., 1.],
                    [1., 1., 1., 1.],
                    [0.2, 0.8, 0.1, 0.5],
                    [1., 0., 0., 0.],
                ] {
                    for thin in [false, true] {
                        let prepared = correction.prepare(color, thin).unwrap();
                        assert_eq!(prepared.blend_u8(0), [0; 4]);
                        assert_eq!(
                            prepared.blend_u8(255),
                            [
                                color[0] * color[3],
                                color[1] * color[3],
                                color[2] * color[3],
                                color[3]
                            ]
                            .map(|v| (v * 255.).round() as u8)
                        );
                        let mut previous = 0;
                        for a in 0..=255 {
                            let rgba = prepared.blend_u8(a);
                            assert!(rgba[..3].iter().all(|c| *c <= rgba[3]));
                            assert!(rgba[3] >= previous);
                            previous = rgba[3];
                        }
                    }
                }
            }
        }
        assert!(GrayscaleCorrection::new(f32::NAN, 1.).is_err());
        assert!(GrayscaleCorrection::new(1.8, -1.).is_err());
        let c = GrayscaleCorrection::new(1.8, 1.).unwrap();
        assert!(c.prepare([f32::NAN, 0., 0., 1.], false).is_err());
        assert!(
            c.prepare([0.; 4], false)
                .unwrap()
                .blend(f32::INFINITY)
                .is_err()
        );
    }
    #[test]
    fn colored_pixels_match_the_original_cpp_hlsl_reference() {
        // Regenerate with tests/assets/directwrite-correction-reference.cpp.
        // The oracle uses the pinned upstream functions, not this Rust port.
        let samples = [
            (1., 0., [0.2, 0.4, 0.8, 0.5], 128, false, [13, 26, 51, 64]),
            (1.8, 1., [0., 0., 0., 1.], 128, false, [0, 0, 0, 118]),
            (1.8, 1., [1., 1., 1., 1.], 128, false, [178, 178, 178, 178]),
            (1.8, 1., [1., 0., 0., 1.], 128, false, [141, 0, 0, 141]),
            (1.8, 1., [0., 1., 0., 1.], 128, false, [0, 153, 0, 153]),
            (1.8, 1., [0., 0., 1., 0.5], 128, false, [0, 0, 70, 70]),
            (2.2, 0.5, [0.1, 0.7, 0.3, 0.4], 64, true, [4, 29, 12, 42]),
            (
                1.1,
                10.,
                [0.9, 0.2, 0.1, 1.],
                200,
                false,
                [224, 50, 25, 248],
            ),
            (2.2, 10., [1., 1., 1., 1.], 1, true, [4, 4, 4, 4]),
            (1.8, 1., [0.3, 0.5, 0.8, 0.], 200, true, [0, 0, 0, 0]),
        ];
        for (gamma, contrast, color, coverage, thin, expected) in samples {
            let actual = GrayscaleCorrection::new(gamma, contrast)
                .unwrap()
                .prepare(color, thin)
                .unwrap()
                .blend_u8(coverage);
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 1),
                "{actual:?} != {expected:?}"
            );
        }
    }
}
