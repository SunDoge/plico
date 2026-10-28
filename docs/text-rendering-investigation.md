# 文字渲染排查结论（2026-10-24）

## 已落地修复

正式窗口/离屏 renderer 的加粗容差现在按非零扩张量的 10% 计算，上限 0.1px。
例如 14px、1.75x、boost 2 时使用 0.0049px，避免转角被近似省略、直边变斜。
保留默认 hint 与真实字体字重，默认 boost 已进一步改为 0：普通文字不再额外扩张轮廓，
避免对齐后的笔画产生新淡边。`PLICO_TEXT_STEM_BOOST` 仅作为显式实验覆盖，
非有限值会忽略；显式扩张与合成粗体仍使用收紧后的容差。未将实验 coverage LUT 加入正式管线。
矩形笔画的轴对齐回归测试覆盖多个小扩张量。

取消默认扩张后的正式输出已在 1x/1.75x、Latin/CJK 下与未扩张 hinted 参考逐像素
比对一致。1.75x 中文同一组样本的极淡像素（1～15/255）从旧策略 18660 个降到
6888 个，去掉了旧扩张新增的 13658 个背景覆盖像素；没有用阈值删除正常抗锯齿。
覆盖率 LUT 仍只是离线实验，不作为默认锐化滤镜。解析默认值的测试不再修改进程环境，
避免多线程测试中相互干扰。

运行中的 native 宿主需要重启才能应用 renderer 改动，单纯热重载 UI 动态库不会替换
宿主中的渲染代码。重新运行 `compare-small-text.py` 会生成嵌入图片的对照页与
同字体、同字号的 `before-after.png`。

Linux 默认 UI 字体栈改为 `Noto Sans, Noto Sans CJK SC, sans-serif`，让安装了
Noto 的系统优先使用中英文对应的真实字重；字体是可选系统字体，不打包新字体。
直接 TextContext 排版和 Source 使用同一默认值，显式字体栈优先；其他平台保持原默认。
繁体/日文等应用可继续通过 `Source::with_font_family` 或 `set_font_family` 指定字体栈。

下文各实验数字是修复前的基线与独立对照；“正式参数未改”的历史描述不再代表当前代码。

## 范围

用户确认的主要症状为“小字号发灰、笔画不够实”和“中文笔画模糊或不均匀”。
后续优先验证静态文字；滚动实验作为独立发现，不用它替代静态观感问题的验收。

当前管线为 Parley → Glifo/Skrifa → Vello GPU。实验不替换正式后端。
使用 Noto Sans 与 Noto Sans CJK SC、12/14/16px、1/1.25/1.5/2x、
多个物理像素相位，对照系统 FreeType、Pango/Cairo 和 Chrome 155。
这些是 Linux 离屏结果，不代表实际窗口合成、显示器与所有字体的最终观感。

## 当前可定位的问题

针对用户确认的静态症状，已分离出三个可干预因素：

| 因素 | 直接证据 | 处理候选 |
|---|---|---|
| 中文回退与字重不一致 | 本机中文 400/500 都落到 Sarasa Regular，英文会切换 Medium | 明确字体栈并使用真实字重 |
| hint 之后再做小幅几何扩张 | 原本对齐的笔画外出现淡边；默认扩张容差还会省略转角，使直边变斜 | 取消补偿性几何加粗，或使用更合理的扩张容差 |
| 文字 coverage 分布不同 | 同字体/坐标下，冻结的源亮度 LUT 能改善部分差异；不同深浅颜色的结果不同 | 独立文字 coverage 策略，不能全画面套 gamma |

以上是可复现的机制与干预结果，不等于已证明每一项对用户观感的相对贡献。
按原物理尺寸生成的样张仍待主观验收，正式渲染默认值没有更改。

像素对齐并不缺少显示器参数：Shell 已从窗口取得 scale_factor，本机报告 1.75；
真实窗口表面也已与相同物理尺寸的离屏图比对。静态文字的对齐仍可能被后续几何扩张
破坏，不能因“已有缩放 API”就认为整条管线始终对齐，也不能把它归因于缺少物理 PPI。
灰度 coverage 不依赖 RGB 子像素排列，本机 fontconfig 也选择 RGBA_NONE。

