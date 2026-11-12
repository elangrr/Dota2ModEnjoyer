use dme_core::catalog::{Catalog, Hero, Slot};
use dme_core::heroes::Attribute;
use dme_core::loadout::Loadout;
use eframe::egui::{self, Color32, Ui, vec2};

use super::loadout::Row;
use super::tiles::{self, Detail, Look, Tile};
use super::{Open, dim, fits, rarity, title_bar};
use crate::pictures::Pictures;

const LOOK: Look = Look { picture: vec2(176.0, 104.0), name_lines: 1, note: false, name_over: true };
const FOUND_LOOK: Look = Look { picture: vec2(150.0, 100.0), name_lines: 2, note: true, name_over: false };

const LETTERS_FOR_ITEMS: usize = 3;
const MOST_FOUND: usize = 120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub attribute: Option<Attribute>,
    pub picked: bool,
}

impl Filter {
    fn shows(&self, hero: &Hero, loadout: &Loadout) -> bool {
        self.attribute.is_none_or(|attribute| hero.attribute == Some(attribute)) && (!self.picked || loadout.of(Some(&hero.id)).next().is_some())
    }

    fn chips(&mut self, ui: &mut Ui, any_picks: bool) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            for attribute in Attribute::ALL {
                let on = self.attribute == Some(attribute);
                if rarity::pill(ui, attribute.name(), color_of(attribute), on).clicked() {
                    self.attribute = (!on).then_some(attribute);
                }
            }
            if (any_picks || self.picked) && rarity::pill(ui, "With picks", crate::theme::accent(), self.picked).clicked() {
                self.picked = !self.picked;
            }
            if *self != Filter::default() && ui.add(egui::Button::new(dim("Show all")).frame(false)).clicked() {
                *self = Filter::default();
            }
        });
        ui.add_space(4.0);
    }
}

fn color_of(attribute: Attribute) -> Color32 {
    match attribute {
        Attribute::Strength => Color32::from_rgb(0xe2, 0x4f, 0x3d),
        Attribute::Agility => Color32::from_rgb(0x4f, 0xc8, 0x5c),
        Attribute::Intelligence => Color32::from_rgb(0x4a, 0xa3, 0xe8),
        Attribute::Universal => Color32::from_rgb(0xb5, 0x86, 0xe8),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub hero: Option<String>,
    pub row: Row,
    pub name: String,
    pub picture: Option<String>,
    pub rarity: Option<usize>,
    pub place: String,
}

#[derive(Default)]
pub struct Found {
    wanted: String,
    hits: Vec<Hit>,
    more: bool,
}

impl Found {
    fn look_for(&mut self, catalog: &Catalog, wanted: &str) {
        if self.wanted == wanted {
            return;
        }
        self.wanted = wanted.to_string();
        (self.hits, self.more) = match wanted.chars().count() >= LETTERS_FOR_ITEMS {
            true => hits(catalog, wanted, MOST_FOUND),
            false => (Vec::new(), false),
        };
    }
}

pub fn show(ui: &mut Ui, catalog: &Catalog, loadout: &Loadout, pictures: &mut Pictures, search: &mut String, found: &mut Found, filter: &mut Filter) -> Option<Open> {
    title_bar(ui, "Pick a hero", None, Some((search, "Find a hero or an item")), |_| {});
    filter.chips(ui, !loadout.is_empty());

    let wanted = search.trim().to_lowercase();
    let named = self::found(&catalog.heroes, &wanted);
    let heroes: Vec<&Hero> = named.iter().copied().filter(|hero| filter.shows(hero, loadout)).collect();
    found.look_for(catalog, &wanted);
    if heroes.is_empty() && found.hits.is_empty() {
        let few = wanted.chars().count() < LETTERS_FOR_ITEMS;
        ui.label(dim(match (named.is_empty(), few) {
            (false, _) => "No hero here is of the kind you chose.",
            (true, true) => "No hero goes by that name. Type a letter more to look for items.",
            (true, false) => "No hero, item or set has that in its name.",
        }));
        return None;
    }
    let open_hero = |hero: &Hero| Open { hero: Some(hero.id.clone()), row: Row::Sets, search: String::new() };
    let open_hit = |hit: &Hit| Open { hero: hit.hero.clone(), row: hit.row.clone(), search: hit.name.clone() };
    let mut clicked = None;
    if !wanted.is_empty() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        clicked = heroes.first().map(|hero| open_hero(hero)).or_else(|| found.hits.first().map(open_hit));
    }

    let hero_tile = |ui: &mut Ui, index: usize, look: Look, pictures: &mut Pictures| {
        let hero = heroes[index];
        let picks = picks_note(loadout.of(Some(&hero.id)).count());
        let tile = Tile {
            picture: hero.picture.as_deref(),
            name: &hero.name,
            note: None,
            badge: picks.as_deref(),
            band: None,
            picked: picks.is_some(),
            faint: false,
            warning: None,
            detail: None,
            mark: hero.attribute.map(color_of),
        };
        tiles::tile(ui, pictures, look, &tile).clicked().then(|| open_hero(hero))
    };
    if found.hits.is_empty() {
        tiles::grid(ui, "heroes", heroes.len(), LOOK, |ui, index, look| clicked = hero_tile(ui, index, look, pictures).or(clicked.take()));
        return clicked;
    }

    egui::ScrollArea::vertical().id_salt("found").auto_shrink(false).show(ui, |ui| {
        if !heroes.is_empty() {
            heading(ui, "Heroes");
            tiles::flow(ui, heroes.len(), LOOK, |ui, index, look| clicked = hero_tile(ui, index, look, pictures).or(clicked.take()));
        }
        heading(ui, "Items and sets");
        tiles::flow(ui, found.hits.len(), FOUND_LOOK, |ui, index, look| {
            let hit = &found.hits[index];
            let detail = Detail { rarity: rarity::named(&catalog.rarities, hit.rarity), looks: Vec::new(), facts: vec![hit.place.clone()] };
            let tile = Tile {
                picture: hit.picture.as_deref(),
                name: &hit.name,
                note: Some(&hit.place),
                badge: None,
                band: rarity::color(&catalog.rarities, hit.rarity),
                picked: false,
                faint: false,
                warning: None,
                detail: Some(&detail),
                mark: None,
            };
            if tiles::tile(ui, pictures, look, &tile).clicked() {
                clicked = Some(open_hit(hit));
            }
        });
        if found.more {
            ui.add_space(6.0);
            ui.label(dim(format!("These are the first {MOST_FOUND}. Type more of the name to find fewer.")));
        }
    });
    clicked
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(crate::theme::title(text, 16.0));
    ui.add_space(2.0);
}

