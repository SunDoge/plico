# C# outer shadows

```csharp
using (ui.Column().Radius(20).CornerSmoothing()
    .Background(0xffffff).BackgroundAlpha(.78f)
    .Shadow(y: 4, blur: 20, spread: -2, color: 0x43354f, opacity: .075f))
{
    ui.Text("A soft surface");
}
```

`Shadow` is a shared primitive paint property exposed on both Builder and Container.
It sets one outer shadow; repeated calls replace it. `NoShadow` clears it.
Coordinates, offsets and spread are logical pixels; blur follows CSS/MyGO radius
semantics, converted once to Gaussian sigma (`blur / 2`) at the adapter. Blur is
0–128, opacity 0–1, color is RGB, and offset/spread magnitude is bounded to 16384.
Changing a shadow invalidates paint, not layout. Native schema revision is 3;
rebuild the native library once and restart an older process.

The box interior is cut out using its actual continuous/circular corner contour.
A translucent background therefore does not expose a dark rectangle beneath it.
Empty shadow geometry after negative spread draws nothing. Damage bounds still
include the shadow's displacement, spread and blur extent. The blur caster itself
uses the existing rounded-box primitive; this is not MyGO's analytical continuous
shadow shader, and large continuous corners may differ at their outer penumbra.

Read-only references: MyGO `ui/element.go` (outer-only shadow semantics),
`ui/widgets.go` (small contact shadows and larger lifted controls), and
`internal/gpu/gpu.go` (blur-to-sigma conversion). The renderer implementation uses
Plico's existing box shadow primitive with a reversed-contour clip; no MyGO source
is copied. Parameters and elevation roles stay in C#, not Rust widget presets.

The Warden preview uses quiet tinted shadows on panels, tighter contact shadows
under icon tiles and restrained shadows on round toolbar controls. The static
WGSL background remains static. Reports/screenshots are local outputs only.

Tests cover shadow property serialization including zero fields, removal,
invalid ranges, paint-only invalidation, high property bits and outer clip
winding/continuous corners. Plico's style property mask now uses u128 because the
new property crosses its former 64-field limit; unrelated fields must not alias.