`compare-small-text.py` 会生成 `review.html`，使用 1–4 键切换同一行样张，并选择
主题、字号、字重、DPI。预览位于浏览器物理像素原点，按 DPR 反算 CSS 图像尺寸，
避免再次缩放。已用 Chrome CDP 校验浏览器 DPR 1/1.25/1.75、两种主题、12/14px、
四种方案，共 48 个组合，预览区域均与原 PNG 裁片逐字节一致。样张 DPI 为 1.75、
字重 400；不把该校验扩展到所有浏览器、样张 DPI 和字重。HTML 模板为
`scripts/text-comparison.html`，生成页与图片仍不提交。

```sh
python3 scripts/compare-chrome-live-motion.py --review reports/small-text/review.html \
  --output reports/text-review-verification
```

## 已排除或限定的解释

- **排版宽度不是主要差异。** 指定相同字体后，Parley 与 Chrome Canvas 的宽度差
  小于 0.002 logical px；Pango 关闭度量取整后也接近。相同 glyph 与位置仍能产生不同像素。
- **未发现通用图片 alpha 混合错误。** 同一 FreeType 灰度 mask 交给 Cairo 与 Vello，
  初始组合最大通道差为 1/255；后续黑底白字相位对照逐字节相同。
- **LCD hint target 不能等同于 LCD 输出。** 直接复现 Glifo 的 Skrifa 配置能逐像素
  重现生产路径。只把 target 从 LCD 改为 LIGHT/NORMAL，本组字体输出完全相同；
  symmetric rendering 会改变拉丁字形，但未改变本组 CJK CFF 字形。
- **Chrome DOM 与 Canvas 并无本组预想的差距。** 深浅两种颜色、两种字体和四档 DPI
  共 16 组输出逐字节相同，黑底白字 1x 的 RGB 通道也完全相同，不是 RGB LCD 抗锯齿。
- **同机 GPU 不是本组差异来源。** Intel、NVIDIA 与 llvmpipe 的 1x 中英文输出
  逐字节相同。尚未覆盖所有 DPI 和桌面窗口合成。

## 静态文字排查

### 字体选择

本机 generic sans-serif 的拉丁文字使用 Noto Sans，中文却使用 Sarasa Gothic CL；
Pango 未指定中文语言时使用 Sarasa Mono CL，指定 zh-CN 后使用 Noto Sans CJK SC。
因此 generic family 的截图并不是同字体的光栅化对照。

显式使用 `Noto Sans, Noto Sans CJK SC, sans-serif` 已能让当前 Parley 管线选择
预期中文字体。诊断输出现在记录 family、PostScript 名称、face index 和 synthetic bold。
优先检查实际选中的字体，再讨论更换排版器。

`compare-small-text.py` 进一步用实际 Theme 前景/背景色，12/14/16px、400/500 字重、
1x 与本机 1.75x，生成 generic sans-serif、显式 Noto、boost 0/2/4、无 hint 样张。
没有强制注册字体，记录真实系统回退。默认中文的 400/500 都选 Sarasa Gothic CL
Regular，但拉丁会从 Noto Sans Regular 切换 Medium；显式 Noto 栈则让中文 500
选到 Noto Sans CJK SC Medium。这是默认中英文字重响应不一致的具体来源之一。
不同字体的 baseline/行高也不同，故不能将两张样图的像素差全部当作光栅化差异。
样张以原物理尺寸拼接，查看时应避免图片查看器再缩放；视觉判断还需要用户确认。

### 针对本机 1.75x 的静态复核

为贴近用户确认的静态症状，补测同一 Noto 文件、12/14/16px、八种相位、1.75x。
基线严格控制，几何加粗关闭：

| 路径对 Chrome 黑底白字的覆盖 L1 | Latin | CJK |
|---|---:|---:|
| 当前默认 hint | 8.58% | 6.28% |
| 无 hint | 18.56% | 33.27% |
| normal-ft hint | 7.56% | 6.28% |
| FreeType LIGHT、相同取整基线 | 6.48% | 5.36% |

对齐基线后，当前 CJK hinted 路径与 FreeType LIGHT 的差约 2.23%；
额外 Y snap 输出相同；只改 hint target 不改变本组 CJK 输出。
无 hint 的部分相位还产生位置差，故该表既不能当成主观画质排名，也不支持“关 hint
即可改善中文”的推断。它约束了下一步：先保留 hint，处理字体/字重选择和覆盖率，
而非直接替换排版器或关闭全部 hint。

