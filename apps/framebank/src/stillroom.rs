use super::*;
use plico::{
    ContentTransition, Easing, GlassStyle, LiquidGlass, MasonryLayout, PinnedRect, SpringOptions,
};
use std::time::Duration;

const BASE: Color = Color::Literal(0x141820);
const SURFACE: Color = Color::Literal(0x202732);
const LINE: Color = Color::Literal(0x394453);
const TEXT: Color = Color::Literal(0xf2f6fc);
const MUTED: Color = Color::Literal(0xb1bece);
const ACCENT: Color = Color::Literal(0x9fe1d7);
const HEADER: f32 = 62.;
const GUTTER: f32 = 18.;
const RADIUS: f32 = 15.;
// MyGO gallery's Magic Move spring. See Stillroom's README for the source.
const TILE_FLIGHT: SpringOptions = SpringOptions {
    duration: Duration::from_millis(450),
    bounce: 0.12,
    ..SpringOptions::SMOOTH
};
const RETURN_FLIGHT: SpringOptions = SpringOptions {
    duration: Duration::from_millis(330),
    bounce: 0.,
    ..TILE_FLIGHT
};
const LANDING_FADE_DISTANCE: f32 = 8.;
const LANDING_REST_DISTANCE: f32 = 0.25;
const PANEL_SPRING: SpringOptions = SpringOptions {
    duration: Duration::from_millis(285),
    ..SpringOptions::SMOOTH
};

pub(super) fn theme() -> Theme {
    Theme {
        surface: 0x141820,
        panel: 0x202732,
        foreground: 0xf2f6fc,
        muted: 0xb1bece,
        control: 0x2b3544,
        control_hover: 0x354454,
        control_pressed: 0x425465,
        border: 0x394453,
        selection: 0x334d54,
        accent: 0x9fe1d7,
        accent_hover: 0xb6f4e9,
        accent_pressed: 0x86cfc5,
        on_accent: 0x121923,
        focus: 0xb6f4e9,
        ..Theme::dark()
    }
}

fn glass(intensity: f32) -> LiquidGlass {
    LiquidGlass {
        style: GlassStyle::Custom,
        frost: 0.44,
        refraction: 0.6,
        bevel: 16.,
        rim: 0.48,
        tint: Color::Literal(0x17202c),
        tint_opacity: intensity,
        saturation: 1.08,
        fallback: SURFACE,
        ..LiquidGlass::default()
    }
}

#[derive(Clone, Copy)]
struct Chrome {
    reveal: f32,
    live_glass: bool,
}

impl Chrome {
    fn surface<'u, 'e>(self, layout: plico::Layout<'u, 'e>, tint: f32) -> plico::Layout<'u, 'e> {
        if self.live_glass {
            layout.material(glass(tint))
        } else {
            // Fit-mode chrome sits over the flat canvas. Match its settled glass
            // fill without capturing and convolving the same solid color.
            let fill = if tint >= 0.8 { 0x252d37 } else { 0x2a313b };
            layout
                .background(Color::Literal(fill))
                .border(1., Color::Literal(0x444a54))
        }
    }
}

#[derive(Default)]
pub(super) struct PreviewState {
    immersive: bool,
    anchor: [f32; 2],
    zoom_phase: f32,
    flight_phase: f32,
    selection: ContentTransition<u64>,
    filter: ContentTransition<Filter>,
    painted_photo: Option<(u64, Bounds)>,
    outgoing_photo: Option<(u64, Bounds)>,
    matched_tile: Option<(u64, Bounds)>,
    return_photo: Option<(u64, Bounds)>,
    return_phase: f32,
    painted_native_video: bool,
    return_native_video: bool,
    grid_scroll: f32,
    restore_grid_scroll: bool,
}

impl PreviewState {
    pub(super) fn reset(&mut self) {
        self.immersive = false;
        self.anchor = [0.5, 0.5];
        self.zoom_phase = 0.;
        self.outgoing_photo = self.painted_photo.take();
        self.painted_native_video = false;
    }

    pub(super) fn toggle(&mut self, anchor: [f32; 2]) {
        if !self.immersive && self.zoom_phase == 0. {
            self.anchor = anchor;
        }
        self.immersive = !self.immersive;
    }

    fn return_progress(&self) -> f32 {
        1. - (self.flight_phase / self.return_phase.max(0.001)).clamp(0., 1.)
    }

