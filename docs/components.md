# Context 与独立组件

Plico 的主要入口是 Context 首参的普通函数，不要求组件实现统一 Widget trait。

```rust
use plico::prelude::*;

#[component]
fn settings(cx: &mut UiContext, dirty: bool) {
    (column(cx).gap(12).padding(16)).children(|cx| {
        text(cx, "Settings").font_size(24);
        if button(cx).text("Save").primary().disabled(!dirty).clicked() {
            // 更新应用模型。
        }
    })
}
```

UiContext 是当前声明过程的上下文，借用 Engine，管理当前父节点、作用域、主题、时间和输入。控件函数位于 plico::widgets，prelude 同时导出它们。普通应用组件可以返回 ()、Response 或自己的具体结果；调用方直接调用函数。

当前 Rust builder 在配置完成后提交：叶子在读取 response/clicked 或 Drop 时提交，容器在 children 时提交。它没有 `.show(cx)` / `add` 收尾，也不是 MyGO 那种构造时就注册、随后任意修改的通用 Element。借用期间不能再独立使用 Context；`+ |cx| { ... }` 显式重新借用 child context，原生 children 仍可用。`plico::Widget` 仅用于可组合的 immediate 辅助类型；应用组件优先使用普通函数。

## 用户组件和可配置容器

行为组件是普通函数：

```rust
#[component]
fn project_action(cx: &mut UiContext, name: &str) -> Response {
    (button_base(cx, name).padding(12).background(Color::Selection)).children(|cx| {
        text(cx, name).color(Color::Accent);
    }).response
}
```

button_base 复用按钮的 pointer、键盘、焦点和无障碍行为，本身没有默认按钮背景、padding 或最小高度。标准 button 添加主题外观；两者都能承载文字、SVG 和布局子内容。accessible label 用于语义，不重复绘制。装饰子节点不会进入 Tab 顺序；交互子控件嵌套会返回 NestedControl，避免焦点和点击归属含混。

需要可配置容器时，函数可以返回自己的 builder，提供 children 和隐藏的 __implicit_key 委托；gallery 的 components::panel 是实际例子：

```rust
(panel(cx, "Projects").padding(16)).children(|cx| {
    project_action(cx, "Plico");
})
```

这种借用 Context 的 factory 不加 #[component]，由 component 在 children 接收者位置附加调用点身份；其 children 内构建内容。普通返回值组件使用 #[component]。当前宏识别内置构造函数名称；任意重命名导入、任意返回借用 builder 的 helper 在 children 接收者位置之外还没有通用推导机制，不能宣称所有函数签名都已透明支持。

## 身份与热重载

#[component] 为静态声明与普通组件调用生成词法身份，原生 children 与 `+ |cx| { ... }` 内部同样被覆盖。Context 首参构造函数直接给 builder 附加身份，避免借用 builder 从临时作用域闭包逸出。动态列表仍使用 `cx.keyed(item.id, ...)` 或显式 `.key(item.id)`，宏无法从循环自动推导业务身份。

宏身份不来自文件行号。文字、空白等编辑可以保持身份；在前方插入新声明仍可能改变后续词法序号，不承诺任意源码重构保持节点身份。应用可序列化模型跨 reload 恢复，schema 不兼容时重置；Context、临时 builder、格式化参数与组件闭包均不作为跨模块对象保存。

## 共享主题与继承

Theme 集中提供 foreground、muted、surface、panel、control、selection、accent、on_accent、border。Metrics 提供 spacing、radius、font_size、label_size、title_size、control_height、interaction_ms；`space(n)` 是间距单位的倍数，compact 使用更紧凑的默认值。

UiContext.theme 返回 Copy 快照，自定义控件与内置控件读取同一组值。文字的字体大小、字重、前景色和 disabled 在容器中继承；子内容显式样式可以覆盖字体/颜色，禁用祖先不能由子节点重新启用。primary 按钮的文字颜色可传给子内容，离开容器后恢复原上下文。

Response 公开 clicked、hovered、enabled 和 focused；press 等瞬时绘制状态仍主要由 host 驱动。checkbox/switch 首参函数返回可配置的 Toggle builder，支持 disabled、gap、字体、尺寸和 motion；`.changed()` 或 `.response()` 读取消费结果，普通语句也会在结束时提交。它们仍不支持任意子内容的 headless base。


## 受控开关与复选框

