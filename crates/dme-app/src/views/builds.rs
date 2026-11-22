use dme_core::builds::{self, Bought, Build, Builds, GameTexts, NeutralBuild, NeutralTier, Order, Other, POINT_LEVELS, ProItem, STARTING_GOLD, Talent};
use dme_core::catalog::{Catalog, Hero};
use dme_core::draft::{Rank, Role};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::draft::{Asked, Fetched, LOOK, Line, RANKS_WIDE, ROLES_WIDE, SOURCE_PAGE, credit_of, list_box, list_line, painted, waiting};
use super::tiles::{self, Tile};
use super::{dim, heroes, title_bar};
use crate::lang::{number, words};
use crate::pictures::Pictures;
use crate::theme;
use crate::worker::Failure;

const LABEL: f32 = 92.0;
const ITEM: egui::Vec2 = vec2(44.0, 32.0);
const UNDER: f32 = 15.0;
const LEAST_ITEM_WIDE: f32 = 30.0;
const GAP: f32 = 6.0;
const POINT: f32 = 30.0;
const OVER: f32 = 13.0;
const TALENT: f32 = 22.0;
const BETWEEN_TALENTS: f32 = 3.0;
const GROWS: f32 = 244.0;
const STAYS: f32 = 92.0;
const NEUTRAL_GROWS: f32 = 40.0;
const NEUTRAL_STAYS: f32 = 12.0;
const TIER_WIDE: f32 = 40.0;
const MOST_LARGER: f32 = 1.75;
const SCROLL_BAR: f32 = 14.0;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Chosen {
    pub hero: Option<String>,
    pub role: Option<Role>,
    pub search: String,
}

impl Chosen {
    pub fn set(&mut self, hero: &str, role: Option<Role>) {
        self.hero = Some(hero.to_string());
        self.role = role;
        self.search.clear();
    }

    fn role_of(&self, roles: &[Role]) -> Option<Role> {
        self.role.filter(|role| roles.contains(role)).or(roles.first().copied())
    }
}

fn in_its_place(hero: &str, asked: Role, shown: Role) -> Option<String> {
    (asked != shown).then(|| tr!(InItsPlace, hero, asked = words::role(asked), shown = words::role(shown)))
}

pub struct View<'a> {
    pub catalog: &'a Catalog,
    pub pictures: &'a mut Pictures,
    pub chosen: &'a mut Chosen,
    pub builds: Option<&'a Builds>,
    pub fetching: bool,
    pub failure: Option<&'a Failure>,
    pub texts: Option<&'a GameTexts>,
    pub rank: Rank,
    pub enemies: &'a [String],
    pub now: u64,
}

#[derive(Default)]
pub struct Answer {
    pub rank: Option<Rank>,
    pub try_again: bool,
    pub copy_details: bool,
    pub open_source: bool,
}

