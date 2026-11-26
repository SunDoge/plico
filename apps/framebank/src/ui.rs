use crate::{
    Design,
    icons::*,
    media::{Asset, Kind},
};
use plico::{SvgAsset, Theme, prelude::*};
use plico_dev::App;
use std::{collections::VecDeque, path::PathBuf};

#[path = "stillroom.rs"]
mod stillroom;

const BG: Color = Color::Literal(0x171613);
const RAIL: Color = Color::Literal(0x1d1c18);
const PANEL: Color = Color::Literal(0x21201c);
const EDGE: Color = Color::Literal(0x35332c);
const INK: Color = Color::Literal(0xf6efe2);
const SUB: Color = Color::Literal(0xb3aa99);
const GOLD: Color = Color::Literal(0xf2c078);
const ACTIVE: Color = Color::Literal(0x3a3022);
const VIEWPORT: Color = Color::Literal(0x100f0e);
const RADIUS: f32 = 10.;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    Photo,
    Video,
    Favorites,
    Field,
    Form,
}

impl Filter {
    fn includes(self, asset: &Asset) -> bool {
        match self {
            Self::All => true,
            Self::Photo => asset.kind == Kind::Photo,
            Self::Video => asset.kind == Kind::Video,
            Self::Favorites => asset.favorite,
            Self::Field => asset.group == "Field notes",
            Self::Form => asset.group == "Form studies",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => "All assets",
            Self::Photo => "Images",
            Self::Video => "Videos",
            Self::Favorites => "Favorites",
            Self::Field => "Field notes",
            Self::Form => "Form studies",
        }
    }
}

#[derive(Default, Clone, PartialEq)]
pub struct Playback {
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub ended: bool,
    pub muted: bool,
    pub error: Option<String>,
}

pub enum Effect {
    Import,
    Save,
    Open(PathBuf),
    Copy(String),
    LoadVideo(u64),
    Pause(bool),
    Seek(f64),
    Mute(bool),
}

pub struct Library {
    pub design: Design,
    stillroom: stillroom::PreviewState,
    pub assets: Vec<Asset>,
    pub effects: VecDeque<Effect>,
    pub notice: String,
    pub playback: Option<Playback>,
    pub importing: bool,
    pub selected: Option<u64>,
    pub video_inline: bool,
    query: String,
    search_focus: bool,
    filter: Filter,
    related: bool,
    zoom: f32,
    pan: f32,
    next_id: u64,
}

impl Library {
    pub fn new(assets: Vec<Asset>, video_inline: bool, design: Design) -> Self {
        let mut result = Self {
            design,
            stillroom: stillroom::PreviewState::default(),
            assets: Vec::new(),
            effects: VecDeque::new(),
            notice: String::new(),
            playback: None,
            importing: false,
            selected: None,
            video_inline,
            query: String::new(),
            search_focus: false,
            filter: Filter::All,
            related: false,
            zoom: 1.,
            pan: 0.5,
            next_id: 0,
        };
        for asset in assets {
            result.insert(asset);
        }
        result
    }

    pub fn insert(&mut self, mut asset: Asset) -> bool {
        if asset.path.is_some()
            && self
                .assets
                .iter()
                .any(|existing| existing.path == asset.path)
        {
            return false;
        }
        self.next_id += 1;
        asset.id = self.next_id;
        self.assets.push(asset);
        true
    }

    fn filtered(&self) -> Vec<&Asset> {
        let query = self.query.to_lowercase();
        self.assets
            .iter()
            .filter(|asset| self.filter.includes(asset) && asset.search(&query))
            .collect()
    }

    pub fn select(&mut self, id: u64) {
        let Some(asset) = self.assets.iter().find(|asset| asset.id == id) else {
            return;
        };
        if self.selected == Some(id) {
            return;
        }
        self.effects.push_back(Effect::Pause(true));
        self.selected = Some(id);
        self.stillroom.reset();
        self.playback = None;
        self.zoom = 1.;
        self.pan = 0.5;
        if asset.kind == Kind::Video && self.video_inline {
            self.effects.push_back(Effect::LoadVideo(id));
        }
    }

    fn back(&mut self) {
        self.effects.push_back(Effect::Pause(true));
        self.selected = None;
        self.stillroom.leave();
        self.playback = None;
        if self.design == Design::Framebank {
            self.related = false;
        }
    }

