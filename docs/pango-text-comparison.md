# Pango / Cairo 与当前文字管线的离屏对照

> 后续受控实验发现并修正了 probe 的半像素基线误差。下文保留早期实验记录，
> 其中相位差异表不再作为当前判断依据；最新结论见 [文字渲染排查](text-rendering-investigation.md)。

## 目的与实现范围

验证文字观感差异来自排版还是排版之后的绘制。没有替换 Parley、Glifo、Vello，
没有把 Pango 加入正式 Rust 依赖。Linux 可选脚本使用系统 PangoCairo 与 Python GI。

运行前安装系统工具及字体（Debian 包名）：`python3-gi`、`python3-gi-cairo`、
`python3-cairo`、`gir1.2-pango-1.0`、`fontconfig`、`fonts-noto-core`、
`libfreetype6-dev`、`pkg-config` 及 C 编译器；
CJK 与彩色 emoji 样例另需 `fonts-noto-cjk`、`fonts-noto-color-emoji`。
Rust 离屏绘制需要可用的 wgpu 后端。

```sh
python3 scripts/compare-pango-text.py
```

脚本通过 usage-rs 参数调用 `plico-shell` 的 `text_quality_probe` example。
共享 JSON fixture、原始图、逐 run 的字体与 glyph 记录、度量 JSON 和放大对照图，
均输出到忽略的本地 reports 目录。可通过 `--output` 修改目录。
PNG 放大采用最近邻，不通过平滑缩放改变原始像素。

## 控制条件

- Noto Sans Regular / Bold：以 Fontconfig 匹配的文件加载进 Parley，并记录文件 SHA256；
  Parley glyph run 的字体文件通过字节比对确认，不只依赖 family 名称。
- 固定文字、前景/背景色、逻辑坐标、基线和物理输出尺寸。
- 12、13、14、16px 常规体，另加 14px 粗体；拉丁、中英混排、阿拉伯混排和 emoji。
- DPI 比例 1、1.25、1.5、2，浅色/深色两组。
- 当前生产绘制路径：Parley -> hinted glyph commands -> Glifo/Vello GPU，
  分别设置 `PLICO_TEXT_STEM_BOOST=0` 与当前默认 2。
- Pango -> Cairo 三组：灰度 AA + none / slight hinting（关闭 hint metrics 和位置取整），
  以及灰度 AA + full hinting（开启 hint metrics 和位置取整）。
- **保留 Parley 输出，单换 Cairo 绘制**：拉丁常规/粗体共五行，原样重放 Parley
  glyph ID 和坐标；不重跑 Pango 排版。先校验 Cairo 所选 face 对整个 ASCII 样例
  产生完全相同的 glyph ID 序列，不一致即中止。

Cairo toy font API 未暴露实际 font 文件，故最后一组只做到 family/weight、Fontconfig
匹配和全部 ASCII glyph ID 校验，不声称完全证明两端 font face 字节一致。
它不扩展到 CJK/emoji fallback；这些内容仅在完整 Pango 对照中观察。

## 本机观察（2026-10-23）

环境：Linux，Cairo 1.18.4，Pango 1.56.3，Mesa 软件 Vulkan 离屏输出。

14px 拉丁常规样例：

| 比例 | Parley advance（逻辑 px） | Pango slight 差值 | Pango full 差值 |
|---|---:|---:|---:|
| 1 | 295.62405 | -0.00198 | +5.37595 |
| 1.25 | 295.62405 | -0.00198 | +4.36911 |
| 1.5 | 295.62405 | -0.00198 | -2.30374 |
| 2 | 295.62405 | -0.00198 | -2.62405 |

关闭度量取整时，这个样例的排版宽度基本一致。full 模式同时改变了 hint metrics
和位置取整，不能把它的宽度差异解释成某个 shaping 引擎错误。

保留 Parley glyph 坐标、用 Cairo slight 绘制，与 Plico boost=0 相比，在指定的
14px 拉丁 ROI 中，1x 浅色约 12.13% 像素不同，RGB 平均绝对差约 3.20/255；
1.25x 浅色约 12.11%，平均差约 5.89/255。统计包含空白，不是视觉质量评分。
两种背景都能观察到笔画覆盖、横笔和边缘强度差异；Cairo slight/full 的观感也
明显受 hinting 配置影响，不能简单断言 Pango 在所有字号和 DPI 都更好。

