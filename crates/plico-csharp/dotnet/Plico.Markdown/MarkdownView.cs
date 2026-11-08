using Markdig;
using Markdig.Syntax;
using Markdig.Syntax.Inlines;
using System.Text;

namespace Plico.Markdown;

/// Managed Markdown composition. Parsing is cached by source; Rust shapes paragraphs.
public sealed class MarkdownView
{
    private string source = "";
    private Markdig.Syntax.MarkdownDocument document = Markdig.Markdown.Parse("");
    public string Source => source;
    public void SetText(string text)
    {
        if (source == text) return;
        source = text; document = Markdig.Markdown.Parse(text);
    }
    public void Show(Ui ui, uint foreground = 0x202024, uint muted = 0x71717a, uint accent = 0x285bd6, uint codeBackground = 0xf4f4f5)
    {
        for (int i = 0; i < document.Count; i++)
            using (ui.Scope((ulong)i)) Draw(ui, document[i], foreground, muted, accent, codeBackground);
    }
    private static void Draw(Ui ui, Block block, uint foreground, uint muted, uint accent, uint codeBackground)
    {
        switch (block)
        {
            case HeadingBlock h:
                Paragraph(ui, h.Inline, h.Level == 1 ? 28U : h.Level == 2 ? 22U : 18U, 700, foreground, accent); break;
            case ParagraphBlock p: Paragraph(ui, p.Inline, 16, 400, foreground, accent); break;
            case CodeBlock c:
                using (ui.Column().FillWidth().Padding(16).Radius(8).Background(codeBackground).Shrink())
                    ui.Text(c.Lines.ToString()).FillWidth().Monospace().FontSize(14).Color(foreground).Shrink();
                break;
            case ListBlock list:
                for (int i = 0; i < list.Count; i++)
                    using (ui.Scope((ulong)i))
                    using (ui.Row().FillWidth().Gap(8).AlignStart().Shrink())
                    {
                        ui.Text(list.IsOrdered ? $"{i + 1}." : "•").Width(22).Color(muted);
                        using (ui.Column().Width(0).Grow().MinWidth(0).Gap(8))
                            if (list[i] is ContainerBlock item)
                                foreach (var child in item) Draw(ui, child, foreground, muted, accent, codeBackground);
                    }
                break;
            case QuoteBlock quote:
                using (ui.Column().FillWidth().Padding(12).Gap(8).Border(1, muted).Radius(6).Shrink())
                    foreach (var child in quote) Draw(ui, child, muted, muted, accent, codeBackground);
                break;
            case ThematicBreakBlock:
                using (ui.Column().FillWidth().Height(1).Background(muted).Shrink()) { } break;
            case ContainerBlock container:
                foreach (var child in container) Draw(ui, child, foreground, muted, accent, codeBackground); break;
            case LeafBlock leaf:
                ui.Text(leaf.Lines.ToString()).FillWidth().Shrink(); break;
        }
    }
    private static void Paragraph(Ui ui, ContainerInline? inline, uint size, ushort weight, uint foreground, uint accent)
    {
        var text = new StringBuilder(); var spans = new List<RichTextSpan>(); int utf8 = 0;
        void Append(string value, ushort w, bool italic, uint? color)
        {
            int start = utf8; utf8 += Encoding.UTF8.GetByteCount(value); text.Append(value);
            if (utf8 > start) spans.Add(new(start, utf8, w, italic, color));
        }
        void Visit(Inline? node, ushort w, bool italic, uint? color)
        {
            for (; node is not null; node = node.NextSibling)
                switch (node)
                {
                    case LiteralInline literal: Append(literal.Content.ToString(), w, italic, color); break;
                    case CodeInline code: Append(code.Content, 550, italic, accent); break;
                    case LineBreakInline br: Append(br.IsHard ? "\n" : " ", w, italic, color); break;
                    case EmphasisInline e: Visit(e.FirstChild, e.DelimiterCount >= 2 ? (ushort)700 : w, italic || e.DelimiterCount == 1, color); break;
                    case LinkInline link: Visit(link.FirstChild, w, italic, accent); break;
                    case ContainerInline container: Visit(container.FirstChild, w, italic, color); break;
                }
        }
        Visit(inline?.FirstChild, weight, false, null);
        ui.RichText(text.ToString(), spans).FillWidth().FontSize(size).Weight(weight).Color(foreground).Shrink();
    }
}
