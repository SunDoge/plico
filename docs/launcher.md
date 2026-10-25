# Launcher 与命令面板

`launcher` 是独立的 `lib.rs + main.rs` 应用，复用当前 Plico 原生后端。
它验证搜索 → 选择 → 确认 → 反馈的完整流程，不扫描机器上的应用，不执行任意命令。
应用目录是固定演示数据，应用条目只预览；切换主题、清除搜索和关闭窗口是真实动作。

```sh
cargo run -p launcher
cargo test -p launcher
# 可选：隔离 X11 原生窗口回归，需要 xvfb-run 与 xdotool
cargo build -p launcher
xvfb-run -a python3 apps/launcher/scripts/check-native.py
cargo run -p launcher --example launcher-capture -- --out /tmp/launcher.png
cargo run -p launcher --example launcher-capture -- --out /tmp/launcher-light.png --light --width 520
cargo run -p launcher --example launcher-capture -- --out /tmp/launcher-empty.png --query zzzzz
```

## 框架接口

输入控件可以保持编辑焦点，同时同步返回结果列表的导航意图：

```rust
use plico_core::prelude::*;

fn search(cx: &mut UiContext, query: &TextBuffer) {
    let response = text_input(cx, "Search", query)
        .autofocus(true)
        .navigation(true)
        .response();
    if response.changed {
        // 更新匹配项；应用自己决定评分和选中业务 ID。
    }
    if let Some(action) = response.navigation {
        match action {
            InputNavigation::Next => { /* 下一项 */ }
            InputNavigation::Previous => { /* 上一项 */ }
            InputNavigation::PageDown | InputNavigation::PageUp => { /* 翻页 */ }
            InputNavigation::Cancel => { /* 清空搜索或关闭 */ }
        }
    }
    if response.submitted {
        // 执行当前选中项；没有长期保存的事件闭包。
    }
}
```

- `autofocus(true)` 仅首次挂载申请焦点，后续声明不会从其他控件抢回焦点。
- `request_focus(bool)` 显式申请一次焦点；应用在点击结果或切换筛选后使用。
- `navigation(true)` 仅将无修饰的 Up/Down/PageUp/PageDown/Escape 路由给该输入；默认关闭。
  Home/End/左右键、复制粘贴等仍然编辑文字。上下键支持按住重复，Escape 不因重复关闭窗口。
- IME 组合期间，导航键和 Enter 留给输入法，第一次 Escape 仅取消组合。
- 导航与提交在 response 中消费后请求稳定化，不会每一帧重复执行动作。
- 自动聚焦产生的原生 text client 会直接启用宿主 IME，不依赖首次点击。
- 窗口重新获得焦点时恢复此前仍有效的控件；不会越过 modal 焦点限制。

`virtual_list(...).reveal(Some(index))` 让指定行可见而不改变输入焦点。
只在键盘选择、筛选等操作发生时传 Some，普通帧传 None，避免对抗鼠标滚轮。
框架同时改变虚拟行的声明区间与后端滚动位置；定位立即生效，滚轮仍可使用 smooth scroll。

原生 `View::on_unhandled_key` 是窗口局部的兜底：先由控件处理，IME 组合及浮层期间不调用。
demo 用它保证 Tab 到其他控件后 Escape 仍可退出。它不是系统级快捷键注册。

`View::take_host_action` 可以发送 `HostAction::Window(WindowCommand::Close)` 等当前窗口命令。
动作不进入 display tree、Serde 状态或重载快照。宿主在声明结束后也会取出动作，
因此按钮和输入 response 触发的动作不必等待下一次输入。
窗口配置复用已有的 `decorations(false)`、`centered(true)`、`window_level(AlwaysOnTop)`。
置顶、居中是否生效仍取决于窗口管理器。

## 验证与边界

- 集成测试覆盖首次聚焦、保持焦点的键盘导航、虚拟列表定位、Enter、鼠标等价的语义点击、
  筛选、大小写与多词搜索、中文 IME、无结果、两阶段 Escape、主题动作与窗口关闭队列。
- 后端测试覆盖聚焦不抢占、显式聚焦、卸载重挂、禁用、失焦恢复，以及浮层/IME 对兜底按键的隔离。
- Linux X11 的 Xvfb 会话中验证真实窗口启动、无需点击直接输入、Escape 清空，再 Escape 关闭进程。
  此验证不代表 Wayland/macOS/Windows 或真实系统输入法已全部验证。
- 已查看深色 680px、浅色 520px 与空结果的真实 Parley/Vello 离屏截图，见 `reports/launcher/`。

当前 demo 使用原生 `View` 和 `TextBuffer`，没有导出可热重载的应用模块。
现有 JSON 模块协议仍不能迁移原生输入绑定，不能因为其他 App 支持 reload 就声称此 demo 也支持。
未来可让宿主持有输入与服务，把可重载视图和搜索策略单独放入模块。

尚未提供系统级召回快捷键、托盘驻留/隐藏再召回、平台应用索引、图标解析、文件搜索、
异步 provider、模糊匹配评分或实际程序启动。它们需要各自的宿主服务与取消/过期结果策略，
不应通过每次 show 扫描文件系统或执行 shell 命令实现。

本地生成图：`../reports/launcher/dark.png`

本地生成图：`../reports/launcher/light-520.png`
