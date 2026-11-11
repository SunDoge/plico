# 架构与后端边界


当前 crate 划分与旧协议清理见 [后端清理](backend-cleanup.md)；下文部分迁移记录保留历史上下文。

`plico-renderer` 定义布局、文字、paint scene 与经过验证的 shader effect 描述，不能依赖
具体 GPU API。wgpu 的纹理、bind group、pipeline cache 和 render pass 执行归
`plico-wgpu` 所有；Skia 与其他后端只消费同一份 effect 描述。CI 通过
`scripts/check-renderer-boundary.py` 守卫这条依赖方向。

与 MyGO 的公开 `mygo`/`ui` 和私有 `internal/scene`/`internal/surface` 分界一致，
`plico-shell` 只提供窗口循环、宿主扩展及桌面系统服务，不再二次公开整套
`plico-renderer` API。应用从 `plico`/`plico-runtime`/`plico-desktop` 使用声明与窗口
接口；渲染探针和后端实现直接依赖 `plico-renderer`，依赖所有权因此保持显式。

测试遵循 MyGO `ui.Tester` 的封装方式：`plico_runtime::testing::Driver` 不公开内部
`Source`/Engine/Adapter，只提供 view 断言、受控状态更新、语义快照、声明快照、retained
layout 查询、类型化事件与虚拟时钟。运行时缓存因此可以重构，而不要求应用测试绑定内部字段。

`Source` 不保留可变的“上次错误”旁路状态。测试、capture、recipe、Web 与 Bevy 等需要处理
失败的调用方使用 `try_prepare_for_event`、`try_build_frame`、`try_render_retained_frame` 和
`try_build_and_paint`，声明或时钟错误沿当前调用直接返回。动态窗口宿主的
`FrameSource::prepare_for_event`、`render_retained_frame` 与 `build_and_paint` 同样返回
`FrameSourceError`；shell 统一处理遥测、首帧失败与截图完成，不再由 runtime 记录日志后伪装
成成功事务。后续按同一方向
先补齐查询和受控更新接口，再逐步收回仍公开的 Engine、Adapter 与 view 字段，避免一次机械
迁移所有旧测试。

和 MyGO 将 runtime、scene 与帧统计留在内部一致，普通宿主通过 `Source::view`、`update`、
`placed_nodes`、`semantics`、`selected_text`、`focused`、`theme` 和只读 hit-test 诊断读取或更新稳定能力，不拼接
Engine 与 Adapter 的内部字段。性能工具读取只读 `FrameDiagnostics` 快照；声明计数、耗时与
后端统计不再是可由调用方改写的 `Source` 状态。CI 的
`scripts/check-runtime-boundary.py` 禁止生产应用、宿主、可运行 examples 与性能 probes 重新
穿透这条边界。跨语言 ABI 实验已经退役，不再为它保留第二套错误、批次或组件边界。历史集成测试分批
迁往 `testing::Driver` 或同一稳定查询面；已迁移的 `backend-settings`、`launcher` 与
`services-demo` 测试也由边界脚本守卫。
Gallery 的 calendar、2048、mines、glass、markdown 与 wgpu 测试已全部按同一方式迁移。
`plico-wgpu` 后端测试也只读取 retained nodes、语义、shader diagnostics 与帧诊断，不再将
runtime 的 Engine/Adapter 当作渲染器测试接口。
Runtime 外部测试按功能分批迁移；焦点、字体、语义指示器、retained frame、滚动布局动效、
测试协议、toast、用户活动与虚拟表格宽度测试已进入同一边界守卫。
Alert dialog、deadline、系统显示偏好与 secondary-click 生命周期测试也已迁移；shader 存在性
直接从生成的 Scene 断言，不再读取 Engine 声明树。
Component kit、hover 与 range-slider 测试也已进入守卫。Collapsible 的挂载、裁剪和退场从
retained layout 与语义快照验证；hit-test 工作量通过类型化只读快照暴露，不泄漏 Adapter 元组。
固定／可变虚拟列表的稀疏挂载、锚点、sticky header 与 follow-end 测试也只读取 view 和
retained placed nodes，并通过 `Source::update` 驱动业务状态。最后一组 `adapter_work` 缓存与
增量测试也已改读统一的 `FrameDiagnostics.backend`、retained nodes、语义和 shader 诊断；
runtime 的全部外部集成测试已进入同一边界守卫。
Desktop 测试开始按同一原则迁移：absolute/backdrop/font/rich-text/retained-frame、组合按钮、
route menu、材质与滚动裁剪首批只使用 `Source` 查询、更新和统一诊断快照。直接验证 renderer-free
`Engine::frame` 的核心布局测试仍可独立使用 Engine，不经过 Source，也不属于边界穿透。
第二批覆盖 atomic compact metrics、feedback motion、玻璃像素/交互、native surface、scene layer、
密码输入与保存按钮稳定性。密码声明快照的核心保密断言保留在 `plico` Engine 测试；desktop
只验证宿主可观察的 IME、剪贴板与 retained paint，避免重复暴露声明树。
第三批覆盖 grid/flex 布局、平滑滚动、样式几何和图标缓存。SVG 与 WGSL 资源错误都通过只读
diagnostics 查询，业务状态统一经 `Source::update` 修改；独立验证序列化声明的测试直接使用
renderer-free `Engine`，不再借 `Source` 穿透运行时实现。
随后将时钟、对齐、基础滚动、结构增量同步和 SVG 资源生命周期也迁入守卫。宿主级 flex 测试
通过 retained paint 标记定位实际布局节点，不要求公开 key→Entity→Taffy 的内部映射；专门验证
Engine/Adapter 契约的后端测试仍可直接组合二者，与应用侧 `Source` 边界分开。
Capability、组合按钮、calendar、toggle 与 shader transition 测试同样只依赖语义快照、retained
paint、帧诊断和业务 view。语义焦点使用快照自己的稳定 ID，不将内部 `Entity` 映射泄漏给
应用测试；shader 动画采样也不再直接读取 Adapter registry。
批处理、布局复用、虚拟列表和 GPU shader 测试也已进入边界守卫。批处理仍通过 `BatchStats`
验证通知合并和声明提交次数，虚拟列表通过 retained node identity 验证复用；shader 声明的 JSON
往返由独立 Engine 测试承担，窗口路径只验证实际像素、资源诊断和增量帧工作量。
按钮状态、slider、style motion 与跨段文字选择也只从业务 view、语义、retained paint 和诊断
观察行为。Sidebar 的焦点选中测试以语义 bounds 关联最终 paint，不再读取 Engine 声明节点；
文本选择统一通过 `selected_text` 查询，保留复制、反向拖选、重声明和卸载覆盖。

