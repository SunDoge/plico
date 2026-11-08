# 系统文字优先：竖排文档接口

Plico 的分工是：系统文字引擎处理排版、字体选择/回退、方向、字形和栅格化；wgpu 处理纹理、遮罩、合成、damage 与特效；Rust 管理边界、资源、缓存和生命周期。已有平台能力优先补绑定，不因为语言纯度重新实现文字系统。

`plico-native` 的 `vertical-text` feature 在当前桌面 OS 上选用已有系统服务，提供 `vertical::{Engine, Request, Bitmap}`：

```rust,ignore
let mut text = plico_native::vertical::Engine::new()?;
let request = plico_native::vertical::Request::new("中文（测试）ABC 123", [120., 240.], 24.);
let bitmap = text.rasterize(&request)?;
if !bitmap.fits { /* 缩小字号或扩大文本框，不能静默导出前缀 */ }
```

Engine 创建/使用限于同一线程；Bitmap 为自有、top-down、straight RGBA，可跨线程交给 GPU。文本、缩放、尺寸和输出内存均有上限。透明外边距被裁掉便于框内居中；系统排好的字形及其相对位置不重新排列。

- Linux：复用 `text::SystemText` 有界缓存；vertical 与 horizontal 的缓存键区分。Pango gravity 决定 CJK/Latin 的方向，Cairo 将布局绘制成像素；最大列高交给 Pango 换行。
- macOS：CoreText vertical forms 和 CTFramesetter 的 RightToLeft 帧。visible range 必须覆盖全部 UTF-16 输入才报告 fits。
- Windows：DirectWrite 设置 reading/flow/vertical glyph orientation；升级 glyph collector 到 IDWriteTextRenderer1，方向矩阵由 IDWriteTextAnalyzer2 提供。暂不实现非零方向下的 underline/strikethrough/inline object，明确返回 E_NOTIMPL，不悄悄画错。

Linux 有实际输出与缓存隔离测试。macOS 与 Windows 当前仅交叉编译/Clippy，尚未实机验证竖排像素、字体回退和混排结果。原生缺字统计未统一支持：未取得统计时返回 None，不当作零。

这只是文档竖排 raster 服务，不表示现有输入编辑器的光标/选区已支持竖排，也不替代各平台原有的 paragraph/line APIs。后续字体注册、文字选择、IME、富文本与无障碍继续围绕系统能力完善。
