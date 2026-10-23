# ESP32-P4 / ESP-IDF 最小实验

普通 Rust 状态、`#[component]`、`+ |cx|` 和 if 创建计数器，通过同一个 Plico
Engine / oxivgl adapter 更新真实 LVGL 控件。这里输出 Rust **静态库**，不是独立
可烧录固件。ESP-IDF BSP 负责初始化、显示、触摸、时钟和 LVGL 任务。

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

脚本只读取 IDF 编译数据库及头文件。在本仓库忽略的 `target/esp32p4/` 下载固定
oxivgl 提交 `ae2328976dab755bd29d97f45b3e8c4253b9adc2`，应用 ESP-IDF 构建适配，
生成实际 sdkconfig 对应的绑定并编译 inline wrappers。不会修改输入 IDF 工程，
不会读取网关私有配置，也不烧录设备。首次需要联网；嵌入式依赖使用隔离的 Cargo.lock，
不改变桌面 workspace 的依赖解析。

输出：`target/esp32p4/build/riscv32imafc-esp-espidf/release/libplico_lvgl_esp32p4.a`。
这是带 std 的目标静态库；最终 IDF 链接、镜像大小、运行内存与触摸仍需另行验证。

## 接入 BSP

在 IDF 组件中使用 `add_prebuilt_library` 导入上述 archive，链接到调用组件，并把
`plico_p4.h` 放入 include 路径。必须在 **同一个 GUI 任务、持有 LVGL 锁** 时：

1. BSP 初始化 LVGL、显示及触摸后，调用 `plico_p4_create(width, height)`，检查返回值。
2. GUI timer 中调用 `plico_p4_update()`；LVGL 输入回调先排队 Entity，update 才触发
   Plico 声明。没有点击时不执行 show 或 sync。
3. 关闭时先停 update timer，再调用 `plico_p4_destroy()`，然后才能删除 screen。

单实例，挂载在当前 active screen 下，不接管整个 screen 的所有权。当前没有任意
父容器挂载 API；不要假设能直接嵌入 Brookesia app 的 content 容器。
不要调用 oxivgl 的 `LvglDriver::init()` 或再启动 timer_handler；初始化和事件循环
已经由 BSP 管理。违反 GUI 线程或 screen 生命周期约定可能造成内存错误。

## 来源与范围

ESP-IDF 外部 LVGL 绑定适配源自本机 air780-gateway 的 P4 factory 实验，在 Plico 内
独立维护，正常桌面构建不依赖那个 checkout。oxivgl 保留上游 MIT / Apache-2.0 许可。
这里不复制网关业务、网络探针、SIP 或私有配置。

后续上板目标为 Waveshare ESP32-P4-WIFI6-Touch-LCD-4.3。已有实验对该板的显示和
GT911 验证，不等于此 Plico 示例已运行。当前不承诺完整 LVGL widget / style 覆盖、
热重载、性能数据或 ESP32 全系列支持。
