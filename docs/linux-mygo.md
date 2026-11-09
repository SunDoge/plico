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

系统辅助功能偏好已接入，详见下文；真实 Wayland 高刷、分数缩放和各桌面后端的
偏好暴露情况仍需验证，未验证的能力不列作已经完成。


## Linux 原生显示偏好

参考 MyGO `internal/linux/system.go` 的 `Preferences` / `watchPortalSettings`，
Linux 宿主共享一个后台 portal 观察服务。初始 ReadAll 和 SettingChanged 订阅通过
zbus 完成；UI 只读取快照，并由合并的 wake 回调接收变化，没有绘制期 D-Bus 查询
或周期性轮询。订阅先于读取，避免初始化期间漏掉设置变化。

- `org.freedesktop.appearance` 的 `reduced-motion` 和 `contrast` 接入共享偏好。
- portal 暴露 GNOME 设置时，兼容 `enable-animations`、`gtk-theme` 和
  `org.gnome.desktop.a11y.interface` 的 `high-contrast`。
- 减少动画或高对比度的任一来源提出请求便生效；portal 的“无偏好”不取消 GNOME
  已提出的请求。非法类型和无关设置忽略，恢复设置后可撤销对应来源的请求。
- 多窗口复用同一服务，订阅使用弱引用。最后一个宿主释放服务时主动关闭连接，
  解除阻塞的信号监听；UI 不等待线程 join 或远程方法答复。

框架已有的语义主题高对比度、Modal 减少动效和应用自身减少动效选项的合并逻辑
直接消费这些 WindowMetrics，应用无需重复实现或保存系统偏好。

初始读取异步进行，首帧可能先使用默认值；portal 不存在或不可用时保持默认值，
不会阻止窗口创建。尝试通过总线激活惰性 portal，但不引入 GTK 设置依赖。
portal 启动较晚或更换进程时，NameOwnerChanged 会触发完整快照更新；恢复前保留
最后的合法偏好。session bus 本身断连后尚未实现重连，也未实现 Linux 原生减少透明度、
accent-color 或 text-scaling-factor。GNOME 兼容键取决于当前 portal 是否暴露。

验证覆盖真实私有 D-Bus 上的初始快照、变化信号、唤醒合并和连接关闭，以及
无偏好回退、恢复、非法类型和无关字段。可重建命令：

```sh
dbus-run-session -- cargo test --locked -p plico-shell --lib portal_initial_snapshot_signals -- --ignored
```

该测试使用 mock Settings 服务，证明协议和生命周期，不等同于 GNOME/KDE 实机设置面板验证。


### Portal 生命周期与慢快照

设置和服务所有者变化均在同一个后台异步循环中处理。匹配规则可以在 portal 尚未
拥有总线名称时注册；注册后才解析当前所有者并读取快照，避免初始化的所有者切换
造成永久失联。换进程后重新读取完整快照，清除旧后端不再提供的键。

ReadAll 待完成时继续消费信号，避免队列积满后连方法回复也无法前进。每个已知偏好
记录信号版本；快照只更新读取期间没有新信号的键，因此旧回复不会覆盖新设置。
忽略旧进程的迟到信号。没有为恢复服务增加重试计时器或额外连接/观察线程。

额外的私有 D-Bus 回归覆盖服务延迟出现、进程替换后的快照和继续收信号，以及
ReadAll 暂停期间的 128 条变化消息；后者确认消息被持续消费且旧快照不回退最新设置。

```sh
dbus-run-session -- cargo test --locked -p plico-shell --lib linux_preferences -- --ignored
```


宿主多击配置已通过共享 FrameSource / Widget 契约下传，Linux 暂用 500ms / 4 逻辑
像素 fallback；本轮尚未对接 GTK/XSettings 的双击设置。macOS 的 NSEvent 间隔
适配与虚拟时钟/终端覆盖范围见 `macos-mygo.md`。

## 编辑框选择复用原生布局

对照 MyGO `ui/editor_navigation.go` 的 press / drag 和初始选择锚点，补齐
普通输入框及多行编辑框的字符拖选、双击选词后按词拖选、三击选视觉行。
反向拖动仍保留最初选中的词/行；Shift 扩展原锚点。使用共享多击阈值和注入时钟，
不在控件中读取系统时间。密码框只提供掩码位置，按词/行操作选择整个秘密字段。

Linux 从已有 Pango LayoutIter 获取原始 UTF-8 视觉行范围，复用同一份排版，
不通过左右极端 x 坐标猜测双向文字的行边界。Parley 和 macOS CoreText 提供同一契约；
其他未提供视觉行的后端回退逻辑行。布局范围中的硬换行符不纳入单行选择。

回归覆盖虚拟时钟下的双击与三击、折行、拖选反向、按住时离开窗口后继续拖选，
以及密码掩码和 Unicode 组合字符。Pango 原生测试额外核对中文、阿拉伯文、
1/1.25/2 倍缩放和截断行的范围。尚未实现指针停在边缘时的持续自动滚动。

不可见窗口的直接 redraw 现在也经过共享呈现门控：后台 source/host 工作及显式截图
保留，普通窗口帧等恢复可见再提交；截图临时重建的 GPU 资源在不可见时立即回收。
Xvfb 原生回归覆盖遮挡、生命周期隐藏和恢复，以及后台轮询与截图完成。
完整行为及测试范围见 `macos-mygo.md` 的“不可见窗口不因后台 redraw 恢复呈现资源”。