pub fn show(ui: &mut Ui, view: View<'_>) -> Answer {
    let View { catalog, pictures, chosen, builds, fetching, failure, texts, rank, enemies, now } = view;
    let mut answer = Answer::default();
    title_bar(ui, tr!(BuildsTitle), Some(tr!(BuildsNote)), Some((&mut chosen.search, tr!(HeroNameHint))), |_| {});

    let wanted = chosen.search.trim().to_lowercase();
    if !wanted.is_empty() {
        fitting(ui, catalog, pictures, chosen, &wanted);
        return answer;
    }
    let Some(hero) = chosen.hero.as_deref().and_then(|id| catalog.any_hero(id)) else {
        nobody(ui);
        return answer;
    };
    let Some(builds) = builds else {
        match waiting(ui, fetching, failure, Fetched::Builds, "") {
            Some(Asked::Again) => answer.try_again = true,
            Some(Asked::CopyDetails) => answer.copy_details = true,
            None => {}
        }
        return answer;
    };
    let Some(texts) = texts else {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.2).min(60.0));
            ui.spinner();
            ui.label(dim(tr!(ReadingFiles)));
        });
        return answer;
    };

    let roles = builds.roles(&hero.id, rank);
    let role = chosen.role_of(&roles);
    let asked = chosen.role.or(role);
    head(ui, pictures, hero, asked, role, rank, chosen, &mut answer);
    let Some(build) = role.and_then(|role| builds.of(&hero.id, rank, role)) else {
        ui.add_space(8.0);
        ui.label(dim(tr!(NoBuild, hero = hero.name)));
        return answer;
    };

    let against: Vec<&str> = enemies.iter().map(String::as_str).collect();
    let others = builds.others(&hero.id, build, texts, &against);
    let mut core: Vec<Core<'_>> = build.core(texts).into_iter().map(Core::Ours).collect();
    core.extend(builds.pro_core(&hero.id, build, texts).into_iter().map(Core::Pros));
    core.sort_by_key(Core::minute);
    let pros = builds.pro_build(&hero.id, build.role).map_or(0, |pro| pro.players);
    let order = builds.order(&hero.id, build.role);
    let own = texts.talents(&hero.id);
    let start = build.starting(texts);
    let start_note = tr!(StartNote, cost = Build::starting_cost(&start, texts), gold = STARTING_GOLD);
    let neutrals = builds.neutral_tiers(&hero.id, build.role, texts);
    let (grows, stays) = if neutrals.is_some() { (GROWS + NEUTRAL_GROWS, STAYS + NEUTRAL_STAYS) } else { (GROWS, STAYS) };
    let larger = ((ui.available_height() - stays) / grows).clamp(1.0, MOST_LARGER);
    let item = (ITEM * larger).round();
    let line = item.y + under(item);
    egui::ScrollArea::vertical().id_salt("build").auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = (5.0 * larger).round();
        ui.add_space(2.0);
        part(ui, tr!(PartStart), &start_note, line, |ui| {
            for bought in &start {
                let shown = Shown { item: &bought.item, copies: bought.copies, corner: None, under: format!("{:.0}%", build.share(bought.matches)), ink: theme::text_dim(), face: None };
                let Some(tile) = item_tile(ui, pictures, item, &shown) else { break };
                tile.on_hover_ui(|ui| said(ui, &texts.item_name(&bought.item), &item_words(build, bought)));
            }
        });
        part(ui, tr!(PartCore), tr!(PartCoreNote), line, |ui| {
            let item = fitted(item, core.len(), ui.available_width() - SCROLL_BAR);
            for one in &core {
                let shown = match one {
                    Core::Ours(bought) => Shown { item: &bought.item, copies: 1, corner: Some(tr!(Minutes, minute = bought.minute)), under: format!("{:.0}%", build.share(bought.matches)), ink: theme::text_dim(), face: None },
                    Core::Pros(pro) => Shown { item: &pro.item, copies: 1, corner: Some(tr!(Minutes, minute = pro.minute)), under: format!("{:.0}%", pro.share), ink: theme::text_dim(), face: None },
                };
                let Some(tile) = item_tile(ui, pictures, item, &shown) else { break };
                match one {
                    Core::Ours(bought) => tile.on_hover_ui(|ui| said(ui, &texts.item_name(&bought.item), &item_words(build, bought))),
                    Core::Pros(pro) => tile.on_hover_ui(|ui| said(ui, &texts.item_name(&pro.item), &pro_words(&hero.name, build, pro, pros))),

                };
            }
            if core.is_empty() {
                ui.label(dim(tr!(NoCore)).small());
            }
        });
        part(ui, tr!(PartOther), if against.is_empty() { tr!(PartOtherNote) } else { tr!(PartOtherDraft) }, line, |ui| {
            let name_of = |id: &str| catalog.any_hero(id).map_or_else(|| id.to_string(), |hero| hero.name.clone());
            for other in &others {
                let face = other.against.first().and_then(|(enemy, _)| catalog.any_hero(enemy)).and_then(|enemy| enemy.picture.as_deref());
                let ink = if other.against.is_empty() { theme::text_dim() } else { theme::good() };
                let shown = Shown { item: &other.item.item, copies: 1, corner: None, under: format!("{:.0}%", other.share), ink, face };
                let Some(tile) = item_tile(ui, pictures, item, &shown) else { break };
                let usually = builds.bought_against(&other.item.item);
                tile.on_hover_ui(|ui| said(ui, &texts.item_name(&other.item.item), &other_words(build, other, &usually, &name_of)));
            }
            if others.is_empty() {
                ui.label(dim(tr!(NoOther)).small());
            }
        });
        if let Some((neutral, tiers)) = &neutrals {
            let note = match neutral.role {
                Some(_) => tr!(NeutralNote, players = neutral.players, patch = builds.neutral_patch),
                None => tr!(NeutralNoteAny, players = neutral.players, patch = builds.neutral_patch),
            };
            part(ui, tr!(PartNeutral), &note, line, |ui| neutral_line(ui, pictures, texts, item, &hero.name, neutral, tiers, &builds.neutral_patch));
        }
        part(ui, tr!(PartSkills), tr!(PartSkillsNote), (POINT * larger).round() + OVER, |ui| points(ui, pictures, texts, own, order, larger));
        if !own.is_empty() {
            let high = (TALENT * larger).round();
            let counted = own.iter().any(|talent| build.talent(&talent.id).is_some_and(|taken| taken.matches > 0));
            let note = if counted { tr!(TalentsCounted) } else { tr!(TalentsNotCounted) };
            part(ui, tr!(PartTalents), note, 4.0 * high + 3.0 * BETWEEN_TALENTS, |ui| talents(ui, own, build, high, counted));
        }
        ui.horizontal(|ui| {
            ui.label(dim(tr!(FromMatches, count = with_spaces(build.matches))).small());
            let source = match builds.pro_source.trim() {
                "" => builds.source.clone(),
                pros => tr!(SourcesAnd, first = builds.source, second = pros),
            };
            let credit = ui.add(egui::Button::new(dim(credit_of(&source, &builds.patch, builds.age_in_days(now))).small()).frame(false));
            let mut from = match builds.sampled {
                0 => String::new(),
                sampled => tr!(FromSampled, count = with_spaces(sampled)),
            };
            if builds.pro_matches > 0 {
                from.push_str(&tr!(FromPros, count = with_spaces(builds.pro_matches), source = builds.pro_source));
            }
            if builds.neutral_matches > 0 {
                from.push_str(&tr!(FromNeutrals, count = with_spaces(builds.neutral_matches), patch = builds.neutral_patch, source = builds.neutral_source));
            }
            answer.open_source = credit.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(format!("{from}{}", tr!(Opens, address = SOURCE_PAGE))).clicked();
        });
    });
    answer
}

