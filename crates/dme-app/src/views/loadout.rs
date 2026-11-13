use std::collections::BTreeSet;

use dme_core::apply::{EffectsFor, Pick};
use dme_core::catalog::{Entry, Gap, Group, Hero, Rarity, Set, Slot};
use dme_core::loadout::Loadout;
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::tiles::{self, Detail, Look, Tile, WHOLE, fit};
use super::{dim, fits, rarity, title_bar};
use crate::pictures::Pictures;
use crate::theme;

const LIST_WIDTH: f32 = 310.0;
pub(super) const ROW_HEIGHT: f32 = 60.0;
const ITEM_LOOK: Look = Look { picture: vec2(150.0, 100.0), name_lines: 2, note: false, name_over: false };
const SET_LOOK: Look = Look { picture: vec2(110.0, 167.0), name_lines: 3, note: true, name_over: false };

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Sets,
    Slot(String),
}

pub struct Page<'a> {
    pub hero: Option<&'a Hero>,
    pub slots: &'a [Slot],
    pub loadout: &'a mut Loadout,
    pub pictures: &'a mut Pictures,
    pub row: &'a mut Row,
    pub search: &'a mut String,
    pub rarities: &'a [Rarity],
    pub asked: &'a mut BTreeSet<usize>,
}

pub fn show(ui: &mut Ui, mut page: Page<'_>) -> bool {
    page.open_a_row_that_exists();
    let mut changed = false;
    egui::Panel::left("slots")
        .exact_size(LIST_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 14, top: 0, bottom: 0 }))
        .show(ui, |ui| changed |= page.list(ui));
    egui::CentralPanel::no_frame().show(ui, |ui| changed |= page.choices(ui));
    changed
}

