use std::collections::BTreeSet;

use dme_core::catalog::{Catalog, Hero};
use dme_core::draft::{Numbers, Rank, Role, Suggestion};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Layout, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::tiles::{self, Look, Tile};
use super::{dim, heroes, title_bar};
use crate::pictures::Pictures;
use crate::theme;
use crate::worker::Failure;

pub const SOURCE_PAGE: &str = "https://stratz.com";

const NOTE: &str = "Put the heroes in as they are picked: click a place, type a few letters of the name and press Enter. \
     The places are in the order of a ranked draft: each team picks two heroes, then two more, then its last. \
     The first two are taken to be the supports, the next two the carry and the offlaner, and the last the mid. Each place shows its role as a small drawing in its corner. Where a role is not right, click the drawing and choose another. \
     The list is for the place that is lit: it shows which heroes of that role win more often there, against the other team and beside their own, in matches of your rank. \
     A click on a hero in the list puts it in that place and goes on to the next, the same as typing its name. Its cross hides a hero that is banned, or that you do not play. \
     \"My role\" marks your own place with \"You\". \
     Once both teams are full, the page says which draft is better, at the top right. \
     The app does not see your game: nothing is read from Dota.";

const SEATS: usize = 5;
const PHASES: [&[usize]; 3] = [&[0, 1], &[2, 3], &[4]];
const PHASE_NAMES: [&str; 3] = ["First picks", "Second picks", "Last pick"];
const ROLES_BY_PLACE: [Role; SEATS] = [Role::SoftSupport, Role::HardSupport, Role::Carry, Role::Offlane, Role::Mid];
pub(super) const ROLES_WIDE: f32 = 172.0;
pub(super) const RANKS_WIDE: f32 = 172.0;
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
    hidden: BTreeSet<String>,
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
            hidden: BTreeSet::new(),
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
        self.team.iter().chain(&self.enemies).all(Option::is_none) && self.hidden.is_empty() && self.team_roles == ROLES_BY_PLACE && self.enemy_roles == ROLES_BY_PLACE
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
        self.hidden.remove(hero);
        let seat = self.seat;
        self.side_mut(seat.enemy)[seat.place] = Some(hero.to_string());
        self.seat = self.next_free(seat);
        self.search.clear();
    }

    fn around_the_seat(&self) -> (Vec<&str>, Vec<&str>) {
        let own = self.at(self.seat);
        let beside = self.heroes(self.seat.enemy).into_iter().filter(|hero| Some(*hero) != own).collect();
        (beside, self.heroes(!self.seat.enemy))
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
    pub now: u64,
}

#[derive(Default)]
pub struct Answer {
    pub rank: Option<Rank>,
    pub role: Option<Option<Role>>,
    pub try_again: bool,
    pub copy_details: bool,
    pub open_source: bool,
}

pub fn show(ui: &mut Ui, view: View<'_>) -> Answer {
    let View { catalog, pictures, draft, numbers, fetching, failure, rank, role, now } = view;
    let mut answer = Answer::default();
    let mut start_over = false;
    let any = !draft.is_empty();
    title_bar(ui, "Draft", Some(NOTE), Some((&mut draft.search, "Type a hero's name")), |ui| {
        start_over = ui.add_enabled(any, egui::Button::new("New draft")).on_hover_text("Take every hero off, for the next match").clicked();
    });
    if start_over {
        draft.start_over();
    }

    let chance = numbers.filter(|_| draft.is_full()).map(|numbers| numbers.chance(rank, &draft.seated(false), &draft.seated(true)));
    choices(ui, pictures, rank, role, chance, &mut answer);
    seats(ui, catalog, pictures, draft, role);
    ui.add_space(8.0);

    let wanted = draft.search.trim().to_lowercase();
    if !wanted.is_empty() {
        fitting(ui, catalog, pictures, draft, &wanted);
    } else if let Some(numbers) = numbers {
        suggestions(ui, catalog, pictures, draft, numbers, rank, role, now, &mut answer);
    } else {
        match waiting(ui, fetching, failure, "numbers", "You can still put the heroes in.") {
            Some(Asked::Again) => answer.try_again = true,
            Some(Asked::CopyDetails) => answer.copy_details = true,
            None => {}
        }
    }
    answer
}

