# plico-terminal

Plico 的原生终端控件，使用 rio-vt / PTY、Parley 与 Plico 的保留绘制场景。
不依赖原 Wabou checkout、JavaScript host 或旧 UI protocol。

`TerminalWidget::spawn_default_shell()` 启动默认 shell，`spawn(command, args, cwd)`
用于指定程序。`headless(columns, rows)` 供确定性解析/显示测试使用；`feed` 输入终端字节，
`visible_line` 和 `selected_text` 可读取显示文本。

终端由宿主窗口持有；UI 声明负责 headerbar 等应用界面。PTY 会话在控件卸载时清理，
具体进程组退出和平台限制见 [终端文档](../../docs/terminal.md)。

```sh
cargo run -p terminal
cargo test -p plico-terminal
```

源代码从 Wabou 复制后由 Plico 独立维护。原始来源与许可记录见
[TERMINAL-UPSTREAM.json](../../backend/TERMINAL-UPSTREAM.json) 和
[后端许可证](../../backend/LICENSE)。