impl<'a> Page<'a> {
    fn hero_id(&self) -> Option<&'a str> {
        self.hero.map(|hero| hero.id.as_str())
    }

    fn open_a_row_that_exists(&mut self) {
        let has_sets = self.hero.is_some_and(|hero| !hero.sets.is_empty());
        let exists = match &*self.row {
            Row::Sets => has_sets,
            Row::Slot(id) => self.slots.iter().any(|slot| slot.id == *id),
        };
        if !exists {
            let first_slot = self.slots.first().map(|slot| Row::Slot(slot.id.clone()));
            *self.row = if has_sets { Row::Sets } else { first_slot.unwrap_or(Row::Sets) };
        }
    }

    fn list(&mut self, ui: &mut Ui) -> bool {
        let hero_id = self.hero_id();
        let mut changed = false;
        egui::ScrollArea::vertical().id_salt("slot list").auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;

            if let Some(hero) = self.hero.filter(|hero| !hero.sets.is_empty()) {
                let worn = worn_set(hero, self.loadout);
                let any = self.loadout.of(hero_id).next().is_some();
                let warning = worn.and_then(|set| set_gaps_note(set, self.slots));
                let look = RowLook {
                    label: "Whole set",
                    value: match worn {
                        Some(set) => &set.name,
                        None if any => "Mixed pieces",
                        None => "Default look",
                    },
                    picked: any,
                    picture: worn.and_then(|set| set.picture.as_deref()).or(hero.picture.as_deref()),
                    band: worn.and_then(|set| rarity::color(self.rarities, set.rarity)),
                    open: *self.row == Row::Sets,
                    clear_hint: "Back to the default look",
                    warning: warning.as_deref(),
                };
                let answer = row(ui, self.pictures, &look);
                if answer.cleared {
                    self.loadout.clear_owner(hero_id);
                    changed = true;
                }
                if answer.clicked {
                    *self.row = Row::Sets;
                    self.search.clear();
                }
            }

            let mut group = Group::Worn;
            for slot in self.slots {
                if slot.group() != group {
                    group = slot.group();
                    caption(ui, group_name(group));
                }
                let pick = self.loadout.get(hero_id, &slot.id);
                let picked = pick.and_then(|pick| Some((pick, slot.entry(&pick.item)?)));
                let (value, picture) = match picked {
                    Some((pick, entry)) => in_words(slot, pick, entry),
                    None => ("Default".to_string(), slot.default.picture.as_deref()),
                };
                let warning = picked.and_then(|(_, entry)| gaps_note(&entry.gaps));
                let look = RowLook {
                    label: &slot.name,
                    value: &value,
                    picked: picked.is_some(),
                    picture,
                    band: picked.and_then(|(_, entry)| rarity::color(self.rarities, entry.rarity)),
                    open: matches!(&*self.row, Row::Slot(id) if *id == slot.id),
                    clear_hint: "Back to the default",
                    warning: warning.as_deref(),
                };
                let answer = row(ui, self.pictures, &look);
                if answer.cleared {
                    self.loadout.clear(hero_id, &slot.id);
                    changed = true;
                }
                if answer.clicked {
                    *self.row = Row::Slot(slot.id.clone());
                    self.search.clear();
                }
            }
        });
        changed
    }

    fn choices(&mut self, ui: &mut Ui) -> bool {
        match self.row.clone() {
            Row::Sets => self.hero.is_some_and(|hero| self.sets(ui, hero)),
            Row::Slot(id) => self.slots.iter().find(|slot| slot.id == id).is_some_and(|slot| self.items(ui, slot)),
        }
    }

    fn sets(&mut self, ui: &mut Ui, hero: &Hero) -> bool {
        title_bar(ui, "Whole sets", None, Some((&mut *self.search, "Type to find a set")), |_| {});
        rarity::chips(ui, self.rarities, &hero.sets.iter().filter_map(|set| set.rarity).collect(), self.asked);

        let wanted = self.search.trim().to_lowercase();
        let named: Vec<&Set> = hero.sets.iter().filter(|set| fits(&set.name, &wanted)).collect();
        let shown: Vec<&Set> = named.iter().copied().filter(|set| rarity::wanted(self.asked, set.rarity)).collect();
        let default_first = wanted.is_empty();
        if shown.is_empty() {
            ui.label(dim(if named.is_empty() { "No set has that in its name." } else { "No set here is of a rarity you chose." }));
            if !default_first {
                return false;
            }
        }

        let hero_id = Some(hero.id.as_str());
        let bare = self.loadout.of(hero_id).next().is_none();
        let mut changed = false;
        tiles::grid(ui, ("sets", &hero.id), shown.len() + usize::from(default_first), SET_LOOK, |ui, index, look| {
            if default_first && index == 0 {
                let tile = Tile { picture: hero.picture.as_deref(), name: "Default look", note: None, badge: None, band: None, picked: bare, faint: true, warning: None, detail: None, mark: None };
                if tiles::tile(ui, self.pictures, look, &tile).clicked() && !bare {
                    self.loadout.clear_owner(hero_id);
                    changed = true;
                }
                return;
            }
            let set = shown[index - usize::from(default_first)];
            let worn = self.loadout.wears(&set.pieces);
            let note = pieces_note(set.pieces.len());
            let warning = set_gaps_note(set, self.slots);
            let detail = Detail { rarity: rarity::named(self.rarities, set.rarity), looks: Vec::new(), facts: pieces_of(set, self.slots) };
            let tile = Tile {
                picture: set.picture.as_deref(),
                name: &set.name,
                note: Some(&note),
                badge: None,
                band: rarity::color(self.rarities, set.rarity),
                picked: worn,
                faint: false,
                warning: warning.as_deref(),
                detail: Some(&detail),
                mark: None,
            };
            if tiles::tile(ui, self.pictures, look, &tile).clicked() && !worn {
                self.loadout.put_set(&set.pieces, hero.sets.iter().map(|set| set.pieces.as_slice()));
                for piece in &set.pieces {
                    if let Some(slot) = self.slots.iter().find(|slot| slot.id == piece.slot) {
                        make_way(self.loadout, slot);
                    }
                }
                changed = true;
            }
        });
        changed
    }

    fn items(&mut self, ui: &mut Ui, slot: &Slot) -> bool {
        let hero_id = self.hero_id();
        let mut changed = false;
        let current = self.loadout.get(hero_id, &slot.id).cloned();
        let current_entry = current.as_ref().and_then(|pick| slot.entry(&pick.item));
        let in_the_open = slot.id == dme_core::apply::PERSONA_SELECTOR || !slot.shared_with.is_empty();
        let note = match () {
            () if slot.is_for_effects() => Some(effects_note(current_entry)),
            () if !slot.shared_with.is_empty() && note_of(slot).is_none() => Some(shared_note(slot)),
            () => note_of(slot).map(str::to_string),
        };
        title_bar(ui, &slot.name, note.as_deref().filter(|_| !in_the_open), Some((&mut *self.search, "Type to search")), |_| {});
        if let Some(note) = note.filter(|_| in_the_open) {
            ui.label(dim(note));
            ui.add_space(6.0);
        }
        let for_heroes = self.hero.is_none() && (slot.is_for_an_event() || slot.is_for_an_emblem());
        if for_heroes && let Some(why) = reaching_nobody(self.loadout) {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(why).color(theme::warn()).size(theme::SMALL));
                let every = ui.button("Give it to every hero").on_hover_text("Every hero gets the effects of events and the emblem. Settings can change it again.");
                if every.clicked() {
                    self.loadout.set_effects_for(EffectsFor::Every);
                    changed = true;
                }
            });
            ui.add_space(6.0);
        }
        rarity::chips(ui, self.rarities, &slot.items.iter().filter_map(|entry| entry.rarity).collect(), self.asked);

        if let (Some(pick), Some(entry)) = (&current, current_entry) {
            if !entry.styles.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(dim(if slot.is_for_an_event() { "Level" } else { "Style" }));
                    for style in &entry.styles {
                        let chosen = pick.style == style.index;
                        if ui.selectable_label(chosen, style.name.as_str()).clicked() && !chosen {
                            self.loadout.set(pick.clone().styled(style.index));
                            changed = true;
                        }
                    }
                });
                ui.add_space(4.0);
            }
            if let Some(gem) = effect_row(ui, slot, pick, self.hero.is_some()) {
                self.loadout.set(gem);
                changed = true;
            }
            if let Some(warning) = gaps_note(&entry.gaps) {
                ui.label(RichText::new(warning).color(theme::warn()).size(theme::SMALL));
                ui.add_space(4.0);
            }
        }

        let wanted = self.search.trim().to_lowercase();
        let named: Vec<&Entry> = slot.items.iter().filter(|entry| fits(&entry.name, &wanted)).collect();
        let shown: Vec<&Entry> = named.iter().copied().filter(|entry| rarity::wanted(self.asked, entry.rarity)).collect();
        let default_first = wanted.is_empty();
        if shown.is_empty() {
            ui.label(dim(if named.is_empty() { "Nothing here has that in its name." } else { "Nothing here is of a rarity you chose." }));
            if !default_first {
                return changed;
            }
        }

        tiles::grid(ui, ("items", hero_id, &slot.id), shown.len() + usize::from(default_first), ITEM_LOOK, |ui, index, look| {
            if default_first && index == 0 {
                let picked = current.is_none();
                let tile = Tile { picture: slot.default.picture.as_deref(), name: "Default", note: None, badge: None, band: None, picked, faint: true, warning: None, detail: None, mark: None };
                if tiles::tile(ui, self.pictures, look, &tile).clicked() && current.is_some() {
                    self.loadout.clear(hero_id, &slot.id);
                    changed = true;
                }
                return;
            }
            let entry = shown[index - usize::from(default_first)];
            let picked = current.as_ref().filter(|pick| pick.item == entry.id);
            let picture = match picked {
                Some(pick) => entry.in_style(pick.style).1,
                None => entry.picture.as_deref(),
            };
            let styles = styles_note(entry.styles.len(), slot.is_for_an_event());
            let warning = gaps_note(&entry.gaps);
            let detail = Detail {
                rarity: rarity::named(self.rarities, entry.rarity),
                looks: entry.styles.iter().map(|style| (style.name.as_str(), style.picture.as_deref().or(entry.picture.as_deref()))).collect(),
                facts: Vec::new(),
            };
            let tile = Tile {
                picture,
                name: &entry.name,
                note: None,
                badge: styles.as_deref(),
                band: rarity::color(self.rarities, entry.rarity),
                picked: picked.is_some(),
                faint: false,
                warning: warning.as_deref(),
                detail: Some(&detail),
                mark: None,
            };
            if tiles::tile(ui, self.pictures, look, &tile).clicked() && picked.is_none() {
                self.loadout.set(pick_of(hero_id, slot, entry, current.as_ref()));
                make_way(self.loadout, slot);
                changed = true;
            }
        });
        changed
    }
}