#[allow(clippy::too_many_arguments)]
fn head(ui: &mut Ui, pictures: &mut Pictures, hero: &Hero, asked: Option<Role>, shown: Option<Role>, rank: Rank, chosen: &mut Chosen, answer: &mut Answer) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (portrait, _) = ui.allocate_exact_size(vec2(78.0, 44.0), Sense::hover());
        tiles::fill_with(ui.painter(), pictures, hero.picture.as_deref(), portrait, 9, 1.0);
        ui.label(theme::title(&hero.name, 22.0));
        ui.add_space(6.0);
        if let Some(role) = asked {
            let open = list_box(ui, pictures, &Line::of_role(role), ROLES_WIDE).on_hover_text(tr!(RoleHint));
            egui::Popup::menu(&open).show(|ui| {
                ui.set_width(ROLES_WIDE);
                ui.spacing_mut().item_spacing.y = 2.0;
                for other in Role::ALL {
                    if list_line(ui, pictures, &Line::of_role(other), other == role, ROLES_WIDE).clicked() {
                        chosen.role = Some(other);
                    }
                }
            });
        }
        let open = list_box(ui, pictures, &Line::of_rank(rank), RANKS_WIDE).on_hover_text(tr!(RankHint));
        egui::Popup::menu(&open).show(|ui| {
            ui.set_width(RANKS_WIDE);
            ui.spacing_mut().item_spacing.y = 2.0;
            for other in Rank::ALL {
                if list_line(ui, pictures, &Line::of_rank(other), other == rank, RANKS_WIDE).clicked() && other != rank {
                    answer.rank = Some(other);
                }
            }
        });
        if let Some(said) = asked.zip(shown).and_then(|(asked, shown)| in_its_place(&hero.name, asked, shown)) {
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(said).color(theme::text_dim()).size(12.0)).wrap());
        }
    });
}

fn part(ui: &mut Ui, name: &str, note: &str, high: f32, add: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        let (rect, _) = ui.allocate_exact_size(vec2(LABEL - GAP, high), Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.text(rect.left_top() + vec2(0.0, 1.0), Align2::LEFT_TOP, name, theme::heavy(14.0), theme::text());
            let words = LayoutJob::simple(note.to_string(), FontId::proportional(11.0), theme::text_dim(), rect.width());
            painter.galley(rect.left_top() + vec2(0.0, 20.0), painter.layout_job(words), theme::text_dim());
        }
        add(ui);
    });
}

fn said(ui: &mut Ui, name: &str, words: &str) {
    ui.set_max_width(340.0);
    ui.label(theme::title(name, 15.0));
    ui.label(painted(words, FontId::proportional(theme::BODY), theme::text(), 340.0));
}

fn under(size: egui::Vec2) -> f32 {
    (UNDER + 3.0 * (size.y / ITEM.y - 1.0)).round()
}

enum Core<'a> {
    Ours(&'a Bought),
    Pros(&'a ProItem),
}

impl Core<'_> {
    fn minute(&self) -> u32 {
        match self {
            Core::Ours(bought) => bought.minute,
            Core::Pros(pro) => pro.minute,
        }
    }
}

struct Shown<'a> {
    item: &'a str,
    copies: u32,
    corner: Option<String>,
    under: String,
    ink: Color32,
    face: Option<&'a str>,
}

fn fitted(size: egui::Vec2, count: usize, room: f32) -> egui::Vec2 {
    if count == 0 {
        return size;
    }
    let wide = ((room + GAP) / count as f32 - GAP).floor();
    if wide >= size.x {
        size
    } else {
        let wide = wide.max(LEAST_ITEM_WIDE);
        vec2(wide, (wide * ITEM.y / ITEM.x).round())
    }
}

const MOST_COPIES: u32 = 4;

fn drawn(copies: u32, size: egui::Vec2, room: f32) -> Option<(u32, Option<String>)> {
    let wide = |count: u32| size.x * count as f32 + 2.0 * (count as f32 - 1.0);
    let copies = copies.max(1);
    if copies <= MOST_COPIES && wide(copies) <= room {
        Some((copies, None))
    } else if wide(1) <= room {
        Some((1, (copies > 1).then(|| format!("×{copies}"))))
    } else {
        None
    }
}

