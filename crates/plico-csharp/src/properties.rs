//! Optional primitive attributes; this is layout/paint data, not widget state.
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Properties {
    pub keys: Vec<String>,
    pub absolute: Option<[f32; 2]>,
    pub translate: Option<[f32; 2]>,
    pub blur: f32,
    pub foreground_blur: f32,
    pub corner_smoothing: f32,
    pub shadow: Option<Shadow>,
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: u32,
    pub opacity: f32,
}
impl Shadow {
    fn as_shadow(&self) -> plico::BoxShadow {
        plico::BoxShadow {
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            blur: self.blur,
            spread: self.spread,
            color: plico::Color::Literal(self.color),
            opacity: self.opacity,
        }
    }
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
        if p.keys.len() > 32
            || p.keys
                .iter()
                .any(|key| key.is_empty() || key.len() > 64 || key.contains('\0'))
        {
            return Err("invalid unhandled-key bindings".into());
        }
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
            || !p.foreground_blur.is_finite()
            || !(0. ..=64.).contains(&p.foreground_blur)
            || !p.corner_smoothing.is_finite()
            || !(0. ..=1.).contains(&p.corner_smoothing)
            || p.shadow
                .as_ref()
                .is_some_and(|s| s.color > 0xffffff || !s.as_shadow().valid())
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
        style.foreground_blur = self.foreground_blur;
        style.corner_smoothing = self.corner_smoothing;
        style.shadow = self.shadow.as_ref().map(Shadow::as_shadow);
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
    fn shadow_parameters_are_bounded_and_map_without_unit_conversion() {
        let json = r#"{"shadow":{"offset_x":0,"offset_y":2,"blur":12,"spread":-1,"color":0,"opacity":0.08}}"#;
        let p = Properties::parse(json, "").unwrap();
        let mut style = plico::Style::default();
        p.apply(&mut style);
        let shadow = style.shadow.unwrap();
        assert_eq!(shadow.blur, 12.);
        assert_eq!(shadow.spread, -1.);
        assert_eq!(shadow.opacity, 0.08);
        for (from, to) in [("12", "-1"), ("12", "129"), ("0.08", "2"), ("-1", "16385")] {
            assert!(Properties::parse(&json.replace(from, to), "").is_err());
        }
        Properties::default().apply(&mut style);
        assert_eq!(style.shadow, None);
    }
    #[test]
    fn utf8_spans_and_primitive_ranges_are_checked() {
        let valid = r#"{"spans":[{"start":0,"end":3,"weight":700}],"absolute":[36,112],"blur":18,"alpha":0.32}"#;
        let p = Properties::parse(valid, "你a").unwrap();
        let mut style = plico::Style::default();
        p.apply(&mut style);
        assert_eq!(style.layout.position, plico::Position::Absolute);
        assert_eq!(style.absolute, Some([36., 112.]));
        assert_eq!(style.backdrop_blur, 18.);
        let foreground = Properties::parse(r#"{"foreground_blur":5}"#, "").unwrap();
        foreground.apply(&mut style);
        assert_eq!(style.foreground_blur, 5.);
        Properties::parse(r#"{"corner_smoothing":1}"#, "")
            .unwrap()
            .apply(&mut style);
        assert_eq!(style.corner_smoothing, 1.);
        assert!(Properties::parse(r#"{"corner_smoothing":1.1}"#, "").is_err());
        assert!(Properties::parse(r#"{"corner_smoothing":-0.1}"#, "").is_err());
        assert!(Properties::parse(r#"{"foreground_blur":-1}"#, "").is_err());
        assert!(Properties::parse(r#"{"foreground_blur":65}"#, "").is_err());
        assert!(Properties::parse(r#"{"spans":[{"end":1,"weight":700}]}"#, "你").is_err());
        assert!(Properties::parse(r#"{"blur":65}"#, "").is_err());
        assert!(Properties::parse(r#"{"alpha":2}"#, "").is_err());
        assert!(Properties::parse(r#"{"unknown":true}"#, "").is_err());
    }
}

#[cfg(test)]
mod key_binding_tests {
    use super::*;
    #[test]
    fn key_bindings_are_bounded_before_crossing_the_event_bridge() {
        assert!(Properties::parse(r#"{"keys":["ArrowLeft","ArrowRight"]}"#, "").is_ok());
        for keys in [
            vec!["".to_owned()],
            vec!["x".repeat(65)],
            vec!["ArrowLeft".to_owned(); 33],
        ] {
            let json = serde_json::json!({"keys": keys}).to_string();
            assert!(Properties::parse(&json, "").is_err());
        }
    }
}
