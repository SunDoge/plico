//! Optional primitive attributes; this is layout/paint data, not widget state.
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Properties {
    pub absolute: Option<[f32; 2]>,
    pub translate: Option<[f32; 2]>,
    pub blur: f32,
    pub padding_x: Option<f32>,
    pub padding_y: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub scale: Option<[f32; 2]>,
    pub alpha: Option<f32>,
    pub clip: bool,
    pub draggable: bool,
    pub selectable: bool,
    pub sticky: Option<f32>,
    pub follow_end: bool,
    pub smooth_scroll: bool,
    pub mono: bool,
    pub placeholder: Option<String>,
    pub focus: bool,
    pub password_visible: bool,
    pub transition_ms: u16,
    pub spans: Vec<Span>,
    pub shader: Option<Shader>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub weight: u16,
    pub italic: bool,
    pub color: Option<u32>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shader {
    pub values: Vec<f32>,
    pub backdrop: bool,
    pub module: bool,
    pub pointer: bool,
    pub time: f32,
    pub paused: bool,
}
impl Properties {
    pub fn parse(json: &str, text: &str) -> Result<Self, String> {
        let p: Self =
            serde_json::from_str(json).map_err(|e| format!("invalid primitive properties: {e}"))?;
        if [p.padding_x, p.padding_y, p.max_width, p.max_height]
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0. ..=16384.).contains(&v))
            || p.scale.is_some_and(|v| {
                v.iter()
                    .any(|v| !v.is_finite() || !(0.001..=100.).contains(v))
            })
            || p.absolute.is_some_and(|v| !v.iter().all(|v| v.is_finite()))
            || p.translate
                .is_some_and(|v| !v.iter().all(|v| v.is_finite()))
            || !p.blur.is_finite()
            || !(0. ..=64.).contains(&p.blur)
            || p.alpha
                .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
            || p.placeholder.as_ref().is_some_and(|s| s.len() > 16384)
            || p.sticky.is_some_and(|v| !v.is_finite())
            || p.spans.len() > 10_000
            || p.spans
                .iter()
                .any(|s| !s.as_span().valid_for(text) || s.color.is_some_and(|c| c > 0xffffff))
            || p.shader.as_ref().is_some_and(|s| {
                s.values.len() > 256
                    || !s.values.iter().all(|v| v.is_finite())
                    || !s.time.is_finite()
            })
        {
            return Err("invalid primitive property range".into());
        }
        Ok(p)
    }
    pub fn apply(&self, style: &mut plico::Style) {
        style.transition_ms = self.transition_ms;
        style.padding_x = self.padding_x;
        style.padding_y = self.padding_y;
        style.max_width = self.max_width;
        style.max_height = self.max_height;
        style.scale = self.scale.unwrap_or([1., 1.]);
        style.absolute = self.absolute;
        if self.absolute.is_some() {
            style.layout.position = plico::Position::Absolute;
        }
        style.translate = self.translate.unwrap_or([0., 0.]);
        style.backdrop_blur = self.blur;
        style.background_alpha = self.alpha.unwrap_or(1.);
        style.clip |= self.clip;
        style.selectable = self.selectable;
        style.input_password_visible = self.password_visible;
        style.sticky_top = self.sticky;
        style.follow_end = self.follow_end;
        style.smooth_scroll = self.smooth_scroll;
        if self.mono {
            style.font_family = plico::FontFamily::Monospace;
        }
    }
}
impl Span {
    pub fn as_span(&self) -> plico::TextSpan {
        plico::TextSpan {
            range: self.start..self.end,
            weight: self.weight,
            italic: self.italic,
            color: self.color.map(plico::Color::Literal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_spans_and_primitive_ranges_are_checked() {
        let valid = r#"{"spans":[{"start":0,"end":3,"weight":700}],"absolute":[36,112],"blur":18,"alpha":0.32}"#;
        let p = Properties::parse(valid, "你a").unwrap();
        let mut style = plico::Style::default();
        p.apply(&mut style);
        assert_eq!(style.layout.position, plico::Position::Absolute);
        assert_eq!(style.absolute, Some([36., 112.]));
        assert_eq!(style.backdrop_blur, 18.);
        assert!(Properties::parse(r#"{"spans":[{"end":1,"weight":700}]}"#, "你").is_err());
        assert!(Properties::parse(r#"{"blur":65}"#, "").is_err());
        assert!(Properties::parse(r#"{"alpha":2}"#, "").is_err());
        assert!(Properties::parse(r#"{"unknown":true}"#, "").is_err());
    }
}