```rust
switch(cx, "Notifications", &mut settings.notifications)
    .disabled(read_only)
    .gap(8)
    .motion(Motion::Smooth);

if checkbox(cx, "Save automatically", &mut settings.autosave)
    .disabled(read_only)
    .changed()
{
    // 输入已同步更新 settings.autosave；只在这次事件执行。
}
```

`.response()` 返回不持有借用的 ToggleResponse，可查询 changed/checked/enabled。所有配置在消费输入前生效；禁用的父容器不能被子控件重新启用。读取响应和 Drop 共用同一个提交路径，不会切换两次；checked 同时更新业务 bool 和当前声明，后续稳定化再更新依赖它的其他内容。未消费的禁用点击不会残留到重新启用后的下一帧。

Context 首参的 switch/checkbox 从旧的直接 bool 返回值改为 builder；原来 `if switch(...)` 应改为 `if switch(...).changed()`，原来的 `switch(...);` 不需要改。独立值 Switch/Checkbox.show 仍返回 bool，供 immediate 辅助类型使用。

默认指示器尺寸跟随 Metrics.spacing：默认 spacing=4 的 switch 是 36×20、checkbox 是 18×18；compact spacing=3 分别是 27×15 和 13.5×13.5。布局预留与绘制使用同一个几何定义，gap 动画会移动文字并保持指示器靠近 padding，位置受 Taffy 的像素取整影响。颜色参照 MyGO：switch 关闭轨道为 border 向 foreground 混合 15%，checkbox 未选中面为 panel、描边为 border 向 foreground 混合 25%（悬停时描边转为 accent），选中面混合到 accent；thumb 在亮色下为白色、暗色下为 #e0e1e1，SVG 勾选标记使用 on_accent。


## 图标与文字对齐

switch/checkbox 的指示器按 Parley 实际排版的首行行高居中，多行标签保持对齐首行；不使用固定 y 偏移或字号倍数猜测。SVG + Text 的可组合按钮继续由 Row 的 center 对齐处理，容忍布局盒独立取整产生的亚像素差异。

文字测量将 Taffy 的 border-box known dimensions 转为内容尺寸，绘制约束也使用扣除 padding/border 后的 content width，因此窄控件和长标签不会使用两套换行宽度。对齐在布局完成或相关 paint patch 后更新；未变的行高复用指示器 scene，纯 checked 动画不强制重排。真实截图和回归范围见 reports/alignment。


Lucide 图标可用 icon(cx, &plico::icons::SAVE) 添加；默认大小跟随继承字号，颜色继承当前前景色，也可 .color 显式覆盖。SVG 仍为装饰子内容，不制造独立交互节点。checkbox 选中态现在绘制 Lucide CHECK SVG，不再以小方块代替勾选，过渡不改变布局。

### Composed controls

`plico::widgets` and the prelude now export `badge`, `separator`, `choice`,
`segmented`, `collapsible` and `progress`. These are ordinary `#[component]`
functions implemented using public Context-first constructors and children closures;
there is no new renderer widget kind or persistent callback.

```rust
use plico::prelude::*;
#[component]
fn release(cx: &mut UiContext, channel: &mut usize, details: &mut bool) {
    segmented(cx, channel, &["Stable", "Beta", "Nightly"]);
    badge(cx, "Preview");
    separator(cx);
    collapsible(cx, "Details", details, |cx| {
        progress(cx, "Validation coverage", 0.75);
    });
}
```

- `choice(cx, label, &mut selected, value)` supports any `PartialEq` value and
  returns the normal button response. For dynamic lists, wrap each call in
  `cx.keyed(business_id, ...)`. Disable an enclosing container to disable choices.
- `segmented` is a wrapping selected-button group, with a fixed ordered label
  slice and an application-owned index. Empty lists leave the index alone;
  invalid indices show no selection. Every button participates in Tab order;
  Enter/Space select. It does not claim radio/tab semantics or arrow navigation.
- `collapsible` accepts an application-owned bool and returns `Option<R>` from
  its immediate content callback. Enter/Space toggle its button; assistive
  technology receives `expanded`. Following MyGO, its indicator crossfades and
  its measured panel grows/shrinks behind a clip over 200ms. Closing content is
  retained only for the exit, immediately disabled and hidden from semantics,
  then unmounted; reduced motion mounts/unmounts immediately. Keep durable state
  in the application. Inside `accordion`, Up/Down/Home/End move between headers
  and clamp at the ends.
- `progress` shows a labeled fraction and percentage, clamps finite values to
  0..=1 and maps non-finite values to zero. Its composed root exposes a native
  progress-bar role, accessible name and numeric 0..=1 range. Fill animation is
  not implied.
