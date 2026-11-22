use dme_core::apply::EffectsFor;
use dme_core::catalog::{Catalog, Hero};
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align2, CornerRadius, CursorIcon, FontId, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{dim, tiles};
use crate::pictures::Pictures;
use crate::theme;

pub fn effects_for(ui: &mut Ui, catalog: Option<&Catalog>, current: &EffectsFor) -> Option<EffectsFor> {
    let mut chosen = None;
    let these = matches!(current, EffectsFor::These(_));
    if ui.radio(*current == EffectsFor::Picked, tr!(OnlyPicked)).clicked() && *current != EffectsFor::Picked {
        chosen = Some(EffectsFor::Picked);
    }
    if ui.radio(current.is_every(), tr!(EveryHero)).clicked() && !current.is_every() {
        chosen = Some(EffectsFor::Every);
    }
    if ui.radio(these, tr!(OnlyChosen)).clicked() && !these {
        chosen = Some(EffectsFor::These(Vec::new()));
    }
    if let (EffectsFor::These(heroes), Some(catalog)) = (current, catalog) {
        ui.indent("chosen heroes", |ui| {
            ui.horizontal_wrapped(|ui| {
                let name_of = |hero: &str| catalog.every_hero.iter().find(|(id, _)| id.eq_ignore_ascii_case(hero)).map(|(_, name)| name.clone());
                for hero in heroes {
                    let name = name_of(hero).unwrap_or_else(|| hero.clone());
                    let chip = ui.button(format!("{name}  ×")).on_hover_text(tr!(TakeOffList));
                    if chip.clicked() {
                        chosen = Some(EffectsFor::These(heroes.iter().filter(|other| *other != hero).cloned().collect()));
                    }
                }
                let others = catalog.every_hero.iter().filter(|(id, _)| !heroes.iter().any(|known| known.eq_ignore_ascii_case(id)));
                egui::ComboBox::from_id_salt("add a hero").selected_text(tr!(AddHero)).width(160.0).height(320.0).show_ui(ui, |ui| {
                    for (id, name) in others {
                        if ui.selectable_label(false, name.as_str()).clicked() {
                            let mut more = heroes.clone();
                            more.push(id.clone());
                            chosen = Some(EffectsFor::These(more));
                        }
                    }
                });
            });
            if heroes.is_empty() {
                ui.label(dim(tr!(NoHeroYet)));
            }
        });
    }
    chosen
}

