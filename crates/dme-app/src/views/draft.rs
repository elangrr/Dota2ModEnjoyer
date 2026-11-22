use std::collections::BTreeSet;

use dme_core::catalog::{Catalog, Hero};
use dme_core::draft::{Numbers, Rank, Role, Style, Suggestion, Trend, Way};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::tiles::{self, Look, Tile};
use super::{dim, heroes, title_bar};
use crate::lang::{self, T, number, signed, words};
use crate::pictures::Pictures;
use crate::theme;
use crate::worker::Failure;

pub const SOURCE_PAGE: &str = "https://stratz.com";

const SEATS: usize = 5;
const PHASES: [&[usize]; 3] = [&[0, 1], &[2, 3], &[4]];
const PHASE_NAMES: [T; 3] = [T::FirstPicks, T::SecondPicks, T::LastPick];
const ROLES_BY_PLACE: [Role; SEATS] = [Role::SoftSupport, Role::HardSupport, Role::Carry, Role::Offlane, Role::Mid];
pub(super) const ROLES_WIDE: f32 = 172.0;
pub(super) const RANKS_WIDE: f32 = 172.0;
const BOX_LEAST: f32 = 124.0;
const NOMINEES: usize = 3;
const SUGGESTED: usize = 3;
const LINE_HIGH: f32 = 30.0;
const EVEN_WITHIN: f32 = 1.0;
pub(super) const LOOK: Look = Look { picture: vec2(132.0, 78.0), name_lines: 1, note: false, name_over: true };
const ROW: f32 = 58.0;
const NAMED_IN_A_ROW: usize = 2;
const WORTH_NAMING: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seat {
    enemy: bool,
    place: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Draft {
    team: [Option<String>; SEATS],
    enemies: [Option<String>; SEATS],
    team_roles: [Role; SEATS],
    enemy_roles: [Role; SEATS],
    seat: Seat,
    pub search: String,
    banned: BTreeSet<String>,
    by_hand: bool,
}

impl Default for Draft {
    fn default() -> Draft {
        Draft {
            team: Default::default(),
            enemies: Default::default(),
            team_roles: ROLES_BY_PLACE,
            enemy_roles: ROLES_BY_PLACE,
            seat: Seat { enemy: true, place: 0 },
            search: String::new(),
            banned: BTreeSet::new(),
            by_hand: false,
        }
    }
}

impl Draft {
    fn side(&self, enemy: bool) -> &[Option<String>; SEATS] {
        if enemy { &self.enemies } else { &self.team }
    }

    fn side_mut(&mut self, enemy: bool) -> &mut [Option<String>; SEATS] {
        if enemy { &mut self.enemies } else { &mut self.team }
    }

    fn at(&self, seat: Seat) -> Option<&str> {
        self.side(seat.enemy)[seat.place].as_deref()
    }

    fn role(&self, seat: Seat) -> Role {
        if seat.enemy { self.enemy_roles[seat.place] } else { self.team_roles[seat.place] }
    }

    fn set_role(&mut self, seat: Seat, role: Role) {
        let roles = if seat.enemy { &mut self.enemy_roles } else { &mut self.team_roles };
        if let Some(other) = roles.iter().position(|had| *had == role) {
            roles[other] = roles[seat.place];
        }
        roles[seat.place] = role;
    }

    fn own_seat(&self, role: Option<Role>) -> Option<Seat> {
        let place = self.team_roles.iter().position(|had| Some(*had) == role)?;
        Some(Seat { enemy: false, place })
    }

    fn list_seat(&self, role: Option<Role>) -> Seat {
        match self.own_seat(role) {
            Some(own) if self.nothing_picked() && !self.by_hand => own,
            _ => self.seat,
        }
    }

    pub fn own_hero(&self, role: Option<Role>) -> Option<(&str, Role)> {
        Some((self.at(self.own_seat(role)?)?, role?))
    }

    pub fn against(&self, hero: &str) -> Vec<String> {
        let of_the_enemy = self.enemies.iter().flatten().any(|picked| picked == hero);
        self.heroes(!of_the_enemy).into_iter().map(str::to_string).collect()
    }

    fn heroes(&self, enemy: bool) -> Vec<&str> {
        self.side(enemy).iter().flatten().map(String::as_str).collect()
    }

    fn seated(&self, enemy: bool) -> Vec<(&str, Role)> {
        (0..SEATS).filter_map(|place| Some((self.at(Seat { enemy, place })?, self.role(Seat { enemy, place })))).collect()
    }

    fn is_full(&self) -> bool {
        self.team.iter().chain(&self.enemies).all(Option::is_some)
    }

    fn is_picked(&self, hero: &str) -> bool {
        self.team.iter().chain(&self.enemies).flatten().any(|picked| picked == hero)
    }

    fn is_empty(&self) -> bool {
        self.nothing_picked() && self.banned.is_empty() && self.team_roles == ROLES_BY_PLACE && self.enemy_roles == ROLES_BY_PLACE
    }

    fn nothing_picked(&self) -> bool {
        self.team.iter().chain(&self.enemies).all(Option::is_none)
    }

    fn ban(&mut self, hero: &str) {
        self.unseat(hero);
        self.banned.insert(hero.to_string());
        self.search.clear();
    }

    fn unseat(&mut self, hero: &str) {
        for place in self.team.iter_mut().chain(&mut self.enemies) {
            if place.as_deref() == Some(hero) {
                *place = None;
            }
        }
    }

    fn next_free(&self, seat: Seat) -> Seat {
        let phase = PHASES.iter().position(|places| places.contains(&seat.place)).unwrap_or(0);
        let in_its_phase = [seat.enemy, !seat.enemy].map(|enemy| (phase, enemy));
        let in_the_others = (1..PHASES.len()).flat_map(|later| [true, false].map(|enemy| ((phase + later) % PHASES.len(), enemy)));
        in_its_phase
            .into_iter()
            .chain(in_the_others)
            .flat_map(|(phase, enemy)| PHASES[phase].iter().map(move |&place| Seat { enemy, place }))
            .find(|free| self.at(*free).is_none())
            .unwrap_or(seat)
    }

    fn seat_hero(&mut self, hero: &str) {
        self.unseat(hero);
        self.banned.remove(hero);
        let seat = self.seat;
        self.side_mut(seat.enemy)[seat.place] = Some(hero.to_string());
        self.seat = self.next_free(seat);
        self.search.clear();
    }

    fn seat_hero_at(&mut self, seat: Seat, hero: &str) {
        let lit = self.seat;
        self.seat = seat;
        self.seat_hero(hero);
        if seat != lit && self.at(lit).is_none() {
            self.seat = lit;
        }
    }

    fn around(&self, seat: Seat) -> (Vec<&str>, Vec<&str>) {
        let own = self.at(seat);
        let beside = self.heroes(seat.enemy).into_iter().filter(|hero| Some(*hero) != own).collect();
        (beside, self.heroes(!seat.enemy))
    }

    fn in_the_lane_of(&self, seat: Seat) -> (Vec<&str>, Vec<(&str, Role)>) {
        let role = self.role(seat);
        let partner = self.seated(seat.enemy).into_iter().filter(|(_, other)| Some(*other) == role.lane_partner()).map(|(hero, _)| hero).collect();
        let opponents = self.seated(!seat.enemy).into_iter().filter(|(_, other)| role.lane_opponents().contains(other)).collect();
        (partner, opponents)
    }

    fn picked(&self) -> Vec<&str> {
        self.team.iter().chain(&self.enemies).flatten().map(String::as_str).collect()
    }

    fn empty(&mut self, seat: Seat) {
        self.side_mut(seat.enemy)[seat.place] = None;
        self.seat = seat;
    }

    fn start_over(&mut self) {
        *self = Draft::default();
    }
}

pub struct View<'a> {
    pub catalog: &'a Catalog,
    pub pictures: &'a mut Pictures,
    pub draft: &'a mut Draft,
    pub numbers: Option<&'a Numbers>,
    pub fetching: bool,
    pub failure: Option<&'a Failure>,
    pub rank: Rank,
    pub role: Option<Role>,
    pub style: Style,
    pub mine: &'a [String],
    pub all_heroes: bool,
    pub now: u64,
}

#[derive(Default)]
pub struct Answer {
    pub rank: Option<Rank>,
    pub role: Option<Option<Role>>,
    pub style: Option<Style>,
    pub try_again: bool,
    pub copy_details: bool,
    pub open_source: bool,
    pub to_build: Option<(String, Role)>,
    pub all_heroes: Option<bool>,
    pub suggest: Option<String>,
    pub open_pool: bool,
}

pub fn show(ui: &mut Ui, view: View<'_>) -> Answer {
    let View { catalog, pictures, draft, numbers, fetching, failure, rank, role, style, mine, all_heroes, now } = view;
    let choices = Choices { rank, role, style, mine, all_heroes, now };
    let mut answer = Answer::default();
    let own = numbers.map(|numbers| own_picks(catalog, draft, numbers, &choices)).unwrap_or_default();
    let nominees = numbers.map(|numbers| nominate(catalog, draft, numbers, &choices, &own)).unwrap_or_default();
    let mut start_over = false;
    let any = !draft.is_empty();
    title_bar(ui, tr!(DraftTitle), Some(tr!(DraftNote)), Some((&mut draft.search, tr!(HeroNameHint))), |ui| {
        start_over = ui.add_enabled(any, egui::Button::new(tr!(NewDraft))).on_hover_text(tr!(NewDraftHint)).clicked();
        ui.add_space(4.0);
        answer.style = style_switch(ui, style);
    });
    if start_over {
        draft.start_over();
    }

    let chance = numbers.filter(|_| draft.is_full()).map(|numbers| numbers.chance(rank, &draft.seated(false), &draft.seated(true)));
    self::choices(ui, pictures, &choices, chance, &mut answer);
    seats(ui, catalog, pictures, draft, role);
    ui.add_space(6.0);
    bans(ui, catalog, pictures, draft, &nominees, role);
    ui.add_space(6.0);
    if draft.is_full() {
        let own = draft.own_hero(role);
        let name = own.map(|(id, _)| catalog.any_hero(id).map_or(id, |hero| hero.name.as_str()));
        if to_build(ui, name) {
            answer.to_build = own.map(|(hero, role)| (hero.to_string(), role));
        }
        ui.add_space(8.0);
    }

    let wanted = draft.search.trim().to_lowercase();
    if !wanted.is_empty() {
        fitting(ui, catalog, pictures, draft, &wanted);
    } else if let Some(numbers) = numbers.filter(|numbers| style == Style::Team || numbers.has_lanes()) {
        suggestions(ui, catalog, pictures, draft, numbers, &choices, &mut answer);
    } else if numbers.is_some() {
        match lanes_waiting(ui, fetching, failure) {
            Some(Asked::Again) => answer.try_again = true,
            Some(Asked::CopyDetails) => answer.copy_details = true,
            None => {}
        }
    } else {
        match waiting(ui, fetching, failure, Fetched::Numbers, tr!(StillPutIn)) {
            Some(Asked::Again) => answer.try_again = true,
            Some(Asked::CopyDetails) => answer.copy_details = true,
            None => {}
        }
    }
    answer
}

