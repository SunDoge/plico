# MyGO 组件移植覆盖

本文对照本机 MyGO `docs/ui` 的组件清单，记录 Plico 已移植的控件、由既有子系统覆盖的项，
以及受当前核心能力限制而暂缓的项。判断标准是“能否用 Plico 现有的公开 immediate API
忠实表达”，而不是逐字复制 MyGO 源码。

## 已移植（MyGO 对齐的控件）

| MyGO | Plico | 说明 |
|---|---|---|
| Tabs | `tabs` | 选中态、Tab/TabList 角色；方向键/Home/End 移动焦点 |
| Toast | `ToastHost` / `toast_viewport` | 存在动画、悬停暂停、同 ID 续期、transient |
| Toolbar | `toolbar` / `toolbar_spacer` | 换行与推动尾部操作 |
| Drag and drop | `drag_area` | 按下期间的逐帧位移；宿主文件拖放另由事件提供 |
| Breadcrumbs | `breadcrumbs` | Navigation/Link 角色，末项为当前项 |
| Meter | `meter` / `MeterLevels` | 阈值着色，支持低值更差的降序阈值 |
| Spinner | `spinner` | 注入时钟驱动的渐隐点圈 |
| Rating | `rating` | 每星独立按钮，选中/取消与夹取 |
| Stepper | `stepper` | 吸附、夹取、按住 400 ms 后每 80 ms 重复 |
| Split | `split` / `split_vertical` / `split_percent` / `split_vertical_percent` | divider 拖动，像素或百分比 |
| Sidebar | `sidebar` / `sidebar_section` / `sidebar_item` | tree 角色、section 展开卸载、图标与选中 |
| Search field | `search_field` | 放大镜、清空、Enter 提交 |
| Tree | `tree` / `tree_item` | 显式 depth、展开卸载、树角色 |
| Color picker | `color_picker` | H/S/B 滑杆与色板；无二维方块/透明度/hex 编辑 |
| Color well | `color_well` | 色块就地展开取色器 |
| Form / Field / Fieldset | `form` / `field` / `fieldset` | 标签、说明、Danger 错误 |
| Radio | `radio` / `radio_group` | radio 角色与选中态 |
| Link | `link` | Link 角色与行为由调用方决定 |
| Back/forward buttons | `back_button` / `forward_button` | 依赖 `Router`，两端禁用 |
| Date input | `date_input` / `date_input_with` | ISO 显示 + 日历 popover |
| Number input | `number_input` | 有界数字框 + 加减步进；Up/Down/失焦格式化未接入 |
| Range slider | `range_slider` | 双 thumb 单轨道；共享一个焦点与一个语义值 |
| Sticky | `Layout::sticky` | 仅顶部吸附；滚动/包含块后处理 |
| Time input | `time_input` / `TimeOfDay` | 校验 HH:MM 文本框；非分段/12 小时制 |
| Find bar | `find_bar` | 匹配计数、前后跳转、Done、打开聚焦 |
| Alert dialog | `alert_dialog` | alert 角色、Escape dismissed、外部点击无效 |
| Accordion | `accordion` + `collapsible` | 边框容器；item 复用 collapsible |
| Accordion item / Collapsible | `collapsible` | 200ms 测量高度裁剪、退出保留、reduced motion、expanded 状态 |
| Segmented | `segmented` | 选中按钮组 |
| Progress | `progress` | 标签与百分比 |
| Badge / Avatar / Ruby text | `badge` / `avatar` / `ruby_text` | 主题派生样式 |
| Calendar | `calendar` / `calendar_content` | 月网格、范围模式、注入 today |
| Tooltip / Popover | `tooltip` / `popover` | 锚定面板与 portal |
| Select / Combobox / Autocomplete | `select`（可搜索） | 列表选择、过滤、稳定值 |
| Menu / Context menu / Menu button | `menu` / `menu_item` | 命令菜单与 portal |
| Dialog | `modal` | 模态、嵌套、Escape/backdrop 可配置 |
| Slider | `slider` | 单值滑杆 |
| Switch / Checkbox / Toggle | `switch` / `checkbox` | 受控布尔控件 |
| Text input / Editable text | `text_input` / `text_input_string` | 原生编辑、IME、选择 |
| Table / Grid view / List | `table_header` / `table_row` / `virtual_list` / `variable_virtual_list` | 固定表头；固定/可变行、稀疏测量、业务锚点及可打断 FollowEnd 动画 |
| Image / Icon / SVG | `image` / `svg` / `icon` | 资源缓存与 currentColor |
| Shader | `shader` | WGSL surface |
| Inspector | `property_field` | 属性行 |
| Layout / Grid / Scroll | `row` / `column` / `grid` / `scroll_y` | 布局原语 |

