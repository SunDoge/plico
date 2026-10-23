# 常用 widget 与 custom shader 验收

目标原文：**补充常用的widget，并接入custom shader，参考wabou和mygo**。

验收依据是当前源码、交互测试、真实 GPU 与模块重载，不把设计文档或编译成功单独当成功能证据。下面对应本轮新增能力；原有 button/text/SVG/switch/checkbox、布局、主题和输入能力继续由已有 gallery / 输入测试覆盖。

| 要求 | 当前实现与直接证据 | 结论 |
|---|---|---|
| 补充常用交互 widget | Slider、Modal、Popover、Select、Menu/MenuItem、Tooltip 已进入 core widgets/prelude 与真实后端。crates/plico-backend/tests/{slider,modal,popover,select_menu,tooltip}.rs 验证受控值、输入、导航、选择/取消、禁用、焦点、滚动、遮罩和命中，而非仅构建快照 | 已验证 |
| 支持普通 Rust 组合与立即响应 | Context 首参、children、同步响应和应用持值；内容闭包在声明时调用，不保存应用回调。静态调用身份由 component 生成，SelectOption 显式业务 key；重排焦点测试在 select_menu.rs | 已验证 |
| 接入 custom shader | core ShaderAsset/ShaderParams → ViewNode → adapter retained Surface → 内置 Wabou ShaderEffectSource/GPU。tests/shader.rs 读取实际 GPU 像素，覆盖 uniform、时间、暂停、reduced-motion、DPI、完整模块、静态复用、无效 WGSL 保留上一程序、卸载及主题转场版本隔离 | 已验证 |
| widget 可与 shader 同屏 | Gallery Motion 的 Exposure slider 控制 WGSL uniform；tooltip.rs 实际 GPU 测试验证 shader 场景中的提示及其关闭后的 scene cache，静态提示不会随 shader 每帧重排 | 已验证 |
| 时钟与动画符合框架方向 | VirtualClock 测试覆盖 modal presence、slider press、shader playback、tooltip delay。中间动画/等待帧不重复应用声明或主树布局；期限到达再提交必要卸载。时钟从宿主注入 | 已验证 |
| 有限热重载 | scripts/check-modal-reload.py 通过真正 CLI + cdylib + resident host 操作 Modal、Slider、shader、Popover、Select、Menu、Tooltip，再修改代码重载并断言状态/载荷/事件/焦点。reports/modal/reload.json 为本次执行结果，各组件目录有实际 GPU 截图 | 已验证 |
| 参考 Wabou / MyGO 而非复制另一套 runtime | docs/{modal,popover,select-menu,tooltip,shaders}.md 记录对应源文件、采用的规则及差异。只读参考原仓库，行为接入 Plico 的 retained engine、host clock 和现有 copied backend，未添加 Go/Solid runtime | 已验证 |
| 开发体验与可查看成果 | Gallery Controls 与 Motion 提供实际组件；reports/{modal,sliders,popover,select-menu,tooltip,shaders} 有截图。34 项真实 rust-analyzer completion、cargo fmt 幂等格式、严格 Clippy、332 项 workspace tests 均通过 | 已验证 |

验证命令：

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
python3 scripts/check-lsp.py
cargo build -p plico-cli
python3 scripts/check-modal-reload.py
git apply --check reports/macros/arrowless-experiment.patch
```

本次结论限于目标中的组件补充与 shader 接入，不宣称完整组件库或所有平台认证。未来可继续增加 Tabs/Radio/虚拟列表等控件；多字符搜索、复杂子菜单、富文本 Tooltip、自定义 shader 纹理/backdrop、Bevy 后端不在当前实现中。原生 TextBuffer 仍有已记录的模块传输限制，见 text-input.md；本轮新控件及 shader 的重载验证不替代该限制。详情以各组件文档为准。
