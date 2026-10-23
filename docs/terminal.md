# 原生 Terminal

Wabou terminal 已复制到本仓库的 plico-terminal-widget，提供 Plico 原生终端示例。
原 Wabou checkout 没有被修改；来源 commit 与复制前文件哈希在
[TERMINAL-UPSTREAM.json](../backend/TERMINAL-UPSTREAM.json)，许可证沿用 backend/LICENSE。

## 运行

```sh
cargo run -p plico-terminal
```

默认启动用户 shell。也可以直接指定程序及 JSON 参数数组：

```sh
cargo run -p plico-terminal -- --command /bin/sh --command-args '["-l"]'
```

参数数组直接传给 PTY 程序，不由 CLI 拼接成 shell command。
终端本身运行 shell 时，该 shell 仍按正常规则解释用户键入的命令。

示例用 Plico children 构建标题栏和布局，TerminalWidget 保持常驻。点击 Compact chrome
或调整窗口大小会更新布局与 PTY 网格，不重建终端进程。标题与退出事件回流到 Rust view。

## 依赖和迁移边界

终端继续使用 rio-vt 处理 VT/grid/scrollback，teletypewriter 驱动 PTY，
rio-graphics 处理已有 Kitty 图形能力。它们只由可选终端 crate/示例引入，
普通 Plico/gallery 不依赖终端。未恢复 Wabou 的 JS runtime、bindgen、
wabou-host-api、wabou-shell-api 或 wabou-protocol。

原事件/Widget contracts 改用 plico-renderer，旧事件号仅保留为原生 WidgetNodeEvent
兼容常量。时钟改接 Arc<dyn plico::Clock>，内部桥接到既有原生 Instant deadline。
光标和选择自动滚动继续使用注入时钟；PTY 等待/子进程回收的机械超时不改为 UI 时钟。

## SurfaceSource

plico-backend::SurfaceSource<V,W> 把一个宿主拥有的 Widget 放进一个唯一静态 key 的布局 slot。
终端对象不放进 UI checkpoint。slot 的尺寸由 Plico 布局提供，窗口缩放后更新本地几何，
widget scene 放在该节点的绘制位置，保留场景的裁剪和上下绘制顺序。

该第一版针对普通、显式尺寸的布局 slot，不提供多 surface 注册系统、intrinsic-size
反向布局或经过 transform 的 native surface。它保留渲染宿主中的终端生命周期，
当前 terminal 示例仍是静态链接应用，没有增加终端的 dylib 热重载入口。

路由支持焦点、局部指针/滚轮、拖动捕获、键盘、文本/IME 事件与候选框基础几何。
已有 widget 的剪贴板 HostAction 转为 typed effects，再路由结果到原 widget request id。
上层 Plico 控件优先命中；模态范围参与 surface 输入检查。
IME、图形和系统剪贴板并未宣称在所有桌面平台完成端到端验收。

## 验证

```sh
cargo test -p plico-terminal-widget --lib
cargo test -p plico-backend --test native_surface
cargo build -p plico-terminal
xvfb-run -a python3 apps/terminal/tests/native_smoke.py
```

保留并通过原有 99 项终端测试：VT、PTY、选区、剪贴板、Kitty 键盘/图形、
字体网格、高 DPI、注入时钟和子进程回收。
新增 surface 测试覆盖局部坐标、IME 几何、可见性恢复不重建实例、上层控件命中和剪贴板 effects。

Linux X11 实际窗口验证：键盘输入经 PTY 到达子进程；Plico UI 更新后进程继续运行；
窗口缩小时网格由 34×115 更新为 25×92；通过 WM_DELETE_WINDOW 正常关闭并退出。
可设置 PLICO_TERMINAL_CAPTURE 指定本地截图路径，截图不参与提交。
其他平台目前只有继承实现，没有新增本机实测结论。