fn item_tile(ui: &mut Ui, pictures: &mut Pictures, size: egui::Vec2, shown: &Shown<'_>) -> Option<egui::Response> {
    const ROUND: u8 = 6;
    let (count, how_many) = drawn(shown.copies, size, ui.available_width() - SCROLL_BAR)?;
    let wide = size.x * count as f32 + 2.0 * (count as f32 - 1.0);
    let (rect, response) = ui.allocate_exact_size(vec2(wide, size.y + under(size)), Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let picture = builds::item_picture(shown.item);
        let more = size.y / ITEM.y - 1.0;
        for copy in 0..count {
            let room = Rect::from_min_size(rect.min + vec2((size.x + 2.0) * copy as f32, 0.0), size);
            tiles::fill_with(painter, pictures, Some(&picture), room, ROUND, 1.0);
        }
        if let Some(corner) = shown.corner.as_ref().or(how_many.as_ref()) {
            let words = painter.layout_no_wrap(corner.clone(), FontId::proportional(10.0 + 2.0 * more), Color32::WHITE);
            let plate = Rect::from_min_size(pos2(rect.right() - words.size().x - 6.0, rect.top() + size.y - words.size().y - 2.0), words.size() + vec2(6.0, 2.0));
            painter.rect_filled(plate, CornerRadius { nw: 5, ne: 0, sw: 0, se: ROUND }, Color32::from_black_alpha(200));
            painter.galley(plate.min + vec2(3.0, 1.0), words, Color32::WHITE);
        }
        if let Some(face) = shown.face {
            let room = Rect::from_min_size(rect.min, vec2((size.x * 0.52).round(), (size.x * 0.52 * 0.56).round()));
            tiles::fill_with(painter, pictures, Some(face), room, 4, 1.0);
            painter.rect_stroke(room, CornerRadius::same(4), Stroke::new(1.0, theme::good()), StrokeKind::Outside);
        }
        painter.text(pos2(rect.center().x, rect.bottom() + 1.0), Align2::CENTER_BOTTOM, &shown.under, theme::heavy(11.5 + 2.0 * more), shown.ink);
    }
    Some(response)
}

#[allow(clippy::too_many_arguments)]
fn neutral_line(ui: &mut Ui, pictures: &mut Pictures, texts: &GameTexts, size: egui::Vec2, hero: &str, neutral: &NeutralBuild, tiers: &[NeutralTier<'_>], patch: &str) {
    let shown = |tier: &NeutralTier<'_>| tier.trinkets.len() + tier.enhancements.len().min(1);
    let count: usize = tiers.iter().map(shown).sum();
    let tier_wide = tiers.iter().map(|tier| ui.painter().layout_no_wrap(tr!(Tier, tier = tier.tier), theme::heavy(12.0), theme::accent_text()).size().x).fold(TIER_WIDE, f32::max);
    let room = ui.available_width() - SCROLL_BAR - tiers.len() as f32 * (tier_wide + GAP);
    let size = fitted(size, count, room);
    for tier in tiers {
        let (rect, response) = ui.allocate_exact_size(vec2(tier_wide, size.y + under(size)), Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.text(pos2(rect.right(), rect.top() + 2.0), Align2::RIGHT_TOP, tr!(Tier, tier = tier.tier), theme::heavy(12.0), theme::accent_text());
            painter.text(pos2(rect.right(), rect.top() + 18.0), Align2::RIGHT_TOP, tr!(Minutes, minute = tier.minute), FontId::proportional(11.0), theme::text_dim());
        }
        response.on_hover_ui(|ui| said(ui, &tr!(Tier, tier = tier.tier), &tier_words(hero, neutral, tier, patch)));
        for (name, share) in &tier.trinkets {
            let shown = Shown { item: name, copies: 1, corner: None, under: format!("{share:.0}%"), ink: theme::text_dim(), face: None };
            let Some(tile) = item_tile(ui, pictures, size, &shown) else { return };
            tile.on_hover_ui(|ui| said(ui, &texts.item_name(name), &neutral_words(hero, neutral, tier, *share, true, patch)));
        }
        if let Some((name, share)) = tier.enhancements.first() {
            let shown = Shown { item: name, copies: 1, corner: None, under: format!("{share:.0}%"), ink: theme::text_dim(), face: None };
            let Some(tile) = item_tile(ui, pictures, size, &shown) else { return };
            tile.on_hover_ui(|ui| said(ui, &texts.item_name(name), &neutral_words(hero, neutral, tier, *share, false, patch)));
        }
    }
}

fn neutral_pros(hero: &str, neutral: &NeutralBuild) -> String {
    match neutral.role {
        Some(role) => tr!(NeutralPros, hero, role = words::as_played(role)),
        None => tr!(NeutralProsAny, hero),
    }
}

fn neutral_words(hero: &str, neutral: &NeutralBuild, tier: &NeutralTier<'_>, share: f32, trinket: bool, patch: &str) -> String {
    let (share, pros) = (format!("{share:.0}"), neutral_pros(hero, neutral));
    match trinket {
        true => tr!(TrinketWords, share, tier = tier.tier, players = neutral.players, pros, reached = tier.reached, patch),
        false => tr!(EnhancementWords, share, tier = tier.tier, players = neutral.players, pros, reached = tier.reached, patch),
    }
}

fn tier_words(hero: &str, neutral: &NeutralBuild, tier: &NeutralTier<'_>, patch: &str) -> String {
    let mut words = tr!(TierWords, minute = tier.minute, players = neutral.players, pros = neutral_pros(hero, neutral), reached = tier.reached, patch);
    if tier.kept > 0 {
        words.push_str(&tr!(TierKept, count = tier.kept));
    }
    words
}

fn item_words(build: &Build, item: &Bought) -> String {
    let share = format!("{:.0}", build.share(item.matches));
    let bought = match (item.minute, item.copies) {
        (0, copies) if copies > 1 => tr!(BoughtStartTimes, copies, share),
        (0, _) => tr!(BoughtStart, share),
        (minute, _) => tr!(BoughtAt, share, minute),
    };
    match builds::win_rate(item.matches, item.wins) {
        Some(wins) => tr!(BoughtWins, bought, wins = number(f64::from(wins), 1)),
        None => bought,
    }
}

fn pro_words(hero: &str, build: &Build, pro: &ProItem, players: u64) -> String {
    let pros = tr!(ProWords, hero, role = words::as_played(build.role), share = format!("{:.0}", pro.share), players, minute = pro.minute);
    match build.items.iter().find(|item| item.item.eq_ignore_ascii_case(&pro.item)) {
        Some(item) => tr!(ProWordsRank, pros, share = format!("{:.0}", build.share(item.matches))),
        None => pros,
    }
}

fn other_words(build: &Build, other: &Other<'_>, usually: &[(&str, f32)], name_of: &dyn Fn(&str) -> String) -> String {
    let mut words = item_words(build, other.item);
    for (enemy, times) in &other.against {
        words.push_str(&tr!(AgainstTimes, hero = name_of(enemy), times = number(f64::from(*times), 1)));
    }
    if !other.against.is_empty() {
        words.push_str(&tr!(InThisMatch, share = format!("{:.0}", other.share)));
    }
    let more: Vec<String> = usually.iter().filter(|(enemy, _)| !other.against.iter().any(|(named, _)| named.eq_ignore_ascii_case(enemy))).take(3).map(|(enemy, times)| tr!(HeroTimes, hero = name_of(enemy), times = number(f64::from(*times), 1))).collect();
    if !more.is_empty() {
        match other.against.is_empty() {
            true => words.push_str(&tr!(MoreOftenAgainst, heroes = in_a_list(&more))),
            false => words.push_str(&tr!(AlsoMoreOftenAgainst, heroes = in_a_list(&more))),
        }
    }
    words
}

fn in_a_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [before @ .., last] => tr!(ListAnd, first = before.join(", "), last),
    }
}

