# 呈现生命周期与 MyGO 对照

本轮以 MyGO `ee3d4bd9bd741f197de231b9fd9b002e9db55577` 的现有实现为参考，
按 Plico 的 retained host / wgpu API 迁移对应行为，不引入 Go 运行时或 checkout 依赖。
MyGO 为 MIT，许可证保留于 `crates/plico-wgpu/third_party/mygo-LICENSE`。

| MyGO 参考 | Plico 实现与验证 |
| --- | --- |
| `internal/gpu/metal/metal.go` 的 backdrop 批次先结束当前 pass、读取已绘制背景 | wgpu 延迟执行含背景读取的隔离组，先完成父级前缀，再捕获所需区域，渲染子树并统一混合；不能提前对透明纹理采样 |
| `internal/gpu/gpu.go` 的 `OpGlyphs` 使用 atlas 资源、变换字形几何 | runtime 为等比缩放动画提供稳定栅格密度；原生布局与字形缓存复用，动画仅改变四边形，终点与静态原生文字逐像素一致 |
| `ui/transition.go` 的退出 ghost 保留离场元素及原兄弟顺序 | Modal 关闭时保留最后一次声明的完整子树，不再执行业务内容闭包；保留局部状态，立即屏蔽交互，统一退出后清理 |
| `ui/base.go` 的 `coreDialogBase` 组织 backdrop 和 panel | Modal 统一管理遮罩存在期和面板入场，面板子控件不分别启动存在动画 |

以上是实现对照，不声称 MyGO 已实现 Plico 新增的背景替换合成、缓存密度提示或 deadline API。

## 应用接口

`enter_from(Presentation { opacity, scale, translate })` 声明入场的呈现初值，
配合 `transition` / `motion` 由宿主采样；布局保持目标值。时间来自注入时钟。
原生文字稳定密度复用目前覆盖正向等比缩放；旋转、非等比与任意应用逐帧改字号不在此保证内。

Modal 面板为 140 ms 淡入并上移 8 px，遮罩、内容使用同一时间轴。锚定 popover 不位移，
避免破坏定位。reduced motion 直接呈现最终状态，退出时也不保留动画。
关闭后的 `ModalResponse.inner` 为 `None`；内容闭包不执行，因此清空业务数据不会使退出中的文字先消失。

`request_rebuild_at` 的 deadline 由节点生命周期拥有；重新声明或移除节点会清理旧 deadline。
未到期时复用声明和保留场景；宿主仍可能按 vsync 唤醒，尚不是操作系统级定时调度。

## 合成与内存边界

含背景读取的透明组将完整结果和进入该组前的背景混合一次，避免重复叠加半透明背景。
捕获区域按组输出和采样输入裁剪，按嵌套深度复用 scratch；计入现有 128 MiB 每帧隔离层预算。
保留的背景 scratch 限制为 64 MiB，trim 可全部释放。没有提高预算掩盖分配问题。

背景读取组发生变化时，目前保守重绘父级整帧；静止帧仍可复用。
尚未为这些组建立精确的局部背景依赖图，因此不能声称已完成局部重绘优化。

## 验证

- `plico-desktop` 的 modal / popover / presentation_motion：同步进度、关闭快照、输入屏蔽、重开局部状态、reduced motion 与锚定位置。
- `plico-wgpu` 的 backdrop_groups / layers / effects（需 GPU）：嵌套透明组、先后绘制顺序、透明背景、裁剪、整帧像素对照与内存回收。
- Gallery `wgpu` 的 retained_scale_motion（需系统文字与 GPU）：不重新声明、不布局、不新增原生文字缓存 miss，终点等于静态字号。
- Mines 的规则与连续帧测试：展开和彩纸动画不增删节点、不重新布局；整秒之间不重新声明。

本机 GPU 验证使用 Linux / Vulkan，不能替代 macOS 窗口、CoreText 和原生合成器验收。
Gallery 另有 animated_image_surface 缓存断言失败；已在未修改的 `c03bdd7` 复现，属于独立问题。