fn choices(ui: &mut Ui, pictures: &mut Pictures, rank: Rank, role: Option<Role>, chance: Option<f32>, answer: &mut Answer) {
    let label = |ui: &mut Ui, words: &str| {
        let words = ui.painter().layout_no_wrap(words.to_string(), FontId::proportional(theme::SMALL), theme::text_dim());
        let (rect, _) = ui.allocate_exact_size(vec2(words.size().x, LINE_HIGH), Sense::hover());
        ui.painter().galley(pos2(rect.left(), rect.center().y - words.size().y / 2.0), words, theme::text_dim());
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        label(ui, "My rank");
        let open = list_box(ui, pictures, &Line::of_rank(rank), RANKS_WIDE);
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
        label(ui, "My role");
        let open = list_box(ui, pictures, &Line::of_any_role(role), ROLES_WIDE);
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
        Line { drawings: vec![role_logo(role)], room: 1, plain: true, name: role.name() }
    }

    fn of_any_role(role: Option<Role>) -> Line {
        role.map_or(Line { drawings: Vec::new(), room: 1, plain: true, name: "Any role" }, Line::of_role)
    }

    pub(super) fn of_rank(rank: Rank) -> Line {
        Line { drawings: vec![rank_medal(rank)], room: 1, plain: false, name: rank.name() }
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
        line.paint(painter, pictures, rect, theme::text());
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
                ui.label(dim(if enemy { "The enemy" } else { "Your team" }).small());
                ui.horizontal(|ui| {
                    for (phase, places) in PHASES.iter().enumerate() {
                        for &place in *places {
                            let seat = Seat { enemy, place };
                            if place > 0 {
                                ui.add_space(if place == places[0] { PHASE_GAP } else { GAP });
                            }
                            let hero = draft.at(seat).and_then(|id| catalog.hero(id));
                            let look = SeatLook { size, filling: draft.seat == seat, yours: own == Some(seat), phase: PHASE_NAMES[phase], role: draft.role(seat) };
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
            let words = painter.layout_no_wrap("You".to_string(), theme::heavy(10.0), Color32::WHITE);
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
    let whose = if look.yours { " This is your place." } else { "" };
    badge.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(format!("{}.{whose} Click to choose another role. The hero of this team that had it gets this one.", look.role.name()));

    let name = hero.map_or(id.unwrap_or_default(), |hero| hero.name.as_str());
    let hint = match (taken, filling) {
        (true, _) => format!("{name}. Click to take it off."),
        (false, true) => format!("{}. The next hero you pick goes here.{whose}", look.phase),
        (false, false) => format!("{}. Click to pick a hero for this place.{whose}", look.phase),
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
            painter.text(room.center(), Align2::CENTER_CENTER, &role.name()[..1], theme::heavy(room.height() * 0.8), ink);
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
    let room = ui.available_width();
    let short = room < BAR.x;
    let wide = if short { room.clamp(96.0, 130.0) } else { BAR.x };
    let (rect, bar) = ui.allocate_exact_size(vec2(wide, BAR.y), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let split = rect.left() + rect.width() * team / 100.0;
        painter.rect_filled(rect, CornerRadius::same(ROUND), theme::bad());
        painter.rect_filled(Rect::from_min_max(rect.min, pos2(split, rect.bottom())), CornerRadius { nw: ROUND, sw: ROUND, ne: 0, se: 0 }, theme::good());
        let (ours, theirs) = match short {
            true => (format!("{team:.0}%"), format!("{:.0}%", 100.0 - team)),
            false => (format!("Your team {team:.0}%"), format!("The enemy {:.0}%", 100.0 - team)),
        };
        painter.text(pos2(rect.left() + 9.0, rect.center().y), Align2::LEFT_CENTER, ours, theme::heavy(12.0), Color32::WHITE);
        painter.text(pos2(rect.right() - 9.0, rect.center().y), Align2::RIGHT_CENTER, theirs, theme::heavy(12.0), Color32::WHITE);
    }
    bar.on_hover_text(format!("{words}, by the numbers of your rank: your team {team:.0}%, the enemy {:.0}%. How the match is played decides it.", 100.0 - team));
    let said = ui.painter().layout_no_wrap(words.to_string(), theme::heavy(13.0), color);
    if said.size().x + 10.0 <= ui.available_width() {
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(said.size(), Sense::hover());
        ui.painter().galley(rect.min, said, color);
    }
}

fn verdict_words(chance: f32) -> (&'static str, Color32) {
    if chance >= 50.0 + EVEN_WITHIN {
        ("Your draft is better", theme::good())
    } else if chance <= 50.0 - EVEN_WITHIN {
        ("The enemy's draft is better", theme::bad())
    } else {
        ("The two drafts are even", theme::text())
    }
}

fn fitting(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, draft: &mut Draft, wanted: &str) {
    let fitting: Vec<&Hero> = heroes::found(&catalog.heroes, wanted).into_iter().filter(|hero| !draft.is_picked(&hero.id)).collect();
    let whose = if draft.seat.enemy { "the enemy" } else { "your team" };
    let free = draft.at(draft.seat).is_none();
    ui.horizontal(|ui| {
        ui.label(theme::title("Heroes that fit", 16.0));
        ui.label(dim(match (fitting.is_empty(), free) {
            (true, _) => "No hero that is still free goes by that name.".to_string(),
            (false, true) => format!("Click one, or press Enter for the first. It goes to {whose}."),
            (false, false) => "Both teams are full. Click a hero above to take it off first.".to_string(),
        }).small());
    });
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
            if tiles::tile(ui, pictures, look, &tile).clicked() {
                chosen = Some(hero.id.clone());
            }
        });
    });
    if let Some(hero) = chosen {
        draft.seat_hero(&hero);
    }
}

#[allow(clippy::too_many_arguments)]
fn suggestions(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, draft: &mut Draft, numbers: &Numbers, rank: Rank, role: Option<Role>, now: u64, answer: &mut Answer) {
    let seat = draft.seat;
    let seat_role = draft.role(seat);
    let (beside, against) = draft.around_the_seat();
    let all = numbers.suggest(rank, Some(seat_role), &beside, &against);
    let there = draft.at(seat);
    let shown: Vec<(&Suggestion, &Hero)> = all.iter().filter(|suggestion| !draft.hidden.contains(&suggestion.hero) && Some(suggestion.hero.as_str()) != there).filter_map(|suggestion| Some((suggestion, catalog.hero(&suggestion.hero)?))).collect();

    let roomy = ui.available_width() > 760.0;
    ui.horizontal(|ui| {
        ui.label(theme::title(list_title(seat, seat_role, draft.own_seat(role) == Some(seat)), 16.0));
        if roomy {
            ui.label(dim(if seat.enemy { "what does best for them, first" } else { "the best first" }).small());
        }
        if !draft.hidden.is_empty() {
            ui.add_space(6.0);
            let hidden = match draft.hidden.len() {
                1 => "1 hidden. Show it again".to_string(),
                many => format!("{many} hidden. Show them again"),
            };
            if ui.add(egui::Button::new(dim(hidden).small()).frame(false)).clicked() {
                draft.hidden.clear();
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let credit = ui.add(egui::Button::new(dim(credit_of(&numbers.source, &numbers.patch, numbers.age_in_days(now))).small()).frame(false));
            answer.open_source = credit.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(format!("Opens {SOURCE_PAGE}")).clicked();
        });
    });
    if shown.is_empty() {
        ui.label(dim("No hero is played in that role often enough to say anything about it. Type a hero's name to put one in."));
        return;
    }

    let name_of = |id: &str| catalog.hero(id).map_or_else(|| id.to_string(), |hero| hero.name.clone());
    let (mut picks, mut hides) = (None, None);
    egui::ScrollArea::vertical().id_salt("suggestions").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 5.0;
        for (suggestion, hero) in &shown {
            match row(ui, pictures, suggestion, hero, seat_role, &name_of) {
                Some(Clicked::Row) => picks = Some(suggestion.hero.clone()),
                Some(Clicked::Cross) => hides = Some(suggestion.hero.clone()),
                None => {}
            }
        }
    });
    if let Some(hero) = hides {
        draft.hidden.insert(hero);
    } else if let Some(hero) = picks {
        draft.seat_hero(&hero);
    }
}