struct Choices<'a> {
    rank: Rank,
    role: Option<Role>,
    style: Style,
    mine: &'a [String],
    all_heroes: bool,
    now: u64,
}

fn style_switch(ui: &mut Ui, style: Style) -> Option<Style> {
    let short = ui.available_width() < 640.0;
    let words = Style::ALL.map(|each| match (each, short) {
        (Style::Team, true) => tr!(TeamShort),
        (Style::Lane, true) => tr!(LaneShort),
        (each, false) => words::style(each),
    });
    let hints = [tr!(StyleTeamHint), tr!(StyleLaneHint)];
    let on = Style::ALL.iter().position(|each| *each == style).unwrap_or(0);
    halves(ui, "draft style", 30.0, words, hints, on).map(|index| Style::ALL[index])
}

fn halves(ui: &mut Ui, salt: &str, high: f32, words: [&str; 2], hints: [&str; 2], on: usize) -> Option<usize> {
    let font = FontId::proportional(theme::SMALL + 0.5);
    let wide = words.map(|words| ui.painter().layout_no_wrap(words.to_string(), font.clone(), theme::text()).size().x + 24.0);
    let (rect, _) = ui.allocate_exact_size(vec2(wide.iter().sum::<f32>() + 4.0, high), Sense::hover());
    if ui.is_rect_visible(rect) {
        let round = CornerRadius::same(theme::ROUND);
        ui.painter().rect_filled(rect, round, theme::card());
        ui.painter().rect_stroke(rect, round, Stroke::new(1.0, theme::line()), StrokeKind::Inside);
    }
    let mut asked = None;
    let mut left = rect.left() + 2.0;
    for index in 0..2 {
        let half = Rect::from_min_size(pos2(left, rect.top() + 2.0), vec2(wide[index], high - 4.0));
        left += wide[index];
        let response = ui.interact(half, egui::Id::unique(salt).with(index), Sense::click());
        let chosen = index == on;
        if chosen {
            ui.painter().rect_filled(half, CornerRadius::same(7), theme::accent().gamma_multiply(if theme::is_day() { 0.18 } else { 0.26 }));
        } else if response.hovered() {
            ui.painter().rect_filled(half, CornerRadius::same(7), theme::text().gamma_multiply(0.08));
        }
        let ink = if chosen { theme::accent_text() } else { theme::text() };
        ui.painter().text(half.center(), Align2::CENTER_CENTER, words[index], font.clone(), ink);
        if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hints[index]).clicked() && !chosen {
            asked = Some(index);
        }
    }
    asked
}

fn lanes_waiting(ui: &mut Ui, fetching: bool, failure: Option<&Failure>) -> Option<Asked> {
    if fetching || failure.is_some() {
        return waiting(ui, fetching, failure, Fetched::Lanes, tr!(LanesStill));
    }
    let mut asked = None;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.2).min(60.0));
        ui.label(theme::title(tr!(LanesNotYet), 18.0));
        ui.add_space(2.0);
        ui.label(dim(tr!(LanesWhen)));
        ui.add_space(8.0);
        if ui.button(tr!(FetchAgain)).clicked() {
            asked = Some(Asked::Again);
        }
    });
    asked
}

fn choices(ui: &mut Ui, pictures: &mut Pictures, choices: &Choices, chance: Option<f32>, answer: &mut Answer) {
    let (rank, role) = (choices.rank, choices.role);
    let label = |ui: &mut Ui, words: &str| {
        let words = ui.painter().layout_no_wrap(words.to_string(), FontId::proportional(theme::SMALL), theme::text_dim());
        let (rect, _) = ui.allocate_exact_size(vec2(words.size().x, LINE_HIGH), Sense::hover());
        ui.painter().galley(pos2(rect.left(), rect.center().y - words.size().y / 2.0), words, theme::text_dim());
    };
    let wide_of = |ui: &Ui, words: &str| ui.painter().layout_no_wrap(words.to_string(), FontId::proportional(theme::SMALL), theme::text_dim()).size().x;
    let taken = wide_of(ui, tr!(MyRank)) + wide_of(ui, tr!(MyRole)) + 3.0 * 8.0 + 10.0 + if chance.is_some() { 120.0 } else { 0.0 };
    let box_wide = ((ui.available_width() - taken) / 2.0).floor().clamp(BOX_LEAST, RANKS_WIDE);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        label(ui, tr!(MyRank));
        let open = list_box(ui, pictures, &Line::of_rank(rank), box_wide);
        egui::Popup::menu(&open).show(|ui| {
            ui.set_width(RANKS_WIDE);
            ui.spacing_mut().item_spacing.y = 2.0;
            for other in Rank::ALL {
                if list_line(ui, pictures, &Line::of_rank(other), other == rank, RANKS_WIDE).clicked() && other != rank {
                    answer.rank = Some(other);
                }
            }
        });
        ui.add_space(10.0);
        label(ui, tr!(MyRole));
        let open = list_box(ui, pictures, &Line::of_any_role(role), box_wide);
        egui::Popup::menu(&open).show(|ui| {
            ui.set_width(ROLES_WIDE);
            ui.spacing_mut().item_spacing.y = 2.0;
            for other in std::iter::once(None).chain(Role::ALL.map(Some)) {
                if list_line(ui, pictures, &Line::of_any_role(other), other == role, ROLES_WIDE).clicked() && other != role {
                    answer.role = Some(other);
                }
            }
        });
        if let Some(chance) = chance {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| verdict(ui, chance));
        }
    });
    ui.add_space(4.0);
}

pub(super) struct Line {
    drawings: Vec<&'static str>,
    room: usize,
    plain: bool,
    name: &'static str,
}

impl Line {
    pub(super) fn of_role(role: Role) -> Line {
        Line { drawings: vec![role_logo(role)], room: 1, plain: true, name: words::role(role) }
    }

    fn of_any_role(role: Option<Role>) -> Line {
        role.map_or(Line { drawings: Vec::new(), room: 1, plain: true, name: tr!(AnyRole) }, Line::of_role)
    }

    pub(super) fn of_rank(rank: Rank) -> Line {
        Line { drawings: vec![rank_medal(rank)], room: 1, plain: false, name: words::rank(rank) }
    }

    fn paint(&self, painter: &egui::Painter, pictures: &mut Pictures, rect: Rect, ink: Color32) {
        let side = if self.plain { 18.0 } else { 29.0 };
        let step = side + 2.0;
        for (place, drawing) in self.drawings.iter().enumerate() {
            let room = Rect::from_center_size(pos2(rect.left() + 8.0 + side / 2.0 + step * place as f32, rect.center().y), vec2(side, side));
            if let Some((texture, _)) = pictures.get(drawing) {
                painter.image(texture, room, tiles::WHOLE, if self.plain { ink } else { Color32::WHITE });
            }
        }
        let left = rect.left() + 8.0 + step * self.room as f32 + 6.0;
        painter.text(pos2(left, rect.center().y), Align2::LEFT_CENTER, self.name, FontId::proportional(theme::BODY - 1.0), ink);
    }
}

pub(super) fn list_box(ui: &mut Ui, pictures: &mut Pictures, line: &Line, wide: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(wide, LINE_HIGH), Sense::click());
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered(), 0.12);
        let painter = ui.painter();
        let round = CornerRadius::same(theme::ROUND);
        painter.rect_filled(rect, round, theme::card().lerp_to_gamma(theme::card_hover(), lit));
        painter.rect_stroke(rect, round, Stroke::new(1.0, theme::line().lerp_to_gamma(theme::text_dim(), lit)), StrokeKind::Inside);
        let room = Rect::from_min_max(rect.min, pos2(rect.right() - 24.0, rect.max.y));
        line.paint(&painter.with_clip_rect(room), pictures, rect, theme::text());
        let tip = pos2(rect.right() - 14.0, rect.center().y + 3.0);
        let arrow = vec![tip + vec2(-4.5, -5.5), tip + vec2(4.5, -5.5), tip];
        painter.add(egui::Shape::convex_polygon(arrow, theme::text_dim(), Stroke::NONE));
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

pub(super) fn list_line(ui: &mut Ui, pictures: &mut Pictures, line: &Line, chosen: bool, wide: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(wide, LINE_HIGH), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let round = CornerRadius::same(7);
        if chosen {
            painter.rect_filled(rect, round, theme::accent().gamma_multiply(if theme::is_day() { 0.18 } else { 0.26 }));
        } else if response.hovered() {
            painter.rect_filled(rect, round, theme::text().gamma_multiply(0.08));
        }
        line.paint(painter, pictures, rect, if chosen { theme::accent_text() } else { theme::text() });
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn rank_medal(rank: Rank) -> &'static str {
    const MEDALS: [&str; 8] = [
        "panorama/images/rank_tier_icons/rank1_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank2_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank3_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank4_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank5_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank6_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank7_psd.vtex_c",
        "panorama/images/rank_tier_icons/rank8_psd.vtex_c",
    ];
    MEDALS[rank.place()]
}

