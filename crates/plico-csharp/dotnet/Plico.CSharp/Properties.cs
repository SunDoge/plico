using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Plico;

/// UTF-8 offsets within one paragraph, independently shaped by the native text engine.
public readonly record struct RichTextSpan(int Start, int End, ushort Weight = 400, bool Italic = false, uint? Color = null);

public sealed unsafe partial class Ui
{
    internal sealed class NodeProperties
    {
        public float[]? Absolute { get; set; }
        public float[]? Translate { get; set; }
        public float Blur { get; set; }
        public float? PaddingX { get; set; }
        public float? PaddingY { get; set; }
        public float? MaxWidth { get; set; }
        public float? MaxHeight { get; set; }
        public float[]? Scale { get; set; }
        public float? Alpha { get; set; }
        public bool Clip { get; set; }
        public bool Draggable { get; set; }
        public bool Selectable { get; set; }
        public float? Sticky { get; set; }
        public bool FollowEnd { get; set; }
        public bool SmoothScroll { get; set; }
        public bool Mono { get; set; }
        public string? Placeholder { get; set; }
        public bool Focus { get; set; }
        public bool PasswordVisible { get; set; }
        public ushort TransitionMs { get; set; }
        public IReadOnlyList<RichTextSpan>? Spans { get; set; }
        public ShaderProperties? Shader { get; set; }
    }
    internal sealed class ShaderProperties
    {
        public float[]? Values { get; set; }
        public bool Backdrop { get; set; }
        public bool Module { get; set; }
        public bool Pointer { get; set; }
        public float Time { get; set; }
        public bool Paused { get; set; }
    }
    private readonly Dictionary<int, NodeProperties> properties = new();
    private bool animate;
    public void RequestAnimationFrame() { Verify(generation); animate = true; }
    private NodeProperties Properties(int index)
    {
        if (!properties.TryGetValue(index, out var p)) properties[index] = p = new();
        return p;
    }
    public Builder RichText(string text, IReadOnlyList<RichTextSpan> spans, string? key = null,
        [CallerFilePath] string file = "", [CallerLineNumber] int line = 0)
    {
        var b = Text(text, key, file, line); Properties(b.Index).Spans = spans; return b;
    }
    public readonly partial struct Builder
    {
        public Builder PaddingX(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).PaddingX = value; return this; }
        public Builder PaddingY(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).PaddingY = value; return this; }
        public Builder MaxWidth(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).MaxWidth = value; return this; }
        public Builder MaxHeight(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).MaxHeight = value; return this; }
        public Builder Scale(float x, float y) { _ = Current; ui.Properties(index).Scale = [x, y]; return this; }
        public Builder Opacity(float alpha) { Current.opacity = alpha; return this; }
        public Builder Absolute(float x, float y) { _ = Current; ui.Properties(index).Absolute = [x, y]; return this; }
        public Builder Placeholder(string text) { _ = Current; ui.Properties(index).Placeholder = text; return this; }
        public Builder RequestFocus(bool requested = true) { _ = Current; ui.Properties(index).Focus = requested; return this; }
        public Builder PasswordVisible(bool visible = true) { _ = Current; ui.Properties(index).PasswordVisible = visible; return this; }
        public Builder Autofocus(bool enabled = true) { if (enabled) Current.flags |= 1U << 26; else Current.flags &= ~(1U << 26); return this; }
        public Builder InputNavigation(bool enabled = true) { if (enabled) Current.flags |= 1U << 25; else Current.flags &= ~(1U << 25); return this; }
        public Builder Monospace() { _ = Current; ui.Properties(index).Mono = true; return this; }
        public Builder ShaderValues(params float[] values) { _ = Current; (ui.Properties(index).Shader ??= new()).Values = values; return this; }
        public Builder ReadBackdrop(bool enabled = true) { _ = Current; (ui.Properties(index).Shader ??= new()).Backdrop = enabled; return this; }
        public Builder ShaderModule(bool enabled = true) { _ = Current; (ui.Properties(index).Shader ??= new()).Module = enabled; return this; }
        public Builder FollowPointer(bool enabled = true) { _ = Current; (ui.Properties(index).Shader ??= new()).Pointer = enabled; return this; }
        public Builder ShaderTime(float time, bool paused = false) { _ = Current; var p = ui.Properties(index).Shader ??= new(); p.Time = time; p.Paused = paused; return this; }
    }
    public readonly partial struct Container
    {
        public Container PaddingX(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).PaddingX = value; return this; }
        public Container PaddingY(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).PaddingY = value; return this; }
        public Container MaxWidth(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).MaxWidth = value; return this; }
        public Container MaxHeight(float value) { _ = Current; if (!float.IsFinite(value) || value < 0 || value > 16384) throw new ArgumentOutOfRangeException(nameof(value)); ui.Properties(index).MaxHeight = value; return this; }
        public Container Scale(float x, float y) { _ = Current; ui.Properties(index).Scale = [x, y]; return this; }
        public Container Opacity(float alpha) { Current.opacity = alpha; return this; }
        public Container Absolute(float x, float y) { _ = Current; ui.Properties(index).Absolute = [x, y]; return this; }
        public Container Translate(float x, float y) { _ = Current; ui.Properties(index).Translate = [x, y]; return this; }
        public Container BackdropBlur(float sigma) { _ = Current; ui.Properties(index).Blur = sigma; return this; }
        public Container BackgroundAlpha(float alpha) { _ = Current; ui.Properties(index).Alpha = alpha; return this; }
        public Container Clip(bool enabled = true) { _ = Current; ui.Properties(index).Clip = enabled; return this; }
        public Container Selectable(bool enabled = true) { _ = Current; ui.Properties(index).Selectable = enabled; return this; }
        public Container Sticky(float top = 0) { _ = Current; ui.Properties(index).Sticky = top; return this; }
        public Container FollowEnd(bool enabled = true) { _ = Current; ui.Properties(index).FollowEnd = enabled; return this; }
        public Container SmoothScroll(bool enabled = true) { _ = Current; ui.Properties(index).SmoothScroll = enabled; return this; }
    }
}