本机 `fc-match 'sans-serif:lang=zh-cn'` 的配置是 antialias=true、hintstyle=1、rgba=5、
lcdfilter=1；系统 fontconfig 头文件定义前两项枚举分别为 HINT_SLIGHT 与 RGBA_NONE。
因此这里没有证据支持“桌面用了 LCD，而 Plico 没有 LCD”是主要原因。

在同一 1.75x 中文样本中，检查很淡的 coverage（1～15/255）像素数：

| 路径 | 很淡的像素数 | 总覆盖量相对 Chrome |
|---|---:|---:|
| Chrome | 3648 | 1.000 |
| native，boost 0 | 6888 | 0.962 |
| native，默认 boost 2 | 18660 | 0.997 |
| FreeType LIGHT、相同基线 | 7108 | 约 0.962 |

boost 2 增加了总覆盖，也明显增加极淡边缘像素。默认总墨量并不低于参考很多，
但边缘覆盖的空间分布不同；这比“整体太细”更精确。阈值计数依赖样本、位置与量化，
不能据此证明主观模糊，更不能认为淡边缘都是错误像素。

取消几何加粗、保留 hint、应用此前冻结的 coverage 模型后，1.75x 中文白字的 L1
从 boost 0 的 6.27% 降到 4.80%，总覆盖比从 0.962 到 1.003；默认 boost 2 则为
5.94% / 0.997。黑字对应的新策略为 5.10%，默认 boost 2 为 7.07%。因此它是比
继续增加几何加粗更值得视觉验收的候选，但仍只是离线 text-only 预览。
`compare-small-text.py` 的 `noto-coverage` 样张用真实主题色重建覆盖并应用固定
gamma=1.2、contrast=0.2；它不是已接入正式 renderer 的滤镜，也不作用于应用全画面。

### 加粗后的转角近似会造成不均匀扩张

检查 Glifo 0.4.0 可知：先由 Skrifa 生成 hinted 轮廓，再用 Kurbo 0.13.1
`expand_path` 做几何加粗。默认 expansion tolerance=0.1px，而默认 boost 在
14px、1.75x 下仅为 0.049px，容差超过扩张量。

`embolden_probe` 用像素对齐的矩形笔画 `(20,20)..(21,44)` 复现：

- 无加粗时，中部横切像素为 `[0,0,255,0,0,0]`。
- 加粗量 0.049、容差 0.1 时，生成的轮廓出现斜边，例如从 `(21,19.951)`
  直接连接到 `(21.049,44)`。中部横切为 `[0,7,255,5,0,0]`。
- 只把容差降到 0.01 或 0.001，转角连接点恢复，边保持水平/垂直，横切为
  `[0,12,255,12,0,0]`；与同 bounds 的直接矩形输出一致。

Kurbo `do_join` 会根据容差忽略很小的法线变化；这是近似误差预算的结果，
不是已证明的 Vello GPU coverage bug。即便修正容差，几何扩张本身仍然会在
对齐笔画外生成淡边；容差解释的是扩张不均匀，不能单独解释所有发灰。

`compare-embolden-tolerance.py` 用直接 Skrifa + 同样 expand_path 控制实际字体：
0.1 容差时，Latin/CJK 的输出与正式默认管线逐像素相同；只改成 0.01 后，
本组 CJK 有 55588 个像素变化、覆盖量比从 0.997 到 1.017，L1 对 Chrome 从
5.94% 到 6.20%。所以“更忠实的几何扩张”不等于“更接近浏览器”，不能只据
L1 决定是否保留几何加粗，也尚未证明这是用户主观症状的全部来源。

早期样张中的 `noto-fine-embolden` 只修正容差，仍有几何扩张。
当前 `compare-small-text.py` 的 `noto-default` 使用编译后的真实默认值（现为不扩张），
`noto-coarse-embolden` 则显式恢复旧 boost 2 + 0.1px 容差，保留可重建的前后对照；
`noto-coverage` 仍只作实验预览。

```sh
cargo run -p plico-shell --example embolden_probe
python3 scripts/compare-embolden-tolerance.py
```