fn seats(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, draft: &mut Draft, role: Option<Role>) {
    const GAP: f32 = 5.0;
    const PHASE_GAP: f32 = 12.0;
    const BETWEEN: f32 = 24.0;
    let gaps = BETWEEN + 2.0 * (2.0 * GAP + 2.0 * PHASE_GAP);
    let wide = ((ui.available_width() - gaps) / (2 * SEATS) as f32).clamp(40.0, 116.0).floor();
    let size = vec2(wide, (wide * 0.56).round());
    let own = draft.own_seat(role);
    let (mut clicked, mut asked) = (None, None);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 3.0);
        for enemy in [false, true] {
            ui.vertical(|ui| {
                ui.label(dim(if enemy { tr!(TheEnemy) } else { tr!(YourTeam) }).small());
                ui.horizontal(|ui| {
                    for (phase, places) in PHASES.iter().enumerate() {
                        for &place in *places {
                            let seat = Seat { enemy, place };
                            if place > 0 {
                                ui.add_space(if place == places[0] { PHASE_GAP } else { GAP });
                            }
                            let hero = draft.at(seat).and_then(|id| catalog.any_hero(id));
                            let look = SeatLook { size, filling: draft.seat == seat, yours: own == Some(seat), phase: lang::t(PHASE_NAMES[phase]), role: draft.role(seat) };
                            let did = seat_button(ui, pictures, &look, hero, draft.at(seat));
                            if did.clicked {
                                clicked = Some(seat);
                            }
                            if let Some(other) = did.role {
                                asked = Some((seat, other));
                            }
                        }
                    }
                });
            });
            if !enemy {
                ui.add_space(BETWEEN);
            }
        }
    });
    if let Some(seat) = clicked {
        match draft.at(seat).is_some() {
            true => draft.empty(seat),
            false => draft.seat = seat,
        }
        draft.by_hand = true;
    }
    if let Some((seat, other)) = asked {
        draft.set_role(seat, other);
    }
}

struct SeatLook {
    size: egui::Vec2,
    filling: bool,
    yours: bool,
    phase: &'static str,
    role: Role,
}

struct Did {
    clicked: bool,
    role: Option<Role>,
}

fn seat_button(ui: &mut Ui, pictures: &mut Pictures, look: &SeatLook, hero: Option<&Hero>, id: Option<&str>) -> Did {
    const ROUND: u8 = 9;
    let (rect, response) = ui.allocate_exact_size(look.size, Sense::click());
    let narrow = look.size.x < 70.0;
    let side = if narrow { 15.0 } else { 22.0 };
    let plate = Rect::from_min_size(pos2(rect.left() + 4.0, rect.bottom() - side - 4.0), vec2(side, side));
    let badge = ui.interact(plate, response.id.with("role"), Sense::click());
    let (taken, filling) = (id.is_some(), look.filling);
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered(), 0.12);
        let painter = ui.painter();
        let round = CornerRadius::same(ROUND);
        if taken {
            tiles::fill_with(painter, pictures, hero.and_then(|hero| hero.picture.as_deref()), rect, ROUND, 1.0);
            if lit > 0.0 {
                painter.rect_filled(rect, round, Color32::from_black_alpha((150.0 * lit) as u8));
                painter.text(rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(22.0), Color32::WHITE.gamma_multiply(lit));
            }
        } else {
            let fill = if filling { theme::accent().gamma_multiply(if theme::is_day() { 0.14 } else { 0.18 }) } else { theme::card().lerp_to_gamma(theme::card_hover(), lit) };
            painter.rect_filled(rect, round, fill);
            painter.text(rect.center(), Align2::CENTER_CENTER, "+", FontId::proportional(if narrow { 15.0 } else { 20.0 }), if filling { theme::accent_text() } else { theme::text_dim() });
        }
        let line = if filling { Stroke::new(2.0, theme::accent()) } else { Stroke::new(1.0, theme::line()) };
        painter.rect_stroke(rect, round, line, StrokeKind::Inside);

        let over = badge.hovered();
        let fill = match (look.yours, over) {
            (true, _) => theme::accent(),
            (false, true) => Color32::from_black_alpha(240),
            (false, false) => Color32::from_black_alpha(190),
        };
        painter.rect_filled(plate, CornerRadius::same(if narrow { 4 } else { 6 }), fill);
        if over {
            painter.rect_stroke(plate, CornerRadius::same(if narrow { 4 } else { 6 }), Stroke::new(1.0, Color32::from_white_alpha(150)), StrokeKind::Inside);
        }
        let ink = if over || look.yours { Color32::WHITE } else { Color32::from_gray(0xdc) };
        role_drawing(painter, pictures, look.role, plate.shrink(if narrow { 2.0 } else { 3.5 }), ink);

        if look.yours && !narrow {
            let words = painter.layout_no_wrap(tr!(You).to_string(), theme::heavy(10.0), Color32::WHITE);
            let tag = Rect::from_min_size(rect.min + vec2(4.0, 4.0), words.size() + vec2(10.0, 4.0));
            painter.rect_filled(tag, CornerRadius::same(7), theme::accent());
            painter.galley(tag.min + vec2(5.0, 2.0), words, Color32::WHITE);
        }
    }

    let mut chosen = None;
    egui::Popup::menu(&badge).show(|ui| {
        ui.set_width(ROLES_WIDE);
        ui.spacing_mut().item_spacing.y = 2.0;
        for other in Role::ALL {
            if list_line(ui, pictures, &Line::of_role(other), other == look.role, ROLES_WIDE).clicked() && other != look.role {
                chosen = Some(other);
            }
        }
    });
    let whose = if look.yours { tr!(YourPlace) } else { "" };
    badge.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(format!("{}{whose}", tr!(RoleBadgeHint, role = words::role(look.role))));

    let name = hero.map_or(id.unwrap_or_default(), |hero| hero.name.as_str());
    let hint = match (taken, filling) {
        (true, _) => tr!(SeatTaken, hero = name),
        (false, true) => format!("{}{whose}", tr!(SeatFilling, phase = look.phase)),
        (false, false) => format!("{}{whose}", tr!(SeatFree, phase = look.phase)),
    };
    let clicked = response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).clicked();
    Did { clicked, role: chosen }
}

fn role_drawing(painter: &egui::Painter, pictures: &mut Pictures, role: Role, room: Rect, ink: Color32) {
    match pictures.get(role_logo(role)) {
        Some((texture, _)) => {
            painter.image(texture, room, tiles::WHOLE, ink);
        }
        None => {
            let first: String = words::role(role).chars().take(1).collect();
            painter.text(room.center(), Align2::CENTER_CENTER, first, theme::heavy(room.height() * 0.8), ink);
        }
    }
}

fn role_logo(role: Role) -> &'static str {
    match role {
        Role::Carry => "panorama/images/rank_tier_icons/handicap/safelaneicon_psd.vtex_c",
        Role::Mid => "panorama/images/rank_tier_icons/handicap/midlaneicon_psd.vtex_c",
        Role::Offlane => "panorama/images/rank_tier_icons/handicap/offlaneicon_psd.vtex_c",
        Role::SoftSupport => "panorama/images/rank_tier_icons/handicap/softsupporticon_psd.vtex_c",
        Role::HardSupport => "panorama/images/rank_tier_icons/handicap/hardsupporticon_psd.vtex_c",
    }
}

fn verdict(ui: &mut Ui, chance: f32) {
    const BAR: egui::Vec2 = vec2(214.0, 22.0);
    const ROUND: u8 = 7;
    let (words, color) = verdict_words(chance);
    let team = chance.round().clamp(1.0, 99.0);
    ui.add_space(14.0);
    let (ours, theirs) = (tr!(BarYourTeam, share = format!("{team:.0}")), tr!(BarTheEnemy, share = format!("{:.0}", 100.0 - team)));
    let width_of = |text: &str| ui.painter().layout_no_wrap(text.to_string(), theme::heavy(12.0), Color32::WHITE).size().x;
    let named = (width_of(&ours) + width_of(&theirs) + 30.0).max(BAR.x);
    let room = ui.available_width();
    let short = room < named;
    let wide = if short { room.clamp(96.0, 130.0) } else { named };
    let (ours, theirs) = match short {
        true => (format!("{team:.0}%"), format!("{:.0}%", 100.0 - team)),
        false => (ours, theirs),
    };
    let (rect, bar) = ui.allocate_exact_size(vec2(wide, BAR.y), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let split = rect.left() + rect.width() * team / 100.0;
        painter.rect_filled(rect, CornerRadius::same(ROUND), theme::bad());
        painter.rect_filled(Rect::from_min_max(rect.min, pos2(split, rect.bottom())), CornerRadius { nw: ROUND, sw: ROUND, ne: 0, se: 0 }, theme::good());
        painter.text(pos2(rect.left() + 9.0, rect.center().y), Align2::LEFT_CENTER, ours, theme::heavy(12.0), Color32::WHITE);
        painter.text(pos2(rect.right() - 9.0, rect.center().y), Align2::RIGHT_CENTER, theirs, theme::heavy(12.0), Color32::WHITE);
    }
    bar.on_hover_text(tr!(VerdictHint, verdict = words, ours = format!("{team:.0}"), theirs = format!("{:.0}", 100.0 - team)));
    let said = ui.painter().layout_no_wrap(words.to_string(), theme::heavy(13.0), color);
    if said.size().x + 10.0 <= ui.available_width() {
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(said.size(), Sense::hover());
        ui.painter().galley(rect.min, said, color);
    }
}

