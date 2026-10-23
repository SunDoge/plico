# 原生依赖清理

本轮确认并删除两条未使用的直接依赖：

- plico-shell -> bytemuck：shell 没有直接调用；renderer 的 glyph/uniform 仍使用它。
- plico-dev -> notify：文件 watcher 实际在 plico-backend / CLI，dev protocol crate 不使用它。

两者仍可能从其他有效路径进入原生依赖图，不能把这次清理描述为原生二进制已经明显变小。
gallery/services-demo 的直接 stabby 依赖由 export 宏生成的 ABI 代码使用，
属于扫描误报；保留并在 cargo-machete metadata 中说明。

以下原生功能正在使用，不能直接删掉而保持现有能力：

| 依赖 | 用途 |
|---|---|
| winit / wgpu / Vello | 窗口、输入与 GPU 呈现 |
| Taffy / Parley / glifo | 布局、文字排版与 glyph 绘制 |
| usvg / image | SVG 图标与图像资源 |
| arboard | 系统剪贴板；终端也使用 |
| rfd | typed effects 的原生文件/消息对话框 |
| notify-rust | 桌面通知 |
| open / url | 安全解析与打开外部链接 |
| notify / libloading / stabby | 开发 watcher、动态库加载和 native ABI |

下一轮值得考虑将对话框、桌面通知、外部链接等平台 capability 做成可选 feature，
保留默认功能并让不使用这些能力的应用退出相应依赖图。此次没有静默删除这些能力。
终端的 rio-vt / teletypewriter / rio-graphics 单独放在终端 crate，不进入普通 Plico/gallery。

另有平台隔离整理：原生 shell、CLI 参数解析和 stabby ABI 不再进入 WASM 正常构建图。
这是跨平台编译清理，不作为原生体积收益。

重复检查：

```sh
cargo machete
cargo tree -p plico-gallery --edges normal
cargo tree -p plico-terminal --edges normal
```

扫描没有未解释的闲置直接依赖，不表示每个已使用依赖都无法进一步替换或 feature 化。