fn list_title(seat: Seat, role: Role, yours: bool) -> String {
    let who = match role {
        Role::Carry => "carry",
        Role::Mid => "mid",
        Role::Offlane => "offlaner",
        Role::SoftSupport => "soft support",
        Role::HardSupport => "hard support",
    };
    match (seat.enemy, yours) {
        (true, _) => format!("Heroes for the enemy's {who}"),
        (false, true) => format!("Heroes for you, {}", role.as_played()),
        (false, false) => format!("Heroes for your team's {who}"),
    }
}

enum Clicked {
    Row,
    Cross,
}

fn row(ui: &mut Ui, pictures: &mut Pictures, suggestion: &Suggestion, hero: &Hero, role: Role, name_of: &dyn Fn(&str) -> String) -> Option<Clicked> {
    const PORTRAIT: egui::Vec2 = vec2(78.0, 44.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width() - 14.0, ROW), Sense::click());
    let cross = Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(28.0, 28.0));
    let hide = ui.interact(cross, response.id.with("hide"), Sense::click());
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered() || hide.hovered(), 0.12);
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(theme::ROUND), theme::card().lerp_to_gamma(theme::card_hover(), lit));
        let portrait = Rect::from_min_size(pos2(rect.left() + 7.0, rect.center().y - PORTRAIT.y / 2.0), PORTRAIT);
        tiles::fill_with(painter, pictures, hero.picture.as_deref(), portrait, 8, 1.0);

        let ink = if hide.hovered() { theme::text() } else { theme::text_dim() };
        painter.text(cross.center(), Align2::CENTER_CENTER, "×", FontId::proportional(20.0), ink);
        let color = match suggestion.points {
            points if points >= WORTH_NAMING => theme::good(),
            points if points <= -WORTH_NAMING => theme::bad(),
            _ => theme::text_dim(),
        };
        let number = painter.layout_no_wrap(format!("{:+.1}%", suggestion.points), theme::heavy(16.0), color);
        let number_left = cross.left() - 12.0 - number.size().x;
        painter.galley(pos2(number_left, rect.center().y - number.size().y / 2.0), number, color);

        let left = portrait.right() + 12.0;
        let room = (number_left - 14.0 - left).max(40.0);
        let mut name = LayoutJob::simple(hero.name.clone(), theme::heavy(15.0), theme::text(), room);
        name.wrap.max_rows = 1;
        name.wrap.overflow_character = Some('…');
        painter.galley(pos2(left, rect.top() + 9.0), painter.layout_job(name), theme::text());
        let mut words = painted(&in_words(suggestion, Some(role), name_of, NAMED_IN_A_ROW), FontId::proportional(theme::SMALL), theme::text_dim(), room);
        words.wrap.max_rows = 1;
        words.wrap.overflow_character = Some('…');
        painter.galley(pos2(left, rect.top() + 32.0), painter.layout_job(words), theme::text_dim());
    }
    if hide.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Hide this hero: it is banned, or you do not play it").clicked() {
        return Some(Clicked::Cross);
    }
    let response = response.on_hover_ui(|ui| {
        ui.set_max_width(420.0);
        ui.label(theme::title(&hero.name, 16.0));
        ui.label(painted(&in_words(suggestion, Some(role), name_of, usize::MAX), FontId::proportional(theme::BODY), theme::text(), 420.0));
        ui.label(dim("Click to put it in the place that is lit.").small());
    });
    response.on_hover_cursor(CursorIcon::PointingHand).clicked().then_some(Clicked::Row)
}

