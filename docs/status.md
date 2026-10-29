# 实现状态与边界

## 当前主线

默认声明模型为 immediate mode：plico 的 Engine / UiContext / widgets，
plico-desktop 负责真实后端。旧 crates/plico 响应式 Runtime、图实验、共享响应式 2048
组件与 archive-gallery / game2048 / task-workspace 旧应用已删除。
Rune 实验同样已移除。当前 2048 功能在主 Gallery 中维护。

默认关闭的 `plico-runtime/regions` 新增 ID 驱动的原生区域重放实验；状态由组件实例拥有，watch 和事件使用显式依赖元组与函数指针。已补 keyed 组件局部状态、卸载释放、宿主结果邮箱和 opt-in MessagePack checkpoint。Text 的 `.reactive(...)` 已支持文字、颜色、字号、字重、字体族、透明度，设置页预览与保存提示使用属性绑定；非法属性、主题更新与开发协议恢复有回归覆盖。已新增 [reactive outlet](router.md#reactive-page-outlet)，沿用 Router 历史并管理活跃页面所有权、参数 key、嵌套销毁与状态恢复；不包含后台 keep-alive。接入与限制见 [区域实验](regions.md)，不替代默认 immediate API。

## 已实现

- 普通 Rust 函数、调用点身份、动态业务 key、受控输入和事件稳定化。
- 保留节点、属性变化 journal、节点/结构删除与布局/绘制/语义失效。
- 声明父子关系使用引擎级连续日志，复用节点 children 容量；不为每个节点保留第二份 children Vec。`Engine::trim_declaration_buffers()` 可显式释放声明日志、格式化暂存和 children 多余容量，不改变 Entity 或清除待处理事件。
- `plico` 的 allocations 示例验证 2 万节点场景第二次声明、预热后的无变化/颜色变化/同容量文本变化均无分配；这只覆盖核心声明路径，不代表整个应用或后端零分配。
- Taffy 布局；Vello 普通文字使用 Parley，Skia 普通文字使用 SkParagraph；字体、选择复制、输入法和 SVG。
- 普通文字默认不额外几何加粗，保留 hint 与真实字重，避免淡边；显式扩张按实际扩张量收紧轮廓容差，避免直边变斜。Linux 默认优先 Noto 中英文字体栈，保留应用字体覆盖能力。
- hinted 字形在轮廓缓存插入时精细离散化，减小小字号曲线的局部粗细误差；当前通过本地 Glifo 补丁实现，无 hint 与位图字体不受影响，仍存在亚像素采样导致的局部覆盖率变化。
- Gallery 的布局、控件、动画、2048、流式 Markdown 与应用内毛玻璃。
- Vello 样式按钮的宿主交互权重：hover/press 颜色、焦点环和禁用透明度；支持打断、reduced motion 与闲时停帧。
- 虚拟列表、smooth scroll、宿主滚动和声明分离、条件帧调度。
- 宿主直接绘制 retained 状态，干净帧和纯表现帧不必导出全量节点。
- 原生 main.rs + lib.rs、模块热重载、Serde 状态恢复/失败重置与宿主服务桥。
- [窗口内快捷键](shortcuts.md)：App/ServiceApp 完整组合键、显式作用域 action 表、冲突检查，以及静态/热重载一致的事件转发。
- [可选系统全局热键](global-hotkeys.md)：plico-native 管理注册与状态，shell extension 负责唤醒和注销。X11 后台触发/冲突/释放已实测，Windows/macOS 通过交叉编译；Wayland portal 已接入并编译，但真实桌面授权与 activation token 聚焦尚未验收。
- usage-rs CLI、plico fmt、应用目录选择与 dev/run 分离。
- plico icons add/check：应用拥有选择集合、生成 Rust、清单和许可证，核心仅内置四个控件图标。
- launcher、terminal、services-demo；编辑器和 WASM 入口保留。

- [Skia / Vello 对照实验](skia.md)：SkParagraph 普通文字、CPU / Vulkan Skia 回放和实验性 Vulkan 窗口；保留相同 Parley glyph 的多缩放对照。

## 同步组件的 local state 与宿主任务

`UiContext::state` 按父节点、组件作用域与调用位置保存局部状态；读写 handle 不借用
UiContext，变化触发现有 settle 流程，卸载释放。普通 state 在热重载时重置；
persisted_state 使用显式名称，跨卸载保留，dev checkpoint 独立恢复各槽；宿主可导入/导出
MessagePack 字节实现磁盘或浏览器存储，不默认自动落盘。
`plico_dev::tasks::Task` 封装宿主 Future 的结果、唤醒、替换与取消；执行器及服务快照
仍由应用提供，不让 Future 穿越模块 ABI。TaskService 提供单任务服务协议，
UI 可用 services.task().status() 匹配状态并发送 start/cancel/reset。见 [说明](local-state-and-tasks.md)。

## 当前代价和限制

一次业务声明仍会执行视图函数、匹配节点并扫描提交状态；增量后端不等于整条链路 O(Δ)。
依赖布局约束的变化可能扩大重排范围。图标、文字、场景等缓存分别由对应层管理。
原生模块协议仍有完整节点快照，不等于所有热重载帧都发送局部 patch。

热重载为 best effort；业务状态不兼容可以重置，不保证任意 Rust 类型无损迁移。
WASM、分离 host/UI 是已存在的入口和实验，不等于浏览器端编译 Rust。
[LVGL 后端实验](lvgl.md) 使用 lightvgl-sys，提供原生控件映射、对象复用与事件回传，
不是完整后端。ESP32-P4 / ESP-IDF 已有上板验证，范围见 [P4 实验记录](p4-counter-measurements.md)。
尚未接入 Bevy、FLTK 或 no_std，也不承诺其他 ESP32 型号。

## 验证入口

```sh
cargo check --workspace --all-targets
cargo test --workspace --all-features --exclude plico-lvgl --exclude lvgl-esp32p4
cargo run -p gallery
cargo plico new /tmp/my-app
cargo plico dev /tmp/my-app --headless --no-watch
cargo plico icons check --output apps/gallery/src/icons.rs
```

具体行为和局限见 [文档索引](README.md)、[声明模型](declaration-model-choice.md)、
[帧调度](frame-scheduling.md)、[增量更新](incremental.md)、[缓存](cache.md)、
[宿主服务](host-services.md)、[热重载协议](hotreload-protocol.md)。
测试、截图和性能数据只代表对应版本与配置；reports 下的本地产物不提交。

## 本次清理验证（2026-10-23）

删除实验后，workspace all-features 测试共 606 项通过，workspace all-targets 编译通过。
通过 plico new 创建的独立项目已完成静态入口编译和真实 headless 模块加载，
点击 Increment 后业务状态从 0 变为 1，后续快照保持 1。
依赖锁文件已清除 Rune、reactive_graph、Sycamore 及仅供旧实验使用的依赖。

[FLTK + Taffy 后端实验](fltk.md) 已在 Linux/X11 验证共享设置页、原生输入/按钮回调、嵌套卸载、滚动与 keyed 重排。它是独立的可选实验；样式、窗口功能、热重载及其他平台尚未达到 Vello 后端的覆盖范围。

FLTK 共享工作台现包含 Router 前进/后退、响应式 Grid、固定表头滚动列表，以及基于注入时钟的侧栏/详情栏/任务进度动画。FLTK 宿主按 ANIMATE 请求调度有限帧，结束后阻塞等待事件；尚未自动支持全部 `.motion(...)` 样式插值，见 [实验范围](fltk.md)。

## Optional native text

Implemented in `plico-native` behind Linux `linux-text`: Pango/Cairo layout and rasterization, system font listing, metrics, bitmap caching and GPU comparison tests. See [native text](native-text.md). Ordinary widget/editor integration, desktop font settings, CoreText and DirectWrite are not implemented.

## Widget input capabilities

Implemented static `Kind::capabilities()` without per-node storage. Focus, Tab navigation, activation, text selection/editing and numeric editing are independent. LVGL rejects unsupported controls before mutation and remounts native subtrees on class changes. See [capability contracts and validation](widget-capabilities.md). `KindFamily` remains only for source compatibility.

## Optional damage tracking

Skia command-stream damage and per-image history, CPU culling and optional Vulkan
incremental-present hints are implemented as opt-in experiments. Normal windows
use full drawing; enable with `PLICO_SKIA_DAMAGE=1`. Simple raster/GPU pixel oracles
pass, but the real settings theme animation exposes four one-channel-step pixels
near a dirty clip edge. A reduced raster probe reproduces this with a rounded
stroke inside a translucent layer, even without command culling. The 240-frame
settings probe permits explicitly bounded AA rounding (at most 16 pixels, one
channel step), checks unchanged pixels outside damage exactly, and retains an
optional strict mode; measured maximum is eight pixels with no growth at rest. Vello's preserved-target partial
clear probe passes, while its normal window renderer remains unchanged. See
[measurements and limits](performance.md).