```sh
python3 scripts/compare-text-phases.py --scales 1.75 --output reports/hint-targets-175
python3 scripts/compare-chrome-text.py --scales 1.75 \
  --source reports/hint-targets-175 --output reports/chrome-text-175
python3 scripts/compare-chrome-text.py --scales 1.75 --light \
  --source reports/hint-targets-175 --output reports/chrome-text-175
python3 scripts/compare-current-text.py --scales 1.75 --source reports/hint-targets-175 \
  --chrome reports/chrome-text-175 --output reports/current-text-175
python3 scripts/analyze-native-coverage.py --scales 1.75 --source reports/hint-targets-175 \
  --chrome reports/chrome-text-175 --current reports/current-text-175 --output reports/native-coverage175
```

### 覆盖率与字重策略

关闭 stem boost 时，14px 白底黑字的覆盖总量已较接近 Chrome；黑底白字则偏低。
当前默认 stem boost=2 会改善黑底白字，但通常让白底黑字更重。几何加粗不能等价替代
文字覆盖率调节，更不能由此推断 Vello 的通用混合有误。

以 FreeType 同位置 mask、四分之一像素 X 相位为输入，参考 Skia 的文字覆盖率公式，
只用 12px Latin 1x 的深浅两种颜色拟合 gamma/contrast，再验证其他字号和中文。
拟合值 1.2/0.2 能减少部分差异，例如 14px CJK 1x 黑底白字的覆盖 L1 差
从 7.61% 降到 4.72%，白底黑字则从 5.49% 到 5.48%。这支持覆盖率策略是原因之一，
但不能证明 Chrome 实际使用这些参数，也没有消除字形轮廓与相位缓存的差异。
L1 除以参考覆盖总量是像素差指标，不是画质评分。

### 中间灰度验证

新增 `compare-text-colors.py` 使用实际 Chrome DOM 和实际 native 输出，非黑白图的
解析反色，验证 1x 下六组前景/背景、两种字体、stem boost 0/2，共 24 个对照。
每组包含 12/14/16px 和八种位置相位；以下数值合并三个字号。总覆盖量按
`sum((pixel-background)/(foreground-background))` 计算，像素 L1 仍以浏览器覆盖量归一化。

| 字体 | 前景 / 背景灰度 | 覆盖总量比 boost 0 → 2 | L1 boost 0 → 2 |
|---|---|---|---|
| Latin | 255 / 0 | 0.917 → 0.954 | 13.94% → 13.11% |
| Latin | 0 / 255 | 0.993 → 1.034 | 13.18% → 13.90% |
| Latin | 160 / 32 | 0.952 → 0.991 | 13.17% → 13.04% |
| CJK | 160 / 32 | 0.974 → 1.009 | 8.15% → 8.36% |
| Latin | 96 / 240 | 0.956 → 0.995 | 13.09% → 12.99% |
| CJK | 96 / 240 | 0.978 → 1.013 | 7.99% → 8.30% |

因此此前黑白背景下观察到的权衡也出现在中间颜色：总覆盖量接近并不保证边缘分布
接近；相同加粗参数对 Latin 和 CJK 的收益不同。这组结果没有单独分离前景与背景的
影响，也没有验证有色文字、透明度或观看者的主观偏好，不能作为自动调参表。

补充背景控制：固定前景灰度 160，背景改为 0/32/240/255。对 Chrome 和 native 的
boost 0/2，用背景 0 的图恢复 coverage，再线性混合到其他背景，全部像素最大差不超过
1/255，18 个组合断言通过。本组没有观察到真实背景改变 coverage 的证据。

将此前仅用 Latin 12px 1x 黑白两色拟合的 gamma=1.2、contrast=0.2，按 Skia 的
源亮度 LUT 公式推广到灰色（不重新拟合），六组颜色、两种字体的覆盖 L1 均降低。
例如 FreeType quarter-X 参考上的 Latin 160/32 为 5.58% → 4.47%，
CJK 224/64 为 8.54% → 6.39%。它支持“文字源亮度相关的 coverage 策略”这一解释，
仍不能证明浏览器内部参数，也没有消除 hinting/轮廓/位置量化差异。

### 覆盖率调整的上限

`analyze-native-coverage.py` 将同一套黑白拟合 LUT 直接作用于 native 输出。
结果不如 FreeType 参考一致：boost 0 下白字改善，拉丁黑字略变差；叠加默认 boost 2
时，多组白字反而变差。因此不能直接把拟合参数叠加到当前默认管线。

