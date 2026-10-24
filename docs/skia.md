# Skia / Vello 绘制对照实验

状态：`plico-skia` 已提供 CPU raster consumer；不是可切换的桌面窗口后端，
不替换默认 Vello renderer，也不表示 Skia GPU 接入已经完成。

## 已实现

同一份 `plico_renderer::Scene` 分别交给 Skia CPU 和 Vello GPU。
文字只由 Parley 整形一次：两端使用相同字体字节、TTC index、glyph ID、位置和字号。
Skia 直接绘制这些 glyph，不重新测量或整形。字体文件按 Blob ID / index 缓存，
小字号使用灰度抗锯齿、subpixel positioning 和按命令启用的 normal hinting。

支持 solid fill 路径、几何 transform、nonzero/even-odd fill、变换后的路径 clip，
以及普通 solid fill glyph run。每次 render 清空旧画面并重置 clip stack，
失败不返回部分像素，也不能把失败帧导出为 PNG；下一帧可以正常重试。
像素 API 返回 tightly packed premultiplied RGBA，PNG 由 Skia 编码。

默认 feature 不加载 Skia；启用 `raster` 才使用 skia-safe。当前 Linux 验证实际下载
并链接了 rust-skia 预编译库，不是源码构建；其他 target / feature 组合是否有预编译库
取决于 rust-skia 的分发矩阵。当前 consumer 仍复用 renderer 和 vello-common 的共享
类型与依赖，不宣称已完全解耦 Vello。

## 运行

```sh
cargo check -p plico-skia
cargo test -p plico-skia --features raster
cargo run -p plico-skia --features raster --example compare
cargo clippy -p plico-skia --all-targets --all-features -- -D warnings
```

示例默认使用 Linux 常见的 DejaVu Sans，并在存在时注册 Noto Sans CJK Regular。
其他系统可设置 `PLICO_PROBE_FONT=/path/to/static-font.ttf` 和
`PLICO_PROBE_CJK_FONT=/path/to/cjk-font.ttc`；显式指定的文件不存在会报错，
样本出现缺字也会报错。中文字体不可用且未显式指定时，示例诊断并跳过中文样本。

输出在 `reports/skia/`，不参与提交：

- `skia-{scale}.png`、`vello-{scale}.png`：独立的原尺寸截图。
- `compare-{scale}.png`：左 Skia CPU、右 Vello GPU，不缩放拼接。
- scale 包括 1、1.25、1.5、2；英文含 12/14/18/32/64px 的 S/s 和数字，
  中文含 14/24px 样本，另有圆角形状及带 clip 的亚像素细线。

已在本机生成这些输出，支持检查两端的光栅化、hinting 和覆盖率差异。
相同 glyph/位置并不强制两端使用相同光栅算法，像素不同本身不等于 Vello 缺陷。
这是 CPU Skia 对 GPU Vello 的质量实验，不以耗时直接判断 renderer 性能胜负。

## 明确限制

以下情况返回 Unsupported，不静默忽略或近似替换：

- 图层合成、滤镜、模糊和阴影；
- stroke、渐变及图片 brush、SVG、自定义 WGSL shader；
- 非默认 variable font 坐标、合成 italic/embolden、stroke glyph。

当前尚不能完整回放 gallery。下一阶段应逐类补齐命令和像素检查，再接 GPU surface，
对同一 gallery 比较渲染质量、帧耗时、内存与平台兼容性。不能据此直接决定正式应用
的默认 renderer，更不能把尚未实现的功能记录为“已支持”。
