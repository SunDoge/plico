# 应用内毛玻璃

运行 `cargo run -p plico-gallery`，进入 **Glass**。可调 Blur、Tint，关闭 Blur background 对比原始边界，关闭 Move background 暂停背景。音乐卡片是演示内容；Save mix 仅改变本地界面状态。

```rust
use plico_core::prelude::*;

#[component]
fn glass(cx: &mut UiContext) {
    (column(cx).padding(24).radius(20.)
        .backdrop_blur(18.)
        .background(Color::Surface).background_alpha(0.32)
        .border(1., Color::Border)).children(|cx| {
        text(cx, "Quick settings").font_size(24);
        button(cx).text("Done");
    })
}
```

`backdrop_blur` 是逻辑像素中的高斯标准差，允许 0–64；0 不创建滤镜。`background_alpha` 只影响底色，范围 0–1，默认 1，子节点文字与控件保持原有透明度。两者都进入 Style 和热重载快照，样式校验拒绝非有限值和越界值；改变参数本身只失效绘制。Gallery 旁边的数值标签变化仍可能引起正常的文字测量和布局。

这是**应用画面内部**的背景模糊，尚未接入操作系统对窗口后方桌面的模糊。

## 首版实现与边界

Scene 构建时截取面板之前的绘制前缀，补齐前缀中尚未关闭的图层，再重放到 Vello 的高斯滤镜层。外层使用圆角隔离图层裁剪结果，随后绘制半透明底色、边框和前景。前景不会作为当前面板的滤镜输入。原生 WGPU 与浏览器 WebGL 共用命令，滤镜与剪裁都由 Vello 执行，没有 CPU 图片模糊。

同时修正了 PaintScene PushLayer 到 Vello 的映射：将 clip 作为隔离层的 clip_path，而不是只在滤镜之前放一个独立 clip。否则滤镜输出会扩散到面板之外。

这是一版可验证的前缀重放方案，适合少量面板，**尚无独立、持久化的 GPU 模糊纹理缓存**：

- 静止画面沿用 Source 的 retained Scene 缓存，不重新构建模糊命令；暂停演示后不继续请求动画帧。缓存的是 Scene，不代表独立滤镜纹理复用。
- 背景运动、面板移动或参数改变会更新 Scene，并再次执行滤镜。滤镜层的输出 clip 已限定到卡片圆角范围；输入仍重放之前的 Scene 前缀，没有局部输入捕获、降采样或持久纹理缓存，不能据 clip 大小断言 GPU 只处理卡片像素。
- 前缀重放增加 CPU 编码和 GPU 绘制；多个后续玻璃面板会再次包含前面的重放命令，开销可快速放大。不应用于大量重叠面板。
- 父级复杂透明隔离层、非默认混合模式及自定义 WGSL 背景的采样/合成语义尚未完整验证，当前不能宣称符合 CSS backdrop-filter 的全部语义。演示使用不透明祖先和普通矢量背景。
- 接下来若需要大规模使用，应引入 renderer 管理的 backdrop 纹理、局部捕获和模糊结果缓存，避免重复重放前缀；无需改变应用 API。

## 验证

```sh
cargo test -p plico-backend --test backdrop
cargo test -p plico-backend --test glass_pixels
cargo test -p plico-gallery --test glass
cargo run -p plico-gallery --example capture -- --page glass --out /tmp/glass.png
cargo run -p plico-gallery --example capture -- --page glass --dark --width 760 --height 740 --out /tmp/glass-dark.png
```

真实 GPU 像素测试在 1×、2× 下验证面板内红蓝边界混合、面板外不变、圆角裁剪及前景不模糊；另外验证静态 Scene 命令共享、背景移动后的结果更新。Gallery 使用虚拟时钟验证背景运动不触发文字测量和布局、暂停后回到空闲。Web smoke 在 1×、2× 下比较玻璃区域开关前后的截图，并继续验证其余页面、resize 和 idle；见 `crates/plico-web/web/smoke.cjs`。
