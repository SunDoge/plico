# LVGL 最小后端实验

`crates/plico-lvgl` 是验证 API 可替换后端的小实验，不是完整 LVGL 产品后端。
同一个 `plico-core::Engine` 执行普通 `#[component]` / `+ |cx|` 函数；adapter
把其保留节点映射到 oxivgl 原生对象，由 LVGL 负责布局、文字和绘制。
没有使用 Plico 的 Taffy、Parley、Vello、窗口层或 JSON 热重载通路。

## 运行

使用发布的 oxivgl **0.9.2**（锁定 oxivgl-sys 0.2.6 / LVGL 9.5.0）。上游当前
依赖 heapless 的 nightly feature；本实验不把主线切换到 nightly，而是默认关闭 `native`，
在单独的 nightly CI 任务验证它。首次构建还会由上游下载并校验 LVGL C 源码，
不要把 Cargo.lock 误解为离线构建所需的全部输入。

Debian/Ubuntu：

```sh
sudo apt-get install libsdl2-dev libclang-dev pkg-config python3-png python3-lz4
rustup toolchain install nightly-2026-10-15 --profile minimal
export DEP_LV_CONFIG_PATH="$PWD/crates/plico-lvgl/config"

# 真正的 SDL 桌面窗口，可点击按钮
cargo +nightly-2026-10-15 run -p plico-lvgl --features native --example counter

# 不创建窗口，执行断言并通过 LVGL 截图
cargo +nightly-2026-10-15 run -p plico-lvgl --features native --example counter -- \
  --headless --out /tmp/plico-lvgl.png
```

这些命令在仓库根执行。`config/lv_conf.h` 基于上游默认 host 配置，保留 MIT 许可和来源，
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
- 未支持的 widget 在修改原生树前报错，不静默画成一个假容器。

headless counter 会执行以上核心断言。程序注入的是 LVGL `CLICKED` 事件，实际经过
oxivgl callback，再由 adapter 投递给 Engine；这不是物理触摸或鼠标驱动的自动化测试。
交互模式的鼠标由 oxivgl SDL 驱动处理。一个进程只能初始化一次 LVGL，所有调用都在同一线程。

## 未实现

不映射全部 Style 字段；字体只选内置 16/24 两档，不承诺富文本、字体 fallback、Grid、
SVG、输入框、虚拟列表、shader、motion、IME、无障碍或热重载。没有新增通用 Backend trait，
也不要求应用实现另一套 widget trait。Plico 仍持有自己的声明树，LVGL 持有原生对象树，
本实验尚未测量双份树的内存与更新成本。

ESP32-P4 采用 ESP-IDF + std 路线，入口与构建说明见
[`apps/lvgl-esp32p4`](../apps/lvgl-esp32p4/README.md)。它输出由 BSP 调用的 Rust
静态库，绑定复用 IDF 工程的真实 LVGL 配置，避免第二份 LVGL；显示、触摸、DMA、
tick 和事件循环由 BSP 管理。宿主跑通或交叉编译成功都不能当作 MCU 已运行的证明。