pub fn language(ui: &mut Ui, offered: &[&'static crate::lang::Language]) -> Option<&'static str> {
    let current = crate::lang::current();
    let mut chosen = None;
    egui::ComboBox::from_id_salt("language").selected_text(current.name).width(200.0).show_ui(ui, |ui| {
        for language in offered {
            if ui.selectable_label(std::ptr::eq(*language, current), language.name).clicked() && !std::ptr::eq(*language, current) {
                chosen = Some(language.code);
            }
        }
    });
    chosen
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(crate::theme::title(text, 17.0));
    ui.add_space(2.0);
}

pub const POOL_LEAST: usize = 3;
pub const POOL_MOST: usize = 10;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct PoolState {
    pub extra: usize,
    pub choosing: Option<usize>,
    pub search: String,
    focus: bool,
}

impl PoolState {
    pub fn places(&self, heroes: usize) -> usize {
        (heroes.max(POOL_LEAST) + self.extra).min(POOL_MOST.max(heroes))
    }

    fn choose(&mut self, place: usize) {
        self.choosing = Some(place);
        self.search.clear();
        self.focus = true;
    }

    pub fn close(&mut self) {
        self.choosing = None;
        self.search.clear();
    }

    fn put(&mut self, pool: &[String], place: usize, hero: &str) -> Vec<String> {
        let mut new: Vec<String> = pool.to_vec();
        match new.get_mut(place) {
            Some(there) => *there = hero.to_string(),
            None => {
                let before = self.places(new.len());
                new.push(hero.to_string());
                self.extra = self.extra.saturating_sub(self.places(new.len()).saturating_sub(before));
            }
        }
        self.close();
        new
    }

    fn take_out(&mut self, pool: &[String], place: usize) -> Vec<String> {
        let before = self.places(pool.len());
        let new: Vec<String> = pool.iter().enumerate().filter(|(at, _)| *at != place).map(|(_, hero)| hero.clone()).collect();
        self.extra += before.saturating_sub(self.places(new.len()));
        self.close();
        new
    }

    fn take_away(&mut self, heroes: usize) {
        self.extra = self.extra.saturating_sub(1);
        if self.choosing.is_some_and(|choosing| choosing >= self.places(heroes) || choosing >= heroes) {
            self.close();
        }
    }

    fn one_more(&mut self, heroes: usize) {
        self.extra += 1;
        self.choose(heroes);
    }
}

pub fn hero_pool(ui: &mut Ui, catalog: Option<&Catalog>, pictures: &mut Pictures, pool: &[String], state: &mut PoolState) -> Option<Vec<String>> {
    const PLACE: egui::Vec2 = vec2(100.0, 84.0);
    const PICTURE: f32 = 56.0;
    let mut changed = None;
    let places = state.places(pool.len());
    let hero_of = |id: &str| catalog.and_then(|catalog| catalog.any_hero(id));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for place in 0..places {
            let id = pool.get(place);
            let hero = id.and_then(|id| hero_of(id));
            let (rect, response) = ui.allocate_exact_size(PLACE, Sense::click());
            let can_go = id.is_some() || place >= POOL_LEAST;
            let cross = Rect::from_center_size(rect.right_top() + vec2(-12.0, 12.0), vec2(20.0, 20.0));
            let gone = can_go.then(|| ui.interact(cross, response.id.with("out"), Sense::click()));
            let chosen = state.choosing == Some(place) || (id.is_none() && state.choosing == Some(pool.len()) && place == pool.len());
            if ui.is_rect_visible(rect) {
                let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered(), 0.12);
                let painter = ui.painter();
                let round = CornerRadius::same(8);
                painter.rect_filled(rect, round, theme::card().lerp_to_gamma(theme::card_hover(), lit));
                let edge = if chosen { Stroke::new(1.5, theme::accent()) } else { Stroke::new(1.0, theme::line()) };
                let picture = Rect::from_min_size(rect.min, vec2(PLACE.x, PICTURE));
                let name_at = pos2(rect.center().x, picture.bottom() + (PLACE.y - PICTURE) / 2.0);
                let font = FontId::proportional(theme::SMALL);
                match id {
                    Some(id) => {
                        tiles::fill_with(painter, pictures, hero.and_then(|hero| hero.picture.as_deref()), picture.shrink(1.0), 7, 1.0);
                        let name = hero.map_or(id.as_str(), |hero| hero.name.as_str());
                        let mut job = LayoutJob::simple(name.to_string(), font, theme::text(), PLACE.x - 10.0);
                        job.wrap.max_rows = 1;
                        job.wrap.overflow_character = Some('…');
                        let galley = painter.layout_job(job);
                        painter.galley(name_at - galley.size() / 2.0, galley, theme::text());
                    }
                    None => {
                        let ink = if chosen || lit > 0.5 { theme::text() } else { theme::text_dim() };
                        painter.text(picture.center() + vec2(0.0, 4.0), Align2::CENTER_CENTER, "+", FontId::proportional(24.0), ink);
                        painter.text(name_at, Align2::CENTER_CENTER, tr!(PoolChoose), font, ink);
                    }
                }
                painter.rect_stroke(rect, round, edge, StrokeKind::Inside);
                if let Some(gone) = &gone {
                    let ink = if gone.hovered() { theme::text() } else { theme::text_dim() };
                    painter.circle_filled(cross.center(), 9.0, theme::panel().gamma_multiply(0.85));
                    painter.text(cross.center(), Align2::CENTER_CENTER, "×", FontId::proportional(15.0), ink);
                }
            }
            if let Some(gone) = gone
                && gone.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(if id.is_some() { tr!(PoolTakeOut) } else { tr!(PoolTakeAway) }).clicked()
            {
                match id {
                    Some(_) => changed = Some(state.take_out(pool, place)),
                    None => state.take_away(pool.len()),
                }
                continue;
            }
            let hint = match hero {
                Some(hero) => tr!(PoolChange, hero = hero.name),
                None => tr!(PoolChooseHint).to_string(),
            };
            if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).clicked() {
                match chosen {
                    true => state.close(),
                    false => state.choose(place.min(pool.len())),
                }
            }
        }
        if places < POOL_MOST {
            let (rect, response) = ui.allocate_exact_size(vec2(64.0, PLACE.y), Sense::click());
            if ui.is_rect_visible(rect) {
                let ink = if response.hovered() { theme::text() } else { theme::text_dim() };
                let painter = ui.painter();
                painter.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, theme::line()), StrokeKind::Inside);
                painter.text(rect.center() - vec2(0.0, 9.0), Align2::CENTER_CENTER, "+", FontId::proportional(20.0), ink);
                painter.text(rect.center() + vec2(0.0, 14.0), Align2::CENTER_CENTER, tr!(PoolMore), FontId::proportional(theme::SMALL), ink);
            }
            if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tr!(PoolMoreHint)).clicked() {
                state.one_more(pool.len());
            }
        }
    });

    let Some(place) = state.choosing else { return changed };
    let Some(catalog) = catalog else {
        ui.label(dim(tr!(PoolNoGame)).small());
        return changed;
    };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let typed = ui.add(egui::TextEdit::singleline(&mut state.search).hint_text(tr!(HeroNameHint)).desired_width(220.0));
        if std::mem::take(&mut state.focus) {
            typed.request_focus();
        }
        if ui.button(tr!(PoolCancel)).clicked() {
            state.close();
        }
    });
    if state.choosing.is_none() {
        return changed;
    }
    let wanted = state.search.trim().to_lowercase();
    let found: Vec<&Hero> = super::heroes::found(catalog.all_heroes(), &wanted).into_iter().filter(|hero| !pool.contains(&hero.id)).collect();
    let mut chosen = None;
    if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        chosen = found.first().map(|hero| hero.id.clone());
    }
    if found.is_empty() {
        ui.label(dim(tr!(NoHeroNamed)).small());
    }
    egui::ScrollArea::vertical().id_salt("pool heroes").max_height(170.0).auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 5.0);
            for hero in &found {
                if super::draft::chip(ui, pictures, hero.picture.as_deref(), &hero.name, false).on_hover_text(tr!(PoolPutHint)).clicked() {
                    chosen = Some(hero.id.clone());
                }
            }
        });
    });
    if let Some(hero) = chosen {
        changed = Some(state.put(pool, place, &hero));
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn the_pool_has_three_places_to_begin_with_and_one_for_every_hero_of_a_larger_one() {
        let state = PoolState::default();
        assert_eq!((state.places(0), state.places(2), state.places(3)), (3, 3, 3));
        assert_eq!(state.places(5), 5, "a pool saved with five heroes shows all five");
        assert_eq!(state.places(14), 14, "and one saved larger than the most there can be, whole too");
    }

    #[test]
    fn places_are_added_one_at_a_time_and_an_empty_one_goes_away_again() {
        let mut state = PoolState::default();
        let three = pool(&["axe", "lina", "puck"]);
        state.one_more(three.len());
        assert_eq!((state.places(3), state.choosing), (4, Some(3)), "the new place is the one to fill");
        let four = state.put(&three, 3, "zuus");
        assert_eq!(four, ["axe", "lina", "puck", "zuus"]);
        assert_eq!((state.places(4), state.extra, state.choosing), (4, 0, None));

        let three = state.take_out(&four, 1);
        assert_eq!(three, ["axe", "puck", "zuus"]);
        assert_eq!(state.places(3), 4);
        state.take_away(3);
        assert_eq!(state.places(3), 3);
        state.take_away(3);
        assert_eq!(state.places(3), 3, "never fewer than three");

        assert_eq!(state.put(&three, 0, "invoker"), ["invoker", "puck", "zuus"]);
        let one = state.put(&[], 0, "axe");
        assert_eq!((one.as_slice(), state.places(1)), (["axe".to_string()].as_slice(), 3));
        while state.places(3) < POOL_MOST {
            state.one_more(3);
        }
        assert_eq!(state.places(3), POOL_MOST);
    }
}
