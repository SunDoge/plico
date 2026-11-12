// Palette, proportions and reveal timing adapted from Graphite Mines (MIT).
use super::{Mines, Status};
use plico::BoxShadow;
use plico::prelude::*;
const PAGE: Color = Color::Literal(0xf3f1ec);
const PANEL: Color = Color::Literal(0xfbfaf7);
const LINE: Color = Color::Literal(0xe2ded6);
const INK: Color = Color::Literal(0x3b3f3d);
const MUTED: Color = Color::Literal(0x8e918c);
const ACCENT: Color = Color::Literal(0x6f9d8d);
const HIDDEN: Color = Color::Literal(0xcbd1c8);
const TINTS: [u32; 9] = [
    0xeceae4, 0xe1eed8, 0xeeeacf, 0xf3e0ce, 0xf0d8db, 0xe4deef, 0xd8eaea, 0xe2e2df, 0xe6e4de,
];
const INKS: [u32; 9] = [
    0x3b3f3d, 0x4e7f4a, 0x86733a, 0xaa6540, 0xa24c62, 0x6d58a0, 0x3e7d80, 0x4c4c4c, 0x7a7a78,
];
static FLAG: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6 21V3m0 1h12l-3 5 3 5H6" fill="none" stroke="#db7461" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##,
);
static MINE: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g stroke="#434a48" stroke-width="2" stroke-linecap="round"><path d="M12 2v20M2 12h20M5 5l14 14M5 19L19 5"/><circle cx="12" cy="12" r="6" fill="#434a48"/></g><circle cx="10" cy="10" r="1.5" fill="#fbfaf7"/></svg>"##,
);
static CLOCK: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="#8e918c" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="8.5"/><path d="M12 6v6l4 2"/></g></svg>"##,
);
fn shadow(y: f32, blur: f32, opacity: f32) -> BoxShadow {
    BoxShadow {
        offset_x: 0.,
        offset_y: y,
        blur,
        spread: 0.,
        color: Color::Literal(0x6b6455),
        opacity,
    }
}
fn stamp(seconds: u64) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}
fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t).powi(3)
}

