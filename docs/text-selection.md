# Read-only text selection

Mark a container with `.selectable(true)` to enable selection of its ordinary
text descendants. Buttons, inputs and other interactive controls keep their own
interaction handling. Selection stays within its selectable owner and respects
modal eligibility and visible transformed bounds.

The current shared runtime supports character dragging, Shift-click extension,
double-click word selection and triple-click logical-paragraph selection. Dragging
after a repeated click extends by whole units, preserving the initial word or
paragraph when direction reverses. Repeated clicks use the injected clock, a
500 ms interval and a four-logical-pixel radius, with pointer identity/category
checks; dragging away, cancellation or focus loss resets the click sequence.
These thresholds are currently framework defaults, not OS preference queries.

Endpoints snap to Unicode extended-grapheme boundaries, avoiding partial combining
sequences and ZWJ emoji. Word units use Unicode UAX #29 segmentation, not a CJK
word dictionary. Triple-click paragraphs stop at explicit Unicode hard separators;
they do not mean a single soft-wrapped visual line. Unicode segmentation does not
implement complete bidi visual caret affinity/navigation.

Ctrl/Cmd+C requests clipboard copying; Ctrl/Cmd+A selects the current owner group,
and Escape clears it. The host performs the system clipboard operation. Geometry
comes from the same native/Parley paragraph used for drawing. Linux Pango's
x/y/width/height rectangles are converted to the shared left/top/right/bottom
contract. Selection changes invalidate the affected retained paint nodes and
refresh the backend even when no application declaration is requested.

The design follows MyGO's unit-based anchor/drag approach; source and MIT notice
are recorded in `crates/plico-runtime/SOURCES.md` and its licenses directory.
Text-input/editor/terminal selection remains owned by those controls; this change
adds read-only text interactions and does not replace their editing models.
Auto-scroll during an edge drag, touch handles and full visual bidi navigation
remain future work.

## Try it

```sh
cargo run --locked -p plico-desktop --no-default-features --features wgpu --example wgpu-window
```

The panel is selectable, including Chinese/Japanese, combining accents and emoji.
Use drag, double/triple click and Ctrl/Cmd+C. The same runtime works across renderers;
the new native pixel/copy regression executes on Linux Pango + wgpu at 1x, 1.25x
and 2x. Windows/macOS native interaction acceptance is not established by this test.
