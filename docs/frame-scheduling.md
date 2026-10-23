# 声明帧与保留帧


2026-10-21：原生 Source 的 retained 输出路径开始将“宿主请求一帧”和“应用需要重新声明”分开。没有引入 memo、依赖追踪或应用侧 changed。

## 当前实现

原生 shell 通过 `FrameSource::render_retained_frame` 请求画面。Source 每次采样仍推进注入时钟，检查字体资源、滚动、presence 和宿主动画，然后决定：

- 首帧、尺寸或主题变化、字体设置变化、待处理输入/服务更新、显式 update/request_rebuild，以及应用请求的逻辑动画：执行声明和 settle，提交必要的布局/绘制变化。
- 无应用失效的表现动画：复用声明，推进 motion/shader/tooltip/theme。需要时更新后端和重新编码场景。
- 无变化的重复绘制请求：复用声明和 Scene，不做后端提交或布局。DPI 变化仍按新比例重编码场景，无需仅为比例变化重跑组件。
- 声明失败：不能把失败帧当作干净缓存，下次重试。直接 `try_declare()` 产生的成功声明即使尚未提交后端，也不会被忽略。

调用方仍负责外部状态变化的通知，不能从任意 Rust 内存写入自动推断变化。正常控件事件、异步服务 poll 和模块重载已有相应通知；宿主直接修改 Source 的应用对象时推荐：

```rust
source.update(|app| {
    app.items.push(item);
});
```

update 在 UI 线程立即执行闭包并标记失效。多次 update 的操作全部执行，下一帧合并声明；它不负责把闭包投递到 UI 线程。保留的 `source.view` 公开字段可直接使用，但修改后必须 request_rebuild。theme 的改变另有比较检查，不应依赖它代替其他状态的失效通知。

`build_frame()` / `build_frame_at()` 是兼容的主动构建快照接口，通常仍执行 view（已有表现动画快路径除外），并导出 PlacedNode Vec；它没有悄悄变成缓存读取接口。离屏工具与测试可继续使用。原生 retained 路径不导出这份 Vec。

组件内的 `request_animation_frame()` 仍表示下一帧需要执行应用逻辑；已有 opacity、transform、shader、theme 等宿主动画不需要组件为每一帧重复请求逻辑动画。业务依赖时间变化时，需要明确请求后续逻辑帧。

BatchStats.retained_samples 统计原生路径复用声明的采样次数；declaration_batches/passes、backend_commits 和 layout_runs 可区分实际工作。未执行声明的采样将 declaration_stats 和 declaration_micros 清零，避免报告上一次声明的耗时。标记 retained 不意味着 GPU 已提交，也不意味着跳过全部渲染工作。

## 模拟时钟验证

`crates/plico-vello/tests/retained_sampling.rs` 覆盖：

- 120 次重复采样，show、声明批次、后端提交次数保持不变，Scene 每次复用；无动画请求。
- 控件点击后的 settle、服务更新、尺寸/主题/字体失效、DPI-only 场景更新。
- 逻辑动画持续执行到截止时间，宿主 opacity 动画不重跑 show，结束后保留最终画面。
- 非法声明重试恢复、显式 try_declare 后提交，以及 100 次外部 update 合并一次声明。

`cargo run --release -p plico-vello --example memo_backend_probe` 比较 1000 文本节点在重复绘制、强制声明、单节点颜色更新和宿主动画中的 CPU 开销，检查实际 show/布局/scene 工作量。三轮前后对比的结果记录在 [memo 实验](memo-experiments.md)；不含窗口、GPU 提交、present 或热重载通信。

## 和 MyGO 的关系及边界

本机 MyGO 的 runtime 区分 runFrame 与 repaintFrame，后者保留树并跳过 view/布局；Plico 这一步采用同样的调度分层，但继续使用自身 retained 节点和 dirty journal，不改成 MyGO 的临时 arena 树。

- 外层 shell 本来就会在没有活动动画/失效时等待；本轮减少的是已经收到重复绘制请求后的工作，没有新增持续轮询，也不声称由微秒样本推出 FPS/功耗。
- 普通 smooth scroll 已可在宿主推进；读取滚动几何的容器（含虚拟列表）仍触发声明。详见 [滚动声明分离](scroll-presentation.md)。
- 现有 VirtualList 仍为固定高度、可见范围加 overscan。可变高度测量缓存、行锚点和 prepend 保持位置尚未实现，本轮没有把它们当作已完成能力。
- 单个节点 paint 变化仍可能重新编码整份 Scene。组件 memo、局部命令编码及布局边界缓存也没有因此实现。