## 当前实现

```text
UiContext / typed builders / component
              ↓
Engine：按 (parent Entity, scope, local Key) 找回节点
              ↓
保留 arena：属性、文本、子节点顺序、缓存
              ├─ Engine::frame：模拟尺寸实验
              └─ Engine::frame_declarations：外部后端负责测量与布局
                            ↓
                 plico-runtime Adapter
                 稳定 Entity → Taffy NodeId
                            ↓
                 Taffy / Parley /（plico-desktop 选择 Vello 或 Skia）
```

三个根接口按受众和宿主能力分层，不要求简单组件实现 trait：

- `plico::View` 是面向应用的最小 UI 接口，只包含 `show` 和 `theme`，可直接驱动
  renderer-free Engine、NSH/LVGL 投影等后端；普通控件和组件继续使用 Rust 函数。
- `plico_dev::App` 是面向应用的完整根接口，在同样的 `show`/`theme` 之上增加
  Serde checkpoint、services、快捷键和 FrameTime；静态桌面入口与开发热重载复用它。
- `plico_runtime::HostView` 面向后端与高级宿主，增加 host action、wake 和宿主更新
  能力。`Source<V>` 与
  `plico_desktop::run_host_view_with_options` 接收这一接口。

普通 `View` 由 `NativeView<V>` 适配到 `HostView`；
`plico_desktop::run_view_with_options` 自动完成该包装，不要求序列化应用状态。
`plico-app-host` 中的 `NativeApp<A>` / `ServiceView<A, S>` 把 App 与 services
协议接入 `HostView`；桌面 crate 重新导出它们。这样 `plico-runtime` 本身不依赖
`plico-dev`，HostView/Source 层只面向 core 与 renderer。

core 不依赖 Taffy、Parley、Vello、Bevy 或平台窗口。声明运行时（adapter、Source、HostView、动效）已从窗口后端中拆出为 plico-runtime；plico-desktop 只保留桌面启动器、开发 CLI 与 Vello/Skia 窗口后端选择，Skia 后端不再依赖 Vello crate 的运行时。当前尚未定义完整的、跨宿主的布局/场景运行时协议，不能把它当成已经完成的通用宿主协议。

另有 [LVGL 后端实验](lvgl.md)：同一 Engine 的容器、文本和组合按钮投影为 LVGL 原生控件，
布局、文字和绘制均交给 LVGL；点击以 Entity 回到普通声明。它验证 API 子集可跨后端，
没有引入通用 Backend trait，不代表所有样式和控件均已跨后端对齐；ESP32-P4 的验证范围见实验文档。

## 身份与存储

`Key::Static`、`Key::Number` 和宏生成的 Generated ID 避免每帧分配路径字符串。component 自动身份见 identity.md。`Auto` 序号只适合子节点顺序和数量稳定的静态结构。动态列表使用业务 ID；兄弟节点重复 key 会报错。