impl Mines {
    #[component]
    #[rustfmt::skip]
    pub fn show(&mut self, cx: &mut UiContext, sidebar_collapsed: bool) {
        self.time = cx.now();
        // Native declarations also support serialized preview state. Repair
        // stale board dimensions before indexing it, including edited recipes.
        if self.playing && self.cells.len() != self.columns.clamp(1, 64) * self.rows.clamp(1, 64) {
            self.restart();
        }
        let [vw, vh] = cx.viewport_size();
        self.columns = self.columns.clamp(1, 64);
        self.rows = self.rows.clamp(1, 64);
        self.mine_count = self.mine_count.min(Self::maximum_mines(self.columns, self.rows));
        let w = (vw - if sidebar_collapsed { 68. } else { 224. } - 48.).max(240.);
        let h = (vh - 100.).max(300.);
        column(cx).width_percent(100.).height(0.).grow(1.).min_height(0.).background(PAGE).radius(16.).clip(true)
        + |cx|
        {
            if self.playing {
                self.board(cx, w, h);
            } else {
                self.setup(cx, w, h);
            }
        };
    }
    #[component]
    #[rustfmt::skip]
    fn setup(&mut self, cx: &mut UiContext, w: f32, h: f32) {
        let scale = ((w - 32.) / 440.).min((h - 24.) / 490.).clamp(0.5, 1.);
        let cw = 440. * scale;
        let preview_h = ((h - 438. * scale - 48.) / scale).clamp(0., 150.);
        column(cx).width_percent(100.).height_percent(100.).align(Align::Center).justify(Justify::Center) + |cx| {
            column(cx).width(cw).align(Align::Center).gap(0) + |cx| {
                text(cx, "Mines").font_size((34. * scale) as u16).weight(700).color(INK);
                column(cx).height(8. * scale) + |_cx| {};
                text(cx, "Choose a field and how many mines hide in it.").font_size((14. * scale) as u16)
                    .color(MUTED);
                if preview_h >= 48. {
                    column(cx).height(24. * scale) + |_cx| {};
                    preview(cx, self.columns, self.rows, cw * 0.66, preview_h * scale);
                }
                column(cx).height(28. * scale) + |_cx| {};
                row(cx).align(Align::Center).justify(Justify::Center) + |cx| {
                    for (i, caption) in ["COLUMNS", "ROWS", "MINES"].iter().enumerate()
                    {
                        if i == 1 {
                            column(cx).width(44. * scale).align(Align::Center) + |cx| {
                                text(cx, "×").font_size(24).color(MUTED);
                            };
                        } else if i == 2 {
                            column(cx).width(37. * scale).align(Align::Center) + |cx| {
                                column(cx).width(1.).height(40. * scale).background(LINE) + |_cx| {};
                            };
                        }
                        let value = [self.columns, self.rows, self.mine_count][i];
                        let selected = self.selected_field == i;
                        if self.fields[i].is_empty() {
                            self.fields[i] = value.to_string();
                        }
                        let mut response = plico::InputResponse::default();
                        column(cx).key(i as u64).width(112. * scale).align(Align::Center).gap(10. * scale) + |cx| {
                            column(cx).width(112. * scale).height(84. * scale).radius(18. * scale)
                                .background(PANEL).border(if selected { 2. } else { 1.5 },
                                    if selected { ACCENT } else { LINE }).focus_within_border(2., ACCENT).shadow(shadow(3., 12., 0.07))
                                .align(Align::Center).justify(Justify::Center) + |cx|
                            {
                                let digits = self.fields[i].len().clamp(1, 4);
                                let font = if digits >= 4 { 31. } else { 40. };
                                response = text_input_string(cx, *caption, &mut self.fields[i]).unstyled().width(112. * scale).height(84. * scale)
                                    .radius(18. * scale).padding(0).padding_x(
                                    ((112. - digits as f32 * font * 0.63 - 4.)
                                        / 2.).max(0.)
                                        * scale,
                                ).font_size((font * scale) as u16).weight(600).color(INK)
                                    .navigation(true).autofocus(i == 0).response();
                            };
                            text(cx, *caption).font_size((11. * scale) as u16).weight(600).color(MUTED);
                            if response.focused() {
                                self.selected_field = i;
                            }
                            if response.changed() {
                                self.selected_field = i;
                                self.fields[i].retain(|c| c.is_ascii_digit());
                                if !self.fields[i].is_empty() {
                                    let n = self.fields[i].parse::<usize>().unwrap_or(usize::MAX);
                                    self.edit_number(n);
                                    self.fields[i] =
                                        [self.columns, self.rows, self.mine_count]
                                            [i].to_string();
                                }
                            }
                            if let Some(nav) = response.navigation() {
                                self.selected_field = i;
                                match nav {
                                    plico::InputNavigation::Previous => {
                                        self.edit_number(value.saturating_add(1))
                                    }
                                    plico::InputNavigation::Next => {
                                        self.edit_number(value.saturating_sub(1))
                                    }
                                    _ => {}
                                }
                                self.fields[i] =
                                    [self.columns, self.rows, self.mine_count][i].to_string();
                            } else if !response.focused() {
                                self.fields[i] =
                                    [self.columns, self.rows, self.mine_count][i].to_string();
                            }
                            if response.submitted() {
                                self.restart();
                            }
                        };
                    }
                };
                column(cx).height(12. * scale) + |_cx| {};
                let n = self.columns * self.rows;
                let hint = if self.custom_mines {
                    format!(
                        "{:.1}% of {n} tiles · {} suggested",
                        self.mine_count as f64 / n as f64 * 100.,
                        Self::suggested_mines(self.columns, self.rows)
                    )
                } else {
                    format!(
                        "{:.1}% of {n} tiles",
                        self.mine_count as f64 / n as f64 * 100.
                    )
                };
                text(cx, &hint).font_size((12. * scale) as u16).color(MUTED);
                column(cx).height(24. * scale) + |_cx| {};
                if action(cx, "Start sweeping", 220. * scale, 48. * scale, true) {
                    self.restart();
                }
                column(cx).height(28. * scale) + |_cx| {};
                row(cx).width_percent(100.).gap(12).align(Align::Center) + |cx| {
                    column(cx).height(1.).width(0.).grow(1.).background(LINE) + |_cx| {};
                    text(cx, "or pick a classic").font_size((12. * scale) as u16).color(MUTED);
                    column(cx).height(1.).width(0.).grow(1.).background(LINE) + |_cx| {};
                };
                column(cx).height(16. * scale) + |_cx| {};
                row(cx).gap(12. * scale) + |cx| {
                    for (label, c, r, m) in [
                        ("Easy", 9, 9, 10),
                        ("Medium", 16, 16, 40),
                        ("Hard", 30, 16, 99),
                    ] {
                        if button_base(cx, label).key(c as u64).width((cw - 24. * scale) / 3.)
                            .height(72. * scale).padding(0).radius(14. * scale).background(PANEL)
                            .hover_background(Color::Literal(0xffffff)).pressed_background(HIDDEN)
                            .border(1., LINE).shadow(shadow(2., 8., 0.08)).children(|cx| {
                            column(cx).width_percent(100.).height_percent(100.).align(Align::Center)
                                .justify(Justify::Center).gap(5) + |cx|
                            {
                                text(cx, label).font_size((14. * scale) as u16).weight(600)
                                    .color(INK);
                                plico::text!(cx, "{c} × {r} · {m} mines")
                                    .font_size((11. * scale) as u16).color(MUTED);
                            };
                        }).clicked()
                        {
                            self.custom_mines = false;
                            self.configure(c, r, m);
                        }
                    }
                };
                column(cx).height(20. * scale) + |_cx| {};
                text(cx, "Tab to select · ↑ ↓ adjust · type a number · Enter to start").font_size((11. * scale) as u16).color(MUTED);
            };
        };
    }
    #[component]
    #[rustfmt::skip]
    fn board(&mut self, cx: &mut UiContext, w: f32, h: f32) {
        let compact = w < 560.;
        let margin = if compact { 12. } else { 24. };
        let pad = if compact { 6. } else { 10. };
        let pitch = ((w - margin * 2. - pad * 2.) / self.columns as f32).min((h - 116. - pad * 2.) / self.rows as f32)
            .clamp(3., 54.);
        let bw = pitch * self.columns as f32 + pad * 2.;
        let bh = pitch * self.rows as f32 + pad * 2.;
        let gap = if pitch >= 12. {
            pitch * 0.09
        } else if pitch >= 6. { 1. } else { 0.5 };
        let tile = pitch - gap;
        let now = self.time.as_secs_f64();
        let result = self.ended() && now >= self.result_at;
        // The app schedules state boundaries. Between them the retained host
        // samples transforms; no full-board declaration runs for a glyph tween.
        for cell in &self.cells {
            if cell.open
                && cell.reveal_at > now
                && let Ok(at) = std::time::Duration::try_from_secs_f64(cell.reveal_at)
            {
                cx.request_rebuild_at(at);
            }
        }
        if self.ended()
            && self.result_at > now
            && let Ok(at) = std::time::Duration::try_from_secs_f64(self.result_at)
        {
            cx.request_rebuild_at(at);
        }
        let manual_animation = !self.reduced_motion
            && self.ended()
            && now
                < self.finished.as_secs_f64() + if self.status == Status::Won { 2.5 } else { 0.9 };
        if manual_animation {
            cx.request_animation_frame();
        }
        if self.status == Status::Running {
            cx.request_rebuild_at(self.started.saturating_add(std::time::Duration::from_secs(
                self.elapsed().as_secs().saturating_add(1))));
        }
        column(cx).width_percent(100.).height_percent(100.).padding(margin).gap(16) + |cx| {
            row(cx).width_percent(100.).height(40.).align(Align::Center).justify(Justify::SpaceBetween) + |cx| {
                if action(cx, if compact { "←" } else { "←  Sizes" }, if compact { 40. } else { 96. }, 40.,
                    false) {
                    self.playing = false;
                }
                row(cx).gap(10) + |cx| {
                    chip(
                        cx,
                        &FLAG,
                        &format!(
                            "{}",
                            self.mine_count as isize - self.flags() as isize
                        ),
                        if compact { 76. } else { 96. },
                    );
                    if w > 350. {
                        chip(cx, &CLOCK, &stamp(self.elapsed().as_secs()),
                            if compact { 94. } else { 106. });
                    }
                };
                if action(cx, if compact { "↻" } else { "New game" }, if compact { 40. } else { 116. }, 40.,
                    false) {
                    self.restart();
                }
            };
            column(cx).width_percent(100.).height(0.).grow(1.).align(Align::Center).justify(Justify::Center) + |cx| {
                column(cx).width(bw).height(bh).shrink(0.).radius(16.).background(PANEL).border(1., LINE)
                    .shadow(shadow(4., 18., 0.05)) + |cx|
                {
                    self.hovered = None;
                    let request_focus = self.focus_cursor;
                    self.focus_cursor = false;
                    let mut focused = None;
                    let mut pressed = None;
                    for i in 0..self.cells.len() {
                        let c = self.cells[i];
                        let age = (now - c.reveal_at) as f32;
                        let revealed = c.open && (age >= 0. || self.reduced_motion);
                        let tint = if revealed {
                            if c.mine { 0xf0d6cf } else { TINTS[c.adjacent.min(8) as usize] }
                        } else {
                            0xcbd1c8
                        };
                        let name = if c.flagged {
                            "flagged".to_string()
                        } else if !c.open {
                            "hidden".into()
                        } else if c.mine {
                            "mine".into()
                        } else {
                            format!("{} adjacent mines", c.adjacent)
                        };
                        let label = format!(
                            "Row {}, column {}: {name}",
                            i / self.columns + 1,
                            i % self.columns + 1
                        );
                        let response = button_base(cx, &label).key(i as u64).absolute(
                            pad + (i % self.columns) as f32 * pitch + gap / 2.,
                            pad + (i / self.columns) as f32 * pitch + gap / 2.).width(tile).height(tile).radius(tile * 0.2).padding(0).border(
                                if self.cursor == i && self.keyboard_cursor && !result { 1.5 } else { 0. },
                                ACCENT).children(|cx| {
                            if request_focus && self.cursor == i {
                                cx.request_focus();
                            }
                            if cx.is_focused() {
                                focused = Some(i);
                            }
                            let secondary = cx.secondary_clicked();
                            if cx.pressed() {
                                pressed = Some(i);
                            }
                            let hovered = cx.hovered();
                            let pressure = if revealed {
                                0.
                            } else {
                                self.pressed_cell.map_or(0., |j| {
                                    let d = self.distance(i, j) as f32 * pitch;
                                    let radius = (pitch * 2.6).max(72.);
                                    (1. - d * d / (radius * radius)).max(0.).powi(2)
                                })
                            };
                            let size = 1.
                                + if hovered && !revealed { 0.07 } else { 0. }
                                - pressure * 0.2;
                            let glyph_scale =
                                if c.flagged || revealed { 1. } else { 0. };
                            let duration =
                                if self.reduced_motion { 0 } else { 240 };
                            column(cx).width_percent(100.).height_percent(100.).radius(tile * 0.2)
                                .background(if hovered && !revealed {
                                Color::Literal(0xdce2d8)
                            } else {
                                Color::Literal(tint)
                            }).scale(size, size).transition(duration).align(Align::Center)
                                .justify(Justify::Center) + |cx|
                            {
                                // The logical reveal creates glyphs once. The wave
                                // only changes presentation, never mounts per frame.
                                if tile >= 10.
                                    && (c.flagged
                                        || c.open && (c.mine || c.adjacent > 0))
                                {
                                    column(cx).width_percent(100.).height_percent(100.)
                                        .align(Align::Center).justify(Justify::Center)
                                        .scale(glyph_scale, glyph_scale).transition(duration)
                                        .enter_from(plico::Presentation {
                                        scale: if self.reduced_motion {
                                            [1., 1.]
                                        } else {
                                            [0., 0.]
                                        },
                                        ..Default::default()
                                    }) + |cx|
                                    {
                                        if c.flagged {
                                            cx.svg(&FLAG).size(tile * 0.58, tile * 0.58);
                                        } else if c.mine {
                                            cx.svg(&MINE).size(tile * 0.6, tile * 0.6);
                                        } else {
                                            plico::text!(
                                                cx, "{}", c.adjacent
                                            ).font_size(
                                                (tile * 0.47).max(8.)
                                                    as u16,
                                            ).weight(650).color(Color::Literal(
                                                INKS[c.adjacent.min(8)
                                                    as usize],
                                            ));
                                        }
                                        if self.status == Status::Lost
                                            && c.flagged
                                            && !c.mine
                                        {
                                            text(cx, "×").font_size((tile * 0.7) as u16).color(INK);
                                        }
                                    };
                                }
                            };
                            secondary
                        });
                        if response.response.hovered() {
                            self.hovered = Some(i);
                        }
                        if !self.ended() {
                            if response.inner {
                                if c.open {
                                    self.chord(i);
                                } else {
                                    self.flag(i);
                                }
                            } else if response.clicked() {
                                self.cursor = i;
                                if c.open {
                                    self.chord(i);
                                } else {
                                    self.reveal(i);
                                }
                            }
                        }
                    }
                    if self.pressed_cell != pressed {
                        self.pressed_cell = pressed;
                        cx.request_rebuild();
                    }
                    if !request_focus && let Some(i) = focused {
                        self.cursor = i;
                    }
                    if let Some(i) = self.exploded.filter(|_| !self.reduced_motion) {
                        explosion(cx, pad + (i % self.columns) as f32 * pitch + pitch / 2.,
                            pad + (i / self.columns) as f32 * pitch + pitch / 2., pitch,
                            (now - self.finished.as_secs_f64()) as f32);
                    }
                    if self.status == Status::Won && !self.reduced_motion {
                        confetti(cx, bw, bh, (now - self.finished.as_secs_f64()) as f32);
                    }
                    if result {
                        let duration = if self.reduced_motion { 0 } else { 240 };
                        column(cx).absolute(0., 0.).width(bw).height(bh).radius(16.).background(PAGE)
                            .opacity(0.86).transition(duration).enter_from(plico::Presentation {
                            opacity: 0.,
                            ..Default::default()
                        }) + |_cx|
                        {};
                        // The result is a sibling above the veil, so its text
                        // and actions retain full contrast and hit testing.
                        let cw = 340_f32.min((w - 48.).max(220.));
                        column(cx).absolute((bw - cw) / 2., (bh - 232.) / 2.).width(cw).height(232.)
                            .radius(22.).background(PANEL).border(1., LINE).shadow(shadow(12., 48., 0.15))
                            .transition(duration).enter_from(plico::Presentation {
                            opacity: 0.,
                            translate: [0., 18.],
                            ..Default::default()
                        }).align(Align::Center).justify(Justify::Center).gap(12) + |cx|
                        {
                            cx.svg(if self.status == Status::Won { &FLAG } else { &MINE })
                                .size(44., 44.);
                            text(cx,
                                if self.status == Status::Won { "Field cleared" } else { "Boom." }).font_size(28).weight(700).color(INK);
                            plico::text!(
                                cx,
                                "{} · {} × {} · {} mines",
                                stamp(self.elapsed().as_secs()),
                                self.columns,
                                self.rows,
                                self.mine_count
                            ).font_size(12).color(MUTED);
                            row(cx).gap(10) + |cx| {
                                if action(cx, "Change size", (cw - 50.) / 2., 40., false) {
                                    self.playing = false;
                                }
                                if action(cx, "Play again", (cw - 50.) / 2., 40., true) {
                                    self.restart();
                                }
                            };
                        };
                    }
                };
            };
            text(cx, "Click to reveal · right-click / F to flag · click a number to clear around it · R to restart")
                .font_size(if compact { 10 } else { 11 }).color(MUTED);
        };
    }
}
#[component]
#[rustfmt::skip]
fn action(cx: &mut UiContext, label: &str, width: f32, height: f32, primary: bool) -> bool {
    button_base(cx, label).width(width).height(height).padding(0).radius(14.)
        .background(if primary { ACCENT } else { PANEL })
        .hover_background(if primary { Color::Literal(0x7caa9a) } else { Color::Literal(0xffffff) })
        .pressed_background(if primary { Color::Literal(0x557f71) } else { HIDDEN })
        .border(if primary { 0. } else { 1. }, LINE).shadow(shadow(if primary { 4. } else { 2. },
            if primary { 16. } else { 8. }, if primary { 0.18 } else { 0.06 })).children(|cx| {
        column(cx).width_percent(100.).height_percent(100.).align(Align::Center).justify(Justify::Center) + |cx| {
            text(cx, label).font_size(14).weight(600).color(if primary {
                Color::Literal(0xffffff)
            } else {
                INK
            });
        };
    }).clicked()
}
#[component]
#[rustfmt::skip]
fn chip(cx: &mut UiContext, icon: &SvgAsset, value: &str, width: f32) {
    row(cx).width(width).height(40.).radius(12.).background(PANEL).border(1., LINE).align(Align::Center)
        .justify(Justify::Center).gap(8) + |cx|
    {
        cx.svg(icon).size(18., 18.);
        text(cx, value).font_size(15).weight(600).color(INK);
    };
}
#[component]
#[rustfmt::skip]
fn preview(cx: &mut UiContext, columns: usize, rows: usize, width: f32, height: f32) {
    let pitch = (width / columns as f32).min(height / rows as f32).min(15.);
    let max_side = columns.max(rows) as f32;
    column(cx).width(pitch * columns as f32).height(pitch * rows as f32) + |cx| {
        for y in 0..rows {
            for x in 0..columns {
                let mut hash = (x as u32).wrapping_mul(374761393).wrapping_add((y as u32).wrapping_mul(668265263));
                hash = (hash ^ (hash >> 13)).wrapping_mul(1274126177);
                hash ^= hash >> 16;
                let u = (x as f32 + 0.5 - columns as f32 * 0.4) / max_side;
                let v = (y as f32 + 0.5 - rows as f32 * 0.56) / max_side;
                let d = u.hypot(v) + (hash & 255) as f32 / 255. * 0.07;
                let color = if d < 0.2 {
                    TINTS[0]
                } else if d < 0.26 { TINTS[1 + hash as usize % 3] } else { 0xcbd1c8 };
                column(cx).key((y * columns + x) as u64).absolute(x as f32 * pitch, y as f32 * pitch)
                    .width(pitch * 0.88).height(pitch * 0.88).radius((pitch * 0.22).min(3.))
                    .background(Color::Literal(color)) + |_cx|
                {};
            }
        }
    };
}
#[component]
#[rustfmt::skip]
fn confetti(cx: &mut UiContext, w: f32, h: f32, elapsed: f32) {
    if !(0. ..2.5).contains(&elapsed) {
        return;
    }
    for i in 0..90_u32 {
        let hash = i.wrapping_mul(2654435761);
        let vx = ((hash & 255) as f32 / 255. - 0.5) * w * 1.3;
        let vy = -100. - ((hash >> 8) & 255) as f32;
        let x = w / 2. + vx * elapsed;
        let y = h * 0.4 + vy * elapsed + 160. * elapsed * elapsed;
        // Keep particle geometry and identities stable; moving absolute
        // coordinates would invalidate layout for the entire board each tick.
        column(cx).key(i as u64).absolute(0., 0.).translate(x, y).width(5.).height(8.).radius(1.5)
            .background(Color::Literal(TINTS[1 + i as usize % 8]))
            .opacity(if x >= 0. && x <= w && y >= 0. && y <= h { (2.5 - elapsed).min(1.) } else { 0. }) + |_cx|
        {};
    }
}

#[component]
#[rustfmt::skip]
fn explosion(cx: &mut UiContext, x: f32, y: f32, pitch: f32, age: f32) {
    if !(0. ..0.9).contains(&age) {
        return;
    }
    let k = ease(age / 0.9);
    for i in 0..2_u64 {
        let radius = pitch * (0.4 + k * (1.5 + i as f32 * 0.7));
        column(cx).key(i).absolute(x - pitch, y - pitch).width(pitch * 2.).height(pitch * 2.).radius(pitch)
            .scale(radius / pitch, radius / pitch).border(2., Color::Literal(0xe07f6e)).opacity((1. - k) * 0.8)
        + |_cx|
        {};
    }
}
