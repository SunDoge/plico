# Cosmic Text 与 Pango 的本机对照

日期：2026-11-11。环境：Linux、Cosmic Text 0.19.0、Pango 1.56.3、
Cairo 1.18.4，Noto Sans / Noto Sans CJK SC / Noto Color Emoji。

这不是 Plico 后端切换方案。Plico 普通文本当前使用 Parley；Linux 的系统参考路径使用
Pango/Cairo。本实验只回答 Cosmic Text 是否足以替代 Pango，probe 位于
`plico-native` 的 `cosmic_pango_probe` example。

## 运行

```sh
cargo build --release -p plico-native \
  --example cosmic_pango_probe --features linux-text

for size in 12 14 18; do
  COSMIC_PROBE_SIZE=$size \
    target/release/examples/cosmic_pango_probe
done
```

截图写入忽略的 `reports/cosmic-pango/`。每张图左侧是
Cosmic Text + Swash，右侧是 Pango + Cairo。样例覆盖 Latin、CJK、Arabic、
Devanagari、BiDi、彩色 emoji、组合字符和混合脚本换行。

## 结果

所有样例均完成塑形；Pango 报告零缺字，Cosmic Text 也为每段生成有效 glyph runs。
CJK 单行宽度完全一致。Latin、Devanagari、emoji 和组合字符的宽度接近；Arabic 与
BiDi 的宽度分别相差约 12px 和 14.5px（18px 字号），说明 fallback、字形度量或
双向段落策略仍不能视为像素等价。混合脚本长句均为两行，但选择的断行位置不同。

布局吞吐使用 300 个带唯一后缀的段落，复用每个引擎的字体上下文，排除进程初始化，
每个字号重复五次。下表为中位数；它比较的是实际 API 工作负载，不是逐阶段完全等价
的微基准，不能外推到整帧速度。

| 字号 | Cosmic Text | Pango | Cosmic 相对速度 |
|---:|---:|---:|---:|
| 12px | 14.61ms | 39.93ms | 2.73× |
| 14px | 12.40ms | 34.34ms | 2.77× |
| 18px | 12.29ms | 34.60ms | 2.82× |

视觉上 18px 已经相当接近。12px 与 14px 下，Pango/Cairo 的 Noto Sans 笔画更黑、
grid fitting 更稳定；Cosmic Text/Swash 偏轻。这里同时包含 Swash 与 Cairo/FreeType
的光栅差异，不能把观感差异全部归因于 shaping。它与现有
`pango-text-comparison.md` 的结论一致：小字质量往往由 hinting、像素相位和覆盖率
生成主导，替换排版器本身不保证改善。

Cosmic Text 0.19 已有字体 fallback、复杂脚本、BiDi、rich text、光标/hit testing、
选择、换行、对齐和 ellipsis，足以支撑普通横排 GUI 与编辑器。当前 API 没有与
Pango gravity 对等的纵排 writing mode；Pango 在系统字体配置、成熟排版选项和
Linux 原生小字观感方面仍更完整。

## 决策

- 不把 Cosmic Text 作为 Plico 的 Pango 质量参考替代品。
- 如果目标是纯 Rust、低依赖和布局吞吐，Cosmic Text 是合格候选，性能有明显优势。
- 如果目标是 Linux 上 12–14px 的系统级观感，当前 Cosmic Text + Swash 尚未达到
  Pango/Cairo；需要单独研究 FreeType/Skrifa hinting 或替换光栅端。
- Plico 继续保留 Parley 主路径和 Pango 参考路径。若评估 Cosmic Text，应把 shaping
  与 rasterizer 分离后再做生产集成实验。