## 结论

对于小字号笔画和清晰度问题，当前证据支持**先排查绘制端，继续保留 Parley**。
即使不换排版结果，仅换光栅化/合成路径，也会出现可见变化。因而“引入 Pango
才能改善文字”的判断不成立。

本实验还不能将差异单独归罪于 Vello：hinting 实现（Skrifa / FreeType）、glyph
光栅化、几何 embolden、像素相位与颜色混合都在绘制路径中。提高 stem boost
只是改变笔画，不构成已定位或修复 gamma 问题的证据。

后续的 FreeType mask 隔离实验已完成，见下一节。
不要先整体替换 Pango，也不应因为本图像对照直接切换整个 renderer。

本次未测性能、缓存成本或实际窗口合成效果，不构成系统级子像素 AA、IME、
复杂脚本及所有 fallback 字体正确性的验收。正式构建不要求 Python/Pango。


## FreeType mask + Vello 合成隔离组

同一脚本现在也编译 `scripts/text-freetype.c` 的可选系统 FreeType shim，输出只在
本地 reports 目录。不把该库链接进正式 Plico。Python 不手写 FreeType 内部结构
偏移；C shim 用系统头文件访问 face / slot，并明确限定返回 buffer 的借用期限。

流程：Parley 的字体文件（字节匹配、face index 0）、glyph ID、字号、坐标保持不变，
由 FreeType 2.13.3 生成灰度覆盖 mask。分别测试 NO_HINTING、TARGET_LIGHT、
TARGET_NORMAL；禁止字体内置 bitmap 与 synthetic embolden。字号及子像素相位
量化到 FreeType 的 1/64 物理像素，不对字形 advance 重新排版。

glyph tile 在整数位置合并到 A8 mask；然后将**同一份覆盖率**用于两种合成：

1. Cairo 直接以相同前景色和背景色合成；
2. `glyph_mask_probe` example 把覆盖率作为 straight RGBA 的 alpha，RGB 为固定
   前景色，交给现有 Vello 图片路径以 1:1 物理像素和低采样质量合成。不先与背景混合，
   不缩放 mask。读取时验证透明背景及中间灰度 alpha，避免把灰度 PNG 当成 alpha。

对照范围仍是五行静态拉丁字体，不包括 emoji、fallback、可变字体和生产 glyph atlas。
整页 mask 是隔离工具，绝不是拟采用的生产缓存设计。

结果：24 组（两种背景 × 四档 DPI × 三种 hinting）均完成并满足合成误差断言。
14px 拉丁 ROI 的平均 RGB 绝对差（255 为满值）：

| 比例 | LIGHT / 浅色 | LIGHT / 深色 |
|---|---:|---:|
| 1 | 0.03693 | 0.02776 |
| 1.25 | 0.03262 | 0.02227 |
| 1.5 | 0.02553 | 0.01722 |
| 2 | 0.01854 | 0.01330 |

额外检查 LIGHT 的整个输出图，所有背景/DPI 的单通道最大差均为 1/255。
误差与整数舍入量级一致，至少在本样例没有明显的图片 alpha 合成差异。

这把优先排查位置进一步收窄到原 glyph 路径中的 hinting、覆盖率生成、像素相位及
该路径特有的处理；不支持“Vello 所有合成都有问题”的推断，也不排除 glyph 路径
与图片路径的处理差异。保留 Parley、用 FreeType mask 再交给 Vello，在技术上可行；
但本次不证明 FreeType 对所有文本更好、更快，不直接切换默认字形后端。

## Hinting / 像素相位 / 中文控制实验

```sh
python3 scripts/compare-text-phases.py
```

扩展了可选 probe 的 `--no-hint` 与 `--snap none|y|xy`，只修改记录后的实验 glyph
commands，不改正式 renderer 配置。每个变体断言字体文件、face index、glyph ID、
原排版坐标和 advance 报告相同，排除重新 shaping 的影响。