pub(super) fn in_words<'a>(slot: &'a Slot, pick: &Pick, entry: &'a Entry) -> (String, Option<&'a str>) {
    let (style, picture) = entry.in_style(pick.style);
    let mut value = match style.filter(|_| pick.style > 0) {
        Some(style) => format!("{} ({style})", entry.name),
        None => entry.name.clone(),
    };
    if let Some(effect) = pick.effect.and_then(|id| slot.effect(id)) {
        value = format!("{value} + {}", effect.name);
    }
    (value, picture)
}

fn pieces_of(set: &Set, slots: &[Slot]) -> Vec<String> {
    let piece = |pick: &Pick| {
        let slot = slots.iter().find(|slot| slot.id == pick.slot)?;
        Some(format!("{}: {}", slot.name, slot.entry(&pick.item)?.name))
    };
    set.pieces.iter().filter_map(piece).collect()
}

pub(crate) fn make_way(loadout: &mut Loadout, slot: &Slot) {
    for other in &slot.shared_with {
        loadout.clear(Some(&other.id), &slot.id);
    }
}

fn shared_note(slot: &Slot) -> String {
    let names: Vec<&str> = slot.shared_with.iter().map(|other| other.name.as_str()).collect();
    format!("The game has one default here for several heroes. What you pick shows on all of them, and only one of them can have a pick: {}.", names.join(", "))
}

