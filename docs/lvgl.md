# LVGL 最小后端实验

`crates/plico-lvgl` 是验证 API 可替换后端的小实验，不是完整 LVGL 产品后端。
同一个 `plico-core::Engine` 执行普通 `#[component]` / `+ |cx|` 函数；adapter
把其保留节点映射到 LVGL 原生对象，由 LVGL 负责布局、文字和绘制。
没有使用 Plico 的 Taffy、Parley、Vello、窗口层或 JSON 热重载通路。

## 运行

使用 lightvgl-sys **9.5.4**（vendored LVGL 9.5.0，与 oxivgl 时代同版本）。绑定是
纯 bindgen 产物，无 heapless/nightly 依赖：`native` feature 直接在 stable 工具链上构建
和验证。oxivgl 安全封装层已移除，widget/样式/事件封装由本 crate 内部实现。
LVGL C 源码随该 crate 打包，不需要另行下载源码；首次构建仍需要获取 Cargo
依赖，以及系统的 C 编译器、libclang 和 SDL2 开发文件。

Debian/Ubuntu：

```sh
sudo apt-get install libclang-dev pkg-config libsdl2-dev
export DEP_LV_CONFIG_PATH="$PWD/crates/plico-lvgl/config"

# SDL2 窗口：鼠标点击，Tab 切换焦点，Enter 激活按钮
cargo run --locked -p plico-lvgl --features native --example counter

# 无窗口模式：执行断言并通过 LVGL 截图
cargo run --locked -p plico-lvgl --features native --example counter -- \
  --headless --out /tmp/plico-lvgl.png
```

这些命令在仓库根执行。lightvgl-sys 自带 LVGL 的 SDL 驱动源码，`native` 构建通过
pkg-config 链接系统 SDL2。窗口目前固定为 480×360；关闭事件先通知 Rust 主循环，
adapter 按子先父的顺序释放控件后程序退出，避免 LVGL 先删除 display 导致悬垂指针。
`--out` 也可用于交互模式，在关闭窗口时保存最后画面。
`config/lv_conf.h` 基于上游默认 host 配置，保留 MIT 许可和来源，
使用宿主 malloc 为离屏截图分配内存；它不是低内存的 MCU 配置。

## 验证内容

只映射 Root、Column、Row、Text、Button/ButtonBase；组合按钮的子文本仍由同一套声明生成。
支持基本尺寸、百分比、间距、padding、
颜色、圆角、边框和 disabled。使用原生 flex，排版细节不保证与 Taffy 相同。

- 普通 Rust 状态在按钮事件后改变，通过核心的同一个 settle 协议重新声明。
- LVGL callback 只通过弱引用队列提交世代 Entity，不捕获应用状态或借用 `UiContext`。
- 文本变化更新原生 label，未变化的节点复用 LVGL 对象；重复 sync 不写样式/文本。
- if 控制原生对象挂载/卸载；带业务 key 的 for 列表重排保留对象身份并移动原生兄弟顺序。
- 卸载按子先父后的顺序释放对象和 callback 路由；迟到事件不会击中地址复用后的新节点。
- 未精确映射的 widget 按 `KindFamily` 降级；降级的交互控件不保证可操作。

headless counter 会执行以上核心断言。程序注入的是 LVGL `CLICKED` 事件，实际经过
LVGL callback，再由 adapter 投递给 Engine；这不是物理触摸或鼠标驱动的自动化测试。
交互模式的鼠标与键盘由 LVGL SDL 驱动处理。一个进程只能初始化一次 LVGL，所有调用都在同一线程。

Linux 上还可用独立的 Xvfb 显示验证真实 SDL 输入和关闭流程（先编译上述示例）：

```sh
sudo apt-get install xvfb xauth xdotool
xvfb-run -a python3 scripts/check-lvgl-sdl.py
```

该测试点击 Increment，再用 Enter 激活，发送窗口管理器关闭消息，断言计数为 2、
进程正常退出且生成 PNG；不会操作当前桌面的窗口。

## 未实现

不映射全部 Style 字段；字体只选内置 16/24 两档，不承诺富文本、字体 fallback、Grid、
SVG、输入框、虚拟列表、shader、motion、IME、无障碍或热重载。没有新增通用 Backend trait，
也不要求应用实现另一套 widget trait。Plico 仍持有自己的声明树，LVGL 持有原生对象树，
本实验尚未测量双份树的内存与更新成本。

ESP32-P4 采用 ESP-IDF + std 路线，入口与构建说明见
[`apps/lvgl-esp32p4`](../apps/lvgl-esp32p4/README.md)。它输出由 BSP 调用的 Rust
静态库，绑定复用 IDF 工程的真实 LVGL 配置，避免第二份 LVGL；显示、触摸、DMA、
tick 和事件循环由 BSP 管理。宿主跑通或交叉编译成功都不能当作 MCU 已运行的证明。

## Native SVG icons

`svg(cx, &SvgAsset)` and button `.icon(&asset)` now project to a native image.
`currentColor` resolves against the node's foreground token; explicit SVG colors
remain unchanged. The LVGL build must enable `LV_USE_SVG`, vector graphics,
ThorVG and snapshots. The bundled host configuration enables these; external
ESP-IDF projects must set their own flags and drawing-thread stack size.

The adapter rasterizes each mounted SVG into a transparent ARGB8888 image of its
explicit pixel size (default 24×24). It retains that image until source, pixel size
or resolved color changes. Unchanged frames do not parse or rasterize again.
Unmount releases the descriptor, pixels and LVGL cache entries. Current support
is static SVG with explicit pixel dimensions, not SVG animation or percentage
raster sizing. `Stats::svg_rasterizations` and `Adapter::svg_cache_bytes()` expose
render counts and retained pixel bytes; the latter excludes object overhead.

The native build supplements lightvgl-sys 9.5.4's C-only build by compiling its
own bundled ThorVG C++ files. It locates that exact installed dependency under
Cargo's registry; custom/vendor layouts can set `PLICO_LVGL_SOURCE` to the same
version's `vendor/lvgl` directory. ESP-IDF already builds these files itself.
The native regression checks raster pixels, transparency, dimensions, recoloring,
source changes, buffer reuse and release on unmount.

Native layout-only objects are not clickable. Otherwise a full-size column inside
an icon/text button consumes pointer hits before the enclosing button receives
them. Scroll containers retain clickability for dragging on empty space; native
buttons/editors retain their own flags. The regression uses LVGL coordinate hit
testing on both the icon and label, rather than only injecting a button event.
