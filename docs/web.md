# WASM / 浏览器宿主

Plico 现在可以在 wasm32-unknown-unknown 上编译并运行 gallery。plico-web-host 使用现有
plico-runtime::Source 与 plico-renderer::web::WebRenderer，后端为 WebGL2 / Vello。
组件、身份、失效分类、Taffy、文字、SVG 与 motion 共用原生实现，没有另建 DOM widget API。

## 运行 gallery

需要 Rust WASM target 和与 Cargo.lock 匹配的 wasm-bindgen CLI。目前锁定的版本为 0.2.129：

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
crates/plico-web-host/web/build.sh
python3 -m http.server 8080 --directory target/plico-web-host
```

打开 http://localhost:8080 。build.sh 可以通过 WASM_BINDGEN 环境变量指定 CLI 路径。
生成的 WASM、JS、字体和 HTML 全部放在已忽略的 target/plico-web-host，不提交生成文件。
演示字体使用仓库中已有 DejaVuSans.ttf，同时复制原许可证。

只检查核心和浏览器目标：

```sh
cargo check -p plico --target wasm32-unknown-unknown
cargo check -p plico-web-host --features gallery --target wasm32-unknown-unknown
cargo clippy -p plico-web-host --features gallery --target wasm32-unknown-unknown -- -D warnings
```

不要对原生桌面应用执行整个 workspace 的 wasm all-targets 构建。CLI、原生窗口、
动态库 watcher 和后台 std::thread 示例仍是原生目标。浏览器宿主直接使用
`plico-runtime`，不依赖 `plico-desktop` 或其原生热重载功能。

## 应用接入

WebHost<V: HostView> 是可复用 Rust 宿主，不要求应用使用 gallery：

- new(canvas, view) 创建 Source、文字上下文、场景和持久 WebGL renderer。
- load_font(bytes) 加载 TTF/OTF；浏览器无法枚举操作系统字体，必须显式提供字体。
- frame(timestamp_ms, logical_width, logical_height, device_scale) 提交帧，
  返回是否还有动画需要继续调度。
- dispatch(timestamp_ms, UiEvent) 在分发输入前消费上一操作并刷新必要几何。
- pointer/key/wheel 提供 DOM 事件转换的基础实现。
- source 可用于 request_rebuild、访问应用与接入 ServiceView。

应用可用 wasm-bindgen 包装自己的 WebHost。可选 gallery feature 只是这种包装的示例，
不要求把业务应用接到 Gallery 类型上。

浏览器宿主通过 VirtualClock 注入 performance/rAF 时间。时间倒退时钳位到已观察时间，
避免同一帧的 rAF 时间戳稍早于输入事件时间导致失败。核心 SystemClock 和后端性能/
motion 时钟使用 web-time；原生目标上的 Instant 类型保持为标准库实现。

rAF 请求合并：多个输入、resize 或 plico-wake DOM 通知只安排一个待执行回调，
有动画时连续采样，静止后停止。CSS 尺寸用于布局和输入，高 DPI 使用独立物理画布尺寸。
WebHost 的主线程服务唤醒发出 plico-wake，JS 宿主监听它并安排 rAF；
服务完成队列仍通过 Source::poll_async 消费。此入口未实现 WASM 多线程 worker 的跨线程唤醒。

## 当前范围

浏览器已验证 SVG、文字、点击、checkbox/switch、主题过渡、2048 键盘操作、
虚拟列表 smooth scroll、尺寸改变和 1x/2x DPI。字体注册会为浏览器的 generic families
补默认映射，支持的字符范围取决于应用加载的字体。

这是 Canvas 宿主的第一版，以下能力没有宣称完成：

- WebGL2 不执行 custom WGSL shader；gallery 的 Motion 页显示限制提示。
  其他应用发送 Shader paint command 时 renderer 明确报错。
- JS 示例支持普通按键和基础字符输入，未接隐藏文本输入节点、完整 IME、
  浏览器软键盘、剪贴板和原生无障碍 DOM 映射。诊断 semantics JSON 不是无障碍桥接。
- 浏览器下载、文件选择、多窗口等平台服务未接入。ServiceView 可复用，
  具体网络/任务服务需使用浏览器异步 API，不能直接复制原生线程服务。
- 不在浏览器中执行原生 dylib 热重载；当前修改后重建 WASM 并刷新页面。
  WASM 模块替换和状态恢复尚未接入 CLI。

## 可重复浏览器验证

在已启动上述 HTTP server、安装 Playwright 和浏览器后运行：

```sh
npm install --prefix /tmp/plico-web-host-qa playwright
NODE_PATH=/tmp/plico-web-host-qa/node_modules node crates/plico-web-host/web/smoke.cjs
```

可用 CHROMIUM_PATH 指定已有 Chromium，可用 PLICO_WEB_URL 覆盖默认 localhost:8080。
测试在 1x/2x DPI 下通过真实指针/按键执行三次计数、主题切换、2048、虚拟滚动、
Motion 页 fallback 和 resize，检查空闲 rAF 停止及 100 次唤醒只产生一帧。
本机 Chromium 的 WebGL2 软件渲染验证通过，不代表跨浏览器/移动设备的完整兼容证明。
