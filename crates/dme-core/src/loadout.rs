use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::apply::{self, EffectsFor, Pick};
use crate::error::Result;
use crate::fsutil;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loadout {
    #[serde(default)]
    picks: Vec<Pick>,
    #[serde(default, rename = "off", skip_serializing_if = "std::ops::Not::not")]
    away: bool,
    #[serde(default, skip_serializing_if = "EffectsFor::is_every")]
    effects_for: EffectsFor,
}

impl Loadout {
    pub fn from_picks(picks: impl IntoIterator<Item = Pick>) -> Loadout {
        let mut loadout = Loadout::default();
        for pick in picks {
            loadout.set(pick);
        }
        loadout
    }

    pub fn picks(&self) -> &[Pick] {
        if self.away { &[] } else { &self.picks }
    }

    pub fn kept_away(&self) -> &[Pick] {
        if self.away { &self.picks } else { &[] }
    }

    pub fn is_put_away(&self) -> bool {
        self.away
    }

    pub fn put_away(&mut self) {
        self.away = !self.picks.is_empty();
    }

    pub fn bring_back(&mut self) {
        self.away = false;
    }

    pub fn effects_for(&self) -> &EffectsFor {
        &self.effects_for
    }

    pub fn set_effects_for(&mut self, effects_for: EffectsFor) {
        self.effects_for = effects_for;
    }

    pub fn len(&self) -> usize {
        self.picks().len()
    }

    pub fn is_empty(&self) -> bool {
        self.picks().is_empty()
    }

    pub fn get(&self, hero: Option<&str>, slot: &str) -> Option<&Pick> {
        self.picks().iter().find(|pick| is_place(pick, hero, slot))
    }

    pub fn of<'a>(&'a self, hero: Option<&'a str>) -> impl Iterator<Item = &'a Pick> {
        self.picks().iter().filter(move |pick| same_owner(pick.hero.as_deref(), hero))
    }

    pub fn set(&mut self, pick: Pick) {
        self.start_over_if_away();
        if let Some(hero) = pick.hero.clone() {
            let is_persona = |slot: &str| slot.eq_ignore_ascii_case(apply::PERSONA_SELECTOR);
            let gives_way = |other: &Pick| match is_persona(&pick.slot) {
                true => apply::persona_takes_the_place_of(&other.slot),
                false => apply::persona_takes_the_place_of(&pick.slot) && is_persona(&other.slot),
            };
            self.picks.retain(|other| !(same_owner(other.hero.as_deref(), Some(&hero)) && gives_way(other)));
        }
        match self.picks.iter_mut().find(|known| is_place(known, pick.hero.as_deref(), &pick.slot)) {
            Some(known) => *known = pick,
            None => self.picks.push(pick),
        }
    }

    pub fn clear(&mut self, hero: Option<&str>, slot: &str) -> bool {
        if self.away {
            return false;
        }
        let before = self.picks.len();
        self.picks.retain(|pick| !is_place(pick, hero, slot));
        self.picks.len() != before
    }

    pub fn clear_owner(&mut self, hero: Option<&str>) {
        if !self.away {
            self.picks.retain(|pick| !same_owner(pick.hero.as_deref(), hero));
        }
    }

    pub fn clear_all(&mut self) {
        if !self.away {
            self.picks.clear();
        }
    }

    pub fn forget(&mut self) {
        *self = Loadout { effects_for: std::mem::take(&mut self.effects_for), ..Loadout::default() };
    }

    pub fn settle(&mut self, settle: impl Fn(&Pick) -> Option<Pick>) {
        self.picks = self.picks.iter().filter_map(settle).collect();
        self.away &= !self.picks.is_empty();
    }

    fn start_over_if_away(&mut self) {
        if self.away {
            self.forget();
        }
    }

    pub fn wears(&self, pieces: &[Pick]) -> bool {
        let picked = |piece: &Pick| self.get(piece.hero.as_deref(), &piece.slot).is_some_and(|pick| pick.item == piece.item);
        !pieces.is_empty() && pieces.iter().all(picked)
    }

