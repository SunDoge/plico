# C# component kit

C# owns widget composition, visual tokens and interaction policy. Rust owns
native text editing, focus, pointer capture, layout, resources and painting.
Components submit the existing flat batch; adding this kit did not add a native
opcode or retain managed widget callbacks in Rust.

The reference is [MyGO's UI package](https://github.com/egoist/mygo/tree/main/ui):
`widgets.go`, `indicators.go`, `feedback.go`, `collapsible.go`, `form.go`, `number.go`
and `tabs.go`. Plico's implementations are independently written C# compositions,
not a vendored Go implementation. MyGO's source is MIT licensed; this is a design
reference, not a claim of API or behavior parity.

## Available components

| Category | Public SDK API |
| --- | --- |
| Layout and surfaces | `Row`, `Column`, `Box`, `Card`, `Field`, `Divider` |
| Actions and selection | `Button`, `ButtonContent`, `Checkbox`, `Switch`, `Radio`, `Chip`, `Tag`, `Segmented` |
| Navigation | `Tabs`, `Breadcrumbs`, `Pagination`, `Collapsible`, `MenuButton` |
| Text and value editing | `TextInput`, `TextArea`, `PasswordField`, `SearchField`, `NumberInput`, `TimeInput`, `FindBar` |
| Choice and dates | `Select` (optional search), `Calendar`, `DatePicker` |
| Numeric controls | `Slider`, `RangeSlider`, `Stepper`, `Rating`, `ColorPicker` |
| Feedback | `Progress`, `Meter`, `Spinner`, `Badge`, `Alert`, `Notification`, `AlertDialog`, `EmptyState`, `Skeleton` |
| Data and resources | `VirtualList`, `Table<T>`, `Avatar`, `Image`, `ImageCanvas`, SVG and WGSL surfaces |
| Overlays and input | `Popup`, `Modal`, `DragArea` |

Gallery's **Component kit** page shows surfaces/feedback, navigation and forms.
The existing **Desktop patterns** page uses the same public field, breadcrumb,
range, disclosure and confirmation components. **Data table** uses `Table<T>`.
The tree view, application sidebar, resizable split and timed toast queue remain
Gallery compositions rather than falsely advertised full SDK widgets.

## Composition and local state

```csharp
public static class Views
{
    [Component]
    public static void Account(Ui ui)
    {
        ref string email = ref ui.State(static () => "");
        ref bool expanded = ref ui.State(static () => false);
        using (ui.Card("Account", "Settings stay on this device."))
        {
            using (ui.Field("Email", "Used for receipts."))
                ui.TextInput("Receipt email", ref email).FillWidth();
            using (var section = ui.Collapsible("Advanced", ref expanded))
                if (section.IsOpen)
                    ui.Text("Ordinary immediate children.");
        }
    }
}
```

`Card` and `Field` are ordinary disposable containers. `Collapsible` requires an
explicit `IsOpen` check, so closed children are not declared. Their local state
unmounts with them. Native buttons retain Enter/Space activation; there is no
stored user closure. Direct component calls use the SDK Interceptors identity
injection. Dynamic collections still need business keys.

Typed value fields keep incomplete/invalid text in component-local edit slots.
For example, typing `-` does not overwrite a valid application `double`. External
value changes replace the edit buffer, and unmounting discards incomplete edits.
`NumberInput` accepts invariant numeric syntax. `TimeInput` displays 24-hour
`HH:mm`; it does not discard existing seconds merely because it was declared.
These controls do not yet implement MyGO's blur normalization or spinbox arrow
key bindings.

## Cost boundaries

- `Pagination` declares a bounded window, even with billions of pages.
- `Table<T>` uses the shared fixed-height `VirtualList`. Formatters and business
  keys run for mounted rows only; sorting/filtering and durable row data stay
  with the application. Header clicks return a column index. Row text clips at
  the fixed row height. There is no variable-height row measurement.
- Native input/capture and popup dismissal are reused. Menus carry stable command
  IDs, reject duplicates and cannot activate disabled items.
- `Skeleton` uses a WGSL shimmer driven by the native clock. It does not request
  a managed declaration every animation frame. Pass `animated: false` when the
  application wants static/reduced-motion placeholders.
- Theme tokens include distinct light/dark informational, success, warning and
  danger colors. No global application reference is needed by a widget.

## Deliberate limits

This is common desktop component coverage, not all of MyGO:

- `Tabs`, `Segmented` and `MenuButton` currently use native button Tab/Enter/Space
  behavior, not roving arrow-key focus or complete tablist/menu accessibility.
- `Collapsible` has native button focus and an SVG disclosure arrow, but no exit
  transition or expanded-state accessibility metadata yet.
- `Field` supplies visible labels/help/errors; pass the input's accessible name
  explicitly. It does not invent a native label association.
- `RangeSlider` is two labeled sliders, not a shared dual-thumb track.
- `ColorPicker` edits RGB channels, without HSV, alpha or an eyedropper.
- `Notification` is caller-owned inline dismissible feedback. A timed toast queue,
  placement, action callbacks and background service policy are not implied.
- `Table<T>` is a virtualized text table, not a spreadsheet/accessible grid with
  cell selection. A tree with complete arrow-key navigation, tooltips, context
  menus, rich code editing, charts and drag/drop lists remain separate work.

## Verification

The SDK Demo's `--self-test` covers same-call-site component isolation, menu Escape
and disabled commands, pagination bounds, disclosure state disposal, search clear,
numeric partial edits/external replacement, time validation, RGB changes, find
wraparound, confirmation dismissal and bounded declaration of a 10,000-row table.
Gallery's self-test covers navigation and kit interactions. These are native
headless protocol checks; they do not require OS window automation.

Run from the repository root with `PLICO_NATIVE_LIBRARY` pointing at the matching
native library:

```sh
dotnet run --project crates/plico-csharp/dotnet/Demo -c Release -- --self-test
dotnet run --project apps/gallery-csharp -c Release -- --self-test
dotnet run --project apps/gallery-csharp -c Release -- --scene apps/gallery-csharp/scenes/components.json --capture /tmp/plico-components.png
```

Scene fixtures also cover forms, navigation and a 760-pixel-wide stacked layout.
Generated screenshots stay under `reports/` or a temporary directory and are not
committed. Linux previews were visually inspected in both themes; this does not
claim macOS/Windows window validation.