fn to_build(ui: &mut Ui, own: Option<&str>) -> bool {
    let mut go = false;
    ui.horizontal(|ui| {
        let words = own.map_or_else(|| tr!(GoToBuild).to_string(), |name| tr!(GoToBuildOf, hero = name));
        let button = egui::Button::new(theme::title(words, 17.0).color(Color32::WHITE)).fill(theme::accent()).min_size(vec2(230.0, 44.0));
        let asked = ui.add_enabled(own.is_some(), button);
        match own {
            Some(name) => go = asked.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr!(GoToBuildHint, hero = name)).clicked(),
            None => {
                ui.add_space(4.0);
                ui.label(dim(tr!(ChooseRoleForBuild)).small());
            }
        }
    });
    go
}

fn verdict_words(chance: f32) -> (&'static str, Color32) {
    if chance >= 50.0 + EVEN_WITHIN {
        (tr!(DraftBetter), theme::good())
    } else if chance <= 50.0 - EVEN_WITHIN {
        (tr!(EnemyBetter), theme::bad())
    } else {
        (tr!(DraftsEven), theme::text())
    }
}

fn fitting(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, draft: &mut Draft, wanted: &str) {
    let fitting: Vec<&Hero> = heroes::found(catalog.all_heroes(), wanted).into_iter().filter(|hero| !draft.is_picked(&hero.id) && !draft.banned.contains(&hero.id)).collect();
    let mut banned = None;
    let free = draft.at(draft.seat).is_none();
    ui.horizontal(|ui| {
        ui.label(theme::title(tr!(HeroesThatFit), 16.0));
        ui.label(dim(match (fitting.is_empty(), free) {
            (true, _) => tr!(NoFreeHero),
            (false, true) if draft.seat.enemy => tr!(GoesToEnemy),
            (false, true) => tr!(GoesToTeam),
            (false, false) => tr!(TeamsFull),
        }).small());
        if let Some(first) = fitting.first() {
            let ban = ui.add(egui::Button::new(tr!(BanHero, hero = first.name)).small());
            if ban.on_hover_text(tr!(BanHeroHint, hero = first.name)).clicked() {
                banned = Some(first.id.clone());
            }
        }
    });
    if let Some(hero) = banned {
        draft.ban(&hero);
        return;
    }
    if !free {
        return;
    }
    let mut chosen = None;
    if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        chosen = fitting.first().map(|hero| hero.id.clone());
    }
    egui::ScrollArea::vertical().id_salt("fitting").auto_shrink(false).show(ui, |ui| {
        tiles::flow(ui, fitting.len(), LOOK, |ui, index, look| {
            let hero = fitting[index];
            let tile = Tile { picture: hero.picture.as_deref(), name: &hero.name, note: None, badge: None, band: None, picked: false, faint: false, warning: None, detail: None, mark: None };
            let response = tiles::tile(ui, pictures, look, &tile);
            response.context_menu(|ui| {
                if ui.button(tr!(Ban)).clicked() {
                    banned = Some(hero.id.clone());
                }
            });
            if response.clicked() {
                chosen = Some(hero.id.clone());
            }
        });
    });
    if let Some(hero) = banned {
        draft.ban(&hero);
    } else if let Some(hero) = chosen {
        draft.seat_hero(&hero);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Whose {
    Everyone,
    Mine,
    AllChosen,
    NoneOfMine,
}

impl Whose {
    fn of(theirs: bool, mine: usize, listed: usize, all_heroes: bool) -> Whose {
        match (theirs && mine > 0, all_heroes, listed) {
            (false, _, _) => Whose::Everyone,
            (true, true, _) => Whose::AllChosen,
            (true, false, 0) => Whose::NoneOfMine,
            (true, false, _) => Whose::Mine,
        }
    }
}

fn is_theirs(draft: &Draft, seat: Seat, role: Option<Role>) -> bool {
    !seat.enemy && (role.is_none() || draft.own_seat(role) == Some(seat))
}

fn list_for<'c>(catalog: &'c Catalog, draft: &Draft, numbers: &Numbers, choices: &Choices, seat: Seat) -> (Vec<(Suggestion, &'c Hero)>, Whose) {
    let seat_role = draft.role(seat);
    let all = match choices.style {
        Style::Team => {
            let (beside, against) = draft.around(seat);
            numbers.suggest(choices.rank, Some(seat_role), &beside, &against)
        }
        Style::Lane => {
            let (partner, opponents) = draft.in_the_lane_of(seat);
            numbers.suggest_in_lane(choices.rank, seat_role, &partner, &opponents, &draft.picked())
        }
    };
    let there = draft.at(seat);
    let listed: Vec<(Suggestion, &Hero)> = all
        .into_iter()
        .filter(|suggestion| !draft.banned.contains(&suggestion.hero) && Some(suggestion.hero.as_str()) != there)
        .filter_map(|suggestion| catalog.any_hero(&suggestion.hero).map(|hero| (suggestion, hero)))
        .collect();
    let of_mine = listed.iter().filter(|(suggestion, _)| choices.mine.contains(&suggestion.hero)).count();
    let whose = Whose::of(is_theirs(draft, seat, choices.role), choices.mine.len(), of_mine, choices.all_heroes);
    match whose {
        Whose::Mine => (listed.into_iter().filter(|(suggestion, _)| choices.mine.contains(&suggestion.hero)).collect(), whose),
        _ => (listed, whose),
    }
}

fn suggestions(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, draft: &mut Draft, numbers: &Numbers, choices: &Choices, answer: &mut Answer) {
    let (rank, role, style) = (choices.rank, choices.role, choices.style);
    let seat = draft.list_seat(role);
    let seat_role = draft.role(seat);
    let (shown, whose) = list_for(catalog, draft, numbers, choices, seat);
    let this_week = draft.nothing_picked() && style == Style::Team;
    let no_pool = whose == Whose::Everyone && choices.mine.is_empty() && is_theirs(draft, seat, role);

    let roomy = ui.available_width() > 760.0;
    ui.horizontal(|ui| {
        ui.label(theme::title(list_title(seat, seat_role, draft.own_seat(role) == Some(seat), this_week), 16.0));
        if whose != Whose::Everyone {
            ui.add_space(6.0);
            let hints = [tr!(OnlyMine), tr!(EveryInRole)];
            let on = usize::from(whose != Whose::Mine);
            if let Some(index) = halves(ui, "draft whose", 26.0, [tr!(MyHeroes), tr!(AllHeroesHalf)], hints, on) {
                answer.all_heroes = Some(index == 1);
            }
        }
        if no_pool && roomy {
            ui.add_space(4.0);
            answer.open_pool = pool_link(ui, tr!(PoolInSettings));
        }
        if roomy && !no_pool {
            let hint = match (style, seat.enemy, this_week && numbers.has_weeks()) {
                (_, _, true) => tr!(HintWeeks),
                (Style::Team, true, false) => tr!(HintEnemyTeam),
                (Style::Team, false, false) => tr!(HintTeam),
                (Style::Lane, true, false) => tr!(HintEnemyLane),
                (Style::Lane, false, false) => tr!(HintLane),
            };
            ui.label(dim(hint).small());
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let credit = ui.add(egui::Button::new(dim(credit_of(&numbers.source, &numbers.patch, numbers.age_in_days(choices.now))).small()).frame(false));
            answer.open_source = credit.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr!(Opens, address = SOURCE_PAGE)).clicked();
        });
    });
    if no_pool && !roomy {
        answer.open_pool = pool_link(ui, tr!(PoolInSettings));
    }
    if whose == Whose::NoneOfMine {
        ui.label(dim(tr!(NoneOfMine, role = words::as_played(seat_role))).small());
    }
    if shown.is_empty() {
        ui.label(dim(tr!(NoneInRole)));
        return;
    }

    let name_of = |id: &str| catalog.any_hero(id).map_or_else(|| id.to_string(), |hero| hero.name.clone());
    let enemy_picked = !draft.heroes(!seat.enemy).is_empty();
    let (mut picks, mut bans) = (None, None);
    egui::ScrollArea::vertical().id_salt("suggestions").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        for (place, (suggestion, hero)) in shown.iter().enumerate() {
            let trend = numbers.trend(rank, seat_role, &suggestion.hero).filter(|_| style == Style::Team);
            let suggest = !seat.enemy && place < SUGGESTED;
            match row(ui, pictures, suggestion, hero, (seat_role, style), trend, suggest, &name_of) {
                Some(Clicked::Row) => picks = Some(suggestion.hero.clone()),
                Some(Clicked::Cross) => bans = Some(suggestion.hero.clone()),
                Some(Clicked::Suggest) => answer.suggest = Some(suggest_line(&hero.name, seat_role, enemy_picked)),
                None => {}
            }
        }
    });
    if let Some(hero) = bans {
        draft.ban(&hero);
    } else if let Some(hero) = picks {
        draft.seat_hero_at(seat, &hero);
    }
}

fn pool_link(ui: &mut Ui, words: &str) -> bool {
    let link = ui.add(egui::Button::new(egui::RichText::new(words).small().underline().color(theme::text_dim())).frame(false));
    link.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr!(PoolInSettingsHint)).clicked()
}

fn nominate(catalog: &Catalog, draft: &Draft, numbers: &Numbers, choices: &Choices, own: &[String]) -> Vec<Suggestion> {
    if !draft.nothing_picked() {
        return Vec::new();
    }
    let all: Vec<&str> = choices.mine.iter().map(String::as_str).collect();
    let in_role: Vec<&str> = match choices.role {
        Some(role) => all.iter().copied().filter(|hero| numbers.is_played_in(choices.rank, role, hero)).collect(),
        None => Vec::new(),
    };
    let mine = if in_role.is_empty() { all } else { in_role };
    let left_out: Vec<&str> = draft.banned.iter().chain(own).map(String::as_str).collect();
    let all = numbers.to_ban(choices.rank, choices.role, &mine, &left_out);
    all.into_iter().filter(|suggestion| !choices.mine.contains(&suggestion.hero) && catalog.any_hero(&suggestion.hero).is_some()).take(NOMINEES).collect()
}

