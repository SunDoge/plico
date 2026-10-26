# Gallery、组件组织与路由


当前 gallery 默认 Workspace，另有 Layout、Controls、Motion 和 Lists。Workspace 提供项目选择、项目独立的偏好草稿、保存/重置、归档与恢复；数据仅为当前会话保存，未接入磁盘持久化。窗口初始 1120×820、最小 640×740；浅/深色和大字号均有真实后端捕获。样本见 reports/gallery。

Layout 展示固定导航、弹性画布与固定 inspector 的空间分配；侧栏宽度可连续变化。交叉轴 Start/Center/End 与主轴分布由 Taffy 求解，Wrap 可切换真实换行。这里只展示已经接入的 flex 能力，不宣称已支持 grid、滚动、FLIP 或完整 CSS。

Gallery 持有可序列化的 Page enum，通过 match 声明页面。切页卸载旧页面节点；计数和偏好放在 App，因此切回来保留。它不是框架 router，尚无路由历史、URL、保留页面树或页面进入/退出动画。未来 Router<Page> 可封装历史，平台层再映射 URL。

应用组件签名使用 `&mut UiContext`（兼容别名 Ui），通过 column(cx)、button(cx).text(...) 等独立函数构建。NavItem、项目卡片是普通函数，panel 返回应用自己的可配置容器；均不需要给 UiContext 增加方法或实现 Widget。内部仍直接借用 Engine，没有内部可变的共享 context。

main.rs 只构造 App 并调用 run_with_options，不解析参数。examples/capture.rs 和 examples/probe.rs 是开发验证工具，由 plico-cli 的 render/probe 调用。

## 基础 motion

Motion::Quick（140ms）、Smooth（240ms）、Emphasized（360ms）和 None 统一时长；布局和叶子 builder 的 .motion(preset) 复用 host 属性补间。原 .transition(milliseconds) 仍可指定时长。按钮默认 Quick，hover/press 逐渐改变 opacity，键盘焦点立即显示；开关默认 Quick，thumb 位置与轨道颜色连续变化，checkbox 的标记随进度出现。

动画不保留 App 闭包；虚拟时钟测试验证中间几何、反向 retarget 连续性、纯 paint 零 layout 及 reduced motion 到目标后停止请求帧。主题由独立 reveal 负责，主题切换时属性 cache 对齐新目标，避免两个动画重复混合目标调色板。

这些仍是第一批交互和属性动画。约束跳变、列表重排、页面进出、共享元素、保留速度的 spring 尚未接通，不用“所有操作都有动画”概括当前状态。

## SVG 资源

```rust
static LOGO: plico_core::SvgAsset = plico_core::svg_asset!("../assets/plico.svg");
// 在组件中：
svg(cx, &LOGO).size(26., 26.);
```

宏展开为单文件 include_str!，相对调用源文件定位。没有资源目录扫描，也没有引用全部图标的总表；最终二进制只需包含实际可达的嵌入内容。按名称动态访问全部图标的 registry 会保留它引用的所有资源；最终链接是否裁掉未使用定义也受链接选项影响，不承诺任意配置都能自动裁剪。

Gallery 的 Lucide 图标由 `plico icons add` 按需生成到应用自己的 Rust 模块；正常构建不需要下载 SVG 或运行 Bun。生成和离线校验方式见 [图标文档](icons.md)。

host 按完整源文缓存 SvgImage，同内容节点共享 Arc；动画帧不重新解析。缓存仅保留仍被节点引用的源文，最后一个节点卸载后释放。损坏内容保留错误缓存，可从 Adapter::svg_diagnostics 查看，不会每帧重试。SVG 保留文件自身颜色、保持宽高比；当前默认作装饰图，不把 XML 放进语义标签。已支持 currentColor 继承与 .color 覆盖；独立的图片替代文本 API 尚未实现。

轻量热重载快照仍以节点 text 字段传送 SVG XML；业务重声明时会重复传输，纯 host 动画帧不经过这条路径。后续可按内容 ID 协商独立资源表，不把当前缓存说成已完成传输去重。


## 主题与工作台

旧私有 Palette 已移除，使用核心 Theme 的语义色和 Metrics。Compact 统一改变内置控件与工作台的间距/字号/默认控制高度；密度变化不触发颜色 reveal。light/dark 切换继续由 host 驱动转场。图标加文字按钮共享按钮行为，禁用状态传递给装饰子内容。动态项目列表使用业务 id，draft/saved 模型属于 App，切换项目、页面和兼容热重载不丢失草稿。没有把尚未实现的弹层、输入框或自动磁盘保存画成假交互。


Controls 页的 Preferences 并排展示正常和禁用的 switch/checkbox，复用同一业务值展示 checked 状态，但禁用项不会修改它。开关和复选框现在可链式配置 disabled、gap、尺寸/字体和 motion；Compact 同时改变指示器与文字的预留空间。最小窗口和大字号继续纳入整页边界测试，不通过增加窗口高度掩盖溢出。


图标来自完整固定版本的 Lucide SVG 集合；scripts/icons.ts（bun）获取上游资产并生成 plico_core::icons 常量，SVG 不提交进仓库。导航、项目卡片、保存/重置/归档/恢复与计数按钮使用真实 SVG；checkbox 使用 CHECK 标记，支持主题颜色与淡入/缩放。详见 [SVG 图标](icons.md)，真实截图在 reports/icons。

Lists 页提供 10,000 条固定行高数据，通过 virtual_list 仅声明可见行和 overscan；见 [虚拟列表](virtual-list.md)。

## 2048

侧栏新增 2048，参考本地 MyGo 实现，包含规则、滑动/合并动画、按键与屏幕方向按钮。原生与可重载模块共享 App 按键处理，棋盘状态可随模块激活延续。实现边界、对比和截图命令见 [2048 文档](2048.md)。

## Images

桌面 gallery 的 Images 页展示网络加载的 WebP/GIF 动图、磁盘缓存、暂停与重播。
宿主的 `GalleryHost` 管理下载，`SurfaceSource` 挂载框架的 `ImageSurface`；
UI 只发送请求和读取加载状态。普通主窗口不解析参数，截图工具可用 `--page images`。
详情见 [图片与动画](images.md)。
