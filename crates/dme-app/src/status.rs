use dme_core::engine::ModState;

pub const TIP: &str = "A pick takes the place of a default item. If it does not show in Dota, unequip your own item in that slot there.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Busy {
    Playing,
    Restoring,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Good,
    Warn,
}

#[derive(Clone, Debug)]
pub struct Facts {
    pub loaded: bool,
    pub load_failed: bool,
    pub busy: Option<Busy>,
    pub starting: bool,
    pub running: bool,
    pub mods: ModState,
    pub picks: usize,
    pub away: bool,
    pub in_game: bool,
    pub reach_nobody: bool,
    pub half_undone: bool,
}

pub const HALF_UNDONE: &str = "Dota was updated while your picks were in it. If it cannot find a match, close it, then press Play here.";

pub const REACH_NOBODY: &str = "No hero gets your event effects or emblem yet. Pick something for a hero, or give them to every hero.";

impl Facts {
    pub fn sentence(&self) -> (Tone, String) {
        let plain = |text: &str| (Tone::Plain, text.to_string());
        let good = |text: &str| (Tone::Good, text.to_string());
        let warn = |text: &str| (Tone::Warn, text.to_string());

        match self.busy {
            Some(Busy::Playing) if self.picks == 0 => return plain("Getting normal Dota ready…"),
            Some(Busy::Playing) => return plain("Putting your picks in the game…"),
            Some(Busy::Restoring) => return plain("Going back to normal Dota…"),
            None => {}
        }
        if !self.loaded {
            return if self.load_failed { warn("Your game's files could not be read.") } else { plain("Reading your game's files…") };
        }
        if self.starting {
            return good("Dota 2 is starting…");
        }
        let ours_in_game = matches!(self.mods, ModState::On | ModState::NeedsRepair(_));
        if self.running {
            return if self.half_undone {
                warn(HALF_UNDONE)
            } else if self.picks == 0 && ours_in_game {
                warn("Dota 2 is running. Close it, then press Play for normal Dota.")
            } else if self.picks == 0 {
                plain("Dota 2 is running.")
            } else if self.in_game {
                good("Dota 2 is running with your picks.")
            } else {
                warn("Dota 2 is running. Close it, then press Play to use your new picks.")
            };
        }
        if matches!(self.mods, ModState::ChangedByOther(_)) {
            return warn("Another program has changed Dota's files. In Steam, verify the game's files, then try again.");
        }
        if self.picks == 0 {
            return match (self.away, ours_in_game) {
                (true, false) => plain("Dota 2 is back to normal. Your picks are put away."),
                (true, true) => plain("Your picks are put away. Press Play for normal Dota."),
                (false, false) => plain("Pick something for a hero or for the world, then press Play."),
                (false, true) => plain("You have no picks. Press Play for normal Dota."),
            };
        }
        if self.reach_nobody {
            return warn(REACH_NOBODY);
        }
        if self.in_game {
            return good("Your picks are in the game. Press Play.");
        }
        match self.picks {
            1 => plain("1 pick is ready. Press Play to see it in Dota."),
            many => (Tone::Plain, format!("{many} picks are ready. Press Play to see them in Dota.")),
        }
    }

    pub fn badge(&self) -> Option<(Tone, &'static str)> {
        if !self.loaded || self.busy.is_some() || self.starting {
            return None;
        }
        let ours_in_game = matches!(self.mods, ModState::On | ModState::NeedsRepair(_));
        Some(match (&self.mods, self.picks, self.in_game) {
            (ModState::ChangedByOther(_), ..) => (Tone::Warn, "Needs attention"),
            _ if self.half_undone && self.running => (Tone::Warn, "Needs attention"),
            (_, 0, _) if ours_in_game => (Tone::Warn, "Old picks in the game"),
            (_, 0, _) => (Tone::Plain, "Normal Dota"),
            (_, _, true) => (Tone::Good, "Mods on"),
            (_, _, false) => (Tone::Warn, "Not in the game yet"),
        })
    }