fn points(ui: &mut Ui, pictures: &mut Pictures, texts: &GameTexts, own: &[Talent], order: Option<&Order>, larger: f32) {
    let Some(order) = order.filter(|order| !order.skills.is_empty()) else {
        ui.label(dim(tr!(NoOrder)).small());
        return;
    };
    const BETWEEN: f32 = 3.0;
    let count = order.skills.len().min(POINT_LEVELS.len());
    let room = ui.available_width() - SCROLL_BAR;
    let side = ((room + BETWEEN) / count as f32 - BETWEEN).clamp(16.0, (POINT * larger).round()).floor();
    ui.spacing_mut().item_spacing.x = BETWEEN;
    for (skill, level) in order.skills.iter().zip(POINT_LEVELS) {
        let (rect, response) = ui.allocate_exact_size(vec2(side, side + OVER), Sense::hover());
        let square = Rect::from_min_size(pos2(rect.left(), rect.top() + OVER), vec2(side, side));
        let (what, tier) = point_of(texts, own, skill);
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.text(pos2(rect.center().x, rect.top()), Align2::CENTER_TOP, level.to_string(), FontId::proportional(10.5), theme::text_dim());
            match tier {
                Some(tier) => {
                    painter.rect_filled(square, CornerRadius::same(6), theme::accent().gamma_multiply(if theme::is_day() { 0.16 } else { 0.22 }));
                    painter.rect_stroke(square, CornerRadius::same(6), Stroke::new(1.0, theme::accent()), StrokeKind::Inside);
                    let ring = if tier == 0 { "T".to_string() } else { tier.to_string() };
                    painter.text(square.center(), Align2::CENTER_CENTER, ring, theme::heavy((side * 0.42).max(9.0)), theme::accent_text());
                }
                None => tiles::fill_with(painter, pictures, Some(&builds::skill_picture(skill)), square, 6, 1.0),
            }
        }
        response.on_hover_text(tr!(LevelPoint, level, what));
    }
}

fn point_of(texts: &GameTexts, own: &[Talent], skill: &str) -> (String, Option<u32>) {
    match own.iter().position(|talent| talent.id.eq_ignore_ascii_case(skill)) {
        Some(place) => {
            let tier = 10 + 5 * (place as u32 / 2);
            (tr!(TalentOfLevel, tier, talent = own[place].text), Some(tier))
        }
        None if skill.to_ascii_lowercase().starts_with("special_bonus") => (tr!(ATalent).to_string(), Some(0)),
        None => (texts.skill_name(skill), None),
    }
}

