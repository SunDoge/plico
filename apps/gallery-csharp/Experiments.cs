using Plico;
using Plico.Markdown;
using System.Globalization;

namespace Gallery;

internal sealed partial class GalleryModel
{
    internal bool Lens, BlurGlass = true, MoveGlass = true, SavedMix;
    internal double GlassBlur = 18, GlassTint = .32;
    private void BlackHole(Ui ui)
    {
        using (ui.Row().Gap(12).AlignCenter())
        {
            ui.Switch("Gravitational lens", ref Lens);
            ui.Text(Lens ? "Overlay on" : "Overlay off").Color(T.Muted);
        }
        using (Card(ui, "How it works"))
            ui.Text("A 20 px dark core sits inside a broad gravitational field. Nearby text and edges bend toward the pointer; distortion fades smoothly within 300 px.").FillWidth().Color(T.Muted);
    }
    private void BlackHoleOverlay(Ui ui)
    {
        if (Lens) ui.Shader(Resource("BlackHole.wgsl"), key: "pointer-lens").ReadBackdrop().FollowPointer().FillWidth().FillHeight().Absolute(0, 0);
    }
    private const string GlassSource = """
        fn plico_effect(uv: vec2f) -> vec4f {
            let t = plico.resolution_time_scale.z;
            let r = plico.resolution_time_scale.xy;
            let p = uv * r;
            var color = vec3f(0.078, 0.149, 0.235);
            let scale = plico.resolution_time_scale.w;
            let q = p / scale;
            let drift = sin(t * 0.6) * 110.0;
            let float_y = cos(t * 0.4) * 35.0;
            color = mix(color, vec3f(0.2196, 0.5294, 0.7373), 1.0 - smoothstep(179.0, 180.0, length(q - vec2f(192.0 + drift, 150.0 + float_y))));
            color = mix(color, vec3f(0.2941, 0.6627, 0.5843), 1.0 - smoothstep(159.0, 160.0, length(q - vec2f(420.0 - drift, 340.0 - float_y))));
            let capsule = q - vec2f(192.0 + drift * 0.8, 270.0);
            let distance = length(vec2f(capsule.x, max(abs(capsule.y) - 118.0, 0.0)));
            color = mix(color, vec3f(0.9608, 0.702, 0.4196), 1.0 - smoothstep(31.0, 32.0, distance));
            return vec4f(color, 1.0);
        }
        """;
    private void Glass(Ui ui)
    {
        using (ui.Row().Gap(16).Wrap().AlignCenter())
        {
            ui.Checkbox("Blur background", ref BlurGlass);
            ui.Checkbox("Move background", ref MoveGlass);
            ui.Text("Blur").FontSize(12).Color(T.Muted);
            ui.Slider("Backdrop blur", ref GlassBlur, 0, 32).Width(100);
            ui.Text($"{GlassBlur:0} px").FontSize(12);
            ui.Text("Tint").FontSize(12).Color(T.Muted);
            ui.Slider("Glass tint", ref GlassTint, .05, .85).Width(100);
        }
        using (ui.Column().FillWidth().Height(420).Grow().MinHeight(420).Radius(20).Clip().Background(0x14263c))
        {
            ui.Shader(GlassSource, animated: MoveGlass).FillWidth().FillHeight().Absolute(0, 0).ShaderTime((float)ui.Seconds, !MoveGlass);
            using (ui.Column().Absolute(26, 30).Gap(4))
            {
                ui.Text("SLOW RADIO").FontSize(42).Weight(800).Color(0xd9efff);
                ui.Text("Music for the space between things.").FontSize(13).Color(0xc3e3ec);
            }
            using (ui.Column().Absolute(36, 112).Width(330).Padding(24).Gap(16).Radius(22)
                .Background(T.Panel).BackgroundAlpha((float)GlassTint).BackdropBlur(BlurGlass ? (float)GlassBlur : 0).Border(1, 0xd5dce7))
            {
                ui.Text("♬  NOW PLAYING").FontSize(11).Weight(700);
                ui.Text("Evening focus").FontSize(26).Weight(750);
                ui.Text("Low tide · Ambient sessions").FontSize(13);
                using (ui.Column().FillWidth().Height(3).Background(T.Border).Radius(2))
                using (ui.Column().WidthPercent(42).Height(3).Background(T.Accent).Radius(3)) { }
                using (ui.Row().FillWidth().SpaceBetween().AlignCenter())
                {
                    ui.Text("02:18 / 05:30").FontSize(11);
                    if (ui.Button(SavedMix ? "Unsave" : "Save mix").Primary().Clicked()) SavedMix = !SavedMix;
                }
                ui.Text(SavedMix ? "Saved to your mixes" : "An ambient mix for a quiet evening").FontSize(11);
            }
        }
    }
    internal int GreetingCount = 1;
    private DateOnly localeMonth = new(2026, 11, 1), localeDate = new(2026, 11, 3);
    private void Localization(Ui ui)
    {
        var culture = CultureInfo.GetCultureInfo(Chinese ? "zh-CN" : "en-US");
        using (Card(ui, Tr("Messages and formatting", "消息与格式")))
        {
            ui.Text(Tr("Hello, Ada!", "你好，Ada！")).FontSize(24).Weight(650);
            ui.Text(Chinese ? $"你有 {GreetingCount} 条消息。" : $"You have {GreetingCount} {(GreetingCount == 1 ? "message" : "messages")}.");
            using (ui.Row().Gap(8))
            {
                if (ui.Button("Add one").Primary().Clicked()) GreetingCount++;
                if (ui.Button("Reset count").Ghost().Clicked()) GreetingCount = 0;
            }
            ui.Text(Tr("Number: ", "数字：") + 1234567.ToString("N0", culture));
            ui.Text(Tr("Date: ", "日期：") + new DateOnly(2026, 11, 3).ToString("D", culture));
        }
        using (Card(ui, Tr("Nested English scope", "嵌套英文作用域")))
        {
            ui.Calendar(ref localeMonth, ref localeDate, CultureInfo.GetCultureInfo("en-GB"));
            Hint(ui, Tr("This content stays in English when the application locale changes.", "应用切换语言时，这部分内容保持英文。"));
        }
    }
    private readonly MarkdownView markdown = new();
    internal bool ChatPaused, ChatFollow = true, ChatReduced;
    internal int RevealMode;
    internal int ChatShown;
    private double chatLast, chatElapsed;
    private static readonly string Reply = "# A small interface, in motion\n\nA good streaming reply should feel **calm and readable**, even when the network delivers a sudden burst. This paragraph keeps *emphasis* and `inline code` inside one shaped text node.\n\n## Built for real text\n\n中文也可以逐步显示。组合字符 e\u0301 和 emoji 👩‍💻 会以完整的字素显示，而不是拆成几个闪烁的字符。\n\n- Keep earlier paragraphs in place.\n- Let new content arrive at a steady pace.\n- Stop following when the reader scrolls up.\n\n```csharp\nusing (ui.Column().Gap(12))\n    markdown.Show(ui);\n```\n\n## When the stream ends\n\nThe remaining text keeps revealing until it catches up. The interface then rests: no background animation loop.\n\nYou can replay this answer, pause its arrival, or deliver the rest in one burst.\n";
    private static readonly int[] ReplyBoundaries = StringInfo.ParseCombiningCharacters(Reply);
    private void MarkdownChat(Ui ui)
    {
        double dt = chatLast == 0 ? 0 : Math.Clamp(ui.Seconds - chatLast, 0, .25);
        chatLast = ui.Seconds;
        if (!ChatPaused) chatElapsed += dt;
        using (ui.Row().Gap(8).Wrap().AlignCenter())
        {
            if (ui.Button("Replay").Primary().Clicked()) { chatElapsed = 0; ChatShown = 0; ChatPaused = false; }
            if (ui.Button(ChatPaused ? "Resume" : "Pause arrival").Clicked()) ChatPaused = !ChatPaused;
            if (ui.Button("Send remaining").Clicked()) ChatShown = Reply.Length;
            ui.Checkbox("Reduce motion", ref ChatReduced);
        }
        using (ui.Row().Gap(8).Wrap().AlignCenter())
        {
            string[] names = ["Character", "Word", "Line", "At once"];
            for (int i = 0; i < names.Length; i++)
                using (ui.Scope(i))
                    if ((RevealMode == i ? ui.Button(names[i]).Primary() : ui.Button(names[i]).Ghost()).Clicked()) RevealMode = i;
            ui.Checkbox("Follow reply", ref ChatFollow);
        }
        int target = Math.Min(Reply.Length, (int)(chatElapsed * 95) + 23);
        if (!ChatPaused && ChatShown < target)
        {
            if (ChatReduced || RevealMode == 3) target = Reply.Length;
            else if (RevealMode is 1 or 2)
            {
                int end = Reply.IndexOf(RevealMode == 1 ? ' ' : '\n', target);
                target = end < 0 ? Reply.Length : end + 1;
            }
            int boundary = Array.BinarySearch(ReplyBoundaries, target);
            if (boundary < 0) boundary = ~boundary;
            ChatShown = boundary >= ReplyBoundaries.Length ? Reply.Length : ReplyBoundaries[boundary];
        }
        ChatShown = Math.Min(ChatShown, Reply.Length);
        markdown.SetText(Reply[..ChatShown]);
        using (ui.Column().FillWidth().Height(0).Grow().MinHeight(0).ScrollY().SmoothScroll().FollowEnd(ChatFollow).Padding(16).Gap(20).Radius(14).Background(T.Panel))
            markdown.Show(ui, T.Foreground, T.Muted, T.Accent, T.Control);
        if (!ChatPaused && ChatShown < Reply.Length) ui.RequestAnimationFrame();
    }
}