fn bans(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, draft: &mut Draft, nominees: &[Suggestion], role: Option<Role>) {
    if nominees.is_empty() && draft.banned.is_empty() {
        return;
    }
    let name_of = |id: &str| catalog.any_hero(id).map_or_else(|| id.to_string(), |hero| hero.name.clone());
    let (mut ban, mut unban) = (None, None);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
        if !nominees.is_empty() {
            ui.label(dim(tr!(NominateBan)).small());
            for nominee in nominees {
                let picture = catalog.any_hero(&nominee.hero).and_then(|hero| hero.picture.as_deref());
                let why = ban_words(nominee, role, &name_of);
                if chip(ui, pictures, picture, &name_of(&nominee.hero), false).on_hover_text(tr!(ClickToBan, why)).clicked() {
                    ban = Some(nominee.hero.clone());
                }
            }
        }
        if !draft.banned.is_empty() {
            if !nominees.is_empty() {
                ui.add_space(10.0);
            }
            ui.label(dim(tr!(BannedLabel)).small());
            for hero in &draft.banned {
                let picture = catalog.any_hero(hero).and_then(|hero| hero.picture.as_deref());
                if chip(ui, pictures, picture, &name_of(hero), true).on_hover_text(tr!(BannedHint)).clicked() {
                    unban = Some(hero.clone());
                }
            }
        }
    });
    if let Some(hero) = ban {
        draft.ban(&hero);
    }
    if let Some(hero) = unban {
        draft.banned.remove(&hero);
    }
}

fn ban_words(nominee: &Suggestion, role: Option<Role>, name_of: &dyn Fn(&str) -> String) -> String {
    let words = in_words(nominee, None, Style::Team, name_of, usize::MAX);
    match (nominee.reasons.is_empty(), role) {
        (true, Some(role)) => {
            let matches = nominee.matches as f32;
            let own = (nominee.win_rate - 50.0) * matches / (matches + 400.0);
            tr!(AgainstRoleRule, words, role = words::as_played(role), points = signed(nominee.points - own))
        }
        _ => words,
    }
}

pub(super) fn chip(ui: &mut Ui, pictures: &mut Pictures, picture: Option<&str>, name: &str, banned: bool) -> egui::Response {
    const PORTRAIT: egui::Vec2 = vec2(36.0, 20.0);
    let font = FontId::proportional(theme::SMALL + 0.5);
    let ink = if banned { theme::text_dim() } else { theme::text() };
    let words = ui.painter().layout_no_wrap(name.to_string(), font, ink);
    let (rect, response) = ui.allocate_exact_size(vec2(4.0 + PORTRAIT.x + 7.0 + words.size().x + 10.0, 28.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered(), 0.12);
        let painter = ui.painter();
        let round = CornerRadius::same(8);
        painter.rect_filled(rect, round, theme::card().lerp_to_gamma(theme::card_hover(), lit));
        painter.rect_stroke(rect, round, Stroke::new(1.0, if banned { theme::bad().gamma_multiply(0.7) } else { theme::line() }), StrokeKind::Inside);
        let portrait = Rect::from_min_size(pos2(rect.left() + 4.0, rect.center().y - PORTRAIT.y / 2.0), PORTRAIT);
        tiles::fill_with(painter, pictures, picture, portrait, 5, 1.0);
        let left = portrait.right() + 7.0;
        let width = words.size().x;
        painter.galley(pos2(left, rect.center().y - words.size().y / 2.0), words, ink);
        if banned {
            painter.rect_filled(portrait, CornerRadius::same(5), Color32::from_black_alpha(120));
            painter.line_segment([pos2(portrait.left() + 2.0, rect.center().y), pos2(left + width, rect.center().y)], Stroke::new(1.5, theme::bad()));
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn own_picks(catalog: &Catalog, draft: &Draft, numbers: &Numbers, choices: &Choices) -> Vec<String> {
    match draft.own_seat(choices.role).map(|seat| (seat, draft.at(seat))) {
        Some((_, Some(hero))) => vec![hero.to_string()],
        Some((seat, None)) => list_for(catalog, draft, numbers, choices, seat).0.into_iter().take(SUGGESTED).map(|(suggestion, _)| suggestion.hero).collect(),
        None => Vec::new(),
    }
}

fn suggest_line(hero: &str, role: Role, enemy_picked: bool) -> String {
    match enemy_picked {
        true => tr!(SuggestAgainst, hero),
        false => tr!(SuggestInRole, hero, role = words::as_played(role)),
    }
}

fn list_title(seat: Seat, role: Role, yours: bool, this_week: bool) -> String {
    let who = words::player(role);
    match (this_week, seat.enemy, yours) {
        (true, true, _) => tr!(BestThisWeekEnemy, who),
        (true, false, true) => tr!(BestThisWeekYou, role = words::as_played(role)),
        (true, false, false) => tr!(BestThisWeekTeam, who),
        (false, true, _) => tr!(HeroesForEnemy, who),
        (false, false, true) => tr!(HeroesForYou, role = words::as_played(role)),
        (false, false, false) => tr!(HeroesForTeam, who),
    }
}

enum Clicked {
    Row,
    Cross,
    Suggest,
}

const SUGGEST_IN_WORDS: f32 = 720.0;

#[allow(clippy::too_many_arguments)]
fn row(ui: &mut Ui, pictures: &mut Pictures, suggestion: &Suggestion, hero: &Hero, (role, style): (Role, Style), trend: Option<Trend>, suggest: bool, name_of: &dyn Fn(&str) -> String) -> Option<Clicked> {
    const PORTRAIT: egui::Vec2 = vec2(78.0, 44.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width() - 14.0, ROW), Sense::click());
    let cross = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(28.0, 28.0));
    let hide = ui.interact(cross, response.id.with("hide"), Sense::click());
    let color = match suggestion.points {
        points if points >= WORTH_NAMING => theme::good(),
        points if points <= -WORTH_NAMING => theme::bad(),
        _ => theme::text_dim(),
    };
    let number = ui.painter().layout_no_wrap(format!("{}%", signed(suggestion.points)), theme::heavy(16.0), color);
    let number_left = cross.left() - 12.0 - number.size().x;
    let widest = ui.painter().layout_no_wrap("+00.0%".to_string(), theme::heavy(16.0), color).size().x;
    let number_slot = number_left.min(cross.left() - 12.0 - widest);
    let worded = rect.width() >= SUGGEST_IN_WORDS;
    let label = worded.then(|| ui.painter().layout_no_wrap(tr!(SuggestToTeam).to_string(), FontId::proportional(theme::SMALL + 0.5), theme::text()));
    let tell = suggest.then(|| {
        let size = label.as_ref().map_or(vec2(30.0, 28.0), |label| vec2(label.size().x + 22.0, 28.0));
        let area = Rect::from_min_size(pos2(number_slot - 14.0 - size.x, rect.center().y - size.y / 2.0), size);
        (area, ui.interact(area, response.id.with("suggest"), Sense::click()))
    });
    let tell_hovered = tell.as_ref().is_some_and(|(_, button)| button.hovered());
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered() || hide.hovered() || tell_hovered, 0.12);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(theme::ROUND), theme::card().lerp_to_gamma(theme::card_hover(), lit));
        let portrait = Rect::from_min_size(pos2(rect.left() + 7.0, rect.center().y - PORTRAIT.y / 2.0), PORTRAIT);
        tiles::fill_with(painter, pictures, hero.picture.as_deref(), portrait, 8, 1.0);

        let ink = if hide.hovered() { theme::text() } else { theme::text_dim() };
        painter.text(cross.center(), Align2::CENTER_CENTER, "×", FontId::proportional(20.0), ink);
        painter.galley(pos2(number_left, rect.center().y - number.size().y / 2.0), number, color);
        let mut words_end = number_left;
        if let Some((area, _)) = &tell {
            words_end = area.left();
            let fill = if tell_hovered { theme::accent().gamma_multiply(if theme::is_day() { 0.18 } else { 0.26 }) } else { theme::text().gamma_multiply(0.06) };
            let edge = if tell_hovered { theme::accent() } else { theme::line() };
            painter.rect_filled(*area, CornerRadius::same(7), fill);
            painter.rect_stroke(*area, CornerRadius::same(7), Stroke::new(1.0, edge), StrokeKind::Inside);
            let ink = if tell_hovered { theme::accent_text() } else { theme::text() };
            match label {
                Some(label) => painter.galley(pos2(area.center().x - label.size().x / 2.0, area.center().y - label.size().y / 2.0), label, ink),
                None => speech_bubble(painter, area.center(), ink),
            }
        }

        let left = portrait.right() + 12.0;
        let room = (words_end - 14.0 - left).max(40.0);
        let mut name = LayoutJob::simple(hero.name.clone(), theme::heavy(15.0), theme::text(), room);
        name.wrap.max_rows = 1;
        name.wrap.overflow_character = Some('…');
        let name = painter.layout_job(name);
        let name_right = left + name.size().x;
        painter.galley(pos2(left, rect.top() + 9.0), name, theme::text());
        if let Some((words, color, up)) = trend.and_then(trend_mark) {
            let said = painter.layout_no_wrap(words.to_string(), FontId::proportional(theme::SMALL), color);
            let at = pos2(name_right + 10.0, rect.top() + 9.0 + 9.5);
            if at.x + 12.0 + said.size().x <= words_end - 14.0 {
                let tip = if up { [at + vec2(0.0, -4.5), at + vec2(4.5, 3.0), at + vec2(-4.5, 3.0)] } else { [at + vec2(0.0, 4.5), at + vec2(4.5, -3.0), at + vec2(-4.5, -3.0)] };
                painter.add(egui::Shape::convex_polygon(tip.to_vec(), color, Stroke::NONE));
                painter.galley(pos2(at.x + 8.0, at.y - said.size().y / 2.0), said, color);
            }
        }
        let mut words = painted(&in_words(suggestion, Some(role), style, name_of, NAMED_IN_A_ROW), FontId::proportional(theme::SMALL), theme::text_dim(), room);
        words.wrap.max_rows = 1;
        words.wrap.overflow_character = Some('…');
        painter.galley(pos2(left, rect.top() + 32.0), painter.layout_job(words), theme::text_dim());
    }
    if hide.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr!(BannedCrossHint)).clicked() {
        return Some(Clicked::Cross);
    }
    if let Some((_, button)) = tell {
        let hint = if worded { tr!(SuggestHint) } else { tr!(SuggestHintShort) };
        if button.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).clicked() {
            return Some(Clicked::Suggest);
        }
    }
    let response = response.on_hover_ui(|ui| {
        ui.set_max_width(420.0);
        ui.label(theme::title(&hero.name, 16.0));
        ui.label(painted(&in_words(suggestion, Some(role), style, name_of, usize::MAX), FontId::proportional(theme::BODY), theme::text(), 420.0));
        if let Some(trend) = trend {
            ui.label(painted(&trend_words(trend, role), FontId::proportional(theme::BODY), theme::text(), 420.0));
        }
        ui.label(dim(tr!(ClickToPut)).small());
    });
    response.on_hover_cursor(CursorIcon::PointingHand).clicked().then_some(Clicked::Row)
}