fn talents(ui: &mut Ui, own: &[Talent], build: &Build, high: f32, counted: bool) {
    const MIDDLE: f32 = 40.0;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = BETWEEN_TALENTS;
        let wide = ui.available_width() - SCROLL_BAR;
        let side = ((wide - MIDDLE) / 2.0).max(100.0);
        for tier in (0..(own.len() / 2).min(4)).rev() {
            let (right, left) = (&own[2 * tier], &own[2 * tier + 1]);
            let taken = |talent: &Talent| build.talent(&talent.id).map_or(0, |taken| taken.matches);
            let new = |talent: &Talent| counted && build.is_new_talent(own, &talent.id);
            let either_new = new(left) || new(right);
            let (rect, _) = ui.allocate_exact_size(vec2(wide, high), Sense::hover());
            let boxes = [(left, Rect::from_min_size(rect.min, vec2(side, high)), !either_new && taken(left) > taken(right)), (right, Rect::from_min_size(pos2(rect.right() - side, rect.top()), vec2(side, high)), !either_new && taken(right) > taken(left))];
            for (place, (talent, room, more)) in boxes.into_iter().enumerate() {
                if ui.is_rect_visible(room) {
                    let painter = ui.painter();
                    let round = CornerRadius::same(7);
                    painter.rect_filled(room, round, if more { theme::card_picked() } else { theme::card() });
                    painter.rect_stroke(room, round, if more { Stroke::new(1.5, theme::accent()) } else { Stroke::new(1.0, theme::line()) }, StrokeKind::Inside);
                    let share = build.talent(&talent.id).map_or(0.0, |taken| build.share(taken.matches));
                    let letters = 12.5 + 2.5 * (high / TALENT - 1.0);
                    let shown = match (counted, new(talent)) {
                        (false, _) => String::new(),
                        (true, true) => tr!(NewMark).to_string(),
                        (true, false) => format!("{share:.0}%"),
                    };
                    let number = painter.layout_no_wrap(shown, theme::heavy(letters - 0.5), if more { theme::accent_text() } else { theme::text_dim() });
                    let number_left = room.right() - 9.0 - number.size().x;
                    painter.galley(pos2(number_left, room.center().y - number.size().y / 2.0), number, theme::text_dim());
                    let mut text = LayoutJob::simple(talent.text.clone(), FontId::proportional(letters), theme::text(), (number_left - 8.0 - room.left() - 9.0).max(20.0));
                    text.wrap.max_rows = 1;
                    text.wrap.overflow_character = Some('…');
                    let text = painter.layout_job(text);
                    painter.galley(pos2(room.left() + 9.0, room.center().y - text.size().y / 2.0), text, theme::text());
                }
                let words = match (counted, new(talent)) {
                    (false, _) => tr!(TalentNotCounted).to_string(),
                    (true, true) => tr!(NewTalent).to_string(),
                    (true, false) => talent_words(build, talent),
                };
                ui.interact(room, ui.make_persistent_id(("talent", tier, place)), Sense::hover()).on_hover_ui(|ui| said(ui, &talent.text, &words));
            }
            if ui.is_rect_visible(rect) {
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, (10 + 5 * tier).to_string(), theme::heavy(13.0), theme::accent_text());
            }
        }
    });
}

fn talent_words(build: &Build, talent: &Talent) -> String {
    let Some(taken) = build.talent(&talent.id).filter(|taken| build.share(taken.matches) >= 0.5) else { return tr!(HardlyAnybody).to_string() };
    let share = format!("{:.0}", build.share(taken.matches));
    match builds::win_rate(taken.matches, taken.wins) {
        Some(wins) => tr!(TakeItWins, share, wins = number(f64::from(wins), 1)),
        None => tr!(TakeIt, share),
    }
}

fn with_spaces(number: u64) -> String {
    let digits = number.to_string();
    let mut said = String::new();
    for (place, digit) in digits.chars().enumerate() {
        if place > 0 && (digits.len() - place).is_multiple_of(3) {
            said.push(' ');
        }
        said.push(digit);
    }
    said
}

fn fitting(ui: &mut Ui, catalog: &Catalog, pictures: &mut Pictures, chosen: &mut Chosen, wanted: &str) {
    let fitting = heroes::found(catalog.all_heroes(), wanted);
    ui.horizontal(|ui| {
        ui.label(theme::title(tr!(HeroesThatFit), 16.0));
        ui.label(dim(if fitting.is_empty() { tr!(NoHeroNamed) } else { tr!(ClickForBuild) }).small());
    });
    let mut picked = None;
    if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        picked = fitting.first().map(|hero| hero.id.clone());
    }
    egui::ScrollArea::vertical().id_salt("fitting").auto_shrink(false).show(ui, |ui| {
        tiles::flow(ui, fitting.len(), LOOK, |ui, index, look| {
            let hero = fitting[index];
            let tile = Tile { picture: hero.picture.as_deref(), name: &hero.name, note: None, badge: None, band: None, picked: false, faint: false, warning: None, detail: None, mark: None };
            if tiles::tile(ui, pictures, look, &tile).clicked() {
                picked = Some(hero.id.clone());
            }
        });
    });
    if let Some(hero) = picked {
        chosen.set(&hero, None);
    }
}

