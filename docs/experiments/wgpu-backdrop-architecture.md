# wgpu 背景依赖与同阶段分组

2026-11-09。优先纯 Rust / wgpu，不增加 FFI 适配或将跨语言验证作为前置。
目标是减少玻璃小变化的整帧工作，以及可共享背景阶段的重复资源与预滤波调度。
没有更改材质参数、删减采样核或降低阴影质量。

## 实现

### 有界背景依赖

`SceneDamageTracker::update_indexed_with_backdrops` 是可选 Rust 后端契约。
wgpu 提供 effect 原始 canvas 的物理输入范围，不能按输出的祖先 clip 缩小输入：
shader 可以在自己的 canvas 中采样远处。普通 tracker 调用及未知输入仍保留全量策略。

按画家顺序传播已变化的前缀：背景脏区碰到输入，就使该 effect 输出变脏。
最终重放还必须把与脏区相交的 effect 输入／输出完整恢复，并与文字／描边的原子范围
反复展开到覆盖率固定点。否则部分清空后会把上帧玻璃或后画的标签当成背景，产生反馈。
重叠玻璃可形成连锁依赖，仍可能扩大到整帧；既有 50% 全量回退与未知 filter/blend 策略保留。

不变的窗口帧通过现有 planner 提前返回，不提交或呈现。离屏 `render_scene` 为了图像读回
可能仍提交空 pass；测试分别检查零 instances、零背景工作和 damage coverage。

### 同阶段纹理数组

请求按物理 copy 宽高分组；前瞻检查从阶段起点到候选之前的所有写入，必须不碰候选输入。
绘制顺序不变，相邻阴影或标签可能阻止合并。快照在首个成员的原绘制点捕获，后续成员
各读一个独立 D2 slice，所以 UV、奇数尺寸降采样、边界夹取与独立路径一致。
各 slice 的 mip view 仍按自己的最大 sigma 限制；compute 按层级跨 slices 执行。

同尺寸不代表必然同阶段，不满足条件时使用原来的独立路径。没有纹理图集坐标重映射，
也不把不同尺寸补齐到大图。没有 compute 或足够设备限额时回退。
每组最多 16 slices、32 MiB，前瞻最多 512 instances；无多块背景时跳过规划。

一次 copy 仍对应一个区域，**copy 数与原始读取字节不因此减少**。减少的是背景 texture
allocations、绑定/调度及预滤波 pass。混合 mip 预算的数组按组内最大层数分配，可能比
独立预算略大；32 MiB 分组上限限制这一代价。
单 surface 与数组核统一使用显式 RGBA8 半向上舍入，避免不同 pipeline 的 tie rounding 差异。
另修正最大 sigma 按 device scale 转为物理预算，避免 Retina 低估 mip 数。

普通快照和分组数组现在进入同一个 renderer-owned 精确规格纹理池。同帧已使用的 entry
不会再次取得，避免后画 effect 覆盖仍会被采样的背景；下一帧依赖 queue 提交顺序安全覆写。
尺寸、array layers、mip 数或用途变化会替换空闲 entry，不积累逐尺寸历史；idle cache trim
会释放整个池。这里不采用放大纹理或尺寸分桶，避免额外区域进入 mip 边缘采样。
真实 GPU 回归覆盖独立与数组路径：稳定四帧不增加 allocation、输出逐字节一致，trim 后释放并重建。

Backdrop shader 的 UV mapping 与最终 composite clip/mix 数据也不再各自逐 draw 创建小 buffer。
Compositor 维护按帧占用的 uniform arena：一个 entry 包含两个按设备
`min_uniform_buffer_offset_alignment` 隔开的区段，跨帧只用 `queue.write_buffer` 覆写。
同一 atomic group 的多个 damage scissor 共享一个 entry；不同 draw 在同帧仍使用不同 entry，
避免提交前的后一次写入改变前一次 draw。idle trim 释放 arena，但保留 compositor pipeline。
`LayerPoolStats` 暴露 entry、累计 allocation 与 reuse，GPU 回归断言稳定帧不再增长。