fn speech_bubble(painter: &egui::Painter, center: egui::Pos2, ink: Color32) {
    let body = Rect::from_center_size(center + vec2(0.0, -1.5), vec2(15.0, 10.0));
    painter.rect_stroke(body, CornerRadius::same(3), Stroke::new(1.4, ink), StrokeKind::Middle);
    let tail = [pos2(body.left() + 3.5, body.bottom()), pos2(body.left() + 3.0, body.bottom() + 4.5), pos2(body.left() + 7.5, body.bottom())];
    painter.add(egui::Shape::line(tail.to_vec(), Stroke::new(1.4, ink)));
    for dot in [-4.0, 0.0, 4.0] {
        painter.circle_filled(body.center() + vec2(dot, 0.0), 1.1, ink);
    }
}

fn trend_mark(trend: Trend) -> Option<(&'static str, Color32, bool)> {
    match trend.way {
        Way::Rising => Some((tr!(Rising), theme::good(), true)),
        Way::Falling => Some((tr!(Falling), theme::bad(), false)),
        Way::Level => None,
    }
}

fn trend_words(trend: Trend, role: Role) -> String {
    let (now, before, role) = (number(f64::from(trend.now), 1), number(f64::from(trend.before), 1), words::as_played(role));
    match trend.way {
        Way::Rising => tr!(TrendRising, now, role, before),
        Way::Falling => tr!(TrendFalling, now, role, before),
        Way::Level => tr!(TrendLevel, now, role, before),
    }
}

fn in_words(suggestion: &Suggestion, role: Option<Role>, style: Style, name_of: &dyn Fn(&str) -> String, most: usize) -> String {
    let least = if most == usize::MAX { 0.0 } else { WORTH_NAMING };
    let named = |beside: bool, good: bool| -> Option<String> {
        let heroes: Vec<String> = suggestion
            .reasons
            .iter()
            .filter(|reason| reason.beside == beside && (reason.points > 0.0) == good && reason.points.abs() >= least)
            .take(most)
            .map(|reason| tr!(HeroPoints, hero = name_of(&reason.hero), points = signed(reason.points)))
            .collect();
        (!heroes.is_empty()).then(|| heroes.join(", "))
    };
    let starts = match style {
        Style::Team => [T::GoodAgainst, T::WeakAgainst, T::GoesWellWith, T::NotGoodWith],
        Style::Lane => [T::StrongInLane, T::WeakInLane, T::LanesWell, T::LanesBadly],
    };
    let mut sentences = Vec::new();
    for ((beside, good), start) in [(false, true), (false, false), (true, true), (true, false)].into_iter().zip(starts) {
        sentences.extend(named(beside, good).map(|heroes| lang::fill(lang::t(start), &[("heroes", &heroes)])));
    }
    let share = number(f64::from(suggestion.win_rate), 1);
    sentences.push(match (style, role) {
        (Style::Team, None) => tr!(WinsMatches, share),
        (Style::Team, Some(role)) => tr!(WinsMatchesAs, share, role = words::as_played(role)),
        (Style::Lane, None) => tr!(WinsLanes, share),
        (Style::Lane, Some(role)) => tr!(WinsLanesAs, share, role = words::as_played(role)),
    });
    sentences.join(" ")
}

pub(super) fn painted(words: &str, font: FontId, plain: Color32, room: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = room;
    let mut add = |text: &str, color: Color32| job.append(text, 0.0, egui::TextFormat::simple(font.clone(), color));
    for (place, word) in words.split(' ').enumerate() {
        if place > 0 {
            add(" ", plain);
        }
        let number = word.trim_end_matches([',', '.']);
        let read = |text: &str| text.replace(',', ".").parse::<f32>().ok();
        let signed = number.chars().next().filter(|sign| "+-".contains(*sign) && read(&number[1..]).is_some()).map(|sign| sign == '+');
        let share = number.strip_suffix('%').and_then(read).filter(|share| *share != 50.0).map(|share| share > 50.0);
        match signed.or(share) {
            Some(good) => {
                add(number, if good { theme::good() } else { theme::bad() });
                add(&word[number.len()..], plain);
            }
            None => add(word, plain),
        }
    }
    job
}

pub(super) fn credit_of(source: &str, patch: &str, days: u64) -> String {
    let age = match days {
        0 => tr!(MadeToday).to_string(),
        days => tr!(DaysOld, count = days),
    };
    let source = if source.trim().is_empty() { "STRATZ" } else { source.trim() };
    match patch.trim() {
        "" => tr!(NumbersFrom, source, age),
        patch => tr!(NumbersFromPatch, source, patch, age),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    Again,
    CopyDetails,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fetched {
    Numbers,
    Builds,
    Lanes,
}

pub(super) fn waiting(ui: &mut Ui, fetching: bool, failure: Option<&Failure>, what: Fetched, still: &str) -> Option<Asked> {
    let (getting, not_here, failed) = match what {
        Fetched::Numbers => (tr!(GettingNumbers), tr!(NumbersNotHere), tr!(NumbersFailed)),
        Fetched::Builds => (tr!(GettingBuilds), tr!(BuildsNotHere), tr!(BuildsFailed)),
        Fetched::Lanes => (tr!(GettingLanes), tr!(LanesNotHere), tr!(LanesFailed)),
    };
    let mut asked = None;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.2).min(60.0));
        if fetching {
            ui.spinner();
            ui.label(dim(getting));
            return;
        }
        if failure.is_none() {
            ui.label(dim(not_here));
            return;
        }
        ui.label(theme::title(failed, 18.0));
        ui.add_space(2.0);
        ui.label(dim(format!("{} {still}", tr!(ComeFromInternet)).trim_end()));
        ui.add_space(8.0);
        if ui.button(tr!(TryAgain)).clicked() {
            asked = Some(Asked::Again);
        }
        if ui.add(egui::Button::new(dim(tr!(CopyDetails)).small()).frame(false)).on_hover_text(tr!(CopyDetailsHint)).clicked() {
            asked = Some(Asked::CopyDetails);
        }
    });
    asked
}

#[cfg(test)]
mod tests {
    use dme_core::draft::Reason;

    use super::*;

    const ENEMY: fn(usize) -> Seat = |place| Seat { enemy: true, place };
    const TEAM: fn(usize) -> Seat = |place| Seat { enemy: false, place };

    #[test]
    fn heroes_that_are_typed_for_go_in_the_order_of_a_ranked_draft() {
        let mut draft = Draft::default();
        assert!(draft.is_empty());
        assert_eq!(draft.seat, ENEMY(0));
        draft.search = "ax".to_string();
        draft.seat_hero("axe");
        assert_eq!((draft.heroes(true), draft.seat), (vec!["axe"], ENEMY(1)));
        assert!(draft.search.is_empty(), "what was typed has done its part");
        assert!(draft.is_picked("axe") && !draft.is_picked("lina") && !draft.is_empty());

        let mut order = Vec::new();
        for hero in ["b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            order.push(draft.seat);
            draft.seat_hero(hero);
        }
        assert_eq!(order, [ENEMY(1), TEAM(0), TEAM(1), ENEMY(2), ENEMY(3), TEAM(2), TEAM(3), ENEMY(4), TEAM(4)]);
        assert_eq!(draft.heroes(true), ["axe", "b", "e", "f", "i"]);
        assert_eq!(draft.heroes(false), ["c", "d", "g", "h", "j"]);
        assert_eq!(draft.seat, TEAM(4), "every place is taken: it stays where it is");
    }

    #[test]
    fn a_team_that_is_begun_is_gone_on_with_before_the_other() {
        let mut draft = Draft { seat: TEAM(0), ..Draft::default() };
        draft.seat_hero("axe");
        assert_eq!(draft.seat, TEAM(1), "the other first pick of the same team");
        draft.seat_hero("lina");
        assert_eq!(draft.seat, ENEMY(0), "then the enemy's first picks");
        draft.seat = TEAM(4);
        draft.seat_hero("sven");
        assert_eq!(draft.seat, ENEMY(4));
        draft.seat_hero("zeus");
        assert_eq!(draft.seat, ENEMY(0));
    }

    #[test]
    fn a_hero_has_one_place() {
        let mut draft = Draft::default();
        draft.seat_hero("axe");
        draft.seat = TEAM(2);
        draft.seat_hero("axe");
        assert_eq!((draft.heroes(true), draft.heroes(false)), (vec![], vec!["axe"]));
        assert_eq!(draft.at(TEAM(2)), Some("axe"));
        assert_eq!(draft.seat, TEAM(3), "the other place of that phase is next");
    }

    #[test]
    fn a_click_on_a_hero_takes_it_off_and_its_place_is_filled_next() {
        let mut draft = Draft::default();
        for hero in ["axe", "lina"] {
            draft.seat_hero(hero);
        }
        draft.empty(ENEMY(0));
        assert_eq!((draft.heroes(true), draft.seat), (vec!["lina"], ENEMY(0)));
        draft.seat_hero("sven");
        assert_eq!(draft.heroes(true), ["sven", "lina"]);
        assert_eq!(draft.seat, TEAM(0));
    }

