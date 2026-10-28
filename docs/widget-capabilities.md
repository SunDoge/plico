# Orthogonal widget input capabilities

`Kind` identifies built-in nodes. `Kind::capabilities()` describes the input
behaviors they support, as a compile-time `Capabilities` bitmask:

| Capability | Meaning |
|---|---|
| `ACTIVATE` | An activation action, such as a button press or backdrop dismissal |
| `FOCUS` | Can receive keyboard focus |
| `TAB_STOP` | Participates in ordinary Tab navigation; implies `FOCUS` |
| `TEXT_EDIT` | Accepts text editing through the editor protocol |
| `TEXT_SELECT` | Supports text selection |
| `VALUE_EDIT` | Accepts numeric-value edits through the numeric protocol |

A dialog has `FOCUS` without `TAB_STOP`: modal focus can land on an empty dialog,
but the dialog itself must not join its children's ordinary Tab cycle. An overlay
can activate without taking keyboard focus. A text label can support selection
without becoming a nested interactive control. Editors and sliders are focusable
without acquiring a generic button activation action.

Capabilities describe support, not live state. `disabled`, current focus, selection,
checked state and modal isolation remain separate. Ordinary paragraph selection
still requires the existing `.selectable(...)` opt-in; the input editor retains its
own selection protocol. A capability does not promise that every backend implements
it. `VALUE_EDIT` does not replace the typed numeric range, and `TEXT_EDIT` does not
replace TextBuffer.

## Representation and composition

The mapping is `const fn`, uses the existing bitflags dependency, and stores no new
field in retained nodes or the reload protocol. No registration, allocation, virtual
call, shared ownership or lifetime change is introduced. Most custom widgets remain
ordinary functions combining existing nodes and builders.

Layout/children, visual content and semantic roles are not input flags. They remain
in their existing typed representations. A composite ButtonBase can contain text
and SVG children, expose a button/menu-item role, and activate independently of its
layout. `Kind::interactive()` remains a compatibility predicate for primary pointer
and control interaction; focus/navigation/activation callers should use the specific
capability they need. Selection alone does not make it return true.

## Backend behavior

Vello uses `TAB_STOP` for navigation, `FOCUS` for focus validity/restoration, and
`ACTIVATE` for ordinary pointer/key/accessibility activation. An assistive click on
a focusable, non-activatable editor or slider becomes a focus request, not a queued
application click. Existing native editing, numeric adjustment, selection and modal
protocols remain responsible for their own behavior.

`KindFamily` and `Kind::family()` remain only for source compatibility. No backend
uses them to select widgets, Flex layout, or fallbacks. They mixed layout, content
and interaction, and cannot define a reliable universal fallback. Serialized Kind
values and frame/node formats are unchanged.

The LVGL adapter now has an explicit backend-local object-class mapping. Its Flex
container decision comes from this native representation, not a core input family.
Kind changes mark `STRUCTURE`, even when Entity stays stable. Changing a native
class remounts the object and its retained descendants, children before parents;
a labeled button and a bare composite button are distinct representations. Queued
events from the replaced native instance are discarded. Unimplemented Switch, Checkbox, Slider, Image and
Shader return `UnsupportedKind`, rather than silently becoming inert containers.
Full sync and incremental patches preflight unsupported kinds and NUL text before
modifying native objects, preserving the previously mounted tree on those errors.
This is not a general rollback guarantee for allocation/FFI failures.

LVGL remains an experiment: its container mapping for Overlay/Dialog/TooltipAnchor
preserves structure, not Vello's modal/tooltip services. See [LVGL limitations](lvgl.md).

## Validation

Core tests distinguish selection, editing, activation, programmatic focus and Tab
stops. Vello tests cover focus order, disabled controls, editor/slider assistive
focus, checkbox activation, button activation, plus the existing editor, numeric,
selection, tooltip and modal regressions. LVGL tests exercise rejection before
mounting and incremental preflight without partial text updates.

```sh
cargo test -p plico -p plico-dev
cargo test -p plico-desktop --test capabilities --test modal --test popover \
  --test slider --test text_selection --test text_input --test select_menu \
  --test button_content --test tooltip
DEP_LV_CONFIG_PATH="$PWD/crates/plico-lvgl/config" \
  cargo test -p plico-lvgl --features native --lib
```

## Declaration cost

Small builder factories, setters and key forwarding use explicit inlining so
by-value chains do not depend on incidental cross-crate optimizer decisions.
Ownership, builder layout and lifetimes are unchanged. Existing allocation probes
continue to assert zero allocations for unchanged borrowed labels and buffered
text after warm-up.

The reproducible comparison runner takes two prebuilt `release/examples` folders:

```sh
python3 scripts/compare-core-probes.py /path/to/before/release/examples \
  /path/to/after/release/examples --rounds 6 --out reports/core-probes
```

Compile comparable source snapshots with the same compiler, profile, features and
node definitions. The runner alternates versions and pins to one allowed CPU on
Linux. It records raw probe output and reports the median of each process's p50.
It is a CPU declaration/commit comparison, not a GPU or end-to-end latency test.

On this Linux host, four alternating rounds against the fixed `80341a3` core
baseline put the ordinary borrowed/format!/text! cases within roughly -4% to +2%
(the 10,000-row text! case was +1%). Style churn was about 5% faster; reverse order
and insert/remove were about 22% faster. These are local observations, not hard
cross-platform guarantees or proof that all small regressions are excluded. No
new per-node capability storage, shared ownership or allocation was introduced.
These measurements isolate the capability/inlining work; they do not benchmark
the concurrently developed Image path. New kinds are appended to preserve the
existing discriminant fast paths.
