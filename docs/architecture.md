# 架构与后端边界


当前 crate 划分与旧协议清理见 [后端清理](backend-cleanup.md)；下文部分迁移记录保留历史上下文。

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
                 plico-vello Adapter
                 稳定 Entity → Taffy NodeId
                            ↓
                 Taffy / Parley / Vello
```

core 不依赖 Taffy、Parley、Vello、Bevy 或平台窗口。当前尚未定义完整的、跨宿主的布局/场景运行时；Plico Vello adapter 加上仓库内 renderer/shell 源码是可执行的边界实验，不能把它当成已经完成的通用宿主协议。

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

Bevy 是宿主集成目标，不强制把所有 Plico 控件映射成 bevy_ui Entity。使用 Bevy 原生 UI 可以作为另一条研究路径，但它会带来布局、文字与样式行为差异，不是当前优先路线。当前没有 Plico Bevy 插件。

布局引擎和 glyph 资源应位于可复用层；后端资源句柄不能反向污染 Ui API。不要现在就用一个巨大 Backend trait 锁死所有能力，也不要承诺抽象场景可以不经转换直接交给任意 GPU 后端。

## 集成边界必须验证的能力

输入坐标与 DPI、焦点与输入消费、IME 与剪贴板、多个 UI surface、时间和休眠、纹理/字体资源生命周期、渲染顺序、裁剪，以及宿主销毁时的清理。Bevy 集成还需要明确 system 排序和输入被 UI 消费后游戏系统能否继续处理。

核心不应被单线程 Rc 状态接口永久绑住。可以在独立运行时内部使用单线程数据，但与 Bevy Resources 和其他宿主状态的桥接应允许短期借用，不要求用户复制整份业务模型。

## 内置后端的维护方向

renderer/shell 及所需 Rust 依赖已复制到本仓库，目前位于 crates/plico-renderer、plico-shell、plico-motion 等 crate，不把外部 Wabou API 当作不可改变的边界。可以针对 Plico 调整测量缓存、场景片段复用、增量 paint、资源生命周期与嵌入呈现。本地 crate 已统一改为 Plico 名称，来源清单仍保留原上游名称；应按具体问题逐步重构，不一次重写已成熟的文字和平台实现。来源与许可见 backend/README.md。

## 开发主线

后续开发以 Plico 为主，原 Wabou 暂停迭代。所需 Wabou 模块直接复制进本仓库后独立维护，不要求先在 Wabou 实现，也不引入指向原 checkout 的 Rust path dependency。保留来源和许可证，不承诺双向同步。可选 Solid 对照工具仍可读取和执行原项目的已有产物，不在原仓库开发新功能。

## 当前开发模块边界

`plico-gallery` 只依赖核心、plico-dev、Serde 与 Stabby，已从渲染宿主中拆出。CLI dev 构建关闭静态 gallery feature 的宿主，并监听/重编译 app cdylib。host 常驻保存 Engine、Taffy 节点、字体与 GPU 资源，应用逻辑通过版本化 JSON 返回 ViewNode 描述。

当前热更新以 Stabby 的稳定 Vec<u8>/Reply 传输字节，以 get_stabbied 验证函数布局，不让 Rust trait object、Ui 引用或普通 Vec 跨模块。逻辑 key 路径传递点击，避免把一版模块的 arena ID 当成另一版的身份。状态用 Serde 迁移，不要求应用所有类型都有稳定 ABI。

核心 arena 已改为 SlotMap 的薄封装，Entity 是不透明 handle；Snafu 提供领域错误，Serde 用于开发协议。仍不依赖 renderer/窗口/Bevy。生产静态 Source 不经过该 JSON 开发通路。
