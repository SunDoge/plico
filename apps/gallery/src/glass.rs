//! One moving application backdrop, one glass surface, and crisp foreground controls.
use plico::prelude::*;
use std::time::Duration;
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Glass {
    pub blur: f64,
    pub tint: f64,
    pub moving: bool,
    pub enabled: bool,
    pub saved: bool,
    elapsed: f64,
    last: Option<Duration>,
}
impl Default for Glass {
    fn default() -> Self {
        Self {
            blur: 18.,
            tint: 0.32,
            moving: true,
            enabled: true,
            saved: false,
            elapsed: 0.,
            last: None,
        }
    }
}
impl Glass {
    #[component]
    #[rustfmt::skip]
    pub fn show(&mut self, cx: &mut UiContext) {
        let now = cx.now();
        if self.moving
            && let Some(last) = self.last
        {
            self.elapsed += now.saturating_sub(last).as_secs_f64();
        }
        self.last = Some(now);
        text(cx, "Glass in motion").font_size(28).weight(750);
        text(cx, "A soft backdrop. A clear foreground.").color(Color::Muted);
        row(cx).gap(12).wrap(true).align(Align::Center) + |cx| {
            checkbox(cx, "Blur background", &mut self.enabled);
            checkbox(cx, "Move background", &mut self.moving);
            text(cx, "Blur").font_size(12).color(Color::Muted);
            slider(cx, "Backdrop blur", &mut self.blur, 0. ..=32.).width(100.).step(1.);
            text!(cx, "{:.0} px", self.blur).font_size(12);
            text(cx, "Tint").font_size(12).color(Color::Muted);
            slider(cx, "Glass tint", &mut self.tint, 0.05..=0.85).width(100.).step(0.05);
        };
        let drift = (self.elapsed * 0.6).sin() as f32 * 110.;
        let float = (self.elapsed * 0.4).cos() as f32 * 35.;
        column(cx).height(420.).grow(1.).width_percent(100.).clip(true).radius(20.).background(Color::Literal(0x14263c))
        + |cx|
        {
            column(cx).absolute(12., -30.).width(360.).height(360.).radius(180.)
                .background(Color::Literal(0x3887bc)).translate(drift, float) + |_cx|
            {};
            column(cx).absolute(260., 180.).width(320.).height(320.).radius(160.)
                .background(Color::Literal(0x4ba995)).translate(-drift, -float) + |_cx|
            {};
            column(cx).absolute(160., 120.).width(64.).height(300.).radius(32.).background(Color::Literal(0xf5b36b))
                .translate(drift * 0.8, 0.) + |_cx|
            {};
            column(cx).absolute(26., 30.).translate(-drift * 0.2, 0.) + |cx| {
                text(cx, "SLOW RADIO").font_size(42).weight(800).color(Color::Literal(0xd9efff));
                text(cx, "Music for the space between things.").font_size(13).color(Color::Literal(0xc3e3ec));
            };
            column(cx).absolute(36., 112.).width(330.).padding(24).gap(16).radius(22.)
                .backdrop_blur(if self.enabled { self.blur as f32 } else { 0. }).background(Color::Surface)
                .background_alpha(self.tint as f32).border(1., Color::Border).children(|cx| {
                row(cx).gap(8).align(Align::Center) + |cx| {
                    icon(cx, &crate::icons::HEADPHONES).size(18., 18.);
                    text(cx, "NOW PLAYING").font_size(11).weight(700);
                };
                column(cx).gap(4) + |cx| {
                    text(cx, "Evening focus").font_size(26).weight(750);
                    text(cx, "Low tide · Ambient sessions").font_size(13);
                };
                column(cx).height(3.).width_percent(100.).radius(2.).background(Color::Border) + |cx| {
                    column(cx).height(3.).width_percent(42.).radius(2.).background(Color::Accent) + |_cx| {};
                };
                row(cx).justify(Justify::SpaceBetween).align(Align::Center) + |cx| {
                    text(cx, "02:18 / 05:30").font_size(11);
                    if button(cx).text(if self.saved { "Unsave" } else { "Save mix" }).primary().clicked() {
                        self.saved = !self.saved;
                    }
                };
                text(cx, if self.saved { "Saved to your mixes" } else { "An ambient mix for a quiet evening" }).font_size(11);
            })
        };
        if self.moving {
            cx.request_animation_frame();
        }
    }
}