另用 1x native boost 0 的全部测试像素，为每个输入 coverage 单独选择参考像素的
加权中位数，得到任意逐点 LUT 的最小 L1 残差。此 LUT 故意用全部待测数据过拟合，
不要求单调；它仅提供这组图的误差下界，不是推荐算法。

| 字体 / 颜色 | 原始 L1 | 最优逐点 LUT 后 | 原差异保留比例 |
|---|---:|---:|---:|
| Latin 白字 | 13.94% | 12.00% | 86% |
| Latin 黑字 | 13.18% | 12.73% | 97% |
| CJK 白字 | 9.54% | 6.67% | 70% |
| CJK 黑字 | 8.05% | 7.15% | 89% |

这将结论限定得更清楚：覆盖率策略确有影响，尤其是白色中文，但它解释不了当前
大部分像素差。剩余项涉及轮廓、hinting、位置/相位及光栅化等空间差异，尚未分完。
不能从这些百分比推导主观画质差距，也不能把覆盖率策略认定为唯一或主要观感根因。

## X 相位与 hinting 的组合控制

`compare-native-phases.py` 直接在 native Scene 的物理 glyph X 坐标上做 1/4px 量化，
保持排版 advance 与 glyph 序列不变，并断言原始排版记录一致。关闭几何加粗，
测试默认 Glifo hint 与直接 Skrifa `normal-ft`（symmetric=true、preserve_linear_metrics=false）
两种策略，覆盖两种字体、四档 DPI、深浅两色。这里改变的是绘制位置，不是布局；
此前“只有背景覆盖率不同”的解释不足以覆盖这个结果。

1x，合并 12/14/16px 和八种相位的覆盖 L1：

| 变体 | Latin 白字 | Latin 黑字 | CJK 白字 | CJK 黑字 |
|---|---:|---:|---:|---:|
| 默认 hint、连续 X | 13.94% | 13.18% | 9.54% | 8.05% |
| 默认 hint、1/4px X | 11.51% | 9.00% | 9.20% | 7.54% |
| normal-ft、连续 X | 11.86% | 11.19% | 9.54% | 8.05% |
| normal-ft、1/4px X | 9.43% | 6.92% | 9.20% | 7.54% |
| 上一项 + 固定 coverage LUT | 6.13% | 6.74% | 6.82% | 7.35% |

Latin 的空间差异有一部分可以明确归到 X 相位和 hint 策略；本组 CJK 对该 hint
配置不敏感，X 量化收益也小。2x Latin 组合后的白/黑字差为 5.25%/3.67%，
再套固定 coverage LUT 为 3.37%/3.62%。这不是最终修复：1/4px 量化可能让慢速
移动发生阶跃，必须独立做动画验收，不能因为更像 Chrome 就直接改生产默认值。
probe 新增 `--x-steps 4`，正式管线未改变。

## 慢速位移的空间响应

`compare-text-motion.py` 用 14px Noto Sans 的 `H`、Noto Sans CJK SC 的 `中`，
分别在 X/Y 方向采样 0 到 1 物理像素，每步 1/32，共 33 个位置。各位置占用独立行，
读取相同大小的局部栅格，测相邻像素是否相同与覆盖重心位移。它测试栅格器的位置
响应，不覆盖事件调度、帧率、跨帧缓存或真实滚动合成，不能称为动画流畅度实测。

| 路径 | X 方向 32 步中不变次数 | 最大 X 重心步长 | Y 方向不变次数 | 最大 Y 重心步长 |
|---|---:|---:|---:|---:|
| 默认 hint、连续 X | 0 | 0.036 / 0.040px | 31 | 1px |
| 默认 hint、1/4px X | 28 | 0.284 / 0.273px | 31 | 1px |
| 无 hint、连续 X | 0 | 0.039 / 0.038px | 0 | 0.036 / 0.035px |

斜杠两项依次为 Latin/CJK，期望输入步长为 0.03125px。全部路径总重心位移为 1px。
默认 hinted Y 的整像素阶跃与 Glifo 的最终 Y round 一致，绝非此次新增 X 量化造成。
因此，缓慢纵向移动时的停顿/跳变有可验证的栅格阶段来源；不能统称为排版不稳定。
X 量化虽然让静态输出更接近 Chrome，却引入同类阶跃；不应据静态图默认启用。
无 hint 响应连续，但前面的静态对照已经显示它未必更接近浏览器。
是否为移动文字采用独立策略，需结合实际 app 动画与静止时的模式切换再设计。