fn pick_of(hero: Option<&str>, slot: &Slot, entry: &Entry, before: Option<&Pick>) -> Pick {
    let effect = before.and_then(|pick| pick.effect).and_then(|id| slot.effect(id));
    let pick = match hero {
        Some(hero) => Pick::hero(hero, &slot.id, &entry.id),
        None => Pick::world(&slot.id, &entry.id),
    };
    pick.styled(entry.first_style()).with_effect(effect.map(|effect| effect.id))
}

fn effect_row(ui: &mut Ui, slot: &Slot, pick: &Pick, on_a_hero: bool) -> Option<Pick> {
    if slot.effects.is_empty() {
        return None;
    }
    let mut changed = None;
    let effect = pick.effect.and_then(|id| slot.effect(id));
    let label = |name: &str, for_couriers: bool| if on_a_hero && for_couriers { format!("{name} (courier)") } else { name.to_string() };
    ui.horizontal(|ui| {
        ui.label(dim("Effect"));
        let shown = effect.map_or("None".to_string(), |effect| label(&effect.name, effect.for_couriers));
        egui::ComboBox::from_id_salt(("effect", &slot.id)).selected_text(shown).width(240.0).height(320.0).show_ui(ui, |ui| {
            if ui.selectable_label(effect.is_none(), "None").clicked() && effect.is_some() {
                changed = Some(pick.clone().with_effect(None));
            }
            for choice in &slot.effects {
                let chosen = effect.is_some_and(|effect| effect.id == choice.id);
                if ui.selectable_label(chosen, label(&choice.name, choice.for_couriers)).clicked() && !chosen {
                    changed = Some(pick.clone().with_effect(Some(choice.id)));
                }
            }
        });
    });
    if on_a_hero && effect.is_some_and(|effect| effect.for_couriers) {
        ui.label(dim(COURIER_EFFECT_NOTE).small());
    }
    ui.add_space(4.0);
    changed
}

const COURIER_EFFECT_NOTE: &str = "An experiment: this effect is made for couriers. On a hero it may sit lower or look smaller than it does on a courier.";

pub(super) fn gaps_note(gaps: &[Gap]) -> Option<String> {
    let said: Vec<&str> = gaps
        .iter()
        .map(|gap| match gap {
            Gap::Files => "Dota no longer has some of the files this is made of, so a part of it is missing for everybody.",
            Gap::ArcanaLevel => "Some of its effects go by the arcana's level, which Dota's servers give only to the player who owns it, so they may not show.",
            Gap::Checkered => "Buggy: its fog of war may look like a checkerboard.",
        })
        .collect();
    (!said.is_empty()).then(|| said.join(" "))
}