范围：Noto Sans Regular（face 0）与 Noto Sans CJK SC（实际 TTC face 2），12/14/16px，
1/1.25/1.5/2x；X 相位 0/0.25/0.5/0.75 物理像素、Y 相位 0/0.5。FreeType shim
现在支持实际 collection index，不把中文 face 当成 TTC 第 0 个字体。
所有字体 run 均用加载文件字节比对，且断言实际 face index 与 Fontconfig 匹配。

为直接观察 coverage，这一轮使用黑底白字，关闭 stem boost。56 个组合
（两种字体 × 四档 DPI × 七个 hint/snap/FreeType 变体），每组 24 行，共 1344 行。
另补八组 FreeType LIGHT mask -> Vello，覆盖中文与拉丁所有 DPI；与 Cairo 的
同 mask coverage 差异断言通过。只做静态、无旋转的字形，不扩展到业务 UI。

记录每行的覆盖总量、部分覆盖像素比例、像素包围盒，以及两条路径间的覆盖率 L1 差。
L1 差除以参考图的覆盖总量，**不是画质评分或“错误率”**。相位下覆盖总量接近，
不代表边缘分布相同，也不能仅靠 soft pixel 比例判断清晰度。

### 观察

以三个字号、八个相位合并的覆盖率差为例：

| 字体 / DPI | FreeType NONE 对 Vello no-hint | FreeType NONE 对 Vello hint | FreeType LIGHT 对 Vello hint |
|---|---:|---:|---:|
| Latin / 1x | 4.99% | 38.71% | 23.66% |
| Latin / 1.25x | 4.32% | 29.83% | 22.44% |
| Latin / 2x | 2.59% | 23.19% | 13.59% |
| CJK / 1x | 2.71% | 53.56% | 28.11% |
| CJK / 1.25x | 2.39% | 37.65% | 24.63% |
| CJK / 2x | 2.39% | 24.13% | 16.85% |

这说明 hinting / grid fitting 对覆盖分布的影响远大于本样例两条无 hint 路径的差异。
无 hint 的剩余差还包含 FreeType 1/64px 相位量化、曲线光栅化和覆盖率整数舍入。
它不证明任一 hinting 实现错误：比较到不一样的 grid-fitting 策略本来就可能不同。

- 额外 Y snapping 与默认 hinted Vello 输出在八个字体/DPI 组中完全相同。
  读当前 Glifo 0.4.0 源码可确认：`calculate_outline_transform` 在存在 hinting instance
  时已经对最终 Y translation 做 `round()`。框架无需再加一遍基线取整。
- XY snapping 改变输出，且相位下覆盖总量完全稳定；但逐 glyph 取整会改变实际字距，
  保留布局 advance 不等于保留绘制位置，不适合直接推广到动画与任意比例缩放。
- 当前 Glifo 的 `HINTING_OPTIONS` 固定为 `Engine::AutoFallback`、`SmoothMode::Lcd`、
  `symmetric_rendering: false`、`preserve_linear_metrics: true`。这是 hinting target，
  不代表当前 GPU 输出了 RGB 子像素抗锯齿。仅有 `.hint(bool)` 不能比较 LIGHT/NORMAL/LCD
  的配置差异；后续值得在相同 Skrifa/Vello 管线中单独控制这些参数。
- 14px 的八个相位下，默认 Vello 覆盖总量的变化约 0.01–0.03%，没有出现大幅字重漂移。
  视觉相位差主要不能用“墨量突然减少”解释。
- 关闭 hinting 会让中小字号的笔画分布变化，但没有足够证据表明中文和拉丁在所有条件
  下都更好。因此不默认关闭 hinting，也不因为 FreeType 对照就替换 Parley。

### 当前选择

继续保留正式文字后端；下一步更有针对性的候选是研究 Glifo/Skrifa hinting target
可配置性，而非整体换 Pango。FreeType 仍作为独立参考。没有完成性能、atlas 缓存、
动画中像素稳定性或实际显示器的观感验收，本轮数据不能决定最终默认字体策略。