    pub fn tip(&self) -> Option<&'static str> {
        let pressing = matches!(self.mods, ModState::ChangedByOther(_)) || self.reach_nobody;
        (self.loaded && self.picks > 0 && !pressing).then_some(TIP)
    }

    pub fn working(&self) -> bool {
        self.busy.is_some() || !(self.loaded || self.load_failed)
    }

    pub fn can_play(&self) -> bool {
        self.loaded && self.busy.is_none() && !self.starting && !self.running
    }

    pub fn play_label(&self) -> &'static str {
        if self.busy == Some(Busy::Playing) {
            "Working…"
        } else if self.starting {
            "Starting…"
        } else if self.running {
            "Dota is running"
        } else {
            "Play"
        }
    }

    pub fn brings_back(&self) -> bool {
        self.away && self.picks == 0
    }

    pub fn switch_label(&self) -> &'static str {
        if self.brings_back() { "Bring back my picks" } else { "Back to normal Dota" }
    }

    pub fn can_switch(&self) -> bool {
        if self.busy.is_some() || self.starting {
            false
        } else if self.brings_back() {
            true
        } else {
            !self.running && (self.picks > 0 || self.mods != ModState::Off)
        }
    }

    pub fn why_no_switch(&self) -> &'static str {
        if self.running {
            "Close Dota 2 first."
        } else if self.starting {
            "Dota 2 is starting."
        } else if self.busy.is_some() {
            "Wait until this is done."
        } else {
            "Dota 2 is as Valve ships it."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            loaded: true,
            load_failed: false,
            busy: None,
            starting: false,
            running: false,
            mods: ModState::Off,
            picks: 0,
            away: false,
            in_game: false,
            reach_nobody: false,
            half_undone: false,
        }
    }

    #[test]
    fn dota_running_with_what_an_update_left_half_undone_is_told_to_close() {
        let half = ModState::NeedsRepair(dme_core::engine::UNDECLARED.to_string());
        let running = Facts { running: true, picks: 3, mods: half.clone(), half_undone: true, ..facts() };
        assert_eq!(running.sentence(), (Tone::Warn, HALF_UNDONE.to_string()));
        assert_eq!(running.badge(), Some((Tone::Warn, "Needs attention")));
        assert!(!running.can_play() && !running.can_switch(), "nothing is written while Dota runs");
        let closed = Facts { running: false, ..running };
        assert_eq!(closed.sentence().1, "3 picks are ready. Press Play to see them in Dota.");
        assert_eq!(Facts { busy: Some(Busy::Restoring), ..closed }.sentence().1, "Going back to normal Dota…");
    }

    #[test]
    fn effects_that_go_on_no_hero_are_not_called_ready() {
        let nobody = Facts { picks: 2, reach_nobody: true, ..facts() };
        assert_eq!(nobody.sentence(), (Tone::Warn, REACH_NOBODY.to_string()));
        assert_eq!(Facts { in_game: true, mods: ModState::On, ..nobody.clone() }.sentence(), (Tone::Warn, REACH_NOBODY.to_string()));
        assert!(nobody.can_play(), "Play still puts the rest in the game");
        assert_eq!(nobody.tip(), None, "unequipping an item of one's own would not help");
        assert_eq!(Facts { busy: Some(Busy::Playing), ..nobody.clone() }.sentence().1, "Putting your picks in the game…");
        assert_eq!(Facts { running: true, ..nobody.clone() }.sentence().1, "Dota 2 is running. Close it, then press Play to use your new picks.");
        assert!(Facts { mods: ModState::ChangedByOther("x".into()), ..nobody }.sentence().1.contains("verify the game's files"));
    }

    #[test]
    fn the_badge_says_whether_the_game_has_the_picks() {
        assert_eq!(facts().badge(), Some((Tone::Plain, "Normal Dota")));
        assert_eq!(Facts { picks: 2, ..facts() }.badge(), Some((Tone::Warn, "Not in the game yet")));
        assert_eq!(Facts { picks: 2, mods: ModState::On, in_game: true, ..facts() }.badge(), Some((Tone::Good, "Mods on")));
        assert_eq!(Facts { picks: 3, mods: ModState::On, ..facts() }.badge(), Some((Tone::Warn, "Not in the game yet")), "other picks than the game has");
        assert_eq!(Facts { mods: ModState::On, ..facts() }.badge(), Some((Tone::Warn, "Old picks in the game")));
        assert_eq!(Facts { mods: ModState::ChangedByOther("x".to_string()), picks: 1, ..facts() }.badge(), Some((Tone::Warn, "Needs attention")));
        assert_eq!(Facts { loaded: false, ..facts() }.badge(), None);
        assert_eq!(Facts { busy: Some(Busy::Playing), picks: 1, ..facts() }.badge(), None);
        assert_eq!(Facts { starting: true, picks: 1, ..facts() }.badge(), None);
    }

    #[test]
    fn waiting_is_told_apart_from_having_failed() {
        let reading = Facts { loaded: false, ..facts() };
        assert_eq!(reading.sentence(), (Tone::Plain, "Reading your game's files…".to_string()));
        assert!(reading.working() && !reading.can_play());

        let failed = Facts { loaded: false, load_failed: true, ..facts() };
        assert_eq!(failed.sentence(), (Tone::Warn, "Your game's files could not be read.".to_string()));
        assert!(!failed.working() && !failed.can_play());

        assert!(!facts().working());
        assert!(Facts { busy: Some(Busy::Restoring), ..facts() }.working());
    }

    #[test]
    fn the_sentence_says_what_to_do_next() {
        let says = |facts: Facts| facts.sentence().1;
        assert_eq!(says(Facts { loaded: false, ..facts() }), "Reading your game's files…");
        assert_eq!(says(facts()), "Pick something for a hero or for the world, then press Play.");
        assert_eq!(says(Facts { picks: 1, ..facts() }), "1 pick is ready. Press Play to see it in Dota.");
        assert_eq!(says(Facts { picks: 12, ..facts() }), "12 picks are ready. Press Play to see them in Dota.");
        assert_eq!(says(Facts { picks: 12, in_game: true, mods: ModState::On, ..facts() }), "Your picks are in the game. Press Play.");
        assert_eq!(says(Facts { mods: ModState::On, ..facts() }), "You have no picks. Press Play for normal Dota.");
        assert_eq!(says(Facts { busy: Some(Busy::Playing), picks: 2, ..facts() }), "Putting your picks in the game…");
        assert_eq!(says(Facts { busy: Some(Busy::Playing), mods: ModState::On, ..facts() }), "Getting normal Dota ready…");
        assert_eq!(says(Facts { busy: Some(Busy::Restoring), loaded: false, ..facts() }), "Going back to normal Dota…");
        assert_eq!(says(Facts { starting: true, picks: 3, ..facts() }), "Dota 2 is starting…");
    }

    #[test]
    fn after_going_back_to_normal_dota_nothing_is_picked_and_the_picks_can_be_brought_back() {
        let away = Facts { away: true, ..facts() };
        assert_eq!(away.sentence(), (Tone::Plain, "Dota 2 is back to normal. Your picks are put away.".to_string()));
        assert_eq!(away.play_label(), "Play");
        assert_eq!(away.switch_label(), "Bring back my picks");
        assert!(away.can_play() && away.can_switch());
        assert_eq!(away.tip(), None, "nothing is shown, so there is nothing to explain");

        let leftovers = Facts { mods: ModState::NeedsRepair("x".into()), ..away.clone() };
        assert_eq!(leftovers.sentence().1, "Your picks are put away. Press Play for normal Dota.");
        assert_eq!(leftovers.switch_label(), "Bring back my picks");
        assert_eq!(Facts { busy: Some(Busy::Playing), ..leftovers }.sentence().1, "Getting normal Dota ready…");

        let running = Facts { running: true, ..away.clone() };
        assert_eq!(running.sentence().1, "Dota 2 is running.");
        assert!(running.can_switch(), "bringing the picks back writes nothing");
        assert!(!running.can_play());

        let back = Facts { picks: 12, away: false, ..away };
        assert_eq!(back.sentence().1, "12 picks are ready. Press Play to see them in Dota.");
        assert_eq!((back.play_label(), back.switch_label()), ("Play", "Back to normal Dota"));
    }

    #[test]
    fn a_running_game_is_never_written_to() {
        let running = Facts { running: true, picks: 2, mods: ModState::On, ..facts() };
        assert_eq!(running.sentence(), (Tone::Warn, "Dota 2 is running. Close it, then press Play to use your new picks.".to_string()));
        assert!(!running.can_play() && !running.can_switch());
        assert_eq!(running.why_no_switch(), "Close Dota 2 first.");
        assert_eq!(running.play_label(), "Dota is running");

        let with_picks = Facts { in_game: true, ..running.clone() };
        assert_eq!(with_picks.sentence(), (Tone::Good, "Dota 2 is running with your picks.".to_string()));

        let taken_off = Facts { picks: 0, ..running };
        assert_eq!(taken_off.sentence(), (Tone::Warn, "Dota 2 is running. Close it, then press Play for normal Dota.".to_string()));
        assert!(!taken_off.can_switch());
        assert_eq!(Facts { running: true, ..facts() }.sentence().1, "Dota 2 is running.");
    }

    #[test]
    fn buttons_follow_the_state() {
        assert!(facts().can_play());
        assert_eq!(facts().play_label(), "Play");
        assert!(!facts().can_switch(), "no picks and nothing of ours in the game");
        assert_eq!(facts().why_no_switch(), "Dota 2 is as Valve ships it.");
        assert!(Facts { mods: ModState::On, ..facts() }.can_switch());
        assert!(Facts { mods: ModState::NeedsRepair("x".into()), ..facts() }.can_switch());
        assert!(Facts { picks: 3, ..facts() }.can_switch(), "picks that were never played can be put away too");

        assert!(!Facts { loaded: false, ..facts() }.can_play());
        let busy = Facts { busy: Some(Busy::Playing), mods: ModState::On, ..facts() };
        assert!(!busy.can_play() && !busy.can_switch());
        assert_eq!(busy.play_label(), "Working…");
        assert_eq!(busy.why_no_switch(), "Wait until this is done.", "the game may have the picks by now");
        let starting = Facts { starting: true, mods: ModState::On, ..facts() };
        assert!(!starting.can_play() && !starting.can_switch());
        assert_eq!(starting.play_label(), "Starting…");
        assert_eq!(starting.why_no_switch(), "Dota 2 is starting.");
        assert!(!Facts { busy: Some(Busy::Restoring), away: true, ..facts() }.can_switch());
    }

    #[test]
    fn files_changed_by_another_program_are_pointed_out() {
        let foreign = Facts { mods: ModState::ChangedByOther("x".into()), picks: 3, ..facts() };
        let (tone, sentence) = foreign.sentence();
        assert_eq!(tone, Tone::Warn);
        assert!(sentence.contains("verify the game's files"));
        assert_eq!(foreign.tip(), None, "one thing at a time");
    }

    #[test]
    fn the_tip_is_there_while_there_are_picks_to_show() {
        assert_eq!(facts().tip(), None);
        assert_eq!(Facts { picks: 1, ..facts() }.tip(), Some(TIP));
        assert_eq!(Facts { picks: 1, running: true, mods: ModState::On, ..facts() }.tip(), Some(TIP));
        assert_eq!(Facts { picks: 1, loaded: false, ..facts() }.tip(), None);
    }
}
