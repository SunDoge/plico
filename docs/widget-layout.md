# Icon and label layout

Default controls align icon boxes with the shaped text line box. No font-specific
pixel nudges are applied. This rule is shared by button shortcut content and
custom children; an SVG's own asymmetric viewBox may still need correction in
the asset rather than an arbitrary offset in the widget.

Use `icon(cx, asset)` for UI icons: it follows the inherited font size plus two
logical pixels and does not shrink in flex layout. `svg(cx, asset)` remains the
24×24 general asset constructor; `.size(...)` explicitly overrides either one.
Standalone `widgets::Button` and context-first button icon shortcuts use the
same content implementation. Menu icons follow inherited text size as well.

Button shortcut labels and menu/selection labels stay on one line and ellipsize
when constrained. Their full accessible names remain intact. Use a tooltip or
another visible disclosure if users need to inspect the full truncated value.
Custom `button(cx) + |cx| { ... }` content remains freely configurable, including
multiline text. Default children center alignment handles ordinary icon/text
rows; unstyled `button_base` leaves appearance and alignment to the component.

Leaf buttons and toggle text blocks center vertically within the available
content height. Wrapped toggle labels keep the indicator beside the first line,
not the middle of the paragraph. Paint-only animation frames recompute placement
from box geometry so offsets cannot accumulate. Checkbox unchecked state uses an
outlined empty box; checked state interpolates to the theme accent with a check.

Gallery Controls includes mixed Chinese/Latin labels, descenders, larger type,
and constrained labels. Its body scrolls when the window cannot fit all samples.

Validation:

```sh
cargo test -p plico-vello --tests
cargo run -p plico-gallery --example capture -- --page controls --out /tmp/controls.png
cargo run -p plico-gallery --example capture -- --page controls --dark --out /tmp/controls-dark.png
cargo run -p plico-gallery --example capture -- --page controls --large --width 640 --height 780 --out /tmp/controls-narrow.png
```

Alignment tests cover density, font sizes, wrapping, resizing, tall controls,
paint-only frames, label truncation and preservation of accessible names. These
are component-layout guarantees, not a claim that all application surfaces have
completed a visual/accessibility audit.
