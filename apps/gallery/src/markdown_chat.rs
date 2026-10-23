//! A deterministic incoming reply: no network connection or async runtime required.
use plico_core::prelude::*;
use plico_markdown::{RevealUnit, Stream};
use std::time::Duration;

pub const REPLY: &str = "# A small interface, in motion\n\nA good streaming reply should feel **calm and readable**, even when the network delivers a sudden burst. This paragraph keeps *emphasis* and `inline code` inside one shaped text node.\n\n## Built for real text\n\n中文也可以逐步显示。组合字符 e\u{301} 和 emoji 👩‍💻 会以完整的字素显示，而不是拆成几个闪烁的字符。\n\n- Keep earlier paragraphs in place.\n- Let new content arrive at a steady pace.\n- Stop following when the reader scrolls up.\n\n```rust\nfn reply(cx: &mut UiContext, message: &mut Stream) {\n    message.show(cx);\n}\n```\n\n## When the stream ends\n\nThe remaining text keeps revealing until it catches up. The interface then rests: no background animation loop.\n\nYou can replay this answer, pause its arrival, or deliver the rest in one burst.\n";

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Chat {
    pub stream: Stream,
    sent: usize,
    last: Option<Duration>,
    elapsed: Duration,
    pub paused: bool,
    pub follow: bool,
    previous_offset: f32,
    mode: usize,
}
impl Default for Chat {
    fn default() -> Self {
        Self {
            stream: Stream::default(),
            sent: 0,
            last: None,
            elapsed: Duration::ZERO,
            paused: false,
            follow: true,
            previous_offset: 0.,
            mode: 0,
        }
    }
}
impl Chat {
    pub fn burst(&mut self) {
        self.stream.push(&REPLY[self.sent..]);
        self.sent = REPLY.len();
        self.stream.finish();
    }
    #[component]
    #[rustfmt::skip]
    pub fn show(&mut self, cx: &mut UiContext) {
        let now = cx.now();
        if let Some(last) = self.last
            && !self.paused
        {
            self.elapsed += now.saturating_sub(last);
        }
        self.last = Some(now);
        if !self.paused && self.sent < REPLY.len() {
            // Uneven 120ms batches stand in for network delivery. One append per frame.
            let target = ((self.elapsed.as_millis() / 120 + 1) as usize * 23).min(REPLY.len());
            let mut end = target.max(self.sent);
            while end < REPLY.len() && !REPLY.is_char_boundary(end) {
                end += 1;
            }
            self.stream.push(&REPLY[self.sent..end]);
            self.sent = end;
            if self.sent == REPLY.len() {
                self.stream.finish();
            }
        }
        text(cx, "Markdown chat").font_size(28).weight(750);
        text(cx, "A reply that arrives at its own pace.").color(Color::Muted);
        row(cx).gap(8).wrap(true).align(Align::Center) + |cx| {
            if button(cx).text("Replay").primary().clicked() {
                *self = Self::default();
            }
            if button(cx).text(if self.paused { "Resume" } else { "Pause arrival" }).clicked()
            {
                self.paused = !self.paused;
            }
            if button(cx).text("Send remaining").disabled(self.sent == REPLY.len()).clicked()
            {
                self.burst();
            }
            checkbox(cx, "Reduce motion", &mut self.stream.reduced_motion);
        };
        row(cx).gap(12).align(Align::Center).wrap(true) + |cx| {
            segmented(cx, &mut self.mode, &["Character", "Word", "Line", "At once"]);
            self.stream.unit = [
                RevealUnit::Character,
                RevealUnit::Word,
                RevealUnit::Line,
                RevealUnit::AtOnce,
            ][self.mode];
            checkbox(cx, "Follow reply", &mut self.follow);
        };
        separator(cx);
        column(cx).height(0.).grow(1.).scroll_y(true).smooth_scroll(true).gap(20).padding(16).radius(14.)
            .background(Color::Surface) + |cx|
        {
            if let Some([offset, _]) = cx.scroll_viewport() {
                if offset + 2. < self.previous_offset {
                    self.follow = false;
                }
                self.previous_offset = offset;
            }
            column(cx).padding(14).radius(12.).background(Color::Panel).gap(6) + |cx| {
                text(cx, "YOU").font_size(10).weight(700).color(Color::Muted);
                text(cx, "What makes a streaming interface feel good?").font_size(16);
            };
            column(cx).gap(12).padding(4) + |cx| {
                row(cx).gap(8).align(Align::Center) + |cx| {
                    icon(cx, &crate::icons::SPARKLES).size(18., 18.);
                    text(cx, "PLICO").font_size(11).weight(700).color(Color::Muted);
                    badge(
                        cx,
                        if self.stream.is_receiving() {
                            "Receiving"
                        } else if self.stream.is_revealing() { "Revealing" } else { "Complete" },
                    );
                };
                self.stream.show(cx);
            };
            if self.follow {
                cx.scroll_to_y(1_000_000.);
            }
        };
        if !self.paused && self.sent < REPLY.len() {
            cx.request_animation_frame();
        }
    }
}