fn found<'c>(heroes: &'c [Hero], wanted: &str) -> Vec<&'c Hero> {
    let mut fitting: Vec<(u8, &Hero)> = heroes.iter().filter_map(|hero| Some((hero.fits(wanted)?, hero))).collect();
    fitting.sort_by_key(|(fit, _)| *fit);
    fitting.into_iter().map(|(_, hero)| hero).collect()
}

fn hits(catalog: &Catalog, wanted: &str, most: usize) -> (Vec<Hit>, bool) {
    let mut hits = Vec::new();
    let in_slots = |hero: Option<&Hero>, slots: &[Slot], hits: &mut Vec<Hit>| {
        let owner = hero.map_or("World", |hero| hero.name.as_str());
        for slot in slots {
            for entry in slot.items.iter().filter(|entry| fits(&entry.name, wanted)) {
                hits.push(Hit {
                    hero: hero.map(|hero| hero.id.clone()),
                    row: Row::Slot(slot.id.clone()),
                    name: entry.name.clone(),
                    picture: entry.picture.clone(),
                    rarity: entry.rarity,
                    place: format!("{owner} · {}", slot.name),
                });
            }
        }
    };
    in_slots(None, &catalog.world, &mut hits);
    for hero in &catalog.heroes {
        if hits.len() > most {
            break;
        }
        for set in hero.sets.iter().filter(|set| fits(&set.name, wanted)) {
            hits.push(Hit { hero: Some(hero.id.clone()), row: Row::Sets, name: set.name.clone(), picture: set.picture.clone(), rarity: set.rarity, place: format!("{} · Set", hero.name) });
        }
        in_slots(Some(hero), &hero.slots, &mut hits);
    }
    let more = hits.len() > most;
    hits.truncate(most);
    (hits, more)
}

fn picks_note(picks: usize) -> Option<String> {
    match picks {
        0 => None,
        1 => Some("1 pick".to_string()),
        many => Some(format!("{many} picks")),
    }
}

#[cfg(test)]
mod tests {
    use dme_core::apply::Pick;
    use dme_core::catalog::{Entry, Set};

    use super::*;

    fn hero(name: &str, aliases: &[&str]) -> Hero {
        Hero { id: name.to_lowercase(), name: name.to_string(), aliases: aliases.iter().map(|alias| alias.to_string()).collect(), attribute: None, picture: None, sets: Vec::new(), slots: Vec::new() }
    }