## 从 C# 组件包补齐的 Rust 入口

`chip` / `tag`、`pagination`、`alert` / `notification`、`empty_state`、`skeleton`、
`Tone` / `badge_tone` 现已提供 context-first Rust 函数。场景效果提供 typed
`LiquidGlass` / `BackdropBlur` / `ScrollEdgeSide` 和 `backdrop_blur` / `scroll_edge`。
布局和 leaf builder 可使用 `.blur` / `.no_shadow`，无需通过 C# 或手工拼 shader uniforms。

圆角字段、玻璃轮廓及 opaque fallback 已连接；Rust 控件和布局默认连续圆角。
wgpu 材质包含 Regular/Clear/Clarity、悬停／按压反馈、连续内外阴影与受 sigma 预算约束的背景预滤波；
应用复用框架 API，不维护效果源码。
原有共享后端优化与本轮新增项的详细对照、测试和限制见
[C# MyGO 改进迁移到 Rust](experiments/csharp-mygo-rust-migration.md)。

## 由既有子系统覆盖

- Code editor / Rich text editor：`plico-editor`（独立 crate 与 widget 契约）。
- Markdown：`plico-markdown`。
- Video：`plico-video` 宿主播放器。
- Web view / Drawing / 自定义画布：宿主 widget 契约（`plico_renderer::Widget`）与图片 surface；
  核心不提供跨后端的即时 canvas 控件。
- Motion / Transitions / Transforms / Right-to-left / Views / Windows / Testing / Accessibility：
  由 Engine、Clock、样式动画、Direction、SemanticRole 投影与测试协议提供，不是单独控件。

## 暂缓（受当前核心能力限制）

| MyGO | 阻塞原因 |
|---|---|
| Decimal input | 需要 decimal 类型与编辑结束格式化；Plico 没有 decimal，也没有公开的失焦钩子 |

| Token field | 需要在输入中捕获逗号/退格并驱动建议浮层；当前 Input 只暴露 changed/submitted |
| Outline | 带列的层级虚拟化；现有 tree 构建全部可见项，virtual_list 是扁平行 |
| Chart | 没有坐标轴/路径/图例层；适合作为宿主 widget 或独立 crate |
| Diff viewer | 需要 diff 算法与行内标记；可由文本行组合，但尚未确定 MyGO 对齐的算法 |


这些项需要先补核心输入/布局/浮层能力，或作为独立 crate/宿主 widget 实现；在核心能力就位前
不建议用近似控件冒充。

## 证据

- 组件与行为测试：`crates/plico/tests/{desktop,indicator,navigation,search,tree,color,form,radio,date,find,alert,split}_widgets.rs`。
- 语义投影：`crates/plico-runtime/tests/indicator_semantics.rs`、`alert_dialog.rs`。
- 各控件的 API 与已知差异：`docs/components.md`、`docs/desktop-widgets.md`。

## 呈现与合成架构

退出子树、字形几何变换与背景采样顺序的本轮对照见 [呈现生命周期](presentation.md)，包含具体 MyGO revision、源文件、测试与限制。