    #[test]
    fn the_first_picks_are_taken_for_supports_the_next_for_cores_and_the_last_for_the_mid() {
        let mut draft = Draft::default();
        let roles = |draft: &Draft, enemy: bool| [0, 1, 2, 3, 4].map(|place| draft.role(Seat { enemy, place }));
        let by_place = [Role::SoftSupport, Role::HardSupport, Role::Carry, Role::Offlane, Role::Mid];
        assert_eq!((roles(&draft, false), roles(&draft, true)), (by_place, by_place));
        assert_eq!(draft.own_seat(Some(Role::Mid)), Some(TEAM(4)));
        assert_eq!(draft.own_seat(Some(Role::Carry)), Some(TEAM(2)));
        assert_eq!(draft.own_seat(None), None, "with no role chosen, no place is the player's");
        assert_eq!(draft.own_hero(Some(Role::Mid)), None);
        draft.seat = TEAM(4);
        draft.seat_hero("lina");
        assert_eq!(draft.own_hero(Some(Role::Mid)), Some(("lina", Role::Mid)));
        assert_eq!((draft.own_hero(Some(Role::Carry)), draft.own_hero(None)), (None, None));
        draft.empty(TEAM(4));
        draft.seat = ENEMY(0);

        draft.set_role(ENEMY(0), Role::Mid);
        assert_eq!(roles(&draft, true), [Role::Mid, Role::HardSupport, Role::Carry, Role::Offlane, Role::SoftSupport]);
        assert_eq!(roles(&draft, false), by_place, "the other team is as it was");
        assert!(!draft.is_empty(), "there is something to start over from");
        draft.set_role(ENEMY(0), Role::Mid);
        assert_eq!(roles(&draft, true)[0], Role::Mid, "choosing what it has changes nothing");

        draft.set_role(TEAM(1), Role::Mid);
        assert_eq!(draft.own_seat(Some(Role::Mid)), Some(TEAM(1)));
        assert_eq!(draft.own_seat(Some(Role::HardSupport)), Some(TEAM(4)));
    }

    #[test]
    fn the_list_is_for_the_place_that_is_being_filled() {
        let mut draft = Draft::default();
        for hero in ["lion", "cm", "ogre", "jakiro"] {
            draft.seat_hero(hero);
        }
        assert_eq!((draft.seat, draft.role(draft.seat)), (ENEMY(2), Role::Carry));
        assert_eq!(draft.around(draft.seat), (vec!["lion", "cm"], vec!["ogre", "jakiro"]));
        assert_eq!(list_title(draft.seat, Role::Carry, false, false), "Heroes for the enemy's carry");

        draft.seat_hero("pa");
        draft.seat_hero("axe");
        assert_eq!(draft.heroes(true), ["lion", "cm", "pa", "axe"]);
        assert_eq!((draft.seat, draft.role(draft.seat)), (TEAM(2), Role::Carry));
        assert_eq!(draft.around(draft.seat), (vec!["ogre", "jakiro"], vec!["lion", "cm", "pa", "axe"]));
        assert_eq!(list_title(draft.seat, Role::Carry, false, false), "Heroes for your team's carry");
        assert_eq!(list_title(draft.seat, Role::Carry, true, false), "Heroes for you, as carry");
        draft.seat_hero("sven");
        draft.seat_hero("mars");
        assert_eq!(draft.heroes(false), ["ogre", "jakiro", "sven", "mars"]);
        assert_eq!((draft.seat, draft.role(draft.seat)), (ENEMY(4), Role::Mid), "the last picks, the enemy's first");
        assert_eq!(list_title(TEAM(4), Role::Mid, true, false), "Heroes for you, in mid");

        assert_eq!(draft.against("sven"), ["lion", "cm", "pa", "axe"]);
        assert_eq!(draft.against("pa"), ["ogre", "jakiro", "sven", "mars"]);
        assert_eq!(draft.against("zeus"), ["lion", "cm", "pa", "axe"]);

        draft.seat = TEAM(3);
        assert_eq!(draft.around(draft.seat).0, ["ogre", "jakiro", "sven"]);

        draft.start_over();
        assert_eq!(draft, Draft::default());
    }

    #[test]
    fn in_a_lane_the_list_looks_at_the_heroes_of_that_lane_alone() {
        let mut draft = Draft::default();
        for hero in ["lion", "cm", "ogre", "jakiro", "pa", "axe"] {
            draft.seat_hero(hero);
        }
        assert_eq!(draft.picked(), ["ogre", "jakiro", "lion", "cm", "pa", "axe"]);
        assert_eq!((draft.seat, draft.role(draft.seat)), (TEAM(2), Role::Carry));
        assert_eq!(draft.in_the_lane_of(draft.seat), (vec!["jakiro"], vec![("lion", Role::SoftSupport), ("axe", Role::Offlane)]));
        draft.seat = TEAM(3);
        assert_eq!(draft.in_the_lane_of(draft.seat), (vec!["ogre"], vec![("cm", Role::HardSupport), ("pa", Role::Carry)]));
        draft.seat = TEAM(4);
        assert_eq!(draft.in_the_lane_of(draft.seat), (vec![], vec![]));
        draft.set_role(ENEMY(3), Role::Mid);
        assert_eq!(draft.in_the_lane_of(draft.seat), (vec![], vec![("axe", Role::Mid)]));
        draft.seat = ENEMY(4);
        assert_eq!(draft.role(draft.seat), Role::Offlane);
        assert_eq!(draft.in_the_lane_of(draft.seat), (vec!["lion"], vec![("jakiro", Role::HardSupport)]));
    }

    #[test]
    fn in_a_lane_a_suggestion_speaks_of_lanes() {
        let reason = |hero: &str, beside: bool, points: f32| Reason { hero: hero.to_string(), beside, points };
        let suggestion = Suggestion { hero: "viper".to_string(), points: 4.0, win_rate: 55.04, matches: 9000, reasons: vec![reason("axe", false, 3.0), reason("lion", false, -1.0), reason("cm", true, 1.5)] };
        let name_of = |id: &str| id.to_uppercase();
        assert_eq!(
            in_words(&suggestion, Some(Role::Carry), Style::Lane, &name_of, 2),
            "Strong in lane against AXE +3.0. Weak in lane against LION -1.0. Lanes well with CM +1.5. Wins 55.0% of its lanes as carry."
        );
    }

    #[test]
    fn a_suggestion_says_what_speaks_for_it_and_against_it() {
        let reason = |hero: &str, beside: bool, points: f32| Reason { hero: hero.to_string(), beside, points };
        let reasons = vec![reason("axe", false, 3.0), reason("cm", true, 2.5), reason("pudge", false, -2.0), reason("lina", false, 1.2), reason("zeus", false, 0.6), reason("sven", true, -0.2)];
        let suggestion = Suggestion { hero: "viper".to_string(), points: 6.1, win_rate: 52.34, matches: 9000, reasons };
        let name_of = |id: &str| id.to_uppercase();

        assert_eq!(
            in_words(&suggestion, Some(Role::Mid), Style::Team, &name_of, 2),
            "Good against AXE +3.0, LINA +1.2. Weak against PUDGE -2.0. Goes well with CM +2.5. Wins 52.3% of its matches in mid.",
            "two of each at most, and nothing that hardly matters"
        );
        assert_eq!(
            in_words(&suggestion, None, Style::Team, &name_of, usize::MAX),
            "Good against AXE +3.0, LINA +1.2, ZEUS +0.6. Weak against PUDGE -2.0. Goes well with CM +2.5. Not good with SVEN -0.2. Wins 52.3% of its matches.",
            "everything, where there is room"
        );
        let alone = Suggestion { reasons: Vec::new(), ..suggestion };
        assert_eq!(in_words(&alone, Some(Role::Carry), Style::Team, &name_of, 2), "Wins 52.3% of its matches as carry.");
    }

    #[test]
    fn what_a_hero_adds_is_green_and_what_it_takes_is_red() {
        let plain = Color32::from_gray(1);
        let job = painted("Good against Anti-Mage +3.0, Lina +1.2. Weak against Pudge -2.0. Wins 52.3% of its matches.", FontId::proportional(12.0), plain, 300.0);
        assert_eq!(job.text, "Good against Anti-Mage +3.0, Lina +1.2. Weak against Pudge -2.0. Wins 52.3% of its matches.", "the words are as they were");
        let colored: Vec<(&str, Color32)> = job.sections.iter().filter(|section| section.format.color != plain).map(|section| (&job.text[section.byte_range.start.0..section.byte_range.end.0], section.format.color)).collect();
        assert_eq!(colored, [("+3.0", theme::good()), ("+1.2", theme::good()), ("-2.0", theme::bad()), ("52.3%", theme::good())], "the numbers alone, without what follows them");
        assert_eq!(job.wrap.max_width, 300.0);

        let colors = |words: &str| -> Vec<Color32> { painted(words, FontId::proportional(12.0), plain, 300.0).sections.iter().map(|section| section.format.color).filter(|color| *color != plain).collect() };
        assert_eq!(colors("Wins 47.1% of its matches as carry."), [theme::bad()]);
        assert!(colors("Wins 50.0% of its matches.").is_empty());
    }

    #[test]
    fn a_whole_draft_is_said_to_be_better_worse_or_even() {
        assert_eq!(verdict_words(57.0).0, "Your draft is better");
        assert_eq!(verdict_words(44.0).0, "The enemy's draft is better");
        for even in [49.2, 50.0, 50.9] {
            assert_eq!(verdict_words(even).0, "The two drafts are even");
        }
        assert_eq!((verdict_words(51.0).0, verdict_words(49.0).0), ("Your draft is better", "The enemy's draft is better"));

        let mut draft = Draft::default();
        for hero in ["a", "b", "c", "d", "e", "f", "g", "h", "i"] {
            draft.seat_hero(hero);
        }
        assert!(!draft.is_full());
        draft.seat_hero("j");
        assert!(draft.is_full());
        assert_eq!(draft.seated(true), [("a", Role::SoftSupport), ("b", Role::HardSupport), ("e", Role::Carry), ("f", Role::Offlane), ("i", Role::Mid)]);
        draft.set_role(TEAM(0), Role::Mid);
        assert_eq!(draft.seated(false)[0], ("c", Role::Mid));
        draft.empty(TEAM(4));
        assert_eq!(draft.seated(false).len(), 4);
    }

