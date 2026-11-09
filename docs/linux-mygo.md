# Linux 平台：MyGO 对照

本轮参考本地 MyGO `ee3d4bd9bd741f197de231b9fd9b002e9db55577`，优先共享
成熟的行为和资源生命周期。MyGO Linux 宿主是 GTK/GtkGLArea，Plico 是 winit/wgpu；
不复制 GTK 私有结构偏移或引入第二套窗口事件循环。

## 本轮落地

- 对照 `internal/linux/surface.go` 的当前 widget allocation / render 路径：
  resize 事件采用窗口当前实际 surface 尺寸，跳过已经过时的中间尺寸。
  同一次事件内完成几何同步和绘制，不再先配置旧尺寸、绘制时再配置新尺寸。
  单窗口和多窗口共用处理函数；多窗口先通知扩展再绘制。
  重复、过时及零尺寸事件不额外绘制，未改变的 metrics 不重复通知扩展。
- 将已有 MyGO Metal 空闲回收原则适配到 Linux wgpu：两秒无新画面时释放保留
  offscreen target、读回、路径、图片、隔离层、效果和文字派生缓存。
  compositor 持有已呈现的 swapchain 图像；Linux 保持真实 surface extent，
  **不**应用 macOS 的 1×1 drawable 配置。下次绘制强制重建完整像素。
  使用现有 WaitUntil 单次唤醒，不新增周期轮询。应用字体和 pipeline 保持热态。

## 调度与输入核对

MyGO `internal/e2e/frame_clock_linux_test.go` 专门检查高刷屏不被 GTK 的 60 Hz
默认值限速。Plico 不使用 GTK frame clock：Wayland 由 winit 的 frame callback
配合 `pre_present_notify` 调度，wgpu 使用 FIFO 呈现。跳过未变化帧时的退避从当前
monitor 的 refresh rate 计算，未知时才回退 60 Hz。这里只确认实现路径，
没有声称已完成 165 Hz 或多显示器运行验证。

MyGO 精确滚动区分触控板与离散滚轮。Plico 已将 Pixel 输入交给直接滚动并接管
旧插值，Line 输入仍可平滑；这轮没有再次添加惯性或额外平台插值。

## 验证与剩余差异

Xvfb 原生 X11 窗口回归实际创建 winit/wgpu 窗口，验证中间尺寸不回退、单窗口
事件立即绘制、重复/零尺寸不绘制、多窗口共享处理路径、空闲回收不声明 UI，
以及回收后再次呈现并重新安排维护。运行方式：

```sh
xvfb-run -a cargo test --locked -p plico-shell --lib native_resize_uses_latest_extent -- --ignored
```

保留目标重建的 GPU 像素一致性另由 `trimming_idle_target` 回归覆盖。Xvfb 不能
证明真实 Wayland compositor 的遮挡、分数缩放、实际高刷节奏或进程 RSS 降幅。

后续继续对照 MyGO `internal/linux/system.go` 的系统辅助功能偏好（动画和高对比度）
以及 Wayland/X11 交互。当前原生偏好适配仍主要在 macOS；Linux 未实现的能力
不列作已经完成。