pub(super) fn set_gaps_note(set: &Set, slots: &[Slot]) -> Option<String> {
    let piece = |pick: &Pick| {
        let entry = slots.iter().find(|slot| slot.id == pick.slot)?.entry(&pick.item)?;
        Some((entry.name.as_str(), gaps_note(&entry.gaps)?))
    };
    let mut said: Vec<(Vec<&str>, String)> = Vec::new();
    for (name, note) in set.pieces.iter().filter_map(piece) {
        match said.iter_mut().find(|(_, known)| *known == note) {
            Some((names, _)) => names.push(name),
            None => said.push((vec![name], note)),
        }
    }
    let lines: Vec<String> = said.iter().map(|(names, note)| format!("{}: {note}", names.join(", "))).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn worn_set<'a>(hero: &'a Hero, loadout: &Loadout) -> Option<&'a Set> {
    hero.sets.iter().filter(|set| loadout.wears(&set.pieces)).max_by_key(|set| set.pieces.len())
}

fn pieces_note(pieces: usize) -> String {
    match pieces {
        1 => "1 piece".to_string(),
        many => format!("{many} pieces"),
    }
}

fn group_name(group: Group) -> &'static str {
    match group {
        Group::Worn => "Worn",
        Group::Effects => "Ability effects",
        Group::Persona => "Persona",
        Group::PersonaEffects => "Persona's ability effects",
        Group::Events => "Effects",
        Group::Experiments => "Experiments",
    }
}

fn note_of(slot: &Slot) -> Option<&'static str> {
    if slot.id == dme_core::apply::PERSONA_SELECTOR {
        Some(PERSONA_NOTE)
    } else if slot.is_for_an_event() {
        Some(EVENT_NOTE)
    } else if slot.is_for_an_emblem() {
        Some(EMBLEM_NOTE)
    } else if slot.is_for_trees() {
        Some(TREES_NOTE)
    } else if slot.is_for_the_map() {
        Some(MAP_NOTE)
    } else {
        None
    }
}

fn reaching_nobody(loadout: &Loadout) -> Option<&'static str> {
    match loadout.effects_for() {
        _ if loadout.effects_for().reaches_anyone(loadout.picks()) => None,
        EffectsFor::These(_) => Some("No hero gets this yet. It goes on the heroes you choose in Settings, and you have chosen none."),
        _ => Some("No hero gets this yet. It goes on the heroes you picked something for, and nothing is picked for a hero."),
    }
}

const EVENT_NOTE: &str = "Use the effect an event had for this. It goes on the heroes you picked something for; Settings can make that every hero, or heroes you choose. \
     A hero shows it as long as one of its slots in Dota is on its default item.";

const EMBLEM_NOTE: &str = "Put the glow of an event's emblem around the heroes you picked something for; Settings can make that every hero, or heroes you choose. \
     It rides on one default item of each hero, so a hero may not show it if you wear items of your own on it in Dota.";

const PERSONA_NOTE: &str = "An experiment: a persona is put on the hero's own items, so it shows without your owning it. \
     It takes the place of what the hero wears, so picks for those slots are taken off. In Dota, unequip the hero's own items for it to show.";

const TREES_NOTE: &str = "An experiment. Puts the leaves of one of Valve's seasonal terrains on the trees of the normal map. \
     If a tree looks wrong in Dota, pick Default here.";

const MAP_NOTE: &str = "An experiment. Draws the whole map the way one of Valve's events did. If the game looks wrong, pick Default here.";

fn effects_note(picked: Option<&Entry>) -> String {
    const WHAT: &str = "Use the effects an item has for this ability, whatever your hero wears.";
    match picked.and_then(|entry| entry.worn_in.as_deref()) {
        Some(slot) => format!("{WHAT} If they do not show in Dota, unequip your own item in the {slot} slot there."),
        None => WHAT.to_string(),
    }
}

fn caption(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let line = rect.bottom() - 9.0;
    ui.painter().text(pos2(rect.left() + 12.0, line), Align2::LEFT_CENTER, text, FontId::proportional(theme::SMALL), theme::text_dim());
    let words = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(theme::SMALL), theme::text_dim()).size().x;
    let stroke = Stroke::new(1.0, theme::line());
    ui.painter().line_segment([pos2(rect.left() + 22.0 + words, line), pos2(rect.right() - 10.0, line)], stroke);
}

