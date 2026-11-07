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
        public float ForegroundBlur { get; set; }
        public float CornerSmoothing { get; set; }
        public ShadowProperties? Shadow { get; set; }
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
    // All fields are required on the wire, including zero offsets/spread.
    internal sealed record ShadowProperties(
        [property: JsonIgnore(Condition = JsonIgnoreCondition.Never)] float OffsetX,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.Never)] float OffsetY,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.Never)] float Blur,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.Never)] float Spread,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.Never)] uint Color,
        [property: JsonIgnore(Condition = JsonIgnoreCondition.Never)] float Opacity);
    private void SetShadow(int index, float x, float y, float blur, float spread, uint color, float opacity)
    {
        if (!float.IsFinite(x) || Math.Abs(x) > 16384 || !float.IsFinite(y) || Math.Abs(y) > 16384
            || !float.IsFinite(spread) || Math.Abs(spread) > 16384 || !float.IsFinite(blur) || blur < 0 || blur > 128
            || color > 0xffffff || !float.IsFinite(opacity) || opacity < 0 || opacity > 1)
            throw new ArgumentOutOfRangeException(nameof(blur), "Invalid outer shadow parameters");
        Properties(index).Shadow = new(x, y, blur, spread, color, opacity);
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
        /// Set one outer shadow (last call wins). Blur is CSS radius, not sigma.
        public Builder Shadow(float x = 0, float y = 2, float blur = 8, float spread = 0, uint color = 0, float opacity = .10f)
        { _ = Current; ui.SetShadow(index, x, y, blur, spread, color, opacity); return this; }
        public Builder NoShadow() { _ = Current; ui.Properties(index).Shadow = null; return this; }

        public Builder CornerSmoothing(float amount = 1) { _ = Current; if (!float.IsFinite(amount) || amount < 0 || amount > 1) throw new ArgumentOutOfRangeException(nameof(amount)); ui.Properties(index).CornerSmoothing = amount; return this; }
        public Builder Blur(float sigma) { _ = Current; if (!float.IsFinite(sigma) || sigma < 0 || sigma > 64) throw new ArgumentOutOfRangeException(nameof(sigma)); ui.Properties(index).ForegroundBlur = sigma; return this; }
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
        /// Set one outer shadow (last call wins). Blur is CSS radius, not sigma.
        public Container Shadow(float x = 0, float y = 2, float blur = 8, float spread = 0, uint color = 0, float opacity = .10f)
        { _ = Current; ui.SetShadow(index, x, y, blur, spread, color, opacity); return this; }
        public Container NoShadow() { _ = Current; ui.Properties(index).Shadow = null; return this; }

        public Container CornerSmoothing(float amount = 1) { _ = Current; if (!float.IsFinite(amount) || amount < 0 || amount > 1) throw new ArgumentOutOfRangeException(nameof(amount)); ui.Properties(index).CornerSmoothing = amount; return this; }
        public Container Blur(float sigma) { _ = Current; if (!float.IsFinite(sigma) || sigma < 0 || sigma > 64) throw new ArgumentOutOfRangeException(nameof(sigma)); ui.Properties(index).ForegroundBlur = sigma; return this; }
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