## 像素及边界验证

`crates/plico-wgpu/tests/backdrop_damage.rs` 对照分组/局部与独立/全量渲染，要求完整 RGBA 相等：

- 不变玻璃、玻璃外小变化、玻璃内的背景和前景编辑。
- 重叠依赖链、面板移除、前景不能反馈进背景。
- 圆角 mask、裁剪离屏 opacity group、1/1.5/2x。
- 同尺寸不同 sigma、奇数轴、交错前景，改变局部 array 成员。
- 中间写入和重叠必须拆阶段；AA 边界外的未来成员不能提前复制。

纯规划单测覆盖尺寸/阶段分裂、16-slice 与 32-MiB 上限。
renderer 原有 damage、wgpu effects / layers / prefilter，以及桌面材质、交互玻璃、scroll clip
回归均通过。没有以 Linux 像素测试替代 macOS 实机或窗口事件循环验收。

## 同进程 A/B

以下数据测于跨帧纹理池加入之前；它只比较当时的分组与独立路径，不能当作纹理池收益。
Intel Graphics ARL、Mesa 25.0.7、Vulkan。固定 640×400、16 个 128×64 面板、sigma=8。
预建相同 Scene 序列；四个 renderer 共用 device/queue，正反交错 full/partial 和
separate/batched，预热 6 帧、每模式测 32 帧。走 `render_scene_to_view` 加 GPU 完成等待，
含场景准备和最终 blit，无图像回读；这是端到端墙钟，不是 GPU timestamp，也不包括应用声明、
布局和真实窗口呈现等待。结果是一次本机探针，不是 Plicowarden 全应用速度承诺。

| 场景 | 全量/独立 | 全量/分组 | 局部/独立 | 局部/分组 |
|---|---:|---:|---:|---:|
| 光标在玻璃外 | 4.822 ms | 3.235 ms | 0.657 ms | 0.635 ms |
| 光标在一块玻璃内 | 5.185 ms | 3.420 ms | 1.015 ms | 1.057 ms |

对应确定性工作量：

| 路径 | copies | mip passes | 背景 textures | 背景 payload | 场景 passes |
|---|---:|---:|---:|---:|---:|
| 全量/独立 | 16 | 48 | 16 | 696,320 B | 17 |
| 全量/分组 | 16 | 3 | 1 | 696,320 B | 17 |
| 玻璃外局部变化 | 0 | 0 | 0 | 0 B | 1 |
| 一块玻璃内局部变化 | 1 | 3 | 1 | 43,520 B | 2 |

外部变化覆盖率约 0.049%，内部变化因输入恢复扩大为约 3.51%。
只有一块玻璃重放时没有批处理收益；0.04 ms 差异不作为回退结论。
全量合成仍每块执行 shader 输出和合成，下一阶段可以单独测量中间目标消除；
GPU instance 增量、通用连续图元和帧内资源池未在这批实现。

## 复跑

```sh
cargo test -p plico-renderer --lib damage
cargo test -p plico-wgpu --lib backdrop_batch
cargo test -p plico-wgpu --test backdrop_damage -- --ignored --skip profile_backdrop
cargo test -p plico-wgpu --test backdrop_damage profile_backdrop_architecture -- --ignored --nocapture
cargo test -p plico-wgpu --test effects --test layers --test backdrop_prefilter -- --ignored --skip profile_many
cargo test -p plico-desktop --test materials --test interactive_glass --test scroll_clip -- --ignored
```

`Renderer::force_separate_backdrops(true)` 可关闭分组做像素/性能对照；默认启用。
batch stats `backdrop_textures`、`backdrop_shared_reads` 区分本帧纹理资源与读者数，
`backdrop_copies` 仍按实际 copy 计数。原始日志/截图只放忽略的 reports，不提交。