### Chrome 静态位移采样

`compare-chrome-motion.py` 对相同 H/中字和 1/32px 位置样本使用 Chrome DOM，
比较布局位置、普通 transform、额外声明 `will-change: transform` 三种写法。
两种字体均在 X 方向有 28/32 个相邻样本完全相同，Y 方向有 31/32 个完全相同；
Y 的重心最大步长同样为 1px。三种写法在本实验中输出一致。

这修正了对前述现象的定性：整像素 Y 阶跃确实存在，但也存在于浏览器静态文字
定位路径，不能仅凭这个现象就判为 Plico 特有 bug。该测试将不同位置放在不同元素，
没有让同一元素经历真实动画；`will-change` 是合成提示而非图层复用的证据。
真实跨帧合成与图层重用尚未验证，不能宣称浏览器动画也一定有相同阶跃。

后续 `compare-chrome-live-motion.py` 通过 CDP 操作同一个 14px Noto Sans H 元素，
每次移动 1/32px，等待两次 requestAnimationFrame 后截图，共 33 帧。
布局 top 与普通 translate3d 的 31/32 次相邻图像相同，最大重心步长 1px；
提前声明 `will-change: transform` 的路径不变次数为 16，最大重心步长约 0.0629px。
三组总位移均为 1px。它证明浏览器的移动响应能因合成提示而改变，而不必关闭静态字形
hinting。此 headless 会话没有收到 LayerTree 内容，无法以 layer ID/paintCount 验证
是否复用了纹理；不把合成提示直接当成图层复用证据。截图采样也不证明真实呈现延迟。

这提示后续可独立实验“静态 hinted mask + 移动阶段纹理采样”，与直接关闭 hint
进行画质/成本比较；不需要重建先前已经删除的增量应用 Scene 功能。

### 正式 retained smooth-scroll 路径复核

`plico-desktop` 的 `text_scroll_probe` example 使用普通 View、Source、VirtualClock、
真实 WheelEvent 与 `render_retained_frame`，而非手工构造 glyph 位置。
对 14px Noto Sans 的 H 触发 1 logical px 平滑滚动，1x、每 4ms 采样一次，共 33 帧。
每帧通过正式 shell 离屏 renderer 绘制，保存 glyph 物理位置、局部文字像素和布局计数。

测得 glyph Y 从 34.966001 连续降到 33.974，32 次相邻采样都有坐标变化，
所有帧的 layout_runs 为 0。文字区域像素 31 次不变，仅在 32ms、glyph Y 穿过
34.5 时整体上跳 1px，像素覆盖重心从 29.420126 到 28.420126。glyph 命令 hint=true。
这把此前单字位置实验连接到了实际 retained 滚动路径：本样例的阶跃不能归因于
没有执行布局、滚动坐标不更新或 Source 没有产生新 Scene，发生在下游 hinted 栅格化。
没有测窗口呈现时序，也不代表实际滚动的全部卡顿都来自同一原因。

同一 example 还克隆每帧的 Scene，只将 glyph 的 hint 标志关闭，使用完全相同的
glyph 物理坐标绘制对照。hint 开启时 31 次相邻采样像素不变，最大重心步长 1px；
关闭后不变次数为 0，最大重心步长约 0.0703px，匹配 smooth-scroll 曲线开头较快的
坐标变化。起止重心分别为 29.431579/28.439949，未出现整像素阶跃。
这是对该样例原因的干预验证，而不是仅凭源码推断。静止时的字形/coverage 也随之
变化，且前面的静态对照不支持无 hint 全面更优，因此不直接全局关闭 hint。

```sh
cargo run -p plico-desktop --example text_scroll_probe
```

输出仍为忽略的本地产物；该 example 依赖系统 Noto Sans Regular 字体。

## 对早期结果的修正

早期 probe 用 f32 减去再加回基线，在半物理像素附近的微小误差会跨过 Glifo 的 Y 取整
边界，产生一像素错位，夸大到数十个百分点的像素差。新增 `--exact-baselines` 专供受控
单 run fixture，在最终物理变换上固定基线；重新测量后不再需要整数像素平移来对齐。
这是实验精度修正，不能据此声称正式 UI 有同样的基线 bug。

