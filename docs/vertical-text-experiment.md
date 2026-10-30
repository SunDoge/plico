# 漫画竖排嵌字最小实验

目标只验证：中文翻译结果能按固定字数换列，竖排标点/字形可用，最后由 Skia 离屏合成。
不新增 Widget 或正式排版 API，不接 OCR/翻译服务，不修改普通 UI 的文字路径。

## 运行

```sh
cargo run --locked --release -p plico-skia --features raster --example vertical_text
# 自己的译文；每列 10 个 grapheme clusters，列从右向左
cargo run --locked --release -p plico-skia --features raster --example vertical_text -- --text '你好，世界！今天也要一起努力。' --rows 10
# 无系统 Noto CJK 字体时可显式指定字体和 TTC face
cargo run --locked --release -p plico-skia --features raster --example vertical_text -- --font /path/to/font.ttc --face 2
cargo test --locked -p plico-skia --features raster --example vertical_text
```

默认输出到本地 `reports/vertical-text`：`zh-Hans.png`、`ja.png` 是三种策略的并排
对照，`translated.png` 是可直接查看的中文气泡合成。产物不提交。示例固定画布只支持
1..=12 个字/列、最多 8 列；超出报错，不悄悄截断或改变字号。

## 实现

- HarfRust 使用 TopToBottom 方向，读取字体竖排度量并应用默认竖排特性；输出的
  glyph ID、advance 和 offset 直接转入 Plico GlyphRun，由 Skia 绘制。
- 字体由 Skia FontMgr 查找，HarfRust 与绘制使用相同的原始字体数据与 TTC index。
  缺字报错，本轮不实现多字体 fallback。
- 第一组显式禁用 `vert/vrt2`，第二组正常竖排，第三组另试混排方向和两位数字纵中横。
- 最终气泡使用第二组的普通竖排，换列仅按 grapheme 数量，显式换行也换列。
  组合假名不会在基本字符与浊点间被拆开，但没有语言学断行或标点禁则。
- 所有新增整形/方向/CLI 依赖均为示例用 dev-dependency；库不新增运行依赖。

2026-10-30 本机验证使用系统 Noto Sans CJK Regular：中文 SC face=2，日文 JP face=0。
探针 `、。 「」 （） ー …`（空格只为此处阅读）八种字符的竖排 glyph ID 都不同于横排，
advance 均为 34（示例字号 34）。生成图片已实际查看；日文长音变竖、组合假名正常，
混排对照里的 `12` 可横排合为一格。横排整形后旋转整个中文段落不能替代这一步。

## Parley / SkParagraph 的边界

当前依赖版本 Parley 0.12 没有现成的竖排段落布局入口；VerticalAlign 是行内基线/
上下对齐，不是 writing-mode。当前 skia-safe 0.153.3 暴露的 SkParagraph 段落方向为
LTR/RTL，同样没有直接可用的 TopToBottom 段落模式。这是所用版本接口的检查结果，
不是声称它们永远不能支持竖排。

[HarfBuzz 方向说明](https://harfbuzz.github.io/harfbuzz-hb-common.html) 区分水平和
TTB/BTT 整形；[OpenType 竖排特性](https://learn.microsoft.com/en-us/typography/opentype/spec/features_uz)
负责字形/度量，不负责自动换列；[Unicode UAX #50](https://www.unicode.org/reports/tr50/)
定义默认字符方向。Parley 与此实验共用 HarfRust 这一层，未来不必把专用漫画排版绑定到
Skia；[Parley 官方说明](https://github.com/linebender/parley) 解释各层分工。

## 明确未做

自动适应气泡形状/字号、禁则与标点压缩、自动 ruby、竖排编辑/命中测试、双向复杂混排、
字体 fallback、真正的原图修补/去字、完整的 Unicode 竖排一致性均未实现。混排对照
临时使用已有 unicode-vo 0.1 的旧方向表（revision 17）；它不是最新 Unicode 数据，
也未实现所有 Tr 字符缺少替代字形时的方向回退，不能作为正式排版的完整合规实现。

本轮结论仅是：对“中文译文按固定长度换列再合成”的第一版需求，HarfRust＋已有
GlyphRun/Skia 路径可行，无需等待通用段落库提供完整竖排，也无需先建设复杂排版系统。
