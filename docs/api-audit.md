# Plico API 审查

遇到表面重叠时，按调用方是否能表达不同操作来判断。用真实调用点、输入差异、
错误边界和运行时行为验证。一个入口如果只把参数原封不动转发到另一个入口，
通常可以在保留原能力的前提下收敛。

## 已收敛

| 范围 | 调用方现在使用 |
| --- | --- |
| 桌面渲染选择 | wgpu 由 crate feature 提供。窗口、截图、预览与视频导出使用同一个维护中的后端，旧窗口 JSON 的额外 `renderer` 字段仍可读取 |
| 重复预览 | 单次预览用 `PreviewScenario::render`，批量预览用 `render_in` 传入 `CaptureSession::default()`。自定义帧源用 `render_source` 或 `render_source_in` |
| 帧统计 | `plico_renderer::FrameBudgetSample` 贯通 wgpu、桌面窗口和离屏捕获 |
| 原生宿主服务 | `run_source(Source::new(ServiceView::new(app, service)), options)`，错误为 `DesktopError` |
| Toast | `ToastHost::add(Toast::message("Saved"))`。统一受控的 `Toast` 参数、队列和 ID 续期 |
| 弹簧计算 | 声明通道与渲染通道共享解析采样和停止判据。声明通道负责延迟与动效偏好，渲染通道负责帧间保留与完成通知 |

## 控件入口迁移

- 布局、文字、素材、表单和弹层统一从 `plico::widgets` 构造，`prelude` 可直接导入。
  `ui.column()` → `column(ui)`，`ui.text(label)` → `text(ui, label)`，
  `text_empty(ui)` → `text(ui, "")`。宿主重放继续使用 `element`、`shader_element` 等底层 API。
- `ui.button(label)` 和独立的 `widgets::Button::new(label)` → `button(ui).text(label)`。
  标准按钮共用一个容器绘制文字、图标和自定义子内容，沿用容器的焦点与无障碍响应。
  `button_base(ui, name)` 仍负责带自定义外观的按钮行为。
- `switch(ui, label, value)` 和 `checkbox(ui, label, value)` 可继续链式配置，
  用 `.key(id)` 设置列表身份，用 `.changed()` 读取变化。旧的独立值和 `UiContext` 方法已移除。
- `slider` 和 `range_slider` 返回同一种可配置 builder。双拇指版原来的布尔返回值改为
  `.changed()`，与单值版一样报告输入引起的变化。单拇指玻璃滑块公开为 `liquid::glass_slider`。

## 弹簧和原生启动迁移

- 声明端的 `SpringOptions` 保留时长、回弹、精度及初速度，`physics()` 可把参数传给
  渲染端。旧 `plico_motion::SpringOptions` 迁移到 `SpringPhysics`，把 `epsilon` 改为
  `precision`。描述符的 JSON 字段和各自的默认值保持兼容。
- `plico_desktop::run(app, WindowOptions)` 启动 App，`run_view(view, WindowOptions)`
  启动 View，`run_source(source, WindowOptions)` 接受任意帧源。自定义 HostView 可通过
  `Source::new(view)` 转成帧源。CLI 创建项目的模板也使用同一组入口。
- 工厂、桌面扩展和持久化通过 `EventLoopOptions` 与 `run_windows` 组合。
  `persist_app` 接收初始窗口的资源键和选项，恢复窗口位置并注册应用状态持久化。
  单窗口和多窗口共用一条事件循环启动路径。
- 窗口宿主直接持有初始化完成的 wgpu surface。初次显示和常规呈现共用一个入口，
  surface 尺寸只在初始化和实际调整时更新。

## 帧构建与导出迁移

- 宿主通过 `FrameSource::render_frame(scene, text, (width, height), scale)` 提交一帧。
  体积较小的帧源可保留默认布局和绘制；运行时直接绘制留存场景。快照检查继续使用
  `build_frame`、`try_build_frame`，预览脚本只提交布局时可用 `prepare_layout`。
  对照探针可用 `Source::try_build_and_paint` 强制采样声明。
- 原生控件从这次渲染调用接收 DPI。桌面和离屏路径复用这一契约。原生宿主遥测统一使用
  `frame/build.cpu`。移除了只供测试使用的独立 headless profiler。
- 单帧用 `capture(source, CaptureOptions)`。连续捕获、预览和 recipe 共用 `CaptureSession`。
  每个脚本场景仍创建自己的应用、时钟和文字上下文。合成导出在连续采样间保留文字缓存。
- 视频编码选项集中在 `VideoOptions`。帧范围和进度由 `FrameOptions` 控制，
  可用于帧流、PNG 序列和视频。视频先编码到同目录的独立临时文件，成功后再发布。
- 测试协议统一用 `testing::Tester`，窗口尺寸和位置恢复统一用 `WindowStatePersistence`。
  旧窗口状态文件仍由同一份反序列化逻辑读取。

## 宿主动作迁移

- `HostView`、原生 `Widget` 和 `FrameSource` 统一通过 `EffectRequest` 与宿主通信。
  `EffectQueue` 分配请求 ID。嵌套原生控件的帧源负责分配外部 ID，将乱序完成事件按原
  ID 交还控件或视图。终端剪贴板读取沿同一路径回传。
- 窗口、剪贴板和 URL 由桌面和 Android 各自的一条 effect 分发路径执行。
  `EffectPayload::window(command)` 操作来源窗口，也可以在 `WindowControl` 中指定
  目标窗口。终端使用 `RestoreTitle` 恢复窗口初始标题。
- Effect 协议版本升至 9。已有窗口 ID 和操作编号保持兼容。显式目标窗口的 JSON 形状
  保持兼容，省略目标窗口 ID 时使用来源窗口。Bevy 保留文本输入的剪贴板事件接口，
  移除了普通 `View` 无法产生的宿主动作消息。

## 保留的职责边界

- `View` 声明界面，`App` 提供应用生命周期，`HostView` 接收宿主能力，
  `FrameSource` 面向通用渲染宿主。窗口启动用对应的适配器组合这些接口。
- `build_frame` 导出布局快照，`prepare_layout` 提交供事件与查询使用的布局，
  `render_frame` 生成场景。按调用方需要的产物选择。
- 普通文本编辑可在一次输入分发内完成读取后粘贴。终端等原生控件通过 effect 请求，
  在稍后的完成事件中读取原请求 ID。