    pub(super) fn leave(&mut self) {
        self.return_photo = self.painted_photo.take();
        self.return_native_video = std::mem::take(&mut self.painted_native_video);
        self.return_phase = self.flight_phase.max(0.001);
        self.outgoing_photo = None;
        self.selection = ContentTransition::default();
        self.restore_grid_scroll = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Bounds {
    fn rect(self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }

    fn center(self) -> [f32; 2] {
        [self.x + self.w / 2., self.y + self.h / 2.]
    }

    fn interpolate(self, other: Self, t: f32) -> Self {
        let mix = |a, b| a + (b - a) * t;
        Self {
            x: mix(self.x, other.x),
            y: mix(self.y, other.y),
            w: mix(self.w, other.w),
            h: mix(self.h, other.h),
        }
    }

    fn cover(self, (width, height): (u32, u32)) -> Self {
        let scale = (self.w / width as f32).max(self.h / height as f32);
        let [x, y] = self.center();
        let w = width as f32 * scale;
        let h = height as f32 * scale;
        Self {
            x: x - w / 2.,
            y: y - h / 2.,
            w,
            h,
        }
    }

    fn photo_landing(self, tile: Self, poster: (u32, u32), progress: f32) -> (Self, Self) {
        (
            self.interpolate(tile, progress),
            self.interpolate(tile.cover(poster), progress),
        )
    }

    fn max_edge_delta(self, other: Self) -> f32 {
        [
            self.x - other.x,
            self.y - other.y,
            self.x + self.w - (other.x + other.w),
            self.y + self.h - (other.y + other.h),
        ]
        .into_iter()
        .map(f32::abs)
        .fold(0., f32::max)
    }

    fn remaining_distance(self, tile: Self, dimensions: (u32, u32), progress: f32) -> f32 {
        self.max_edge_delta(tile)
            .max(self.max_edge_delta(tile.cover(dimensions)))
            * (1. - progress)
    }

    fn photo_view(
        body: [f32; 2],
        gallery: f32,
        image: (u32, u32),
        anchor: [f32; 2],
        t: f32,
    ) -> Self {
        let [width, height] = body;
        let (w, h) = (image.0 as f32, image.1 as f32);
        let inset = 28.;
        let top = 84.;
        let fit_height = (height - top - 86.).max(140.);
        let fit_scale = ((gallery - 2. * inset) / w).min(fit_height / h);
        let fit = Self {
            x: (gallery - w * fit_scale) / 2.,
            y: top + (fit_height - h * fit_scale) / 2.,
            w: w * fit_scale,
            h: h * fit_scale,
        };
        let zoom = (width / w).max(height / h) * 1.22;
        let expanded = Self {
            x: (fit.x + (fit.w - w * zoom) * anchor[0]).clamp(width - w * zoom, 0.),
            y: (fit.y + (fit.h - h * zoom) * anchor[1]).clamp(height - h * zoom, 0.),
            w: w * zoom,
            h: h * zoom,
        };
        fit.interpolate(expanded, t)
    }
}

impl Library {
    #[component]
    #[rustfmt::skip]
    pub(super) fn draw_stillroom(&mut self, cx: &mut UiContext) {
        let [width, height] = cx.viewport_size();
        let footer = if self.importing || !self.notice.is_empty() { 27. } else { 0. };
        let open = self.selected.is_some();
        let spring = if open { TILE_FLIGHT } else { RETURN_FLIGHT };
        let flight = cx.spring("library-to-preview", f32::from(open), spring);
        let screen = if open || self.stillroom.return_photo.is_some() { flight.clamp(0., 1.) } else { 0. };
        let chrome = ((screen - 0.48) / 0.52).clamp(0., 1.);
        let wall_opacity = 1. - (screen / 0.76).clamp(0., 1.);
        self.stillroom.flight_phase = flight;
        if !open && flight == 0. {
            self.stillroom.return_photo = None;
            self.related = false;
        }
        let returning = self.stillroom.return_photo.is_some();
        column(cx).width_percent(100.).height_percent(100.).clip(true).background(BASE) + |cx| {
            self.stillroom_header(cx);
            column(cx).width_percent(100.).height(0.).grow(1.).clip(true).background(BASE) + |cx| {
                if !open || wall_opacity > 0. {
                    column(cx).absolute(0., 0.).width_percent(100.).height_percent(100.)
                        .disabled(open || returning) + |cx|
                    {
                        self.stillroom_grid(cx, width);
                    };
                    if wall_opacity < 1. {
                        column(cx).absolute(0., 0.).width_percent(100.).height_percent(100.).background(BASE)
                            .background_alpha(1. - wall_opacity) + |_|
                        {};
                    }
                }
                if let Some(asset) = self.selected
                    .and_then(|id| self.assets.iter().find(|asset| id == asset.id)).cloned()
                {
                    self.stillroom_preview(cx, &asset, width, (height - HEADER - footer).max(300.), chrome,
                        flight);
                } else if let Some((id, from)) = self.stillroom.return_photo
                    && let Some(asset) =
                        self.assets.iter().find(|asset| asset.id == id).cloned()
                {
                    let progress = self.stillroom.return_progress();
                    let remaining = 1. - progress;
                    let tile = self.stillroom.matched_tile.filter(|(tile_id, _)| *tile_id == id);
                    if asset.kind == Kind::Photo
                        && let Some((_, tile)) = tile
                    {
                        returning_photo(cx, &asset, from, tile, progress);
                    } else {
                        let bounds = tile.map_or(from, |(_, to)| {
                            from.interpolate(to.cover(asset.dimensions), progress)
                        });
                        let fade = if tile.is_some() { 1. } else { remaining };
                        column(cx).absolute(bounds.x, bounds.y).width(bounds.w).height(bounds.h).radius(RADIUS)
                            .corner_smoothing(0.).clip(true).opacity(fade) + |cx|
                        {
                            if self.stillroom.return_native_video {
                                column(cx).key("framebank-video").width(bounds.w).height(bounds.h) + |_| {};
                            } else {
                                image(cx, &asset.poster).size(bounds.w, bounds.h);
                            }
                        };
                    }
                    column(cx).absolute(0., 0.).width_percent(100.).height_percent(100.).disabled(true) + |cx| {
                        self.stillroom_chrome(
                            cx,
                            &asset,
                            width,
                            height - HEADER - footer,
                            Chrome {
                                reveal: chrome,
                                live_glass: false,
                            },
                        );
                    };
                }
            };
            if footer > 0. {
                row(cx).width_percent(100.).height(footer).shrink(0.).align(Align::Center).padding_x(GUTTER)
                    .background(SURFACE) + |cx|
                {
                    text(cx, if self.importing { "Reading media…" } else { self.notice.as_str() }).font_size(12).color(MUTED).ellipsis(true);
                };
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_header(&mut self, cx: &mut UiContext) {
        row(cx).width_percent(100.).height(HEADER).shrink(0.).align(Align::Center).gap(16).padding_x(GUTTER)
            .background(BASE).border(1., LINE) + |cx|
        {
            row(cx).gap(9).align(Align::Center).width(158.) + |cx| {
                svg(cx, &APERTURE).size(22., 22.).color(ACCENT);
                text(cx, "Stillroom").font_size(22).weight(750).color(TEXT);
            };
            row(cx).width(0.).grow(1.).height(36.).align(Align::Center).padding_x(9).gap(8).radius(10.)
                .background(SURFACE) + |cx|
            {
                svg(cx, &SEARCH).size(16., 16.).color(MUTED);
                if text_input_string(cx, "Search assets", &mut self.query)
                    .request_focus(std::mem::take(&mut self.search_focus)).placeholder("Search your library")
                    .width(0.).grow(1.).height(34.).background(SURFACE).border(0., LINE).changed()
                    && self.selected.is_some()
                {
                    self.back();
                }
                if !self.query.is_empty()
                    && icon_button(cx, "Clear search", &X, false).clicked()
                {
                    self.query.clear();
                }
            };
            if button(cx).text("Import").icon(&PLUS).primary().height(35.).radius(10.).disabled(self.importing)
                .clicked()
            {
                self.effects.push_back(Effect::Import);
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_grid(&mut self, cx: &mut UiContext, width: f32) {
        let columns = if width >= 1120. { 4 } else { 3 };
        let gap = 13.;
        let card_w =
            ((width - 2. * GUTTER - (columns as f32 - 1.) * gap - 10.) / columns as f32).floor();
        let mut open = None;
        let mut favorite = None;
        let transition = self.stillroom.filter.sample(cx, self.filter, Duration::from_millis(150));
        let reveal = Easing::EaseOut.sample(transition.progress) as f32;
        let returning = self.stillroom.return_photo;
        let return_progress = self.stillroom.return_progress();
        let hidden = self.selected.filter(|id| {
            self.assets.iter().any(|asset| asset.id == *id && asset.kind == Kind::Photo)
        });
        let visible_height = cx.viewport_size()[1] - HEADER;
        let mut destination = None;
        column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).padding(GUTTER).gap(15) + |cx| {
            let offset = if std::mem::take(&mut self.stillroom.restore_grid_scroll) {
                cx.scroll_to_y(self.stillroom.grid_scroll);
                self.stillroom.grid_scroll
            } else {
                cx.scroll_viewport().map_or(self.stillroom.grid_scroll, |port| port[0])
            };
            row(cx).width_percent(100.).gap(8).align(Align::Center) + |cx| {
                for (filter, icon) in [
                    (Filter::All, &LAYOUT_GRID),
                    (Filter::Photo, &IMAGE),
                    (Filter::Video, &FILM),
                    (Filter::Favorites, &STAR),
                ] {
                    if button(cx).key(filter.label()).text(filter.label()).icon(icon).height(34.).radius(10.)
                        .background(if self.filter == filter { SURFACE } else { BASE }).border(0., LINE)
                        .clicked()
                    {
                        self.filter = filter;
                    }
                }
                column(cx).grow(1.) + |_| {};
                text!(cx, "{} assets", self.filtered().len()).font_size(12).color(MUTED);
            };
            let results = self.filtered();
            let wall = masonry(results, columns, card_w, gap);
            if wall.iter().all(Vec::is_empty) {
                text(cx, "No matching assets").font_size(18).weight(650).color(MUTED);
            }
            row(cx).width_percent(100.).align(Align::Start).gap(gap).translate(0., 8. * (1. - reveal))
                .opacity(0.65 + 0.35 * reveal) + |cx|
            {
                for (index, stack) in wall.into_iter().enumerate() {
                    column(cx).key(index).width(card_w).shrink(0.).gap(gap) + |cx| {
                        let x = GUTTER + index as f32 * (card_w + gap);
                        let top = GUTTER + 34. + 15. - offset + 8. * (1. - reveal);
                        for (asset, card_h, card_top) in stack {
                            let y = top + card_top;
                            let bounds = Bounds {
                                x,
                                y,
                                w: card_w,
                                h: card_h,
                            };
                            let mut hide = hidden == Some(asset.id);
                            if let Some((id, from)) = returning
                                && id == asset.id
                                && y + card_h > 0.
                                && y < visible_height
                            {
                                destination = Some((id, bounds));
                                hide = asset.kind == Kind::Video
                                    || from.remaining_distance(bounds, asset.poster_dimensions,
                                        return_progress) >= LANDING_FADE_DISTANCE;
                            }
                            let response =
                                stillroom_tile(cx, asset, card_w, card_h, hide);
                            if response.0 {
                                open = Some((asset.id, bounds, offset));
                            }
                            if response.1 {
                                favorite = Some(asset.id);
                            }
                        }
                    };
                }
            };
        };
        if returning.is_some() {
            self.stillroom.matched_tile = destination;
        }
        if let Some(id) = favorite {
            self.favorite(id);
        }
        if let Some((id, bounds, offset)) = open {
            self.stillroom.matched_tile = Some((id, bounds));
            self.stillroom.grid_scroll = offset;
            self.stillroom.return_photo = None;
            self.select(id);
        }
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_preview(
        &mut self,
        cx: &mut UiContext,
        asset: &Asset,
        width: f32,
        height: f32,
        screen: f32,
        flight: f32,
    ) {
        let sidebar = if width < 1020. { 252. } else { 306. };
        let gallery = width - sidebar - GUTTER;
        let transition = self.stillroom.selection.sample(cx, asset.id, Duration::from_millis(200));
        let selection = Easing::EaseOut.sample(transition.progress) as f32;
        column(cx).absolute(0., 0.).width_percent(100.).height_percent(100.).clip(true) + |cx| {
            if selection < 1.
                && let Some((id, bounds)) = self.stillroom.outgoing_photo
                && let Some(outgoing) = self.assets.iter().find(|asset| asset.id == id)
            {
                column(cx).absolute(bounds.x, bounds.y).width(bounds.w).height(bounds.h).radius(RADIUS)
                    .corner_smoothing(0.).clip(true).opacity(1. - selection * selection) + |cx|
                {
                    image(cx, &outgoing.poster).size(bounds.w, bounds.h);
                };
            }
            if asset.kind == Kind::Photo {
                column(cx).key(asset.id).width_percent(100.).height_percent(100.).absolute(0., 0.) + |cx| {
                    self.stillroom_photo(cx, asset, [width, height], gallery, flight, selection);
                };
            } else {
                self.stillroom_video_surface(cx, asset, gallery, height, flight.clamp(0., 1.).min(selection));
            }
            self.stillroom_chrome(
                cx,
                asset,
                width,
                height,
                Chrome {
                    reveal: screen,
                    live_glass: screen == 1.
                        && asset.kind == Kind::Photo
                        && self.stillroom.zoom_phase > 0.06,
                },
            );
        };
    }

    #[component]
    fn stillroom_chrome(
        &mut self,
        cx: &mut UiContext,
        asset: &Asset,
        width: f32,
        height: f32,
        chrome: Chrome,
    ) {
        if chrome.reveal == 0. {
            return;
        }
        let sidebar = if width < 1020. { 252. } else { 306. };
        let gallery = width - sidebar - GUTTER;
        self.stillroom_toolbar(cx, asset, gallery, chrome);
        self.stillroom_inspector(cx, asset, gallery, sidebar, height - GUTTER * 2., chrome);
        self.stillroom_strip(cx, asset.id, gallery, height, chrome);
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_photo(
        &mut self,
        cx: &mut UiContext,
        asset: &Asset,
        body: [f32; 2],
        gallery: f32,
        screen: f32,
        selection: f32,
    ) {
        let phase = cx.spring("immersive", f32::from(self.stillroom.immersive), PANEL_SPRING).clamp(0., 1.);
        self.stillroom.zoom_phase = phase;
        let anchor = self.stillroom.anchor;
        let fit = Bounds::photo_view(body, gallery, asset.dimensions, anchor, 0.);
        let target = Bounds::photo_view(body, gallery, asset.dimensions, anchor, phase);
        let matched = self.stillroom.matched_tile.filter(|(id, _)| *id == asset.id);
        let view = matched.map_or(target, |(_, tile)| {
            tile.cover(asset.dimensions).interpolate(target, screen.clamp(0., 1.04))
        });
        self.stillroom.painted_photo = Some((asset.id, view));
        let [x, y] = view.center();
        let [fit_x, fit_y] = fit.center();
        let scale = view.w / fit.w;
        let fade = if matched.is_some() { selection } else { screen.clamp(0., 1.).min(selection) };
        let radius = if matched.is_some() && screen < 1. { RADIUS / scale } else { RADIUS };
        // Keep the photo and its layout slot at fit size. Only the retained
        // presentation transform changes as the optical overlays stay in place.
        column(cx).absolute(fit.x, fit.y).width(fit.w).height(fit.h).translate(x - fit_x, y - fit_y).scale(scale, scale)
            .opacity(fade).radius(radius).corner_smoothing(0.).clip(true) + |cx|
        {
            let toggle = button_base(cx, "Toggle immersive zoom").width_percent(100.).height_percent(100.)
                .align(Align::Center).justify(Justify::Center) + |cx|
            {
                image(cx, &asset.poster).size(fit.w, fit.h);
                cx.pointer_fraction()
            };
            if toggle.clicked() {
                self.stillroom.toggle(toggle.inner.map_or([0.5, 0.5], |p| p.map(|v| v.clamp(0., 1.))));
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_toolbar(
        &mut self,
        cx: &mut UiContext,
        asset: &Asset,
        gallery: f32,
        chrome: Chrome,
    ) {
        let screen = chrome.reveal;
        let height = 48.;
        let toolbar = row(cx).absolute(GUTTER, GUTTER).width(gallery - GUTTER * 2.).height(height).align(Align::Center)
            .padding_x(14).gap(8).translate(0., -6. * (1. - screen)).opacity(screen).radius(height / 2.)
            .corner_smoothing(0.);
        chrome.surface(toolbar, 0.72) + |cx| {
            if icon_button(cx, "Back to library", &ARROW_LEFT, false).clicked() {
                self.back();
            }
            text(cx, asset.title.as_str()).font_size(15).weight(700).color(TEXT).width(0.).grow(1.).ellipsis(true);
            let results = self.filtered();
            if let Some(index) = results.iter().position(|candidate| candidate.id == asset.id) {
                text!(cx, "{} / {}", index + 1, results.len()).font_size(12).color(MUTED);
            }
            if asset.kind == Kind::Photo {
                let label = if self.stillroom.immersive { "Fit" } else { "Fill" };
                if button(cx).text(label).icon(&MAXIMIZE).ghost().height(34.).radius(9.).clicked() {
                    self.stillroom.toggle([0.5, 0.5]);
                }
            }
            if icon_button(cx, "Open original file", &EXTERNAL_LINK,
                asset.path.is_none() && asset.kind == Kind::Photo).clicked()
            {
                self.effects.push_back(Effect::Open(asset.path.clone().unwrap_or_default()));
            }
            if icon_button(cx, if asset.favorite { "Remove favorite" } else { "Add favorite" }, &STAR, false).clicked()
            {
                self.favorite(asset.id);
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_video_surface(
        &mut self,
        cx: &mut UiContext,
        asset: &Asset,
        gallery: f32,
        height: f32,
        reveal: f32,
    ) {
        let width = gallery - 56.;
        let available = (height - 226.).max(144.);
        let (w, h) = asset.dimensions;
        let scale = (width / w as f32).min(available / h as f32);
        let w = w as f32 * scale;
        let h = h as f32 * scale;
        let y = 80. + (available - h) / 2.;
        let x = (gallery - w) / 2.;
        self.stillroom.painted_photo = Some((asset.id, Bounds { x, y, w, h }));
        self.stillroom.painted_native_video =
            self.video_inline && self.playback.as_ref().is_some_and(|p| p.error.is_none());
        column(cx).absolute(x, y).width(w).height(h).background(BASE).opacity(0.55 + 0.45 * reveal).radius(RADIUS)
            .clip(true) + |cx|
        {
            if self.stillroom.painted_native_video {
                column(cx).key("framebank-video").width(w).height(h) + |_| {};
            } else {
                image(cx, &asset.poster).size(w, h);
            }
        };
        column(cx).absolute(42., height - 125.).width(gallery - 84.).height(44.).padding_y(4).radius(RADIUS)
            .background(Color::Literal(0x2a313b)).border(1., Color::Literal(0x444a54)) + |cx|
        {
            self.video_controls(cx, asset);
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_strip(
        &mut self,
        cx: &mut UiContext,
        active: u64,
        gallery: f32,
        height: f32,
        chrome: Chrome,
    ) {
        let screen = chrome.reveal;
        let mut target = None;
        let assets = self.filtered();
        let limit = (((gallery - 116.) / 59.).floor() as usize).max(1).min(assets.len());
        let width = (limit as f32 * 59. + 91.).min(gallery - 30.);
        let position = assets.iter().position(|asset| asset.id == active).unwrap_or_default();
        let start = position.saturating_sub(limit / 2).min(assets.len().saturating_sub(limit));
        let mut advance = None;
        let strip = row(cx).absolute((gallery - width) / 2., height - 68.).width(width).height(50.).padding_x(6).gap(4)
            .align(Align::Center).justify(Justify::Center).translate(0., 8. * (1. - screen)).opacity(screen)
            .radius(RADIUS);
        chrome.surface(strip, 0.75) + |cx| {
            if icon_button(cx, "Previous asset", &CHEVRON_LEFT, position == 0).clicked() {
                advance = Some(false);
            }
            for asset in assets.into_iter().skip(start).take(limit) {
                let selected = active == asset.id;
                let control = button_base(cx, asset.title.as_str()).key(asset.id).selected(selected).width(55.)
                    .height(39.).shrink(0.).radius(8.).clip(true).align(Align::Center).justify(Justify::Center)
                    .border(2., if selected { ACCENT } else { LINE }).transition(150);
                if (control + |cx| {
                    image(cx, &asset.thumbnail).size(51., 32.);
                }).clicked()
                {
                    target = Some(asset.id);
                }
            }
            if icon_button(cx, "Next asset", &CHEVRON_RIGHT, position + 1 >= self.filtered().len()).clicked() {
                advance = Some(true);
            }
        };
        if let Some(advance) = advance {
            self.move_selection(advance);
        }
        if let Some(id) = target {
            self.select(id);
        }
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_inspector(
        &mut self,
        cx: &mut UiContext,
        asset: &Asset,
        x: f32,
        width: f32,
        height: f32,
        chrome: Chrome,
    ) {
        let screen = chrome.reveal;
        column(cx).absolute(x, GUTTER).width(width).height(height).translate(8. * (1. - screen), 0.).opacity(screen)
            .gap(9) + |cx|
        {
            let progress = cx.spring("inspector-tab", f32::from(self.related), PANEL_SPRING).clamp(0., 1.);
            let height = 48.;
            let inset = 6.;
            let tab_height = height - 2. * inset;
            let half = (width - 2. * inset) / 2.;
            let tabs = row(cx).height(height).width_percent(100.).shrink(0.).padding(inset).radius(height / 2.)
                .corner_smoothing(0.);
            chrome.surface(tabs, 0.76) + |cx| {
                column(cx).absolute(inset, inset).translate(half * progress, 0.).width(half).height(tab_height)
                    .background(ACCENT).background_alpha(0.18).radius(tab_height / 2.).corner_smoothing(0.) + |_|
                {};
                for (related, label) in [(false, "Info"), (true, "Similar")] {
                    if button(cx).key(label).text(label).ghost().width(half).height(tab_height)
                        .radius(tab_height / 2.).corner_smoothing(0.).clicked()
                    {
                        self.related = related;
                    }
                }
            };
            // As in MyGO's AfterExits, the current pane leaves before the next appears.
            let visible_related = progress >= 0.5;
            let reveal = (2. * progress - 1.).abs();
            let direction = if visible_related { 1. } else { -1. };
            column(cx).width_percent(100.).height(0.).grow(1.).scroll_y(true).gap(8)
                .translate(8. * direction * (1. - reveal), 0.).opacity(reveal) + |cx|
            {
                if visible_related {
                    self.stillroom_similar(cx, asset, width, chrome);
                } else {
                    chrome.surface(column(cx).width_percent(100.).radius(RADIUS), 0.83).padding(13).gap(10)
                        .align(Align::Center) + |cx|
                    {
                        column(cx).width(146.).height(91.).radius(9.).clip(true) + |cx| {
                            image(cx, &asset.thumbnail).size(146., 91.);
                        };
                        text(cx, asset.title.as_str()).font_size(17).weight(700).color(TEXT).ellipsis(true);
                    };
                    chrome.surface(column(cx).width_percent(100.).radius(RADIUS), 0.83).padding(13).gap(8) + |cx| {
                        row(cx).align(Align::Center).justify(Justify::SpaceBetween) + |cx| {
                            text(cx, "Colors").font_size(13).weight(650).color(TEXT);
                            if button(cx).text("Copy palette").ghost().height(28.).clicked() {
                                let colors = asset.palette.iter().map(|color| hex(*color))
                                    .collect::<Vec<_>>();
                                self.effects.push_back(Effect::Copy(colors.join(", ")));
                                self.notice = "Copied palette".into();
                            }
                        };
                        self.stillroom_swatches(cx, asset);
                    };
                    chrome.surface(column(cx).width_percent(100.).radius(RADIUS), 0.83).padding(13).gap(8) + |cx| {
                        detail_row(cx, "Collection", asset.group);
                        detail_row(cx, "Format", asset.format.as_str());
                        detail_row(cx, "Dimensions",
                            &format!("{} × {}", asset.dimensions.0, asset.dimensions.1));
                        detail_row(cx, "File size", &byte_label(asset.bytes));
                        if asset.kind == Kind::Video {
                            detail_row(cx, "Duration", &time_label(asset.duration));
                        }
                    };
                    chrome.surface(column(cx).width_percent(100.).radius(RADIUS), 0.83).padding(13).gap(8) + |cx| {
                        text(cx, "Source").font_size(13).weight(650).color(TEXT);
                        text(
                            cx,
                            if asset.path.is_some() { asset.source() } else { "Built-in sample" },
                        ).font_size(12).width_percent(100.).color(MUTED).ellipsis(true);
                        if asset.path.is_some()
                            && button(cx).text("Copy path").icon(&COPY).ghost().height(30.).clicked()
                        {
                            self.effects.push_back(Effect::Copy(asset.source().to_owned()));
                            self.notice = "Copied file path".into();
                        }
                    };
                }
            };
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_swatches(&mut self, cx: &mut UiContext, asset: &Asset) {
        row(cx).width_percent(100.).gap(5) + |cx| {
            for &color in &asset.palette {
                let label = hex(color);
                let rgb = u32::from_be_bytes([0, color[0], color[1], color[2]]);
                let swatch = button_base(cx, format!("Copy {label}")).key(rgb as u64).width(0.).grow(1.).height(28.)
                    .background(Color::Literal(rgb)).radius(7.).border(1., LINE);
                if (swatch + |_| {}).clicked() {
                    self.effects.push_back(Effect::Copy(label.clone()));
                    self.notice = format!("Copied {label}");
                }
            }
        };
    }

    #[component]
    #[rustfmt::skip]
    fn stillroom_similar(
        &mut self,
        cx: &mut UiContext,
        active: &Asset,
        width: f32,
        chrome: Chrome,
    ) {
        let mut target = None;
        chrome.surface(column(cx).width_percent(100.).radius(RADIUS), 0.84).padding(12).gap(11) + |cx| {
            self.stillroom_swatches(cx, active);
            text(cx, "Closest palettes").font_size(13).color(MUTED);
            let mut related: Vec<_> = self.assets.iter().filter(|item| item.id != active.id)
                .map(|item| (active.similar(item), item)).collect();
            related.sort_by_key(|(distance, item)| (*distance, item.id));
            let card_width = (width - 39.) / 2.;
            let nearest = &related[..related.len().min(4)];
            for pair in nearest.chunks(2) {
                row(cx).key(pair[0].1.id).gap(8) + |cx| {
                    for (_, item) in pair {
                        let tile = button_base(cx, item.title.as_str()).key(item.id).width(card_width)
                            .height(card_width * 0.625).radius(8.).clip(true);
                        if (tile + |cx| {
                            image(cx, &item.thumbnail).size(card_width, card_width * 0.625);
                        }).clicked()
                        {
                            target = Some(item.id);
                        }
                    }
                };
            }
        };
        if let Some(id) = target {
            if !self.filtered().iter().any(|asset| asset.id == id) {
                self.filter = Filter::All;
                self.query.clear();
            }
            self.select(id);
        }
    }
}

fn masonry(
    assets: Vec<&Asset>,
    columns: usize,
    width: f32,
    gap: f32,
) -> Vec<Vec<(&Asset, f32, f32)>> {
    let mut stacks = vec![Vec::new(); columns];
    let mut layout = MasonryLayout::new(columns.try_into().expect("positive column count"), gap);
    for asset in assets {
        let (w, h) = asset.dimensions;
        let height = (width * h as f32 / w as f32).clamp(width * 0.45, width * 1.45);
        let slot = layout.push(height);
        stacks[slot.column].push((asset, height, slot.top));
    }
    stacks
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

// MyGO's match.go fadeIn. The arriving JPEG is opaque at halfway and completely
// covers the outgoing copy within their shared clip, so the latter can be dropped.
fn arriving_photo_alpha(progress: f32) -> f32 {
    let offset = (progress * 2.).clamp(0., 1.) - 1.;
    (1. - offset * offset).sqrt()
}

#[component]
#[rustfmt::skip]
fn returning_photo(cx: &mut UiContext, asset: &Asset, from: Bounds, tile: Bounds, progress: f32) {
    let distance = from.remaining_distance(tile, asset.poster_dimensions, progress);
    let blend = ((distance - LANDING_REST_DISTANCE)
        / (LANDING_FADE_DISTANCE - LANDING_REST_DISTANCE)).clamp(0., 1.);
    let opacity = blend * blend * (3. - 2. * blend);
    if opacity == 0. {
        return;
    }
    let (clip, poster) = from.photo_landing(tile, asset.poster_dimensions, progress);
    let arriving = arriving_photo_alpha(progress);
    let Some(clip_pose) = PinnedRect::new(tile.rect(), clip.rect()) else {
        return;
    };
    let rest = tile.cover(asset.poster_dimensions);
    let Some(image_pose) = PinnedRect::new([rest.x - tile.x, rest.y - tile.y, rest.w, rest.h],
        clip_pose.to_local(poster.rect())) else {
        return;
    };
    let [image_w, image_h] = image_pose.layout_size();
    column(cx).pinned_rect(clip_pose).radius(RADIUS / clip_pose.scale()).corner_smoothing(progress).clip(true)
        .background(SURFACE).opacity(opacity) + |cx|
    {
        column(cx).pinned_rect(image_pose) + |cx| {
            if arriving < 1. {
                image(cx, &asset.poster).size(image_w, image_h);
            }
            if arriving > 0. {
                column(cx).absolute(0., 0.).width(image_w).height(image_h).opacity(arriving) + |cx| {
                    image(cx, &asset.wall_preview).size(image_w, image_h);
                };
            }
        };
    };
}

#[component]
#[rustfmt::skip]
fn stillroom_tile(
    cx: &mut UiContext,
    asset: &Asset,
    width: f32,
    height: f32,
    hidden: bool,
) -> (bool, bool) {
    let mut opened = false;
    let mut favorited = false;
    column(cx).key(asset.id).width(width).height(height).shrink(0.).clip(true).radius(RADIUS).background(SURFACE)
        .opacity(if hidden { 0. } else { 1. }) + |cx|
    {
        let hovered = !hidden && cx.hovered_within();
        let bounds = Bounds {
            x: 0.,
            y: 0.,
            w: width,
            h: height,
        }.cover(asset.poster_dimensions);
        let tile = button_base(cx, asset.title.as_str()).width_percent(100.).height_percent(100.)
            .align(Align::Center).justify(Justify::Center);
        opened = (tile + |cx| {
            column(cx).absolute(bounds.x, bounds.y).width(bounds.w).height(bounds.h) + |cx| {
                image(cx, &asset.wall_preview).size(bounds.w, bounds.h);
            };
        }).clicked();
        let reveal = cx.animate("tile-caption", f32::from(hovered), Duration::from_millis(130));
        if reveal > 0.01 {
            row(cx).absolute(7., height - 42. + (1. - reveal) * 7.).width(width - 14.).height(35.).opacity(reveal)
                .padding_x(8).gap(6).align(Align::Center).radius(9.).background(BASE).background_alpha(0.82)
                .border(1., LINE) + |cx|
            {
                text(cx, asset.title.as_str()).width(0.).grow(1.).font_size(12).weight(650).color(TEXT)
                    .ellipsis(true);
                if icon_button(cx, if asset.favorite { "Remove favorite" } else { "Add favorite" }, &STAR,
                    false).clicked()
                {
                    favorited = true;
                }
            };
        }
        if asset.kind == Kind::Video {
            row(cx).absolute(width - 64., 9.).width(55.).height(26.).align(Align::Center).justify(Justify::Center)
                .gap(4).radius(8.).background(BASE).background_alpha(0.89) + |cx|
            {
                svg(cx, &PLAY).size(12., 12.).color(TEXT);
                text(cx, time_label(asset.duration)).font_size(11).color(TEXT);
            };
        }
    };
    (opened, favorited)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_portrait_ends_at_the_masonry_crop() {
        let detail = Bounds {
            x: 100.,
            y: 50.,
            w: 400.,
            h: 800.,
        };
        let tile = Bounds {
            x: 600.,
            y: 450.,
            w: 300.,
            h: 435.,
        };
        let poster = (1000, 2000);

        assert_eq!(detail.photo_landing(tile, poster, 0.), (detail, detail));
        let (clip, image) = detail.photo_landing(tile, poster, 0.5);
        assert!(clip.h < image.h);
        assert_eq!(clip.center(), image.center());
        let (clip, image) = detail.photo_landing(tile, poster, 1.);
        assert_eq!(clip, tile);
        assert_eq!(image, tile.cover(poster));
    }
}