- Theme spacing, typography, density and colors are read each declaration.
  Choice color transitions use the existing injected-clock host motion system.

Reference comparison: local MyGO `ui/tabs.go` and `ui/collapsible.go` separate
behavior from appearance and keep selected/open state with the application.
Its tabs also implement arrows/Home/End; accordion header traversal is shared
with the runtime focus-group path. Disclosures use the shared scalar animation
and measured clip described above. Local Wabou
`crates/plico-shell/src/widgets/controls.rs` uses native painting/event handling
for slider and progress controls. Plico's simple meter currently needs only
layout primitives; slider dragging, text editing, scrolling and overlays still
need dedicated input/layout infrastructure. No source was copied for this batch.

### Navigation and indicators (MyGO ports)

`tree`/`tree_item` use one sequential keyboard stop per visible row; the
pointer disclosure arrow does not add another Tab stop. Up/Down/Home/End,
Right/Left parent-child traversal and one-second buffered prefix selection
follow MyGO's visible-tree behavior. The application continues to own open and
selected values. `sidebar` enables the source-list variant of the same host
policy: only its selected item is in sequential Tab order, and arrow/Home/End
or prefix movement activates the destination so the application's selected
value changes. Section disclosures are not mixed into that item sequence.

`breadcrumbs`, `meter`, `spinner` and `rating` port the local MyGO
`ui/feedback.go` breadcrumb path and `ui/indicators.go` Meter, Spinner and
Rating. They are ordinary context-first functions over public constructors;
there is no new renderer kind or persistent callback.

```rust
use plico::prelude::*;
#[component]
fn status(cx: &mut UiContext, path: &mut usize, disk: &mut f64, stars: &mut u8) {
    breadcrumbs(cx, &["Home", "Users", "Documents"], path);
    meter(cx, "Disk", *disk, 0.0..=100.0, MeterLevels {
        warning: Some(80.),
        critical: Some(95.),
    });
    spinner(cx);
    rating(cx, stars, 5);
}
```

- `breadcrumbs(cx, items, &mut chosen)` renders a path with chevron
  separators: every item but the last is a link and sets `chosen` on click;
  the last item is plain text. It returns whether the choice changed, clamps
  out-of-range choices and leaves an empty path alone. The row carries the
  navigation role and the parents the link role.
- `meter(cx, label, value, range, levels)` shows a labeled percentage bar.
  Below `warning` it is success colored, from `warning` warning colored and
  from `critical` danger colored; a critical level below the warning level
  makes low values the bad ones, as a battery's. Non-finite values show an
  empty bar. For finite ranges the container exposes its accessible name and
  clamped min/max/value through the native meter role.
- `spinner(cx)` draws twelve muted dots fading in turn every 900 ms sized from
  the theme's font size, and requests animation frames while declared. MyGO
  strokes rotated spokes and names the control with `.Label(...)`; Plico's
  spinner has no label parameter, so place explanatory text beside it.
- `rating(cx, &mut value, max)` renders `max` stars, `value` filled in the
  warning color. A click sets that star and a click on the set star clears the
  rating. Hover previews the pending choice. The stars are pointer-only visual
  parts of one slider semantic control: it occupies one Tab stop, arrows step
  by one, Home clears and End chooses the maximum. Values above `max` are
  clamped and the clamp is reported as a change. This is built on
  `UiContext::value_control`, which lets other custom numeric widgets reuse the
  same value queue, focus, keyboard and accessibility behavior. Use
  `rating_labeled` when the accessible name needs application context.
- `stepper(cx, &mut value, range, step)` puts two arrow buttons in a bordered
  column, snapping the value to a step grid anchored at the low bound and
  clamping it to the range. A press steps once, and holding repeats after
  400 ms every 80 ms; the release adds no extra step, while keyboard activation
  steps once. The container is one spin-button focus stop: Up/Right increment,
  Down/Left decrement and Home/End choose the bounds. The pointer arrows are
  excluded from accessibility because the parent owns the value semantics.
  `stepper_labeled` supplies the caller-specific accessible name; `stepper`
  keeps `Stepper` as its convenience default.

The split panes (`split`, `split_vertical`, `split_percent`,
`split_vertical_percent`) are documented in [desktop widgets](desktop-widgets.md).

### Color picker (MyGO port)

