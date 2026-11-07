using Plico;
namespace Gallery;

internal sealed partial class GalleryModel
{
    // Original demo lyrics: no copyrighted song or network dependency.
    private static readonly string[][] LyricLines = [
        ["Leave", "the", "lights", "on"],
        ["Let", "the", "night", "unfold"],
        ["Every", "little", "moment"],
        ["Turns", "to", "something", "gold"],
        ["We", "are", "moving", "slowly"],
        ["Through", "the", "open", "blue"],
        ["When", "the", "morning", "finds", "us"],
        ["I'll", "be", "here", "with", "you"]
    ];
    internal const double LyricLineSeconds = 4.4;
    internal static double LyricDuration => LyricLines.Length * LyricLineSeconds;
    internal bool LyricPlaying = true;
    internal double LyricStart = 10, LyricPosition;
    private static readonly Plico.Theme LyricTheme = Plico.Theme.Dark with { Accent = 0xf3bfa2, AccentHover = 0xf9d4bf, AccentPressed = 0xe0a784, OnAccent = 0x261a24 };
    private double lyricAnchor;
    private bool lyricInitialized;

    private void Lyrics(Ui ui)
    {
        if (!lyricInitialized) { lyricAnchor = ui.Seconds; lyricInitialized = true; }
        double position = (LyricStart + (LyricPlaying ? Math.Max(0, ui.Seconds - lyricAnchor) : 0)) % LyricDuration;
        if (LyricPlaying) ui.RequestAnimationFrame();
        bool wide = ui.Width > 1100;
        float font = wide ? 40 : 30;
        using (ui.Column().FillWidth().Height(600).Shrink().Radius(24).CornerSmoothing(1).Clip().Background(0x2b2035))
        {
            using var theme = ui.UseTheme(LyricTheme);
            ui.Shader(LyricBackground, animated: LyricPlaying, key: "ambience").FillWidth().FillHeight().Absolute(0, 0)
                .ShaderTime((float)position, paused: !LyricPlaying);
            using (ui.Row().Absolute(30, 28).FillWidth().Gap(10).AlignCenter())
            {
                ui.Svg("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='1.6'><path d='M9 18V5l12-3v13M9 8l12-3'/><ellipse cx='6' cy='18' rx='3' ry='2'/><ellipse cx='18' cy='15' rx='3' ry='2'/></svg>", 20).Color(0xf7e7dc);
                ui.Text("AFTER HOURS").Weight(650).FontSize(14).Color(0xf7e7dc);
            }
            if (wide)
                using (ui.Column().Absolute(34, 130).Width(230).Gap(18))
                {
                    ui.Shader(LyricArtwork, animated: false).Width(230).Height(230).Radius(16);
                    ui.Text("Leave the lights on").FontSize(22).Weight(700).Color(0xfff7ef).FillWidth();
                    ui.Text("The quiet hours").FontSize(15).Color(0xd4b7bb);
                    ui.Text("Original lyric demo · no audio").FontSize(13).Color(0xbda3ae).FillWidth();
                }
            float left = wide ? 310 : 28;
            // Position is continuous through the line handoff. A short eased scroll
            // follows the active line without a discrete jump every 4.4 seconds.
            double lineTime = position / LyricLineSeconds;
            int current = Math.Min(LyricLines.Length - 1, (int)lineTime);
            double fraction = lineTime - current;
            double scroll = current + Smooth(Math.Clamp((fraction - .82) / .18, 0, 1));
            using (ui.Column().Absolute(left, 82).Width(Math.Max(200, ui.Width - 224 - 48 - left - 36)).Height(400).Clip())
            {
                for (int i = Math.Max(0, current - 2); i <= Math.Min(LyricLines.Length - 1, current + 3); i++)
                {
                    using var lineScope = ui.Scope(i);
                    float y = 120 + (float)((i - scroll) * 82);
                    if (y < 0 || y > 330) continue;
                    float edge = (float)Smooth(Math.Clamp(Math.Min(y / 38, (330 - y) / 38), 0, 1));
                    float distance = Math.Abs(i - (float)scroll);
                    float opacity = Math.Clamp(1 - distance * .24f, .10f, 1) * edge;
                    using (ui.Row().Absolute(0, y).Gap(wide ? 10 : 8).Height(74).AlignCenter())
                    {
                        int total = LyricLines[i].Sum(word => word.Length + 1), passed = 0;
                        foreach (string word in LyricLines[i])
                        {
                            using var wordScope = ui.Scope(passed);
                            double start = i * LyricLineSeconds + .16 + passed / (double)total * (LyricLineSeconds - .65);
                            double duration = (word.Length + 1) / (double)total * (LyricLineSeconds - .65);
                            double sung = Math.Clamp((position - start) / duration, 0, 1);
                            LyricWord(ui, word, font, sung, i == current, opacity);
                            passed += word.Length + 1;
                        }
                    }
                }

            }
            using (ui.Column().Absolute(left, 500).Width(wide ? 480 : 370).Gap(10))
            {
                using (ui.Row().Gap(8).AlignCenter())
                {
                    if (ui.Button(LyricPlaying ? "Pause lyrics" : "Play lyrics").Ghost().Color(0xfff4eb).Clicked())
                    { LyricStart = position; lyricAnchor = ui.Seconds; LyricPlaying = !LyricPlaying; }
                    if (ui.Button("Replay").Ghost().Color(0xfff4eb).Clicked())
                    { position = LyricStart = 0; lyricAnchor = ui.Seconds; }
                    ui.Text($"{(int)position / 60}:{(int)position % 60:00} / 0:{(int)LyricDuration:00}").FontSize(13).Color(0xd4b7bb);
                }
                double seek = position;
                ui.Slider("Lyric playback position", ref seek, 0, LyricDuration - .001).FillWidth();
                if (seek != position) { position = LyricStart = seek; lyricAnchor = ui.Seconds; }
            }
        }
        LyricPosition = position;
        ui.Text(Tr("Word-by-word reveal, gentle lift and continuous line follow. Original demo lyrics; no audio playback.",
            "逐词扫光、轻微上浮和连续行跟随。原创演示歌词，不播放音频。")).Color(T.Muted).FillWidth();
    }
    private static double Smooth(double x) => x * x * (3 - 2 * x);