历史文档中的大差异表不应再用于判断 hinting 的优劣。也没有证据支持“改成 LIGHT
即可修复”或“统一增加 stem boost 即可修复”。正式 hint 配置和默认加粗值没有改变。

## 真实窗口表面验证

`text_quality_probe --window` 可以把同一份已编码 Scene 同时交给离屏 renderer 和
正式 Shell 窗口 renderer；要求单档 DPI，且实际窗口 DPI 必须与 fixture 相同。
本机 XWayland 窗口报告 1.75x，显式字体栈、14px、400/500/600/700 字重的中英文
fixture 生成 1155×630 表面。使用 ImageMagick `import -window <窗口 ID>` 读取表面，
与离屏图逐字节相同。这覆盖了真实 Bgra8Unorm 窗口提交路径，但不是桌面合成结果。

KDE Spectacle 的 CaptureActiveWindow 请求未获回复，桌面合成后截图尚未取得，
不能把窗口表面一致扩展成“合成器没有影响”。可用以下方式复测窗口表面：

```sh
# fixture 中 scales 必须与当前窗口报告的 scale factor 一致。
cargo run -p plico-shell --example text_quality_probe -- \
  --cases /path/to/case.json --output reports/window-text --window
# 另一个终端中获取该窗口 ID，再截取表面；关闭窗口即可结束 probe。
xdotool search --name '^Plico text surface probe$'
import -window WINDOW_ID reports/window-text/window.png
```

## 复现顺序与下一步

系统依赖见 [Pango 对照](pango-text-comparison.md)，浏览器实验另需 Google Chrome。
生成文件写入忽略的 reports 目录，不提交。

```sh
python3 scripts/compare-text-phases.py
python3 scripts/compare-chrome-text.py
python3 scripts/compare-chrome-text.py --light
python3 scripts/compare-chrome-text.py --dom
python3 scripts/compare-chrome-text.py --dom --light
python3 scripts/analyze-skia-coverage.py
python3 scripts/compare-current-text.py
python3 scripts/inspect-text-fonts.py
python3 scripts/compare-small-text.py
python3 scripts/compare-text-colors.py
python3 scripts/analyze-text-colors.py
python3 scripts/compare-text-colors.py --background-control --output reports/text-background-control
python3 scripts/analyze-text-colors.py --source reports/text-background-control
python3 scripts/analyze-native-coverage.py
python3 scripts/compare-native-phases.py
python3 scripts/compare-text-motion.py
python3 scripts/compare-chrome-motion.py
python3 scripts/compare-chrome-live-motion.py
```

### 残余杂点与帧历史回归

`python3 scripts/check-text-artifacts.py` 在生产离屏 GPU 渲染路径检查：

- Latin、中文、混排，明暗两种背景，12/14/16px、400/500 字重；
- 1/1.25/1.5/1.75/2 倍缩放，文字起点带不同的小数相位；
- 每组四次静止重复、六种物理像素平移，移动后返回原位置；
- 品红色覆盖后清屏，再恢复文字；跨 DPI/目标尺寸切换后恢复首帧；
- glyph 缓存路径与相同 hint 设置的直接 Skrifa 轮廓路径逐像素比较。

本次 30 组字体/主题/缩放组合均通过，未发现 glyph/轮廓差异或帧历史残留。
缩放至 1x 的 Latin 和混排包含少量面积不超过两个像素的独立连通区域，
直接轮廓路径也生成同样像素；因此连通区域大小只能作为排查线索，不能直接用来去噪。
该对照共享 Skrifa 和底层光栅器，不是独立字体引擎对准确性的证明。

这些测试使用持久化离屏渲染器；同尺寸帧复用资源，目标尺寸改变时重建资源。
它们不覆盖窗口交换链、桌面合成器与实际滚动事件调度，不应据此声称所有窗口残影均已排除。
测试不改变默认加粗、锐化或覆盖率策略，结果和失败截图写入忽略提交的 reports 目录。

下一步应拿实际应用窗口中的具体问题字号、字体和主题验收。字体回退与文字覆盖率需要
分别处理；是否加入文字专用覆盖率策略，还需中间颜色、透明背景、动画和性能验证。
目前不足以支持替换 Parley 或整个 Vello 后端。