`color_picker(cx, &mut Color)` ports MyGO's `ui/colorpicker.go`: a preview
swatch with its hex value, Hue/Saturation/Brightness sliders and twelve named
swatch buttons. It returns whether the color changed and writes
`Color::Literal` on the first edit; theme tokens are edited from the color they
currently resolve to. The hue of a gray is remembered in transient state, as
MyGO does, so saturation can return to the last hue.

MyGO's saturation/brightness square, opacity slider and editable hex field are
not ported: Plico exposes no local pointer coordinates for a custom-painted
square, `Color` carries no alpha, and the hex text is therefore read-only. The
sliders are continuous rather than snapped, so deriving a color from its RGB
does not drift it.

`color_well(cx, label, &mut Color, &mut open)` adds MyGO's `ColorWell`: a
swatch button showing the color and its hex value, with the color-well role and
expanded state, that unfolds the picker below itself while `open`. It returns
whether the picker changed the color. MyGO floats the picker in a popover and
exposes the hex as the well's value; Plico has no custom popover trigger, so
the picker unfolds in place and the hex stays visible text.

### Form fields (MyGO port)

`form`, `field` and `fieldset` port MyGO's `ui/form.go` composition:

```rust
form(cx, |cx| {
    let _ = field(cx, "Email")
        .description("For receipts.")
        .error(if valid { "" } else { "Enter an email address." })
        + |cx| { text_input_string(cx, "Email", &mut email); };
    let _ = fieldset(cx, "Notifications", |cx| {
        let _ = field(cx, "Sound") + |cx| { switch(cx, "Sound", &mut sound); };
    });
});
```