fn nobody(ui: &mut Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.25).min(90.0));
        ui.label(theme::title(tr!(WhoseBuild), 20.0));
        ui.add_space(4.0);
        ui.label(dim(tr!(WhoseBuildHow)));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{"format":2,"made":5,"source":"STRATZ","patch":"7.41f","items":["item_branches","item_bfury","item_basher"],"skills":["special_bonus_x","special_bonus_y","axe_berserkers_call"],
        "heroes":{"axe":[{"role":2,"pair":1,"matches":1000,"start":[[0,2,900,500]],"items":[[1,16,880,495],[2,24,200,110]],"talents":[[0,600,330],[1,0,0]]},{"role":4,"pair":1,"matches":200}]},
        "against":{"lina":[[2,250]],"zeus":[[2,150]]}}"#;

    fn build() -> Build {
        let builds = Builds::parse(FILE.as_bytes()).unwrap();
        builds.of("axe", Rank::Archon, Role::Offlane).unwrap().clone()
    }

    #[test]
    fn an_item_says_how_many_buy_it_when_and_how_often_they_win() {
        let build = build();
        let (branches, bfury) = (&build.start[0], &build.items[0]);
        assert_eq!(item_words(&build, branches), "Bought 2 times at the start, in 90 of 100 matches. Those who buy it win 55.6% of their matches.");
        assert_eq!(item_words(&build, bfury), "Bought in 88 of 100 matches, around minute 16. Those who buy it win 56.2% of their matches.");
        let once = Bought { copies: 1, ..branches.clone() };
        assert!(item_words(&build, &once).starts_with("Bought at the start, in 90 of 100 matches."));
        let never = Bought { matches: 0, wins: 0, ..bfury.clone() };
        assert_eq!(item_words(&build, &never), "Bought in 0 of 100 matches, around minute 16.");
    }

    #[test]
    fn an_other_item_says_which_enemies_it_is_bought_against() {
        let builds = Builds::parse(FILE.as_bytes()).unwrap();
        let build = builds.of("axe", Rank::Archon, Role::Offlane).unwrap();
        let name_of = |id: &str| id.to_uppercase();
        let texts = GameTexts::default();
        let alone = builds.others("axe", build, &texts, &[]);
        let usually = builds.bought_against("item_basher");
        assert_eq!(other_words(build, &alone[0], &[], &name_of), item_words(build, &build.items[1]));
        assert_eq!(other_words(build, &alone[0], &usually, &name_of), format!("{} It is bought more often against LINA (2.5 times) and ZEUS (1.5 times).", item_words(build, &build.items[1])));
        let met = builds.others("axe", build, &texts, &["zeus", "lina", "sven"]);
        assert_eq!(
            other_words(build, &met[0], &usually, &name_of),
            "Bought in 20 of 100 matches, around minute 24. Those who buy it win 55.0% of their matches. Against LINA it is bought 2.5 times as often. Against ZEUS it is bought 1.5 times as often. That makes about 48 of 100 in this match."
        );
        let some = builds.others("axe", build, &texts, &["zeus"]);
        assert!(other_words(build, &some[0], &usually, &name_of).ends_with("in this match. It is also bought more often against LINA (2.5 times)."));
        assert_eq!([in_a_list(&[]), in_a_list(&["A".to_string()]), in_a_list(&["A".into(), "B".into(), "C".into()])], ["", "A", "A, B and C"]);
    }

    #[test]
    fn what_most_pros_buy_says_so_beside_what_the_players_of_the_rank_do() {
        let build = build();
        let bottle = ProItem { item: "item_bottle".to_string(), share: 96.4, minute: 2 };
        assert_eq!(
            pro_words("Axe", &build, &bottle, 109),
            "Most pros who play Axe in the offlane buy it: 96 of 100 of the 109 who did in recent pro matches, around minute 2."
        );
        let basher = ProItem { item: "item_basher".to_string(), share: 60.0, minute: 22 };
        assert!(pro_words("Axe", &build, &basher, 20).ends_with("Of the players of your rank, 20 of 100 buy it."));
    }

    #[test]
    fn a_core_with_more_items_than_room_has_smaller_pictures() {
        let size = vec2(44.0, 32.0);
        assert_eq!(fitted(size, 10, 600.0), size);
        assert_eq!(fitted(size, 10, 494.0), size);
        assert_eq!(fitted(size, 12, 494.0), vec2(35.0, 25.0));
        assert_eq!(fitted(size, 40, 494.0), vec2(30.0, 22.0));
        assert_eq!(fitted(size, 0, 10.0), size);
    }

    #[test]
    fn copies_are_drawn_side_by_side_where_they_fit_and_are_said_where_they_do_not() {
        let size = vec2(44.0, 32.0);
        assert_eq!(drawn(3, size, 200.0), Some((3, None)));
        assert_eq!(drawn(3, size, 136.0), Some((3, None)));
        assert_eq!(drawn(3, size, 135.0), Some((1, Some("×3".to_string()))));
        assert_eq!(drawn(6, size, 900.0), Some((1, Some("×6".to_string()))), "too many to draw");
        assert_eq!(drawn(1, size, 44.0), Some((1, None)));
        assert_eq!(drawn(0, size, 44.0), Some((1, None)));
        assert_eq!(drawn(2, size, 43.0), None, "no room for even one");
    }

    #[test]
    fn a_skill_point_says_what_it_is_put_in() {
        let talent = |id: &str, text: &str| Talent { id: id.to_string(), text: text.to_string() };
        let own = [talent("special_bonus_a", "+1 A"), talent("special_bonus_b", "+2 B"), talent("special_bonus_c", "+3 C"), talent("special_bonus_d", "+4 D")];
        let texts = GameTexts::default();
        assert_eq!(point_of(&texts, &own, "axe_berserkers_call"), ("Axe Berserkers Call".to_string(), None));
        assert_eq!(point_of(&texts, &own, "special_bonus_b"), ("the talent of level 10, +2 B".to_string(), Some(10)));
        assert_eq!(point_of(&texts, &own, "SPECIAL_BONUS_C"), ("the talent of level 15, +3 C".to_string(), Some(15)));
        assert_eq!(point_of(&texts, &own, "special_bonus_other"), ("a talent".to_string(), Some(0)), "one that is not among the hero's own");
    }

    #[test]
    fn a_talent_says_how_many_take_it_and_how_often_they_win() {
        let build = build();
        let talent = |id: &str| Talent { id: id.to_string(), text: String::new() };
        assert_eq!(talent_words(&build, &talent("special_bonus_x")), "60 of 100 take it. They win 55.0% of their matches.");
        assert_eq!(talent_words(&build, &talent("special_bonus_y")), "Hardly anybody takes it.");
        assert_eq!(talent_words(&build, &talent("special_bonus_unknown")), "Hardly anybody takes it.");
        let mut few = build.clone();
        (few.talents[0].matches, few.talents[0].wins) = (3, 3);
        assert_eq!(talent_words(&few, &talent("special_bonus_x")), "Hardly anybody takes it.");
    }

    #[test]
    fn the_build_shown_is_of_the_role_asked_for_or_of_the_one_played_most() {
        let roles = [Role::Offlane, Role::HardSupport];
        let mut chosen = Chosen::default();
        assert_eq!(chosen.role_of(&roles), Some(Role::Offlane), "nothing asked for: the most played");
        chosen.set("axe", Some(Role::HardSupport));
        assert_eq!((chosen.hero.as_deref(), chosen.role_of(&roles)), (Some("axe"), Some(Role::HardSupport)));
        chosen.search = "ax".to_string();
        chosen.set("axe", Some(Role::Mid));
        assert_eq!(chosen.role_of(&roles), Some(Role::Offlane));
        assert!(chosen.search.is_empty(), "what was typed has done its part");
        assert_eq!(chosen.role_of(&[]), None);
        assert_eq!(
            in_its_place("Crystal Maiden", Role::Carry, Role::HardSupport).as_deref(),
            Some("Too few play Crystal Maiden as Carry in this rank for a build of its own. This is the build of Hard support, the role Crystal Maiden is played in most.")
        );
        assert_eq!(in_its_place("Axe", Role::Offlane, Role::Offlane), None);
    }

    #[test]
    fn a_neutral_item_says_how_many_pros_take_it_and_who_they_are() {
        let neutral = NeutralBuild { role: Some(Role::Carry), players: 12, tiers: Vec::new() };
        let tier = NeutralTier { tier: 2, minute: 15, reached: 9, kept: 2, trinkets: vec![("item_brick", 44.4)], enhancements: Vec::new() };
        assert_eq!(neutral_words("Slark", &neutral, &tier, 44.4, true, "7.42"), "44 of 100 took it as their trinket of tier 2: of the 12 pros who played Slark as carry, the 9 who got that far, in pro matches of patch 7.42.");
        assert_eq!(
            tier_words("Slark", &neutral, &tier, "7.42"),
            "Opens at minute 15. Of the 12 pros who played Slark as carry, 9 made something of it, in pro matches of patch 7.42. 2 of them kept the trinket they had and took only a new enhancement."
        );
        let any = NeutralBuild { role: None, ..neutral };
        assert!(tier_words("Slark", &any, &NeutralTier { kept: 0, ..tier }, "7.42").ends_with("pros who played Slark in any role (too few played it in this one), 9 made something of it, in pro matches of patch 7.42."));
    }

    #[test]
    fn large_numbers_are_easy_to_read() {
        assert_eq!([with_spaces(0), with_spaces(950), with_spaces(1000), with_spaces(137_179), with_spaces(1_234_567)], ["0", "950", "1 000", "137 179", "1 234 567"]);
    }
}