    #[test]
    fn with_nothing_picked_the_list_is_of_the_best_heroes_this_week() {
        assert_eq!(list_title(TEAM(4), Role::Mid, true, true), "Best heroes this week for you, in mid");
        assert_eq!(list_title(TEAM(0), Role::SoftSupport, false, true), "Best heroes this week for your team's soft support");
        let trend = |now: f32, before: f32, way: Way| Trend { now, before, way };
        assert_eq!(trend_words(trend(55.04, 51.2, Way::Rising), Role::Mid), "Rising: won 55.0% in mid last week, and 51.2% the week before.");
        assert_eq!(trend_words(trend(47.0, 49.5, Way::Falling), Role::Carry), "Falling: won 47.0% as carry last week, and 49.5% the week before.");
        assert_eq!(trend_words(trend(50.5, 50.1, Way::Level), Role::Carry), "won 50.5% as carry last week, and 50.1% the week before.");
        assert!(trend_mark(trend(55.0, 51.0, Way::Rising)).is_some_and(|(words, _, up)| words == "Rising" && up));
        assert!(trend_mark(trend(47.0, 49.0, Way::Falling)).is_some_and(|(words, _, up)| words == "Falling" && !up));
        assert_eq!(trend_mark(trend(50.0, 50.0, Way::Level)), None, "a hero that is neither has no mark");
    }

    #[test]
    fn a_banned_hero_leaves_its_place_and_the_lists_until_new_draft() {
        let mut draft = Draft::default();
        assert!(draft.nothing_picked());
        draft.search = "rik".to_string();
        draft.ban("riki");
        assert!(draft.banned.contains("riki") && draft.search.is_empty());
        assert!(draft.nothing_picked() && !draft.is_empty(), "a ban is no pick, but there is something to start over from");
        assert_eq!(draft.seat, ENEMY(0), "banning goes on to no other place");

        draft.seat_hero("axe");
        assert!(!draft.nothing_picked());
        draft.ban("axe");
        assert_eq!((draft.heroes(true), draft.banned.len()), (vec![], 2));
        draft.seat_hero("riki");
        assert!(!draft.banned.contains("riki") && draft.is_picked("riki"));

        draft.by_hand = true;
        draft.start_over();
        assert_eq!(draft, Draft::default());
    }

    #[test]
    fn my_heroes_are_for_the_players_own_place_alone() {
        let mut draft = Draft::default();
        assert!(is_theirs(&draft, TEAM(4), Some(Role::Mid)) && !is_theirs(&draft, TEAM(2), Some(Role::Mid)));
        assert!(is_theirs(&draft, TEAM(2), None) && !is_theirs(&draft, ENEMY(4), None));
        draft.set_role(TEAM(2), Role::Mid);
        assert!(is_theirs(&draft, TEAM(2), Some(Role::Mid)), "the own place follows the role");

        assert_eq!(Whose::of(true, 5, 2, false), Whose::Mine);
        assert_eq!(Whose::of(true, 5, 0, false), Whose::NoneOfMine, "none in the role: all heroes, and the page says so");
        assert_eq!(Whose::of(true, 5, 2, true), Whose::AllChosen);
        assert_eq!(Whose::of(true, 0, 0, false), Whose::Everyone, "no heroes chosen: no switch");
        assert_eq!(Whose::of(false, 5, 2, false), Whose::Everyone, "the enemy's place, or a teammate's");
    }

    #[test]
    fn a_hero_suggested_to_the_team_is_good_against_the_enemy_once_the_enemy_has_a_hero() {
        assert_eq!(suggest_line("Outworld Destroyer", Role::Mid, true), "Outworld Destroyer is a good pick against the enemy");
        assert_eq!(suggest_line("Outworld Destroyer", Role::Mid, false), "Outworld Destroyer is a good pick in mid", "before that, good in its role");
        assert_eq!(suggest_line("Lion", Role::SoftSupport, false), "Lion is a good pick as soft support");
    }

    #[test]
    fn with_nothing_picked_the_list_follows_my_role() {
        let mut draft = Draft::default();
        assert_eq!(draft.seat, ENEMY(0));
        assert_eq!(draft.list_seat(Some(Role::Mid)), TEAM(4));
        assert_eq!(draft.role(draft.list_seat(Some(Role::Mid))), Role::Mid);
        assert_eq!(draft.list_seat(Some(Role::Carry)), TEAM(2));
        assert_eq!(draft.role(draft.list_seat(Some(Role::HardSupport))), Role::HardSupport);
        assert_eq!(list_title(TEAM(2), Role::Carry, true, true), "Best heroes this week for you, as carry");
        assert_eq!(draft.list_seat(None), ENEMY(0));
        assert_eq!(list_title(ENEMY(0), Role::SoftSupport, false, true), "Best heroes this week for the enemy's soft support");

        draft.seat_hero_at(draft.list_seat(Some(Role::Mid)), "lina");
        assert_eq!((draft.at(TEAM(4)), draft.seat), (Some("lina"), ENEMY(0)));
        draft.seat_hero("axe");
        assert_eq!(draft.seat, ENEMY(1));
        assert_eq!(draft.list_seat(Some(Role::Mid)), ENEMY(1));

        let mut draft = Draft { by_hand: true, ..Draft::default() };
        assert_eq!(draft.list_seat(Some(Role::Mid)), ENEMY(0));
        draft.seat_hero_at(ENEMY(0), "axe");
        assert_eq!(draft.seat, ENEMY(1));
    }

    #[test]
    fn a_hero_to_ban_says_why() {
        let name_of = |id: &str| id.to_uppercase();
        let reason = |hero: &str, points: f32| Reason { hero: hero.to_string(), beside: false, points };
        let against_mine = Suggestion { hero: "riki".to_string(), points: 4.0, win_rate: 53.0, matches: 10000, reasons: vec![reason("lina", 2.0), reason("axe", -1.0)] };
        assert_eq!(ban_words(&against_mine, Some(Role::Mid), &name_of), "Good against LINA +2.0. Weak against AXE -1.0. Wins 53.0% of its matches.");
        let own = 3.0 * 10000.0 / 10400.0;
        let against_role = Suggestion { hero: "riki".to_string(), points: own + 0.8, win_rate: 53.0, matches: 10000, reasons: Vec::new() };
        assert_eq!(ban_words(&against_role, Some(Role::Mid), &name_of), "Wins 53.0% of its matches. Against the heroes played in mid: +0.8 as a rule.");
        assert_eq!(ban_words(&against_role, None, &name_of), "Wins 53.0% of its matches.");
    }

    #[test]
    fn every_role_has_a_drawing_of_the_games_own() {
        let logos: BTreeSet<&str> = Role::ALL.into_iter().map(role_logo).collect();
        assert_eq!(logos.len(), Role::ALL.len());
        assert!(logos.iter().all(|logo| logo.starts_with("panorama/images/") && logo.ends_with(".vtex_c")));
    }

    #[test]
    fn the_lists_show_the_games_medals_and_drawings_beside_the_names() {
        let medals: BTreeSet<&str> = Rank::ALL.into_iter().map(rank_medal).collect();
        assert_eq!(medals.len(), 8);
        assert!(rank_medal(Rank::Herald).ends_with("/rank1_psd.vtex_c") && rank_medal(Rank::Immortal).ends_with("/rank8_psd.vtex_c"));
        let rank = Line::of_rank(Rank::Ancient);
        assert_eq!((rank.name, rank.drawings.as_slice(), rank.room, rank.plain), ("Ancient", [rank_medal(Rank::Ancient)].as_slice(), 1, false));

        let role = Line::of_any_role(Some(Role::Offlane));
        assert_eq!((role.name, role.drawings.as_slice(), role.plain), ("Offlane", [role_logo(Role::Offlane)].as_slice(), true));
        let any = Line::of_any_role(None);
        assert_eq!((any.name, any.drawings.len(), any.room), ("Any role", 0, 1));
    }

    #[test]
    fn the_page_says_whose_numbers_it_shows_and_how_old_they_are() {
        let rank = r#"{"played":[[9],[0],[0],[0],[0]],"won":[[4],[0],[0],[0],[0]]}"#;
        let pair = r#"{"against":[[0]],"with":[[0]]}"#;
        let (ranks, pairs) = ([rank; 8].join(","), [pair; 4].join(","));
        let file = |patch: &str| format!(r#"{{"format":1,"made":1000,"source":"STRATZ","patch":"{patch}","heroes":["axe"],"ranks":[{ranks}],"pairs":[{pairs}]}}"#);
        let numbers = Numbers::parse(file("7.41f").as_bytes()).unwrap();
        let day = 24 * 3600;
        let credit = |numbers: &Numbers, now: u64| credit_of(&numbers.source, &numbers.patch, numbers.age_in_days(now));
        assert_eq!(credit(&numbers, 1000 + 60), "Numbers from STRATZ · patch 7.41f · made today");
        assert_eq!(credit(&numbers, 1000 + day), "Numbers from STRATZ · patch 7.41f · 1 day old");
        assert_eq!(credit(&numbers, 1000 + 6 * day), "Numbers from STRATZ · patch 7.41f · 6 days old");
        assert_eq!(credit(&Numbers::parse(file("").as_bytes()).unwrap(), 1000), "Numbers from STRATZ · made today");
        assert_eq!(credit_of(" ", "7.41f", 2), "Numbers from STRATZ · patch 7.41f · 2 days old");
        assert!(SOURCE_PAGE.starts_with("https://"));
    }
}