- `field(cx, label)` renders the label, the control its content callback builds
  (preserving the callback's result) and the non-empty description/error texts
  below it; the error uses `Color::Danger`.
- `form` is the spaced column of fields; `fieldset` adds a bold legend.
- MyGO puts a form's labels in a right-aligned column as wide as the widest
  label. Plico's immediate API has no intrinsic measuring pass available to a
  widget, so the port stacks labels above their controls (MyGO's standalone
  `Field`) and does not implement the label column. A label click does not
  focus its control, and the control is not marked invalid by its field's
  error yet.

### Radios, links and history (MyGO port)

`radio`, `radio_group`, `link`, `back_button` and `forward_button` port MyGO's
`Radio`/`RadioGroup` (`ui/widgets.go`), `Link` and the router history buttons.

- `radio(cx, label, selected)` draws the dot and label and returns the normal
  response; the caller owns the chosen value and sets it on `clicked`. The
  button carries the radio-button role and its selected state.
- `radio_group(cx, content)` is the column with the radio-group role and one
  sequential Tab stop. Tab enters at the chosen radio (or the first enabled
  one); either arrow axis wraps through enabled radios and chooses, while
  Home/End choose the first/last. Enter/Space still activates the focused
  radio.
- `link(cx, label)` is accent-colored text with the link role; the declaration
  site decides what a click does, because Plico core does not open URLs. Plico
  text has no underline, so the hover background is the affordance.
- `back_button` / `forward_button` take a `Router`, disable at the ends of the
  history and step it on click. MyGO's right-click history menu and the
  Cmd+[ / Alt+Left shortcuts are not part of the widgets.

### Alert dialog (MyGO port)

`alert_dialog(cx, &mut ModalState, title, message, buttons)` ports MyGO's
`AlertDialog`: while the state is open it renders a modal alert with the
title, message and buttons, the last of which is the accent-colored default.
It returns the clicked button's index, `ALERT_DISMISSED` (-2) in the frame
Escape closes it, and -1 otherwise; a click outside does nothing. The panel
carries the alert-dialog role and is named by its title.

Two differences: Plico cannot move the keyboard focus onto the default button,
so Enter only answers after Tab reaches it; and because Plico may run a
settlement declaration pass after the modal closes, the returned index must be
handled in the same declaration pass (`match` in the view), not stored for
later.

### Time input (MyGO port)

`time_input(cx, label, &mut TimeOfDay)` ports MyGO's `TimeInput` as a
validated `HH:MM` text field over the new `TimeOfDay` value
(`TimeOfDay::new`/`from_minutes`/`minutes`, chronologically ordered and
serde-validated). Typed text applies as soon as it parses (`15:04`, `9:5`,
`1504` and `904` are accepted); Enter rewrites it in canonical form, and
Enter with invalid text restores the current value. An application change
rewrites the field.

MyGO edits hours, minutes and AM/PM as separately focused segments, follows
the regional hour cycle and formats on blur; Plico has no per-segment focus
or blur hook, so the port is a single 24-hour field and invalid text stays
visible until Enter.

### Sticky header (core extension)

`Layout::sticky(top)` ports MyGO's sticky-scroll behavior for the top edge:
the node stays in normal flow, but after layout the runtime keeps its top at
least `top` logical pixels below the nearest scrollable ancestor's port,
bounded by its containing block. Its descendants move with it, and hit
testing, semantics and painting read the shifted geometry.

This is a top-edge sticky constraint only. MyGO's layout also covers other
edges and offsets; Plico currently implements top only, and a sticky node
still occupies its flow slot (as CSS sticky does).

### Range slider (MyGO port)


`range_slider(cx, label, &mut low, &mut high, range)` ports MyGO's
`RangeSlider` on the shared slider core: one track with a low and a high
thumb, the accent fill between them, and the thumbs cannot cross. Pressing
the track picks the nearer thumb; arrows, PageUp/Down, Home and End move the
thumb last touched, and Tab focuses the control.

MyGO gives each knob its own focus stop and exposes two named sliders to
assistive technology. Plico's slider core still has one focusable entity and
one numeric semantic value per slider, so both thumbs share the focus and the
semantic node reports the low bound; the high bound is painted but not yet a
separate accessible slider.

### Number input (MyGO port)


`number_input(cx, label, &mut f64, min, max, step)` ports MyGO's
`NumberInput`: a bounded text field with − and + step buttons. Typed text
applies as soon as it parses inside the bounds; Up/Down and the buttons move by
one step, with the bound button disabled. Enter or leaving the field rewrites
it with the decimals implied by the step; invalid or out-of-range text restores
the current value. Values clamp and step-round to the bounds, and an application
change rewrites the field. Decimal amounts still need a decimal type, as in MyGO.

### Focus traversal

The runtime now exposes the backend focus to declarations: `UiContext::focused()`
returns the focused entity, and `Response::focused()` /
`InputResponse::focused()` report it for the control just declared. Controls
inside a tab strip, radio group, toolbar, tree, list box or menu move focus with
the arrow keys, Home and End; Tab keeps the sequential traversal. Tree and
radio groups use roving Tab order, so only their current item participates in
sequential traversal; radio movement also chooses the focused option. The
focused entity is synchronized from the backend at the start of each frame, so
a declaration reads the previous frame's focus.

### Date input (MyGO port)


`date_input(cx, label, &mut CalendarDate, &mut PopoverState)` ports MyGO's
`ui/dateinput.go`: a pop-up button showing `YYYY-MM-DD` whose popover holds the
shared `calendar_content` widget. Opening it keeps the shown month; choosing a
day writes the date and closes the popover, and the return value reports the
change. `date_input_with` takes the first day of the week (the plain entry
point uses ISO Monday). The calendar's month/day state is transient and never
enters checkpoints.

MyGO writes the user's regional numeric date and edits a `time.Time`, keeping
the time of day; Plico dates carry no time and the localization layer exposes
no numeric date order, so the field shows ISO and a time input would be
composed separately.

### Find bar (MyGO port)

`find_bar(cx, &mut open, &mut query, matches, &mut current)` ports MyGO's
`FindBar` while `open`: a toolbar-role bar holding the search field (with the
magnifier and clear button), the "N of M" status, previous/next buttons and
Done. The application owns the query and counts the matches; `current` is
clamped to them and moves round the ends, and the return value reports that
movement. Opening the bar focuses its field; Enter in the field moves to the
next match.

Escape and Cmd+G / F3 (Shift variants for the previous match) are not wired:
Plico does not route those shortcuts out of the text input yet. A closed bar
declares nothing, so the caller's `open` flag is the single source of truth.

Theme gained `Success`, `Warning` and `Danger` tokens (light and dark values
follow MyGO's theme) for the meter fill and filled stars. Themes serialized
before the addition still deserialize with those defaults.

### Desktop style geometry

`padding_x` and `padding_y` override their respective axis; a later `padding`
resets both overrides. `border(width, color)` contributes to layout and painting,
while changing only its color does not remeasure. `clip(true)` uses the backend's
rounded overflow clipping and excludes clipped controls from pointer hit testing.
`min_height` is available on leaf/control builders.

Theme includes ControlHover/ControlPressed, AccentHover/AccentPressed and Focus.
Control/Accent button backgrounds infer their corresponding state colors;
`hover_background` and `pressed_background` explicitly style custom button bases.
Disabled controls skip interaction colors. Keyboard/assistive focus shows a
2px focus ring with a 1px offset; primary pointer-down preserves focus without
showing the ring. Text inputs keep their fill while the border turns accent on
focus and mixes 25% toward the text under the pointer, following MyGO's
`inputBorder`.
Axis padding transitions use the injected clock without view redeclaration.

Desktop metrics now default to 14px text, 6px radius, 32px minimum controls;
compact uses 13px, 5px and 28px. Actual content may require greater height.
The neutral surfaces, thin borders and separate button axis padding follow the
local MyGO theme/widgets/gallery reference, with Plico's own implementation.

控制样式按组合而不是逐控件复制：控件只声明 `button_base`、`text_input`、
`radio`、`switch`、`slider` 等语义角色，运行时的共享 recipe（quiet border、
switch 轨道、checkbox 面板与描边、thumb 面、input 边框）负责 MyGO 的混色规则，
应用只覆盖 token 或 builder 颜色。新增控件组合这些 recipe 即可，不需要再写一套
主题常量。

### Bounded vertical scroll ports and native state

`column(cx).height(240.).scroll_y(true).children(|cx| { ... })` creates a
bounded vertical viewport. In a flex column use `.height(0.).grow(1.)`.
The host owns offsets; wheel deltas chain through ancestors at a boundary,
thumb dragging and track clicks work, and keyboard/assistive focus reveals the
target. Child removal and viewport changes clamp offsets. Scrolling re-flattens
retained geometry and invalidates the scene without running Taffy. Current wheel
handling still goes through the normal immediate declaration path. This is not
virtualization; horizontal scroll, inertia and auto-hide are not implemented.

`plico_desktop::run_view_with_options` accepts a `View` without serialization.
Service handles and sensitive application data remain ordinary host-owned Rust
state. `plico dev` recompiles and restarts the static executable; no dynamic
loader, App reload protocol or ABI feature is involved.

### Generic font families

Text, control and layout builders expose `.font_family(FontFamily::Monospace)`
(and `SansSerif`, `Serif`, `Default`). Layout and button content inherit the
family; `Default` resolves to the configured window family. The Vello runtime
uses the selected family for both text measurement and painting, and keeps
unchanged per-node font choices in its retained paint cache.

```rust
use plico::prelude::*;
fn code(cx: &mut UiContext) {
    column(cx).font_family(FontFamily::Monospace).children(|cx| {
        text(cx, "let value = 42;");
    });
}
```

## Avatar 与 ruby 注音

两个展示组件从 Lyrune 的实际用法中整理而来，位于
`plico::widgets`，也由 prelude 导出。它们使用普通借用 builder，与基础
控件相同，在语句结束时提交；保持两个生命周期参数，支持 component 宏自动 ID。
Gallery 的 Controls 页有 “Avatars & ruby text” 示例。

```rust
use plico::prelude::*;

avatar(cx).image(cached_image).fallback("JD").size(40.);
ruby_text(cx, "夜空").annotation("よぞら").font_size(28);
ruby_text(cx, "へ").font_size(28);
```

`cached_image` 是 `Option<&ImageAsset>`，也可直接传 `&ImageAsset`。没有图片时
显示调用方提供的占位文字；图片加载、失败重试、账户/缓存生命周期仍由宿主管理。
头像裁成圆形，提供字号、前景/背景色配置。

`ruby_text` 默认继承正文的字号、字重和颜色，注音使用主题 label 字号与 Muted
颜色。注音槽即使没有 reading 也保留，因此同一行混排时正文不会上下跳；默认
高度随注音字号变化，也可用 `annotation_height` 固定。正文 `.spans()` 的范围
是当前片段的本地 UTF-8 字节偏移；歌词解析、全行到片段的范围转换仍留在应用。
支持单独配置注音字号、字重和颜色，不绑定歌曲时间轴或特定业务模型。

### Internal widget state

Number, date, time and color inputs retain their edit state in `cx.state` slots,
identified by the caller and component scope. Drag surfaces, split dividers and
stepper holds keep local slots under their control node. These slots are not
serialized and are reclaimed when no longer declared. Repeated controls in a
loop require `cx.keyed` with a stable item key, as do other stateful components.
Range sliders retain their active thumb per control; removing a slider also
removes its thumb selection. The two thumbs still share one focus stop and
only the low value is exposed in the accessibility tree.

默认控件外观和组合边界见 [MyGO 样式对齐](mygo-style.md)。工具栏内直接使用普通 `button`，不需要逐个 `.ghost()` 或指定投影。
