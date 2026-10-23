# ESP32-P4 / ESP-IDF 最小实验

普通 Rust 状态、`#[component]`、`+ |cx|` 和 if 创建计数器，通过同一个 Plico
Engine / LVGL adapter 更新真实 LVGL 控件。当前还包括信号柱和进度条动画，支持触屏暂停。
ESP-IDF BSP 负责初始化、显示、触摸、时钟和 LVGL 任务。

Waveshare ESP32-P4-WIFI6-Touch-LCD-4.3 已实际运行，用户验证触屏与动画。
完整宿主在 `idf/`；构建/仅更新 app：

```sh
python3 scripts/build-esp32p4-firmware.py
python3 scripts/flash-esp32p4.py
```

该固件仅演示 GUI，不包含网关业务。烧录脚本先验证现有分区、备份原 app，
保留 bootloader、分区表与 NVS。资源数据与实验范围见
[上板实测](../../docs/p4-counter-measurements.md)。下面是独立静态库接入方式。

### 原生动画对照

```sh
PLICO_P4_ANIM_BENCH=1 python3 scripts/build-esp32p4-firmware.py
python3 scripts/flash-esp32p4.py
```

该可选固件在挂载后自动执行 Plico 22 秒（包含启动自测）、LVGL 原生动画 20 秒、
静止 10 秒，之后恢复 Plico。屏幕底部和串口 CPU 日志均标明模式。
原生阶段使用同一批控件、相同正弦公式及约 25 Hz 更新频率，仅由 `lv_anim`
回调设置尺寸；暂停调用 Plico update，避免两边同时控制属性。
测试期间不要操作按钮；这不是生产用的混合动画控制 API。
不设置该环境变量重新构建，即恢复普通交互演示。

## 构建

需要 nightly-2026-10-15（含 rust-src）、ESP-IDF 5.5.5 的 RISC-V GCC、libclang，
以及已经完成 CMake configure 的 IDF 工程。工程必须使用 LVGL **9.5.0**，
导出 `compile_commands.json`，并启用本目录 `sdkconfig.defaults` 中的选项。
不要同时链接 oxivgl 自带的第二份 LVGL。

在 Plico 根目录执行：

```sh
rustup component add rust-src --toolchain nightly-2026-10-15
export PLICO_IDF_BUILD=/absolute/path/to/idf/build
export PLICO_CC=/absolute/path/to/riscv32-esp-elf-gcc
python3 scripts/build-esp32p4.py
```

脚本只读取 IDF 编译数据库及头文件。在本仓库忽略的 `target/esp32p4/` 解包
lightvgl-sys 9.5.4（按工作区 Cargo.lock 校验 registry archive），应用 ESP-IDF 构建适配，
生成实际 sdkconfig 对应的绑定并且只编译 inline wrappers。LVGL C 库来自 IDF，
没有编译 lightvgl 自带的第二份 C 库。不会修改输入 IDF 工程，
不会读取网关私有配置，也不烧录设备。首次需要联网；嵌入式依赖使用隔离的 Cargo.lock，
不改变桌面 workspace 的依赖解析。

输出：`target/esp32p4/build/riscv32imafc-esp-espidf/release/libplico_lvgl_esp32p4.a`。
这是带 std 的目标静态库；也由上面的完整固件构建脚本调用。

## 接入 BSP

在 IDF 组件中使用 `add_prebuilt_library` 导入上述 archive，链接到调用组件，并把
`plico_p4.h` 放入 include 路径。必须在 **同一个 GUI 任务、持有 LVGL 锁** 时：

1. BSP 初始化 LVGL、显示及触摸后，调用 `plico_p4_create(width, height)`，检查返回值。
2. GUI timer 中调用 `plico_p4_update(time_ms)`，由宿主注入毫秒时间；LVGL 输入回调
   先排队 Entity。动画约每 40 ms 触发声明；暂停且没有点击时不执行 show 或 sync。
3. 关闭时先停 update timer，再调用 `plico_p4_destroy()`，然后才能删除 screen。

单实例，挂载在当前 active screen 下，不接管整个 screen 的所有权。当前没有任意
父容器挂载 API；不要假设能直接嵌入 Brookesia app 的 content 容器。
通过 `unsafe { Driver::attach() }` 取得外部初始化凭据，不调用 `Driver::init()`
或再启动 timer_handler；初始化和事件循环由 BSP 管理。违反 GUI 线程或 screen
生命周期约定可能造成内存错误。

## 来源与范围

ESP-IDF 外部 LVGL 绑定适配源自本机 air780-gateway 的 P4 factory 实验，在 Plico 内
独立维护，正常桌面构建不依赖那个 checkout。lightvgl-sys 保留上游 MIT 许可。
这里不复制网关业务、网络探针、SIP 或私有配置。

当前验证范围为该板的显示、GT911、最小控件和上述动画，不承诺完整 LVGL
widget / style 覆盖、热重载或 ESP32 全系列支持。