fn in_words(suggestion: &Suggestion, role: Option<Role>, name_of: &dyn Fn(&str) -> String, most: usize) -> String {
    let least = if most == usize::MAX { 0.0 } else { WORTH_NAMING };
    let named = |beside: bool, good: bool| -> Option<String> {
        let heroes: Vec<String> = suggestion
            .reasons
            .iter()
            .filter(|reason| reason.beside == beside && (reason.points > 0.0) == good && reason.points.abs() >= least)
            .take(most)
            .map(|reason| format!("{} {:+.1}", name_of(&reason.hero), reason.points))
            .collect();
        (!heroes.is_empty()).then(|| heroes.join(", "))
    };
    let mut sentences = Vec::new();
    for (beside, good, start) in [(false, true, "Good against"), (false, false, "Weak against"), (true, true, "Goes well with"), (true, false, "Not good with")] {
        sentences.extend(named(beside, good).map(|heroes| format!("{start} {heroes}.")));
    }
    let role = role.map(|role| format!(" {}", role.as_played())).unwrap_or_default();
    sentences.push(format!("Wins {:.1}% of its matches{role}.", suggestion.win_rate));
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
        let signed = number.chars().next().filter(|sign| "+-".contains(*sign) && number[1..].parse::<f32>().is_ok()).map(|sign| sign == '+');
        let share = number.strip_suffix('%').and_then(|share| share.parse::<f32>().ok()).filter(|share| *share != 50.0).map(|share| share > 50.0);
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
        0 => "made today".to_string(),
        1 => "1 day old".to_string(),
        days => format!("{days} days old"),
    };
    let source = if source.trim().is_empty() { "STRATZ" } else { source.trim() };
    match patch.trim() {
        "" => format!("Numbers from {source} · {age}"),
        patch => format!("Numbers from {source} · patch {patch} · {age}"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    Again,
    CopyDetails,
}

pub(super) fn waiting(ui: &mut Ui, fetching: bool, failure: Option<&Failure>, what: &str, still: &str) -> Option<Asked> {
    let mut asked = None;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.2).min(60.0));
        if fetching {
            ui.spinner();
            ui.label(dim(format!("Getting the {what}…")));
            return;
        }
        if failure.is_none() {
            ui.label(dim(format!("The {what} are not here yet.")));
            return;
        }
        ui.label(theme::title(format!("The {what} could not be fetched"), 18.0));
        ui.add_space(2.0);
        ui.label(dim(format!("They come from the internet, once, and are kept. {still}")));
        ui.add_space(8.0);
        if ui.button("Try again").clicked() {
            asked = Some(Asked::Again);
        }
        if ui.add(egui::Button::new(dim("Copy details").small()).frame(false)).on_hover_text("Copy what happened, to send to whoever helps you").clicked() {
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
        assert_eq!(draft.around_the_seat(), (vec!["lion", "cm"], vec!["ogre", "jakiro"]));
        assert_eq!(list_title(draft.seat, Role::Carry, false), "Heroes for the enemy's carry");

        draft.seat_hero("pa");
        draft.seat_hero("axe");
        assert_eq!(draft.heroes(true), ["lion", "cm", "pa", "axe"]);
        assert_eq!((draft.seat, draft.role(draft.seat)), (TEAM(2), Role::Carry));
        assert_eq!(draft.around_the_seat(), (vec!["ogre", "jakiro"], vec!["lion", "cm", "pa", "axe"]));
        assert_eq!(list_title(draft.seat, Role::Carry, false), "Heroes for your team's carry");
        assert_eq!(list_title(draft.seat, Role::Carry, true), "Heroes for you, as carry");
        draft.seat_hero("sven");
        draft.seat_hero("mars");
        assert_eq!(draft.heroes(false), ["ogre", "jakiro", "sven", "mars"]);
        assert_eq!((draft.seat, draft.role(draft.seat)), (ENEMY(4), Role::Mid), "the last picks, the enemy's first");
        assert_eq!(list_title(TEAM(4), Role::Mid, true), "Heroes for you, in mid");

        draft.seat = TEAM(3);
        assert_eq!(draft.around_the_seat().0, ["ogre", "jakiro", "sven"]);

        draft.start_over();
        assert_eq!(draft, Draft::default());
    }

    #[test]
    fn a_suggestion_says_what_speaks_for_it_and_against_it() {
        let reason = |hero: &str, beside: bool, points: f32| Reason { hero: hero.to_string(), beside, points };
        let reasons = vec![reason("axe", false, 3.0), reason("cm", true, 2.5), reason("pudge", false, -2.0), reason("lina", false, 1.2), reason("zeus", false, 0.6), reason("sven", true, -0.2)];
        let suggestion = Suggestion { hero: "viper".to_string(), points: 6.1, win_rate: 52.34, matches: 9000, reasons };
        let name_of = |id: &str| id.to_uppercase();

        assert_eq!(
            in_words(&suggestion, Some(Role::Mid), &name_of, 2),
            "Good against AXE +3.0, LINA +1.2. Weak against PUDGE -2.0. Goes well with CM +2.5. Wins 52.3% of its matches in mid.",
            "two of each at most, and nothing that hardly matters"
        );
        assert_eq!(
            in_words(&suggestion, None, &name_of, usize::MAX),
            "Good against AXE +3.0, LINA +1.2, ZEUS +0.6. Weak against PUDGE -2.0. Goes well with CM +2.5. Not good with SVEN -0.2. Wins 52.3% of its matches.",
            "everything, where there is room"
        );
        let alone = Suggestion { reasons: Vec::new(), ..suggestion };
        assert_eq!(in_words(&alone, Some(Role::Carry), &name_of, 2), "Wins 52.3% of its matches as carry.");
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