    fn slot(id: &str, items: &[&str]) -> Slot {
        let entry = |name: &&str| Entry { id: name.to_lowercase(), name: name.to_string(), picture: None, styles: Vec::new(), rarity: None, worn_in: None, gaps: Vec::new() };
        Slot { id: id.to_lowercase(), name: id.to_string(), default: entry(&"Default"), items: items.iter().map(entry).collect(), effects: Vec::new(), shared_with: Vec::new() }
    }

    #[test]
    fn typing_finds_heroes_by_any_of_their_names_the_best_fit_first() {
        let heroes = [hero("Anti-Mage", &["am"]), hero("Phantom Assassin", &["pa"]), hero("Shadow Shaman", &["ss"]), hero("Windranger", &["wr"])];
        let names = |typed: &str| -> Vec<&str> { found(&heroes, typed).iter().map(|hero| hero.name.as_str()).collect() };

        assert_eq!(names(""), ["Anti-Mage", "Phantom Assassin", "Shadow Shaman", "Windranger"]);
        assert_eq!(names("am"), ["Anti-Mage", "Shadow Shaman"]);
        assert_eq!(names("ss"), ["Shadow Shaman", "Phantom Assassin"]);
        assert_eq!(names("wr"), ["Windranger"]);
        assert!(names("zeus").is_empty());
    }

    #[test]
    fn typing_finds_items_and_sets_wherever_they_are() {
        let mut axe = hero("Axe", &[]);
        axe.sets = vec![Set { key: "red".to_string(), name: "Red Mist Reaper".to_string(), picture: None, pieces: vec![Pick::hero("axe", "weapon", "1")], rarity: Some(2) }];
        axe.slots = vec![slot("Weapon", &["Red Mist Axe", "Plain Axe"]), slot("Head", &["Mist Helm"])];
        let catalog = Catalog { heroes: vec![axe, hero("Lina", &[])], world: vec![slot("Courier", &["Misty the Donkey", "Baby Roshan"])], ..Catalog::default() };
        let told = |wanted: &str, most: usize| {
            let (hits, more) = hits(&catalog, wanted, most);
            (hits.into_iter().map(|hit| format!("{} ({})", hit.name, hit.place)).collect::<Vec<_>>(), more)
        };

        let all = ["Misty the Donkey (World · Courier)", "Red Mist Reaper (Axe · Set)", "Red Mist Axe (Axe · Weapon)", "Mist Helm (Axe · Head)"];
        assert_eq!(told("mist", 10), (all.map(str::to_string).to_vec(), false));
        assert_eq!(told("mist", 2), (all[..2].iter().map(|hit| hit.to_string()).collect(), true), "no more than asked for, and said so");
        assert_eq!(told("roshan", 10).0, ["Baby Roshan (World · Courier)"]);
        assert!(told("zzz", 10).0.is_empty());

        let (hits, _) = hits(&catalog, "mist", 10);
        assert_eq!((hits[1].hero.as_deref(), &hits[1].row), (Some("axe"), &Row::Sets));
        assert_eq!((hits[2].hero.as_deref(), &hits[2].row), (Some("axe"), &Row::Slot("weapon".to_string())));
        assert_eq!((hits[0].hero.as_deref(), &hits[0].row), (None, &Row::Slot("courier".to_string())));

        let mut found = Found::default();
        found.look_for(&catalog, "mi");
        assert!(found.hits.is_empty(), "two letters fit too much");
        found.look_for(&catalog, "mist");
        assert_eq!(found.hits.len(), 4);
    }

    #[test]
    fn the_grid_shows_the_heroes_of_the_kind_that_was_chosen() {
        let mut axe = hero("Axe", &[]);
        axe.attribute = Some(Attribute::Strength);
        let mut lina = hero("Lina", &[]);
        lina.attribute = Some(Attribute::Intelligence);
        let unknown = hero("New", &[]);
        let loadout = Loadout::from_picks([Pick::hero("lina", "head", "1")]);
        let shown = |filter: Filter| [&axe, &lina, &unknown].map(|hero| filter.shows(hero, &loadout));

        assert_eq!(shown(Filter::default()), [true, true, true]);
        assert_eq!(shown(Filter { attribute: Some(Attribute::Strength), picked: false }), [true, false, false]);
        assert_eq!(shown(Filter { attribute: None, picked: true }), [false, true, false]);
        assert_eq!(shown(Filter { attribute: Some(Attribute::Strength), picked: true }), [false, false, false]);
    }

    #[test]
    fn heroes_with_picks_say_how_many() {
        assert_eq!(picks_note(0), None);
        assert_eq!(picks_note(1).as_deref(), Some("1 pick"));
        assert_eq!(picks_note(5).as_deref(), Some("5 picks"));
    }
}
