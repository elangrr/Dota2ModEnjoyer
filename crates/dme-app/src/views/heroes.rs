use dme_core::catalog::{Catalog, Hero};
use dme_core::loadout::Loadout;
use eframe::egui::{self, Ui, vec2};

use super::tiles::{self, Look, Tile};
use super::{dim, title_bar};
use crate::pictures::Pictures;

const LOOK: Look = Look { picture: vec2(160.0, 90.0), name_lines: 1, note: false };

pub fn show(ui: &mut Ui, catalog: &Catalog, loadout: &Loadout, pictures: &mut Pictures, search: &mut String) -> Option<String> {
    title_bar(ui, "Pick a hero", Some((search, "Type to find a hero")), |_| {});

    let heroes = found(&catalog.heroes, search);
    if heroes.is_empty() {
        ui.label(dim("No hero goes by that name."));
        return None;
    }
    let mut clicked = None;
    if !search.trim().is_empty() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
        clicked = Some(heroes[0].id.clone());
    }

    tiles::grid(ui, "heroes", heroes.len(), LOOK, |ui, index, look| {
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
        };
        if tiles::tile(ui, pictures, look, &tile).clicked() {
            clicked = Some(hero.id.clone());
        }
    });
    clicked
}

fn found<'c>(heroes: &'c [Hero], typed: &str) -> Vec<&'c Hero> {
    let wanted = typed.trim().to_lowercase();
    let mut fitting: Vec<(u8, &Hero)> = heroes.iter().filter_map(|hero| Some((hero.fits(&wanted)?, hero))).collect();
    fitting.sort_by_key(|(fit, _)| *fit);
    fitting.into_iter().map(|(_, hero)| hero).collect()
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
    use super::*;

    #[test]
    fn typing_finds_heroes_by_any_of_their_names_the_best_fit_first() {
        let hero = |name: &str, aliases: &[&str]| Hero {
            id: name.to_lowercase(),
            name: name.to_string(),
            aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
            picture: None,
            sets: Vec::new(),
            slots: Vec::new(),
        };
        let heroes = [hero("Anti-Mage", &["am"]), hero("Phantom Assassin", &["pa"]), hero("Shadow Shaman", &["ss"]), hero("Windranger", &["wr"])];
        let names = |typed: &str| -> Vec<&str> { found(&heroes, typed).iter().map(|hero| hero.name.as_str()).collect() };

        assert_eq!(names(""), ["Anti-Mage", "Phantom Assassin", "Shadow Shaman", "Windranger"]);
        assert_eq!(names("  "), names(""));
        assert_eq!(names("AM"), ["Anti-Mage", "Shadow Shaman"]);
        assert_eq!(names("ss"), ["Shadow Shaman", "Phantom Assassin"]);
        assert_eq!(names("wr "), ["Windranger"]);
        assert!(names("zeus").is_empty());
    }

    #[test]
    fn heroes_with_picks_say_how_many() {
        assert_eq!(picks_note(0), None);
        assert_eq!(picks_note(1).as_deref(), Some("1 pick"));
        assert_eq!(picks_note(5).as_deref(), Some("5 picks"));
    }
}