    pub fn put_set<'a>(&mut self, pieces: &[Pick], sets: impl IntoIterator<Item = &'a [Pick]>) {
        if !pieces.is_empty() {
            self.start_over_if_away();
        }
        let worn: Vec<&[Pick]> = sets.into_iter().filter(|set| self.wears(set)).collect();
        for piece in worn.into_iter().flatten() {
            self.clear(piece.hero.as_deref(), &piece.slot);
        }
        for piece in pieces {
            self.set(piece.clone());
        }
    }

    pub fn load(path: &Path) -> Loadout {
        let saved: Option<Loadout> = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let saved = saved.unwrap_or_default();
        let mut loadout = Loadout::from_picks(saved.picks);
        if saved.away {
            loadout.put_away();
        }
        loadout.effects_for = saved.effects_for;
        loadout
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(folder) = path.parent() {
            fsutil::create_dir_all(folder)?;
        }
        let json = serde_json::to_vec_pretty(self).expect("picks are plain data");
        fsutil::write_atomic(path, &json)
    }
}

fn is_place(pick: &Pick, hero: Option<&str>, slot: &str) -> bool {
    pick.slot.eq_ignore_ascii_case(slot) && same_owner(pick.hero.as_deref(), hero)
}

fn same_owner(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

pub fn same_picks(a: &[Pick], b: &[Pick]) -> bool {
    let key = |pick: &Pick| {
        let place = (pick.hero.as_deref().map(str::to_ascii_lowercase), pick.slot.to_ascii_lowercase());
        (place, pick.item.clone(), pick.style, pick.effect)
    };
    let sorted = |picks: &[Pick]| {
        let mut keys: Vec<_> = picks.iter().map(key).collect();
        keys.sort();
        keys
    };
    sorted(a) == sorted(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXE: &str = "npc_dota_hero_axe";
    const BANE: &str = "npc_dota_hero_bane";

    fn places(loadout: &Loadout) -> Vec<String> {
        loadout.picks().iter().map(|pick| format!("{}={}", pick.place(), pick.item)).collect()
    }

    #[test]
    fn a_persona_and_what_its_hero_wears_of_its_own_take_each_others_place() {
        const PUDGE: &str = "npc_dota_hero_pudge";
        let mut loadout = Loadout::default();
        loadout.set(Pick::hero(PUDGE, "weapon", "1"));
        loadout.set(Pick::hero(PUDGE, "ability_effects_1", "2"));
        loadout.set(Pick::hero(AXE, "weapon", "3"));
        loadout.set(Pick::hero(PUDGE, "persona_selector", "20"));
        assert_eq!(places(&loadout), ["pudge's ability_effects_1 slot=2", "axe's weapon slot=3", "pudge's persona_selector slot=20"]);
        loadout.set(Pick::hero(PUDGE, "head_persona_1", "21"));
        assert_eq!(loadout.len(), 4, "a part of the persona goes with it");
        loadout.set(Pick::hero(PUDGE, "head", "4"));
        assert!(loadout.get(Some(PUDGE), "persona_selector").is_none());
        assert!(loadout.get(Some(PUDGE), "head_persona_1").is_some());
    }

    #[test]
    fn which_heroes_get_the_effects_of_events_is_saved_and_outlives_the_picks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("picks.json");
        let mut loadout = Loadout::from_picks([Pick::world("courier", "1")]);
        assert_eq!(*loadout.effects_for(), EffectsFor::Every);
        loadout.save(&path).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("effects_for"), "the usual choice is not written");

        loadout.set_effects_for(EffectsFor::These(vec!["npc_dota_hero_axe".to_string()]));
        loadout.save(&path).unwrap();
        assert_eq!(Loadout::load(&path), loadout);
        loadout.set_effects_for(EffectsFor::Picked);
        loadout.save(&path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"effects_for\": \"picked\""));
        assert_eq!(*Loadout::load(&path).effects_for(), EffectsFor::Picked);

        loadout.forget();
        assert!(loadout.is_empty());
        assert_eq!(*loadout.effects_for(), EffectsFor::Picked);
    }

    #[test]
    fn a_slot_holds_one_pick() {
        let mut loadout = Loadout::default();
        loadout.set(Pick::hero(AXE, "weapon", "1"));
        loadout.set(Pick::world("courier", "2"));
        loadout.set(Pick::hero("NPC_DOTA_HERO_AXE", "Weapon", "3").styled(1));
        assert_eq!(places(&loadout), ["NPC_DOTA_HERO_AXE's Weapon slot=3", "the courier slot=2"]);
        assert_eq!(loadout.get(Some(AXE), "weapon").unwrap().style, 1);
        assert_eq!(loadout.get(None, "COURIER").unwrap().item, "2");
        assert!(loadout.get(Some(AXE), "courier").is_none(), "a world slot is not a hero's");
        assert!(loadout.get(None, "weapon").is_none());
        assert_eq!(loadout.len(), 2);
    }

    #[test]
    fn picks_are_cleared_by_slot_by_owner_or_all_at_once() {
        let mut loadout = Loadout::from_picks([
            Pick::hero(AXE, "weapon", "1"),
            Pick::hero(AXE, "head", "2"),
            Pick::hero(BANE, "head", "3"),
            Pick::world("courier", "4"),
            Pick::world("ward", "5"),
        ]);
        assert!(loadout.clear(Some(AXE), "weapon"));
        assert!(!loadout.clear(Some(AXE), "weapon"), "nothing was picked there any more");
        assert_eq!(loadout.of(Some(AXE)).count(), 1);
        assert_eq!(loadout.of(None).count(), 2);

        loadout.clear_owner(Some(AXE));
        assert_eq!(loadout.of(Some(AXE)).count(), 0);
        assert_eq!(loadout.len(), 3);
        loadout.clear_owner(None);
        assert_eq!(places(&loadout), ["bane's head slot=3"]);
        loadout.clear_all();
        assert!(loadout.is_empty());
    }

    #[test]
    fn changing_sets_leaves_no_stray_pieces() {
        let set = |pieces: &[(&str, &str)]| -> Vec<Pick> {
            pieces.iter().map(|(slot, item)| Pick::hero(AXE, slot, item)).collect()
        };
        let red = set(&[("weapon", "r1"), ("head", "r2"), ("misc", "r3")]);
        let blue = set(&[("weapon", "b1"), ("head", "b2")]);
        let sets = [red.as_slice(), blue.as_slice()];

        let mut loadout = Loadout::from_picks([Pick::hero(AXE, "belt", "own"), Pick::world("courier", "c")]);
        loadout.put_set(&red, sets);
        assert!(loadout.wears(&red));
        assert_eq!(loadout.of(Some(AXE)).count(), 4);

        loadout.put_set(&blue, sets);
        assert!(loadout.wears(&blue) && !loadout.wears(&red));
        assert!(loadout.get(Some(AXE), "misc").is_none());
        assert_eq!(loadout.get(Some(AXE), "belt").unwrap().item, "own");
        assert_eq!(loadout.get(None, "courier").unwrap().item, "c");

        loadout.set(Pick::hero(AXE, "head", "other"));
        loadout.set(Pick::hero(AXE, "armor", "extra"));
        loadout.put_set(&red, sets);
        assert!(loadout.wears(&red));
        assert_eq!(loadout.get(Some(AXE), "armor").unwrap().item, "extra");

        loadout.set(Pick::hero(AXE, "head", "r2").styled(2));
        assert!(loadout.wears(&red));
        assert!(!Loadout::default().wears(&[]), "an empty set is never worn");
    }

    #[test]
    fn picks_survive_saving_and_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("picks.json");
        assert!(Loadout::load(&path).is_empty(), "no file yet");

        let loadout = Loadout::from_picks([Pick::hero(AXE, "head", "9500").styled(1), Pick::world("courier", "10003")]);
        loadout.save(&path).unwrap();
        assert_eq!(Loadout::load(&path), loadout);

        std::fs::write(&path, b"not json").unwrap();
        assert!(Loadout::load(&path).is_empty(), "an unreadable file is no picks");
        std::fs::write(&path, br#"{"picks":[{"slot":"ward","item":"1"},{"slot":"WARD","item":"2"}]}"#).unwrap();
        assert_eq!(places(&Loadout::load(&path)), ["the WARD slot=2"], "one pick per slot");
    }

    #[test]
    fn picks_that_are_put_away_are_kept_and_used_nowhere() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("picks.json");
        let picks = [Pick::hero(AXE, "head", "1"), Pick::world("courier", "2")];
        let mut loadout = Loadout::from_picks(picks.clone());
        assert!(!loadout.is_put_away() && loadout.kept_away().is_empty());
        loadout.save(&path).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("off"), "nothing is written about it while picks are in use");

        loadout.put_away();
        assert!(loadout.is_put_away());
        assert_eq!(loadout.kept_away(), picks, "the picks are kept");
        assert!(loadout.picks().is_empty() && loadout.is_empty());
        assert_eq!(loadout.len(), 0);
        assert!(loadout.get(Some(AXE), "head").is_none());
        assert_eq!(loadout.of(None).count(), 0);
        assert!(!loadout.wears(&picks[..1]));

        loadout.save(&path).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"off\": true"));
        assert_eq!(Loadout::load(&path), loadout, "being put away survives saving and loading");

        assert!(!loadout.clear(Some(AXE), "head"));
        loadout.clear_owner(None);
        loadout.clear_all();
        loadout.put_away();
        assert_eq!(loadout.kept_away(), picks, "what is put away stays so");

        loadout.bring_back();
        assert!(!loadout.is_put_away());
        assert_eq!(loadout.picks(), picks, "exactly as they were");

        let mut none = Loadout::default();
        none.put_away();
        assert!(!none.is_put_away(), "no picks, nothing to put away");
    }

    #[test]
    fn a_new_pick_starts_over_from_what_was_put_away() {
        let old = [Pick::hero(AXE, "head", "1"), Pick::hero(AXE, "weapon", "2"), Pick::world("courier", "3")];
        let away = || {
            let mut loadout = Loadout::from_picks(old.clone());
            loadout.put_away();
            loadout
        };

        let mut loadout = away();
        loadout.set(Pick::hero(AXE, "belt", "9"));
        assert!(!loadout.is_put_away());
        assert_eq!(places(&loadout), ["axe's belt slot=9"], "the picks that were put away are dropped");

        let set = [Pick::hero(BANE, "head", "7"), Pick::hero(BANE, "arms", "8")];
        let mut loadout = away();
        loadout.put_set(&[], [set.as_slice()]);
        assert_eq!(loadout.kept_away().len(), 3);
        loadout.put_set(&set, [set.as_slice()]);
        assert_eq!(loadout.picks(), set);

        let mut loadout = away();
        loadout.forget();
        assert!(loadout.is_empty() && !loadout.is_put_away());
    }

    #[test]
    fn picks_are_settled_by_a_rule_wherever_they_are() {
        let picks = [Pick::hero(AXE, "head", "1").styled(4), Pick::hero(BANE, "head", "2"), Pick::world("courier", "3")];
        let rule = |pick: &Pick| (pick.item != "2").then(|| pick.clone().styled(0));
        let mut in_use = Loadout::from_picks(picks.clone());
        in_use.settle(rule);
        assert_eq!(in_use.picks(), [Pick::hero(AXE, "head", "1"), Pick::world("courier", "3")]);

        let mut away = Loadout::from_picks(picks);
        away.put_away();
        away.settle(rule);
        assert_eq!(away.kept_away(), in_use.picks(), "the same for picks that are put away");
        assert!(away.is_put_away());
        away.settle(|_| None);
        assert!(!away.is_put_away(), "with nothing left, nothing is put away");
    }

    #[test]
    fn the_same_picks_in_another_order_are_the_same() {
        let a = [Pick::hero(AXE, "head", "1"), Pick::world("courier", "2")];
        let b = [Pick::world("Courier", "2"), Pick::hero("NPC_DOTA_HERO_AXE", "HEAD", "1")];
        assert!(same_picks(&a, &b));
        assert!(!same_picks(&a, &b[..1]));
        assert!(!same_picks(&a, &[Pick::hero(AXE, "head", "1").styled(1), Pick::world("courier", "2")]));
        assert!(same_picks(&[], &[]));
    }
}
