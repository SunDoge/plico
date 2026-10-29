# 原图坐标图片画布

`plico_renderer::image_canvas::{ImageCanvasState, ImageCanvas, CanvasRegion, CanvasTool, CanvasEvent}`
提供宿主拥有的静态图片画布，可通过 `SurfaceSource` 放进一个显式大小的布局槽。

```rust,ignore
let state = ImageCanvasState::default();
state.set_document(Some(original_raster_image), vec![]);
let widget = ImageCanvas::new(state.clone());
let source = SurfaceSource::new(app, widget, "preview");
// App::show / View::show 中必须提交槽，而不是丢弃 builder：
column(cx).key("preview").grow(1.).min_height(100.).end();
```

图片是 `Arc<RasterImage>`，重复 paint 不编码 PNG、不解码、不重新分配图像身份。
控件不负责文件 IO、OCR、撤销、自动保存或模型进程。应用可在宿主线程读取 RGBA 原图用于裁剪；
小裁剪应复制到独立缓冲，不保留整张源图。

## 交互

- `Draw`：主键拖出矩形，反向拖动、越界释放均归一化并限制到原图；小于 2×2 原图像素不提交。
- `Select`：选中最上层矩形；拖内部移动，拖选中框四角拉伸。手柄命中半径按逻辑像素计算，不随缩放变小。
- `Pan`：主键平移；任意工具下中键也可平移。
- 滚轮以指针为锚点缩放；`fit`、`actual_size`、`zoom`、`pan` 为按钮/键盘操作提供替代入口。
- Esc、Cancel、失焦、隐藏、尺寸变化、切换工具或替换文档取消未提交的框编辑。当前平移是即时视口操作，不随 Esc 回滚。
- 仅跟随发起拖动的 pointer ID；非发起指针不能提交该操作。
- Delete/Backspace 发出删除请求，应用决定是否删除并记录撤销。

`take_event()` 返回 `Added`、`Edited`、`Selected`、`Delete`；矩形为原图浮点像素坐标。
应用决定自己的整数化策略，例如左上 floor、右下 ceil。编辑确认后应用应调用 `set_regions` 更新叠层；
选择回报无需再同步整份叠层，否则会取消当前拖动。输入的无效、越界或重复 ID 叠层被忽略。

新图调用 `set_document`，会清掉排队事件、选中框并适应窗口；`set_regions` 不改变缩放。
几何与交互状态不进入应用 checkpoint。控件无动画循环，外部状态更新通过 WakeCallback 通知宿主。
`SurfaceSource` 现在允许非文本 surface 未处理的非 synthetic KeyDown 进入 View 的窗口快捷键回退；
不转发局部 pointer 坐标，不改变文本编辑器/IME 的事件优先级。

## 验证与边界

```bash
cargo test -p plico-renderer --lib image_canvas
cargo test -p plico-desktop --test native_surface
```

已测反向框、越界、原图坐标、缩放锚点、移动/拉伸、失焦/换图取消、静止不重复失效及图像身份复用。
OCR 消费者位于本机相邻仓库 `ppocr-training/apps/plico-ocr`，另有 Xvfb 真窗口的 Skia Vulkan / SkParagraph、
框选、两模型 OCR、滚轮、resize、退出/重启测试。Xvfb 使用 llvmpipe，不等于硬件 GPU 性能或全平台验收。

当前一次显示完整 RasterImage，不是瓦片/金字塔浏览器；应用必须限制解码与纹理尺寸。没有旋转视图、
自动检测、自由多边形、完整画布无障碍树或嵌字排版；矩形的键盘替代操作应由应用提供。