`Entity` 是 SlotMap 提供的、本次进程内有效的不透明世代句柄。slot 复用不会让旧句柄重新有效。它不是持久 ID，跨进程恢复应按稳定的逻辑 key 路径匹配。当前身份带父节点，跨父节点移动会被视作卸载并重新挂载；不承诺保留跨父移动的状态。

arena of records 已足以验证重用和缓存，不预先引入完整 ECS scheduler。未来可以借用 ECS 的稳定实体、分离存储和批处理思想，但 UI 核心不必依赖 Bevy ECS。是否改成 SoA 应由测量决定。

## 差异比较

声明一个节点时直接查找保留记录、比较属性，不需要先创建完整的第二棵重对象树，再调用通用 tree diff。新节点分配一次；文字接受 Cow，借用内容只在变化时复制；变化的 owned String 可移入。

子节点顺序通过两份可复用 Vec 比较与交换，未变化时不更新后端。Vello adapter 在变化时 set_children，并保留已有 Taffy 节点。它目前仍会构造临时集合和输出 Vec，没有零分配承诺。

暂不引入通用 diff 库。只有当后端逐项 move/insert/remove 的代价值得优化时，再评估 keyed-list/LIS 算法；移动不能被误当成控件销毁重建。属性差异必须保留领域语义，例如颜色变化与字号变化的失效范围不同。

## 目标宿主模型：像 egui 一样嵌入

目标是共享 API、状态、交互、布局与文字语义，再向桌面宿主或 Bevy 输出可呈现的场景/绘制数据。宿主输入包括窗口尺度、输入、时间和唤醒；输出包括绘制、重绘需求、光标、IME、语义树和平台请求。

Bevy 是宿主集成目标，不强制把所有 Plico 控件映射成 bevy_ui Entity。使用 Bevy 原生 UI 可以作为另一条研究路径，但它会带来布局、文字与样式行为差异，不是当前优先路线。目前已有实验性 [Plico Bevy 插件](bevy.md)：复用 Source、Taffy 和系统文字，与 Bevy 共享 GPU，在相机提交后合成 primary window 的透明 UI 纹理；多窗口和世界空间 UI 尚未实现。

布局引擎和 glyph 资源应位于可复用层；后端资源句柄不能反向污染 Ui API。不要现在就用一个巨大 Backend trait 锁死所有能力，也不要承诺抽象场景可以不经转换直接交给任意 GPU 后端。

## 集成边界必须验证的能力

输入坐标与 DPI、焦点与输入消费、IME 与剪贴板、多个 UI surface、时间和休眠、纹理/字体资源生命周期、渲染顺序、裁剪，以及宿主销毁时的清理。Bevy 集成还需要明确 system 排序和输入被 UI 消费后游戏系统能否继续处理。

核心不应被单线程 Rc 状态接口永久绑住。可以在独立运行时内部使用单线程数据，但与 Bevy Resources 和其他宿主状态的桥接应允许短期借用，不要求用户复制整份业务模型。

## 内置后端的维护方向

renderer/shell 及所需 Rust 依赖已复制到本仓库，目前位于 crates/plico-renderer、plico-shell、plico-motion 等 crate，不把外部 Wabou API 当作不可改变的边界。可以针对 Plico 调整测量缓存、场景片段复用、增量 paint、资源生命周期与嵌入呈现。本地 crate 已统一改为 Plico 名称，来源清单仍保留原上游名称；应按具体问题逐步重构，不一次重写已成熟的文字和平台实现。来源与许可见 backend/README.md。

## 开发主线

后续开发以 Plico 为主，原 Wabou 暂停迭代。所需 Wabou 模块直接复制进本仓库后独立维护，不要求先在 Wabou 实现，也不引入指向原 checkout 的 Rust path dependency。保留来源和许可证，不承诺双向同步。可选 Solid 对照工具仍可读取和执行原项目的已有产物，不在原仓库开发新功能。

## 当前开发模块边界

`gallery` 是普通静态 Rust 应用，依赖核心、plico-dev 和需要的功能 crate；它不导出 app cdylib。`plico dev` 监听并重新编译同一个 executable，成功后重启，失败时保留旧进程。Engine、Taffy、字体和 GPU 资源随进程重建；需要跨重启保留的数据走应用自己的持久化。

核心 arena 是 SlotMap 的薄封装，Entity 是不透明 handle；Snafu 提供领域错误。核心仍不依赖 renderer、窗口或 Bevy。JSON/TypeScript recipe 只描述应用定义的 preview 配置、交互和断言，不作为 Rust ABI 或任意 App 状态迁移协议。WASM host/UI 值协议是独立能力，不表示桌面动态加载仍存在。