    private static void LyricWord(Ui ui, string word, float font, double sung, bool active, float opacity)
    {
        float lift = active ? (float)(Math.Sin(sung * Math.PI) * -3) : 0;
        using (ui.Box().Height(70).Shrink().Translate(0, lift))
        {
            uint muted = LyricColor(0x342439, 0xb9a0af, opacity * .6f);
            ui.Text(word).FontSize((uint)font).Weight(750).Color(muted);
            if (sung <= 0) return;
            // All copies have the same font and fixed available width. Only the
            // parent clip changes, so the highlight never changes word wrapping.
            if (active && sung < 1)
            {
                float intensity = (float)Math.Pow(Math.Sin(sung * Math.PI), .6) * opacity;
                // The clip contains the revealed glyphs, then the outer layer
                // blurs their alpha. The crisp foreground is drawn afterwards.
                using (ui.Box().Absolute(0, 0).WidthPercent(100).Height(70).Blur(5).Opacity(intensity * .8f))
                using (ui.Box().Absolute(0, 0).WidthPercent((float)(sung * 100)).Height(70).Clip())
                    ui.Text(word).Width(400).Shrink().FontSize((uint)font).Weight(750).Color(0xffeedb);
            }
            for (int band = 3; band >= 0; band--)
            {
                float reveal = (float)Math.Clamp(sung + band * .035 * Math.Sin(sung * Math.PI), 0, 1);
                using (ui.Box().Absolute(0, 0).WidthPercent(reveal * 100).Height(70).Clip())
                    ui.Text(word).Width(400).Shrink().FontSize((uint)font).Weight(750).Color(LyricColor(muted, 0xfff9f2, opacity * (band == 0 ? 1 : band == 1 ? .65f : band == 2 ? .3f : .12f)));
            }
        }
    }
    private static uint LyricColor(uint from, uint to, float amount)
    {
        uint channel(int shift) => (uint)Math.Round(((from >> shift) & 255) * (1 - amount) + ((to >> shift) & 255) * amount);
        return (channel(16) << 16) | (channel(8) << 8) | channel(0);
    }
    private const string LyricBackground = """
        fn plico_effect(uv: vec2f) -> vec4f {
            let t = plico.resolution_time_scale.z * 0.10;
            let a = uv - vec2f(0.18 + sin(t) * 0.12, 0.25);
            let b = uv - vec2f(0.78, 0.75 + cos(t * 0.7) * 0.14);
            let glow = exp(-dot(a, a) * 4.0);
            let dusk = exp(-dot(b, b) * 5.0);
            let color = vec3f(0.09, 0.055, 0.115) + vec3f(0.32, 0.12, 0.10) * glow
                + vec3f(0.10, 0.08, 0.22) * dusk;
            return vec4f(color, 1.0);
        }
        """;
    private const string LyricArtwork = """
        fn plico_effect(uv: vec2f) -> vec4f {
            let p = uv - vec2f(0.5);
            let sun = 1.0 - smoothstep(0.225, 0.23, length(p + vec2f(0.0, 0.09)));
            let horizon = smoothstep(0.20, 0.65, uv.y);
            var color = mix(vec3f(0.80, 0.47, 0.40), vec3f(0.13, 0.12, 0.26), uv.y);
            color = mix(color, vec3f(1.0, 0.79, 0.56), sun);
            let ridge = 0.64 + sin(uv.x * 8.0) * 0.045 + sin(uv.x * 18.0) * 0.022;
            color = mix(color, vec3f(0.13, 0.10, 0.20), smoothstep(ridge, ridge + 0.006, uv.y));
            return vec4f(color - vec3f(horizon * 0.02), 1.0);
        }
        """;
}
