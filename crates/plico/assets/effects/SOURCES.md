# Effect sources

Reference: https://github.com/aaaa-zhen/glsl-playground
Revision: `40b3dab256ef82543b98ca0c62fad8d3c8fe21d2` (retrieved 2026-11-09).
The upstream README explicitly declares **MIT** under `## License`.
At this revision there is no separate LICENSE file or author copyright notice;
this record preserves the upstream attribution and license declaration without
inventing a copyright holder. The original source and README remain available
at the revision link above.

- `voice_core.wgsl`, `voice_wave.wgsl`: resolved spectral wave equations from
  `siri-wave.html`, with a Plico energy parameter.
- `fluid_dots.wgsl`: WGSL translation of `fs-orb` in `siri-wave.html`; gathering,
  pair motion, smooth minimum and spectral separation retained. Screen-space
  dithering is omitted.
- `liquid_lens.wgsl`: Plico adaptation of `liquid-glass.html` and
  `metaball-search.html`: rounded-box analytic gradient, gradient-aware union,
  spherical edge falloff, chromatic sampling, upper mirror and rim. Backdrop
  sampling/prefiltering use Plico rather than the original image textures.
- `edge_light.wgsl`: parabolic corner SDF, two-lobe edge mask and eleven-light
  color mixing from `siri-edge-light.html`. Procedural sine noise replaces the
  source's 3D texture; no external texture assets are copied.

The gallery interaction uses Plico's existing injected clock and analytic spring,
not the reference JavaScript integration loop. No upstream runtime is linked.
