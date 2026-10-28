//! Cached Markdown paragraphs and a clock-driven append stream. No runtime or renderer.
//! The open top-level block is reparsed; reference syntax conservatively reparses all.
use plico::{Color, TextSpan, prelude::*};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::{ops::Range, time::Duration};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlockKind {
    #[default]
    Paragraph,
    Heading(u8),
    Code,
    Quote,
    List,
    Rule,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub source: Range<usize>,
    pub kind: BlockKind,
    pub text: String,
    pub spans: Vec<TextSpan>,
    pub appeared: Duration,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    source: String,
    blocks: Vec<Block>,
    /// Bytes submitted to the parser, useful for checking cache effectiveness.
    pub parsed_bytes: usize,
}
impl Document {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    pub fn update(&mut self, source: &str, now: Duration) {
        if self.source == source {
            return;
        }
        // Keep one complete top-level block open. Blank lines do not close lists,
        // quotes or fences. Reference definitions can affect earlier paragraphs.
        let from = if source.starts_with(&self.source) && !source.contains('[') {
            self.blocks.last().map_or(0, |b| b.source.start)
        } else {
            0
        };
        let keep = self
            .blocks
            .iter()
            .take_while(|b| b.source.end <= from)
            .count();
        let old = self.blocks.split_off(keep);
        self.parsed_bytes += source.len() - from;
        for mut block in parse(&source[from..], from) {
            block.appeared = old
                .iter()
                .find(|b| b.source.start == block.source.start)
                .map_or(now, |b| b.appeared);
            self.blocks.push(block);
        }
        if source.starts_with(&self.source) {
            self.source.push_str(&source[self.source.len()..]);
        } else {
            self.source.clear();
            self.source.push_str(source);
        }
    }
}
fn parse(source: &str, offset: usize) -> Vec<Block> {
    let mut result = Vec::new();
    let mut block: Option<Block> = None;
    let mut depth = 0usize;
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut links = 0usize;
    for (event, range) in Parser::new_ext(
        source,
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .into_offset_iter()
    {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    let kind = match &tag {
                        Tag::Heading { level, .. } => BlockKind::Heading(*level as u8),
                        Tag::CodeBlock(_) => BlockKind::Code,
                        Tag::BlockQuote(_) => BlockKind::Quote,
                        Tag::List(_) => BlockKind::List,
                        _ => BlockKind::Paragraph,
                    };
                    block = Some(Block {
                        source: offset + range.start..offset + range.end,
                        kind,
                        text: String::new(),
                        spans: Vec::new(),
                        appeared: Duration::ZERO,
                    });
                }
                if let Some(b) = &mut block {
                    match tag {
                        Tag::Strong => bold += 1,
                        Tag::Emphasis => italic += 1,
                        Tag::Link { .. } => links += 1,
                        Tag::Item => {
                            if !b.text.is_empty() {
                                b.text.push('\n');
                            }
                            b.text.push_str("•  ");
                        }
                        Tag::Paragraph if !b.text.is_empty() && !b.text.ends_with('\n') => {
                            b.text.push('\n')
                        }
                        _ => {}
                    }
                }
                depth += 1;
            }
            Event::End(tag) => {
                match tag {
                    TagEnd::Strong => bold = bold.saturating_sub(1),
                    TagEnd::Emphasis => italic = italic.saturating_sub(1),
                    TagEnd::Link => links = links.saturating_sub(1),
                    _ => {}
                }
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(mut b) = block.take()
                {
                    b.source.end = offset + range.end;
                    result.push(b);
                }
            }
            Event::Rule => result.push(Block {
                source: offset + range.start..offset + range.end,
                kind: BlockKind::Rule,
                text: String::new(),
                spans: Vec::new(),
                appeared: Duration::ZERO,
            }),
            event => {
                if let Some(b) = &mut block {
                    let code = matches!(event, Event::Code(_));
                    let text = match event {
                        Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => {
                            t
                        }
                        Event::SoftBreak | Event::HardBreak => "\n".into(),
                        Event::TaskListMarker(true) => "☑ ".into(),
                        Event::TaskListMarker(false) => "☐ ".into(),
                        _ => continue,
                    };
                    let start = b.text.len();
                    b.text.push_str(&text);
                    if bold > 0 || italic > 0 || links > 0 || code {
                        b.spans.push(TextSpan {
                            range: start..b.text.len(),
                            weight: if bold > 0 { 700 } else { 400 },
                            italic: italic > 0,
                            color: (links > 0 || code).then_some(Color::Accent),
                        });
                    }
                }
            }
        }
    }
    result
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RevealUnit {
    #[default]
    Character,
    Word,
    Line,
    AtOnce,
}
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "StreamSnapshot")]
pub struct Stream {
    received: String,
    shown: usize,
    done: bool,
    last: Option<Duration>,
    credit: f64,
    pub unit: RevealUnit,
    pub reduced_motion: bool,
    #[serde(skip)]
    document: Document,
}
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct StreamSnapshot {
    received: String,
    shown: usize,
    done: bool,
    last: Option<Duration>,
    credit: f64,
    unit: RevealUnit,
    reduced_motion: bool,
}
impl TryFrom<StreamSnapshot> for Stream {
    type Error = &'static str;
    fn try_from(value: StreamSnapshot) -> Result<Self, Self::Error> {
        if !value.received.is_char_boundary(value.shown) {
            return Err("shown offset must be a UTF-8 boundary inside received text");
        }
        if !value.credit.is_finite() || value.credit < 0. {
            return Err("reveal credit must be finite and nonnegative");
        }
        Ok(Self {
            received: value.received,
            shown: value.shown,
            done: value.done,
            last: value.last,
            credit: value.credit,
            unit: value.unit,
            reduced_motion: value.reduced_motion,
            document: Document::default(),
        })
    }
}
impl Default for Stream {
    fn default() -> Self {
        Self {
            received: String::new(),
            shown: 0,
            done: false,
            last: None,
            credit: 0.,
            unit: RevealUnit::default(),
            reduced_motion: false,
            document: Document::default(),
        }
    }
}
impl Stream {
    pub fn push(&mut self, chunk: &str) {
        self.received.push_str(chunk);
    }
    pub fn finish(&mut self) {
        self.done = true;
    }
    pub fn received(&self) -> &str {
        &self.received
    }
    pub fn shown(&self) -> &str {
        &self.received[..self.shown]
    }
    pub fn is_receiving(&self) -> bool {
        !self.done
    }
    pub fn is_revealing(&self) -> bool {
        self.shown < self.received.len()
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    /// Advance using logical time. Network completion does not skip pending reveal.
    pub fn tick(&mut self, now: Duration) {
        let dt = self
            .last
            .map_or(0., |last| now.saturating_sub(last).as_secs_f64());
        self.last = Some(now);
        if self.reduced_motion || self.unit == RevealUnit::AtOnce {
            self.shown = self.received.len();
        } else {
            let pending = &self.received[self.shown..];
            let count = match self.unit {
                RevealUnit::Character => pending.graphemes(true).count(),
                RevealUnit::Word => pending.split_word_bounds().count(),
                RevealUnit::Line => pending.split_inclusive('\n').count(),
                RevealUnit::AtOnce => 0,
            };
            // Smooth bursts, catching up in roughly 250ms; cap elapsed time after pause.
            self.credit += dt.min(0.25) * (count as f64 / 0.25).max(35.);
            let n = self.credit.floor() as usize;
            let mut bytes = 0;
            match self.unit {
                RevealUnit::Character => {
                    for part in pending.graphemes(true).take(n) {
                        bytes += part.len();
                    }
                }
                RevealUnit::Word => {
                    for part in pending.split_word_bounds().take(n) {
                        bytes += part.len();
                    }
                }
                RevealUnit::Line => {
                    for part in pending.split_inclusive('\n').take(n) {
                        if part.ends_with('\n') || self.done {
                            bytes += part.len();
                        } else {
                            break;
                        }
                    }
                }
                RevealUnit::AtOnce => {}
            }
            self.shown += bytes;
            self.credit = if self.shown == self.received.len() {
                0.
            } else if bytes > 0 {
                self.credit.fract()
            } else {
                self.credit.min(1.)
            };
        }
        self.document.update(&self.received[..self.shown], now);
    }
    /// Declare one text node per paragraph, preserving shaping across inline spans.
    #[component]
    pub fn show(&mut self, cx: &mut UiContext) {
        self.tick(cx.now());
        let now = cx.now();
        let mut fading = false;
        column(cx)
            .gap(14)
            .width_percent(100.)
            .selectable(true)
            .children(|cx| {
                for block in self.document.blocks() {
                    cx.keyed(block.source.start, |cx| {
                        let alpha = if self.reduced_motion {
                            1.
                        } else {
                            now.saturating_sub(block.appeared).as_secs_f32() / 0.24
                        }
                        .clamp(0., 1.);
                        fading |= alpha < 1.;
                        if block.kind == BlockKind::Rule {
                            separator(cx);
                        } else {
                            let code = block.kind == BlockKind::Code;
                            let _ = column(cx)
                                .width_percent(100.)
                                .padding(if code { 14 } else { 0 })
                                .background(if code { Color::Panel } else { Color::Surface })
                                .radius(10.)
                                .opacity(alpha)
                                .children(|cx| {
                                    text(cx, block.text.as_str())
                                        .width_percent(100.)
                                        .spans(&block.spans)
                                        .font_size(match block.kind {
                                            BlockKind::Heading(1) => 26,
                                            BlockKind::Heading(_) => 20,
                                            _ => 15,
                                        })
                                        .weight(if matches!(block.kind, BlockKind::Heading(_)) {
                                            700
                                        } else {
                                            400
                                        });
                                });
                        }
                    });
                }
            });
        if self.is_revealing() || fading {
            cx.request_animation_frame();
        }
    }
}
