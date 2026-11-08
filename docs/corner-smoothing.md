# Continuous corners

`Style::corner_smoothing` / C# `CornerSmoothing(amount)` opt custom-drawn nodes
into eased corners. The default is zero, preserving circular rounded rectangles
and their analytical renderer fast path. Radius retains its existing meaning.

```csharp
using (ui.Column().Width(300).Height(160).Radius(28)
    .CornerSmoothing(1).Clip().Background(ui.Theme.Panel)
    .Border(1, ui.Theme.Border))
{
    // Child content follows the same continuous contour.
}
```

Rust layout/widget builders expose `.corner_smoothing(1.)`. Values must be finite
and in `0..=1`. Native platform controls are not automatically restyled.

## Geometry

`plico_renderer::corner_shape::corner_shape` is the shared geometry constructor
for background fill, outline, uniform/side borders, content clipping and glass
panel clipping. A smoothed uniform border is an even-odd ring whose outer path
matches the background and whose inner path matches the inset content clip.

The previous single-cubic approximation has been replaced with MyGO's implicit
continuous-corner equation, adapted from `internal/raster/corner.go` and
`internal/gpu/metal/shader.metal` at MyGO revision `ee3d4bd9`. It uses the
`1.528665 * radius` transition extent, quartic correction and separate horizontal
and vertical clamp factors. The shape is the intersection of all four corners,
including unequal corners that can extend past a side's midpoint. Radii are
fitted by neighboring sums, as in MyGO.

One uses that reference field. Zero remains the existing circular primitive;
intermediate amounts interpolate the circular and continuous fields. This does
not claim that MyGO exposes the same smoothing slider. MyGO itself defaults to
continuous corners on macOS and circular corners on other platforms; Plico keeps
this an explicit cross-platform choice.

The shared contour is traced adaptively from the field, rather than substituting
an independently chosen Bezier shoulder. Local geometry is cached without its
translation (128 entries, no cached contour above 4096 elements). The adapted
formula's MIT license is retained at
`crates/plico-renderer/third_party/mygo-LICENSE`.

## Renderer cost and limits

Zero smoothing retains `RoundedRect` metadata. Enabled continuous corners are
shared adaptive contours, with segment deviation refined to 0.0125 logical
pixels (subject to the recursion bound). Wgpu currently uses its existing bounded cached path/clip rasterizer;
this is not a new analytical WGSL fast path. Animating radius/smoothing can
invalidate those rasters. Layout size does not change. Foreground blur and
corner easing add two inline floats to retained Paint; the measured 64-bit
record size is 416 bytes including alignment (previously 400). The size guard
was updated explicitly; these features do not add per-node callback objects.

Box-shadow geometry is still the existing rounded-rectangle approximation.
Native/embedded widgets that paint their own background must opt into this
geometry themselves; a containing continuous clip does not rewrite their paint.

## Verification and preview

Golden numerical checks compare the Rust implicit field to ten values produced
by the unmodified local MyGO Go reference, including unequal radii and cramped
sides (tolerance 0.00002). Contour checks verify points on the field, translation
reuse and default behavior. The local adapter-required wgpu corner test compares
fill and clip alpha within two 8-bit levels. Existing circular quad checks remain
unchanged. Plico's path rasterizer and MyGO's fragment shader use different
antialiasing; these checks do not claim pixel-identical renderer output.

Gallery: **Component kit → Corners**, with a smoothing slider and fill, border,
and clipped artwork comparisons. The lyric panel also explicitly enables it.
Rebuild the matching native library before using the new C# property:

```sh
cargo build --locked -p plico-csharp --release
export PLICO_NATIVE_LIBRARY="$PWD/target/release/libplico_csharp.so"
dotnet run --project apps/gallery-csharp -- --scene apps/gallery-csharp/scenes/components-corners.json --capture /tmp/plico-corners.png
```

Generated screenshots are local artifacts, not committed documentation assets.
