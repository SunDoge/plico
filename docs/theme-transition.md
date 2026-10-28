# 主题切换与呈现动画

默认 gallery 支持 200 ms 圆形揭示。点击切换时以指针位置为原点，键盘操作使用窗口中心。动画只改变呈现，语义状态与命中几何立即提交；中途反向切换从最近呈现的画面开始。resize/DPI 改变结束旧尺寸动画。结束后 has_anim=false，恢复事件驱动休眠。

Source::set_theme_transition_duration(Duration::ZERO) 可以关闭过渡。当前尚未读取操作系统 reduced-motion 偏好；这是手动关闭接口。

## 卡顿排查与修复

原 ReloadView watcher 接受目录中的所有事件，包括读取 active descriptor 引发的 Access 事件。读取又唤醒、再读取可能形成无效刷新循环。现仅接受 descriptor 的 create/modify/remove（包括原子 rename）事件，忽略读取及其他构建产物。

原 ReloadView 在每次 show 时都调用动态模块、创建临时 Engine、序列化整棵树；现在 load/activate 或实际点击时调用模块，其余请求复用已经稳定的 ViewNode。未来时间、异步任务驱动的应用更新必须显式接入协议，不能依赖 hover 偶然执行 app。

原 native shell 在 build_frame 之前读取主题背景色，可能把旧背景与新内容混在同一帧；现按提交后的状态编码场景。

## 实现与代价

复用现有 Vello clip 与 retained Scene。目标主题只进行一次声明、布局和文字 scene 编码；纯动画帧复用目标 scene 与旧 scene，CPU 更新圆形裁剪，GPU 绘制两个场景。没有每帧插值文字颜色，因此不会制造一串颜色不同的 Parley layout cache key。没有新增自定义 WGSL，也不是把两个窗口截图上传为纹理。

全屏淡入在 light/dark 中间帧会降低文本对比度，故采用空间揭示。shader 可以进一步做圆边、噪声等效果，但双纹理转场需要额外的离屏纹理生命周期和 render pass，当前没有实现。现方案仍有 scene 命令复制和双场景绘制开销，不承诺大界面恒定成本。

## 证据

模拟时钟覆盖起止、中途反向、resize、关闭动画；Source 集成测试确认动画帧不调用应用、几何保持稳定、动画期间的点击与 prepare_for_event 不会复用过期 scene。`PLICO_THEME_REPORT_DIR=/absolute/path cargo test -p plico-desktop --lib` 可生成 0/100/200 ms 的真实 Vello PNG，当前产物在 reports/theme-transition，已查看中间帧。

cpu-probe.json 是本机单次静态 gallery 的 build/scene 编码计时。主题切换的声明约 7 us、同步约 11 us、布局/flatten 约 6 us、scene 约 227 us；不包含 GPU/present，不能据此声称已定位用户设备上的全部卡顿。真实桌面流畅度与 GPU 驱动仍需实际运行观察。热更新端到端回归报告在 reports/hotreload，全部断言继续通过。

宿主代码修改后需要退出并重新运行 `cargo plico dev`；仅修改 gallery 源码的热更新不会更新正在运行的 host。