    fn move_selection(&mut self, advance: bool) {
        let results = self.filtered();
        let Some(index) = results
            .iter()
            .position(|asset| Some(asset.id) == self.selected)
        else {
            return;
        };
        let target = if advance {
            index.checked_add(1)
        } else {
            index.checked_sub(1)
        };
        if let Some(id) = target
            .and_then(|index| results.get(index))
            .map(|asset| asset.id)
        {
            self.select(id);
        }
    }

    fn favorite(&mut self, id: u64) {
        if let Some(asset) = self.assets.iter_mut().find(|asset| asset.id == id) {
            asset.favorite = !asset.favorite;
            self.effects.push_back(Effect::Save);
        }
    }

    fn zoom_by(&mut self, scale: f32) {
        self.zoom = (self.zoom * scale).clamp(1., 4.);
        if self.zoom == 1. {
            self.pan = 0.5;
        }
    }

    fn toggle_video(&mut self) {
        if let Some(playback) = &self.playback {
            if playback.ended {
                self.effects.push_back(Effect::Seek(0.));
            }
            self.effects
                .push_back(Effect::Pause(!playback.paused && !playback.ended));
        }
    }

    #[component]
    #[rustfmt::skip]
    fn header(&mut self, cx: &mut UiContext) {
        row(cx).height(66.).width_percent(100.).padding_x(22).gap(14).align(Align::Center).shrink(0.).background(RAIL)
        + |cx|
        {
            row(cx).width(173.).gap(9).align(Align::Center) + |cx| {
                column(cx).width(34.).height(34.).align(Align::Center).justify(Justify::Center).background(GOLD)
                    .radius(RADIUS) + |cx|
                {
                    svg(cx, &APERTURE).size(21., 21.).color(BG);
                };
                text(cx, "Framebank").font_size(20).weight(750).color(INK);
            };
            row(cx).width(0.).grow(1.).height(38.).align(Align::Center).gap(8).padding_x(10).background(BG)
                .border(1., EDGE).radius(RADIUS) + |cx|
            {
                svg(cx, &SEARCH).size(17., 17.).color(SUB);
                let search = text_input_string(cx, "Search assets", &mut self.query)
                    .placeholder("Search by title, collection or format")
                    .request_focus(std::mem::take(&mut self.search_focus)).background(BG).border(0., EDGE)
                    .height(35.).width(0.).grow(1.).response();
                if search.changed() && self.selected.is_some() {
                    self.back();
                }
                if !self.query.is_empty()
                    && icon_button(cx, "Clear search", &X, false).clicked()
                {
                    self.query.clear();
                }
            };
            if button(cx).text("Import files").icon(&PLUS).primary().height(38.).radius(RADIUS)
                .disabled(self.importing).clicked()
            {
                self.effects.push_back(Effect::Import);
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn sidebar(&mut self, cx: &mut UiContext) {
        column(cx).width(196.).height_percent(100.).shrink(0.).background(RAIL).padding_x(15).padding_top(23)
            .padding_bottom(17).gap(7) + |cx|
        {
            heading(cx, "LIBRARY");
            for (filter, icon) in [
                (Filter::All, &LAYOUT_GRID),
                (Filter::Photo, &IMAGE),
                (Filter::Video, &FILM),
                (Filter::Favorites, &STAR),
            ] {
                self.nav(cx, filter, icon);
            }
            column(cx).height(1.).width_percent(100.).background(EDGE).margin_top(12).margin_bottom(12) + |_| {};
            heading(cx, "COLLECTIONS");
            for filter in [Filter::Field, Filter::Form] {
                self.nav(cx, filter, &FOLDER);
            }
            column(cx).grow(1.).min_height(8.) + |_| {};
            row(cx).align(Align::Center).gap(7).padding_x(6) + |cx| {
                column(cx).width(7.).height(7.).radius(3.5).background(GOLD) + |_| {};
                text(cx, "Local library").font_size(12).color(SUB);
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn nav(&mut self, cx: &mut UiContext, filter: Filter, icon: &SvgAsset) {
        let active = self.filter == filter;
        let control = button_base(cx, filter.label()).key(filter.label()).selected(active).width_percent(100.)
            .height(38.).padding_x(9.).radius(8.).align(Align::Center).gap(9.)
            .background(if active { ACTIVE } else { RAIL }).hover_background(PANEL);
        let response = control + |cx| {
            svg(cx, icon).size(16., 16.).color(if active { GOLD } else { SUB });
            text(cx, filter.label()).font_size(13).color(if active { GOLD } else { INK });
        };
        if response.clicked() {
            self.filter = filter;
        }
    }

    #[component]
    #[rustfmt::skip]
    fn gallery(&mut self, cx: &mut UiContext, width: f32) {
        let columns = if width >= 1080. {
            4
        } else if width >= 730. { 3 } else { 2 };
        let card_width = ((width - 56. - (columns as f32 - 1.) * 15.) / columns as f32).floor();
        let mut clicked = None;
        let mut starred = None;
        column(cx).width(0.).grow(1.).height_percent(100.).scroll_y(true).padding_x(26).padding_top(28)
            .padding_bottom(24).gap(20) + |cx|
        {
            let results = self.filtered();
            row(cx).align(Align::End).justify(Justify::SpaceBetween).width_percent(100.) + |cx| {
                column(cx).gap(7) + |cx| {
                    text(cx, if self.query.is_empty() { self.filter.label() } else { "Search results" }).weight(750).font_size(33).color(INK);
                    text!(
                        cx,
                        "{} {}",
                        results.len(),
                        if results.len() == 1 {
                            "asset"
                        } else {
                            "assets"
                        }
                    ).font_size(13).color(SUB);
                };
            };
            if results.is_empty() {
                column(cx).width_percent(100.).height(225.).border(1., EDGE).radius(RADIUS).background(PANEL)
                    .align(Align::Center).justify(Justify::Center).gap(10) + |cx|
                {
                    svg(cx, &SEARCH).size(26., 26.).color(SUB);
                    text(
                        cx,
                        if self.filter == Filter::Favorites && self.query.is_empty() {
                            "No favorites yet"
                        } else {
                            "No matching assets"
                        },
                    ).font_size(19).weight(650).color(INK);
                    text(
                        cx,
                        if self.filter == Filter::Favorites && self.query.is_empty() {
                            "Star an asset to keep it here."
                        } else {
                            "Try another search or choose a different collection."
                        },
                    ).font_size(13).color(SUB);
                };
            }
            for group in results.chunks(columns) {
                row(cx).key(group[0].id).gap(15).shrink(0.) + |cx| {
                    for &asset in group {
                        let (open, star) = asset_card(cx, asset, card_width);
                        if open {
                            clicked = Some(asset.id);
                        }
                        if star {
                            starred = Some(asset.id);
                        }
                    }
                };
            }
        };
        if let Some(id) = starred {
            self.favorite(id);
        }
        if let Some(id) = clicked {
            self.select(id);
        }
    }

    #[component]
    #[rustfmt::skip]
    fn detail(&mut self, cx: &mut UiContext, asset: &Asset, width: f32, height: f32) {
        let sidebar = if width >= 1080. { 294. } else { 248. };
        let stage_width = (width - sidebar - 58.).max(260.);
        let footer = if self.importing || !self.notice.is_empty() { 28. } else { 0. };
        let stage_height =
            (height - 239. - footer - if asset.kind == Kind::Video { 44. } else { 0. }).max(205.);
        column(cx).width_percent(100.).height(0.).grow(1.).padding(18).gap(13) + |cx| {
            row(cx).height(40.).width_percent(100.).shrink(0.).gap(10).align(Align::Center) + |cx| {
                if button(cx).text("Library").icon(&ARROW_LEFT).height(36.).radius(8.).clicked() {
                    self.back();
                }
                column(cx).width(1.).height(22.).margin(3).background(EDGE) + |_| {};
                text(cx, asset.title.as_str()).font_size(19).weight(700).color(INK).ellipsis(true);
                column(cx).grow(1.) + |_| {};
                if asset.kind == Kind::Photo {
                    if self.zoom > 1. {
                        if icon_button(cx, "Pan left", &CHEVRON_LEFT, self.pan <= 0.).clicked() {
                            self.pan = (self.pan - 0.2).max(0.);
                        }
                        if icon_button(cx, "Pan right", &CHEVRON_RIGHT, self.pan >= 1.).clicked() {
                            self.pan = (self.pan + 0.2).min(1.);
                        }
                    }
                    if icon_button(cx, "Zoom out", &ZOOM_OUT, self.zoom <= 1.).clicked() {
                        self.zoom_by(0.8);
                    }
                    if button(cx).aria_label("Reset zoom to fit").icon(&MAXIMIZE)
                        .text(format!("{:.0}%", self.zoom * 100.)).width(86.).height(35.).radius(8.).clicked()
                    {
                        self.zoom = 1.;
                        self.pan = 0.5;
                    }
                    if icon_button(cx, "Zoom in", &ZOOM_IN, self.zoom >= 4.).clicked() {
                        self.zoom_by(1.25);
                    }
                }
                if icon_button(cx, "Open original file", &EXTERNAL_LINK,
                    asset.path.is_none() && asset.kind == Kind::Photo).clicked()
                {
                    self.effects.push_back(Effect::Open(asset.path.clone().unwrap_or_default()));
                }
                if star_button(cx, asset.favorite).clicked() {
                    self.favorite(asset.id);
                }
            };
            row(cx).width_percent(100.).height(0.).grow(1.).gap(14).align(Align::Stretch) + |cx| {
                column(cx).width(0.).min_width(0.).height_percent(100.).grow(1.).gap(8).clip(true) + |cx| {
                    self.stage(cx, asset, stage_width, stage_height);
                    if asset.kind == Kind::Video {
                        self.video_controls(cx, asset);
                    }
                    self.filmstrip(cx, asset.id, stage_width);
                };
                self.inspector(cx, asset, sidebar);
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stage(&self, cx: &mut UiContext, asset: &Asset, width: f32, height: f32) {
        let (w, h) = asset.dimensions;
        let scale = (width / w as f32).min(height / h as f32) * self.zoom;
        let size = (w as f32 * scale, h as f32 * scale);
        column(cx).width_percent(100.).height(0.).grow(1.).min_height(150.).background(VIEWPORT).radius(RADIUS)
            .border(1., EDGE).clip(true).scroll_y(self.zoom > 1.) + |cx|
        {
            column(cx).width(size.0.max(width)).height(size.1.max(height)).shrink(0.).align(Align::Center)
                .translate(-(size.0 - width).max(0.) * self.pan, 0.).justify(Justify::Center) + |cx|
            {
                if asset.kind == Kind::Video
                    && self.video_inline
                    && self.playback.as_ref().is_some_and(|p| p.error.is_none())
                {
                    column(cx).key("framebank-video").width(size.0).height(size.1) + |_| {};
                } else {
                    image(cx, &asset.poster).size(size.0, size.1);
                }
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn video_controls(&mut self, cx: &mut UiContext, asset: &Asset) {
        row(cx).height(36.).shrink(0.).align(Align::Center).gap(8).padding_x(6) + |cx| {
            if let Some(playback) = self.playback.clone() {
                if let Some(error) = &playback.error {
                    text(cx, error.as_str()).font_size(12).color(SUB).ellipsis(true);
                } else {
                    if icon_button(cx, if playback.paused || playback.ended { "Play" } else { "Pause" },
                        if playback.paused || playback.ended { &PLAY } else { &PAUSE }, false).clicked()
                    {
                        self.toggle_video();
                    }
                    text(cx, time_label(playback.position)).font_size(12).color(SUB);
                    let duration = playback.duration.max(asset.duration).max(0.1);
                    let mut position = playback.position.clamp(0., duration);
                    if slider(cx, "Playback position", &mut position, 0. ..=duration).step(0.).width(0.).grow(1.)
                        .changed()
                    {
                        self.effects.push_back(Effect::Seek(position));
                    }
                    text(cx, time_label(duration)).font_size(12).color(SUB);
                    if icon_button(cx, if playback.muted { "Unmute" } else { "Mute" },
                        if playback.muted { &VOLUME_X } else { &VOLUME_2 }, false).clicked()
                    {
                        self.effects.push_back(Effect::Mute(!playback.muted));
                    }
                }
            } else {
                if button(cx).text("Play in system player").icon(&PLAY).height(32.).radius(8.).clicked() {
                    self.effects.push_back(Effect::Open(asset.path.clone().unwrap_or_default()));
                }
                text(cx, time_label(asset.duration)).font_size(12).color(SUB);
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn filmstrip(&mut self, cx: &mut UiContext, active: u64, width: f32) {
        let mut selected = None;
        row(cx).height(68.).width_percent(100.).shrink(0.).padding_x(8).gap(9).align(Align::Center).radius(RADIUS)
            .background(PANEL).border(1., EDGE) + |cx|
        {
            if icon_button(cx, "Previous asset", &CHEVRON_LEFT, false).clicked() {
                self.move_selection(false);
            }
            row(cx).width(0.).grow(1.).height(56.).clip(true).align(Align::Center).gap(6) + |cx| {
                let assets = self.filtered();
                let limit = ((width - 106.) / 64.).floor().max(1.) as usize;
                let position = assets.iter().position(|asset| asset.id == active).unwrap_or_default();
                let start = position.saturating_sub(limit / 2).min(assets.len().saturating_sub(limit));
                for asset in assets.into_iter().skip(start).take(limit) {
                    let chip = button_base(cx, asset.title.as_str()).key(asset.id).selected(active == asset.id)
                        .width(58.).height(42.).shrink(0.).radius(7.).clip(true).align(Align::Center)
                        .justify(Justify::Center).border(if active == asset.id { 2. } else { 1. },
                            if active == asset.id { GOLD } else { EDGE });
                    if (chip + |cx| {
                        image(cx, &asset.thumbnail).size(52., 33.);
                    }).clicked()
                    {
                        selected = Some(asset.id);
                    }
                }
            };
            if icon_button(cx, "Next asset", &CHEVRON_RIGHT, false).clicked() {
                self.move_selection(true);
            }
        };
        if let Some(id) = selected {
            self.select(id);
        }
    }

    #[component]
    #[rustfmt::skip]
    fn inspector(&mut self, cx: &mut UiContext, asset: &Asset, width: f32) {
        column(cx).width(width).height_percent(100.).shrink(0.).radius(RADIUS).background(PANEL).border(1., EDGE)
            .clip(true) + |cx|
        {
            row(cx).height(48.).shrink(0.).padding(7).gap(5).width_percent(100.) + |cx| {
                for (related, label, icon) in
                    [(false, "Details", &INFO), (true, "Related", &SPARKLES)]
                {
                    let tab = button(cx).key(label).text(label).icon(icon).width(0.).grow(1.).radius(8.)
                        .background(if self.related == related { ACTIVE } else { PANEL }).border(0., EDGE);
                    if tab.clicked() {
                        self.related = related;
                    }
                }
            };
            column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).padding(14).gap(13) + |cx| {
                if self.related {
                    self.related_assets(cx, asset, width);
                } else {
                    row(cx).align(Align::Center).gap(7) + |cx| {
                        svg(cx, if asset.kind == Kind::Photo { &IMAGE } else { &FILM }).size(15., 15.).color(GOLD);
                        text(cx, if asset.kind == Kind::Photo { "Image" } else { "Video" }).font_size(12).color(GOLD);
                    };
                    text(cx, asset.title.as_str()).font_size(22).weight(740).color(INK);
                    line(cx);
                    heading(cx, "FILE DETAILS");
                    detail_row(cx, "Collection", asset.group);
                    detail_row(cx, "Format", asset.format.as_str());
                    detail_row(cx, "Dimensions", &format!("{} × {}", asset.dimensions.0, asset.dimensions.1));
                    detail_row(cx, "File size", &byte_label(asset.bytes));
                    if asset.kind == Kind::Video {
                        detail_row(cx, "Duration", &time_label(asset.duration));
                    }
                    line(cx);
                    heading(cx, "COLOR PALETTE");
                    row(cx).width_percent(100.).gap(5) + |cx| {
                        for &[r, g, b] in &asset.palette {
                            let value = u32::from_be_bytes([0, r, g, b]);
                            let hex = format!("#{value:06X}");
                            let swatch = button_base(cx, format!("Copy {hex}")).key(value as u64).width(0.)
                                .grow(1.).height(35.).radius(6.).background(Color::Literal(value))
                                .border(1., EDGE);
                            if (swatch + |_| {}).clicked() {
                                self.effects.push_back(Effect::Copy(hex.clone()));
                                self.notice = format!("Copied {hex}");
                            }
                        }
                    };
                    row(cx).width_percent(100.).gap(5) + |cx| {
                        for &[r, g, b] in &asset.palette {
                            let value = u32::from_be_bytes([0, r, g, b]);
                            column(cx).key(value as u64).width(0.).grow(1.) + |cx| {
                                text!(cx, "{value:06X}").font_size(9).color(SUB).ellipsis(true);
                            };
                        }
                    };
                    line(cx);
                    heading(cx, "SOURCE");
                    text(cx, asset.source()).font_size(12).color(SUB).ellipsis(true).width_percent(100.);
                    if asset.path.is_some()
                        && button(cx).text("Copy path").icon(&COPY).ghost().height(32.).clicked()
                    {
                        self.effects.push_back(Effect::Copy(asset.source().to_owned()));
                        self.notice = "Copied file path".into();
                    }
                }
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn related_assets(&mut self, cx: &mut UiContext, active: &Asset, width: f32) {
        let mut similar: Vec<_> = self.assets.iter().filter(|asset| asset.id != active.id)
            .map(|asset| (active.similar(asset), asset)).collect();
        similar.sort_unstable_by_key(|(distance, asset)| (*distance, asset.id));
        heading(cx, "SIMILAR COLORS");
        let mut selected = None;
        for (_, asset) in similar.into_iter().take(4) {
            let link = button_base(cx, asset.title.as_str()).key(asset.id).width_percent(100.).height(68.)
                .align(Align::Center).gap(8.).padding_x(6.).radius(8.).background(BG).hover_background(ACTIVE);
            let response = link + |cx| {
                image(cx, &asset.thumbnail).size(72., 45.);
                text(cx, asset.title.as_str()).width(width - 126.).ellipsis(true).font_size(12).color(INK);
            };
            if response.clicked() {
                selected = Some(asset.id);
            }
        }
        if let Some(id) = selected {
            if !self.filtered().iter().any(|asset| asset.id == id) {
                self.filter = Filter::All;
                self.query.clear();
            }
            self.select(id);
        }
    }

    #[component]
    #[rustfmt::skip]
    fn draw(&mut self, cx: &mut UiContext) {
        let [width, height] = cx.viewport_size();
        column(cx).width_percent(100.).height_percent(100.).background(BG).clip(true) + |cx| {
            self.header(cx);
            line(cx);
            if let Some(asset) = self.selected.and_then(|id| self.assets.iter().find(|asset| id == asset.id))
                .cloned()
            {
                self.detail(cx, &asset, width, height);
            } else {
                row(cx).width_percent(100.).height(0.).grow(1.) + |cx| {
                    self.sidebar(cx);
                    self.gallery(cx, width - 196.);
                };
            }
            if self.importing || !self.notice.is_empty() {
                row(cx).height(28.).shrink(0.).width_percent(100.).padding_x(16).align(Align::Center)
                    .background(RAIL) + |cx|
                {
                    let message = if self.importing { "Reading media…" } else { self.notice.as_str() };
                    text(cx, message).font_size(12).color(SUB).ellipsis(true);
                };
            }
        };
    }
}

impl App for Library {
    fn show(&mut self, cx: &mut UiContext) {
        match self.design {
            Design::Framebank => self.draw(cx),
            Design::Stillroom => self.draw_stillroom(cx),
        }
    }

    fn theme(&self) -> Theme {
        if self.design == Design::Stillroom {
            return stillroom::theme();
        }
        Theme {
            surface: 0x171613,
            panel: 0x21201c,
            control: 0x292720,
            border: 0x35332c,
            foreground: 0xf6efe2,
            muted: 0xb3aa99,
            accent: 0xf2c078,
            on_accent: 0x171613,
            accent_hover: 0xffd396,
            accent_pressed: 0xe5af69,
            focus: 0xffd396,
            control_hover: 0x40372a,
            control_pressed: 0x4b3e2b,
            selection: 0x54402d,
            ..Theme::dark()
        }
    }

    fn on_key_event(&mut self, key: &plico_desktop::KeyEvent) -> bool {
        use plico::Shortcut;
        let plain = |name| Shortcut::new(name, Modifiers::empty()).matches(key);
        if Shortcut::primary("o").matches(key) {
            self.effects.push_back(Effect::Import);
        } else if Shortcut::primary("f").matches(key) {
            self.search_focus = true;
        } else if let Some(id) = self.selected {
            if plain("Escape") {
                self.back();
            } else if plain("ArrowLeft") {
                self.move_selection(false);
            } else if plain("ArrowRight") {
                self.move_selection(true);
            } else if plain("z") && self.design == Design::Stillroom {
                self.stillroom.toggle([0.5, 0.5]);
            } else if plain("f") {
                self.favorite(id);
            } else if plain("Space") || plain(" ") {
                self.toggle_video();
            } else {
                return false;
            }
        } else {
            return false;
        }
        true
    }
}

#[component]
#[rustfmt::skip]
fn heading(cx: &mut UiContext, label: &str) {
    text(cx, label).font_size(11).weight(740).color(SUB);
}

#[component]
#[rustfmt::skip]
fn line(cx: &mut UiContext) {
    column(cx).width_percent(100.).height(1.).shrink(0.).background(EDGE) + |_| {};
}

#[component]
#[rustfmt::skip]
fn detail_row(cx: &mut UiContext, label: &str, value: &str) {
    row(cx).width_percent(100.).gap(8).justify(Justify::SpaceBetween) + |cx| {
        text(cx, label).font_size(12).color(SUB);
        text(cx, value).font_size(12).color(INK).ellipsis(true);
    };
}

#[component]
#[rustfmt::skip]
fn asset_card(cx: &mut UiContext, asset: &Asset, width: f32) -> (bool, bool) {
    let mut result = (false, false);
    column(cx).key(asset.id).width(width).shrink(0.).radius(RADIUS).clip(true).border(1., EDGE).background(PANEL)
    + |cx|
    {
        let height = ((width - 2.) * 0.625).round();
        let thumb = button_base(cx, asset.title.as_str()).width_percent(100.).height(height)
            .justify(Justify::Center).align(Align::Center).background(VIEWPORT);
        let open = thumb + |cx| {
            image(cx, &asset.thumbnail).size(width - 2., height);
        };
        result.0 = open.clicked();
        row(cx).height(59.).width_percent(100.).padding_x(10).align(Align::Center).gap(7) + |cx| {
            column(cx).width(0.).grow(1.).gap(4) + |cx| {
                text(cx, asset.title.as_str()).font_size(14).weight(670).color(INK).ellipsis(true);
                row(cx).align(Align::Center).gap(5) + |cx| {
                    svg(cx, if asset.kind == Kind::Photo { &IMAGE } else { &FILM }).size(12., 12.).color(SUB);
                    text(cx, asset.format.as_str()).font_size(10).color(SUB);
                    text(cx, "·").font_size(10).color(SUB);
                    text(
                        cx,
                        if asset.kind == Kind::Video {
                            time_label(asset.duration)
                        } else {
                            format!(
                                "{} × {}",
                                asset.dimensions.0, asset.dimensions.1
                            )
                        },
                    ).font_size(10).color(SUB).ellipsis(true);
                };
            };
            result.1 = star_button(cx, asset.favorite).clicked();
        };
    };
    result
}

#[component]
#[rustfmt::skip]
fn icon_button(
    cx: &mut UiContext,
    label: &str,
    icon: &SvgAsset,
    disabled: bool,
) -> plico::Response {
    button(cx).aria_label(label).icon(icon).ghost().width(35.).height(35.).radius(8.).disabled(disabled).response()
}

#[component]
#[rustfmt::skip]
fn star_button(cx: &mut UiContext, selected: bool) -> plico::Response {
    let label = if selected { "Remove from favorites" } else { "Add to favorites" };
    let star = button_base(cx, label).selected(selected).width(35.).height(35.).radius(8.).align(Align::Center)
        .justify(Justify::Center).hover_background(ACTIVE);
    (star + |cx| {
        svg(cx, &STAR).size(17., 17.).color(if selected { GOLD } else { SUB });
    }).response
}

fn time_label(seconds: f64) -> String {
    let seconds = seconds.max(0.) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn byte_label(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.)
    } else {
        format!("{bytes} B")
    }
}