fn styles_note(styles: usize, of_an_event: bool) -> Option<String> {
    (styles > 1).then(|| format!("{styles} {}", if of_an_event { "levels" } else { "styles" }))
}

pub(super) struct RowLook<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub picked: bool,
    pub picture: Option<&'a str>,
    pub band: Option<Color32>,
    pub open: bool,
    pub clear_hint: &'a str,
    pub warning: Option<&'a str>,
}

pub(super) struct RowAnswer {
    pub clicked: bool,
    pub cleared: bool,
}

pub(super) fn row(ui: &mut Ui, pictures: &mut Pictures, look: &RowLook<'_>) -> RowAnswer {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let fill = if look.open {
            theme::card_hover()
        } else if response.hovered() {
            theme::card()
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(rect, CornerRadius::same(theme::ROUND), fill);
        if look.open {
            let bar = Rect::from_min_size(rect.min + vec2(0.0, 12.0), vec2(3.0, rect.height() - 24.0));
            painter.rect_filled(bar, CornerRadius::same(2), theme::accent());
        }

        let thumb = Rect::from_min_size(rect.min + vec2(12.0, 8.0), vec2(66.0, 44.0));
        painter.rect_filled(thumb, CornerRadius::same(4), theme::well());
        if let Some((texture, size)) = look.picture.and_then(|path| pictures.get(path)) {
            let tint = if look.picked { Color32::WHITE } else { Color32::from_gray(120) };
            painter.image(texture, fit(size, thumb), WHOLE, tint);
        }
        if let Some(color) = look.band {
            tiles::rarity_band(&painter, thumb, color, 4);
        }
        if look.warning.is_some() {
            tiles::warning_mark(&painter, pos2(thumb.left() + 10.0, thumb.top() + 10.0));
        }

        let left = thumb.right() + 10.0;
        let width = rect.right() - left - if look.picked { 38.0 } else { 10.0 };
        painter.text(pos2(left, rect.top() + 11.0), Align2::LEFT_TOP, look.label, FontId::proportional(theme::SMALL), theme::text_dim());
        let color = if look.picked { theme::text() } else { theme::text_dim() };
        let mut value = LayoutJob::simple(look.value.to_string(), FontId::proportional(theme::BODY), color, width);
        value.wrap.max_rows = 1;
        value.wrap.break_anywhere = true;
        value.wrap.overflow_character = Some('…');
        painter.galley(pos2(left, rect.top() + 29.0), painter.layout_job(value), color);
    }

    let mut cleared = false;
    if look.picked {
        let button = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), vec2(26.0, 26.0));
        let cross = ui.interact(button, response.id.with("clear"), Sense::click());
        let painter = ui.painter();
        if cross.hovered() {
            painter.rect_filled(button, CornerRadius::same(13), theme::line());
        }
        let stroke = Stroke::new(1.6, if cross.hovered() { theme::text() } else { theme::text_dim() });
        let (center, arm) = (button.center(), 4.5);
        painter.line_segment([center + vec2(-arm, -arm), center + vec2(arm, arm)], stroke);
        painter.line_segment([center + vec2(-arm, arm), center + vec2(arm, -arm)], stroke);
        cleared = cross.clicked();
        cross.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(look.clear_hint);
    }
    let clicked = response.clicked() && !cleared;
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if let Some(warning) = look.warning {
        response.on_hover_text(warning);
    }
    RowAnswer { clicked, cleared }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXE: &str = "npc_dota_hero_axe";

    fn set(key: &str, pieces: &[(&str, &str)]) -> Set {
        let pieces = pieces.iter().map(|(slot, item)| Pick::hero(AXE, slot, item)).collect();
        Set { key: key.to_string(), name: key.to_string(), picture: None, pieces, rarity: None }
    }

    fn hero(sets: Vec<Set>) -> Hero {
        Hero { id: AXE.to_string(), name: "Axe".to_string(), aliases: Vec::new(), attribute: None, picture: None, sets, slots: Vec::new() }
    }

    fn entry(id: &str) -> Entry {
        Entry { id: id.to_string(), name: id.to_string(), picture: None, styles: Vec::new(), rarity: None, worn_in: None, gaps: Vec::new() }
    }

    #[test]
    fn a_page_for_the_effects_of_events_says_when_they_would_go_on_no_hero() {
        let mut loadout = Loadout::default();
        assert!(reaching_nobody(&loadout).is_some_and(|why| why.contains("picked something for")));
        loadout.set(Pick::world("emblem", "638"));
        assert!(reaching_nobody(&loadout).is_some());
        loadout.set(Pick::hero(AXE, "weapon", "5"));
        assert_eq!(reaching_nobody(&loadout), None, "Axe gets them");

        let mut chosen = Loadout::default();
        chosen.set_effects_for(EffectsFor::These(Vec::new()));
        assert!(reaching_nobody(&chosen).is_some_and(|why| why.contains("Settings")));
        chosen.set_effects_for(EffectsFor::These(vec![AXE.to_string()]));
        assert_eq!(reaching_nobody(&chosen), None);
        chosen.set_effects_for(EffectsFor::Every);
        assert_eq!(reaching_nobody(&chosen), None);
    }

    #[test]
    fn an_effect_stays_with_the_slot_where_it_can() {
        use dme_core::catalog::Effect;
        let effect = |id: u32| Effect { id, name: id.to_string(), for_couriers: true };
        let courier = Slot {
            id: "courier".to_string(),
            name: "Courier".to_string(),
            default: entry("0"),
            items: vec![entry("1"), entry("2")],
            effects: vec![effect(15), effect(21)],
            shared_with: Vec::new(),
        };
        let (plain, other) = (&courier.items[0], &courier.items[1]);

        assert_eq!(pick_of(None, &courier, plain, None), Pick::world("courier", "1"));
        assert_eq!(pick_of(Some(AXE), &courier, plain, None), Pick::hero(AXE, "courier", "1"));
        let flame = Pick::world("courier", "1").with_effect(Some(21));
        assert_eq!(pick_of(None, &courier, other, Some(&flame)), Pick::world("courier", "2").with_effect(Some(21)));
        let odd = Pick::world("courier", "1").with_effect(Some(99));
        assert_eq!(pick_of(None, &courier, plain, Some(&odd)), Pick::world("courier", "1"));
    }

    #[test]
    fn what_may_not_show_is_said_in_plain_words() {
        assert_eq!(gaps_note(&[]), None);
        assert!(gaps_note(&[Gap::Files]).unwrap().starts_with("Dota no longer has some of the files"));
        let both = gaps_note(&[Gap::Files, Gap::ArcanaLevel]).unwrap();
        assert!(both.contains("missing for everybody. Some of its effects go by the arcana's level"));
        assert!(gaps_note(&[Gap::Checkered]).unwrap().starts_with("Buggy: its fog of war"));
    }

    #[test]
    fn a_set_says_which_of_its_pieces_may_not_show() {
        let slot = |id: &str, items: Vec<Entry>| Slot { id: id.to_string(), name: id.to_string(), default: entry("0"), items, effects: Vec::new(), shared_with: Vec::new() };
        let old = |id: &str, name: &str, gap: Gap| Entry { name: name.to_string(), gaps: vec![gap], ..entry(id) };
        let slots = [slot("weapon", vec![old("1", "Old Axe", Gap::Files)]), slot("head", vec![entry("2")]), slot("belt", vec![old("3", "Old Belt", Gap::Files)]), slot("arms", vec![old("4", "Arms", Gap::ArcanaLevel)])];

        assert_eq!(set_gaps_note(&set("whole", &[("head", "2")]), &slots), None);
        let note = set_gaps_note(&set("old", &[("weapon", "1"), ("head", "2"), ("belt", "3"), ("arms", "4")]), &slots).unwrap();
        let lines: Vec<&str> = note.lines().collect();
        assert_eq!(lines.len(), 2, "said once for the pieces it is the same for, and not of the piece that is whole: {note}");
        assert!(lines[0].starts_with("Old Axe, Old Belt: Dota no longer has some of the files"), "{note}");
        assert!(lines[1].starts_with("Arms: Some of its effects go by the arcana's level"), "{note}");
    }

    #[test]
    fn the_set_a_hero_wears_all_of_is_named() {
        let hero = hero(vec![
            set("small", &[("weapon", "1"), ("head", "2")]),
            set("large", &[("weapon", "1"), ("head", "2"), ("belt", "3")]),
            set("other", &[("weapon", "9")]),
        ]);
        let mut loadout = Loadout::default();
        assert!(worn_set(&hero, &loadout).is_none());

        loadout.set(Pick::hero(AXE, "weapon", "1"));
        assert!(worn_set(&hero, &loadout).is_none(), "one piece of two is a mix");
        loadout.set(Pick::hero(AXE, "head", "2").styled(1));
        assert_eq!(worn_set(&hero, &loadout).unwrap().key, "small");
        loadout.set(Pick::hero(AXE, "belt", "3"));
        assert_eq!(worn_set(&hero, &loadout).unwrap().key, "large", "the set with the most pieces");
    }

    #[test]
    fn a_pick_for_a_default_item_that_heroes_share_takes_the_place_of_the_others() {
        use dme_core::catalog::Sharer;
        const SVEN: &str = "npc_dota_hero_sven";
        let sharer = |id: &str, name: &str| Sharer { id: id.to_string(), name: name.to_string() };
        let pet = Slot {
            id: "summon".to_string(),
            name: "Pet".to_string(),
            default: entry("8366"),
            items: vec![entry("1")],
            effects: Vec::new(),
            shared_with: vec![sharer(SVEN, "Sven"), sharer("npc_dota_hero_riki", "Riki")],
        };
        let mut loadout = Loadout::default();
        loadout.set(Pick::hero(SVEN, "summon", "7"));
        loadout.set(Pick::hero(SVEN, "weapon", "8"));
        loadout.set(Pick::hero(AXE, "summon", "1"));
        make_way(&mut loadout, &pet);
        assert_eq!(loadout.picks(), [Pick::hero(SVEN, "weapon", "8"), Pick::hero(AXE, "summon", "1")], "the other hero's pet goes, and nothing else");
        assert!(shared_note(&pet).ends_with("only one of them can have a pick: Sven, Riki."));
    }

    #[test]
    fn a_page_opens_on_a_row_it_has() {
        let slot = |id: &str| Slot { id: id.to_string(), name: id.to_string(), default: entry("0"), items: Vec::new(), effects: Vec::new(), shared_with: Vec::new() };
        let slots = [slot("courier"), slot("ward")];
        let with_sets = hero(vec![set("a", &[("weapon", "1")])]);
        let without_sets = hero(Vec::new());
        let opens = |hero: Option<&Hero>, row: Row| {
            let ctx = egui::Context::default();
            let (mut loadout, mut pictures, mut row, mut search) = (Loadout::default(), Pictures::new(&ctx), row, String::new());
            let mut page = Page {
                hero,
                slots: &slots,
                loadout: &mut loadout,
                pictures: &mut pictures,
                row: &mut row,
                search: &mut search,
                rarities: &[],
                asked: &mut BTreeSet::new(),
            };
            page.open_a_row_that_exists();
            row
        };
        assert_eq!(opens(None, Row::Sets), Row::Slot("courier".to_string()), "the world has no sets");
        assert_eq!(opens(None, Row::Slot("ward".to_string())), Row::Slot("ward".to_string()));
        assert_eq!(opens(None, Row::Slot("gone".to_string())), Row::Slot("courier".to_string()));
        assert_eq!(opens(Some(&with_sets), Row::Slot("gone".to_string())), Row::Sets);
        assert_eq!(opens(Some(&without_sets), Row::Sets), Row::Slot("courier".to_string()));
    }

    #[test]
    fn notes_count_in_plain_words() {
        assert_eq!(pieces_note(1), "1 piece");
        assert_eq!(pieces_note(5), "5 pieces");
        assert_eq!(styles_note(0, false), None);
        assert_eq!(styles_note(1, true), None);
        assert_eq!(styles_note(3, false).as_deref(), Some("3 styles"));
        assert_eq!(styles_note(3, true).as_deref(), Some("3 levels"));
    }
}
