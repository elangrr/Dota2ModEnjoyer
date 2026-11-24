use std::path::Path;

use dme_core::catalog;
use dme_core::draft::{Mode, Rank, Role, Style};
use dme_core::engine::Note;
use dme_core::error::{Error, Plain};
use dme_core::heroes::Attribute;
use dme_core::share::{self, NotACode};
use dme_core::{effects, trees};

pub fn error(error: &Error, dota: &Path) -> String {
    match error.plain_kind(dota) {
        Plain::Held { dota: true } => tr!(HeldDota).to_string(),
        Plain::Held { dota: false } => tr!(HeldOwn).to_string(),
        Plain::DiskFull => tr!(DiskFull).to_string(),
        Plain::DotaRunning => tr!(DotaRunning).to_string(),
        Plain::SteamBusy => tr!(SteamBusy).to_string(),
        Plain::DotaNotFound([]) => tr!(NoSteamFolder).to_string(),
        Plain::DotaNotFound(folders) => tr!(LookedIn, folders = folders.join("; ")),
        Plain::NotDota(folder) => tr!(NotDota, folder = folder.display()),
        Plain::NoSteam => tr!(NoSteam).to_string(),
        Plain::NotValves => tr!(NotValves).to_string(),
        Plain::MissingModels(models) => tr!(MissingModels, models = models.join(", ")),
        Plain::Other(said) => tr!(OtherError, error = said),
    }
}

pub fn note(note: Note) -> &'static str {
    match note {
        Note::BranchNotValves => tr!(NoteBranch),
        Note::OtherToolsLines => tr!(NoteOtherTool),
    }
}

pub fn not_a_code(why: NotACode) -> String {
    match why {
        NotACode::Missing => tr!(NotACodeMissing, prefix = share::PREFIX),
        NotACode::Broken => tr!(NotACodeBroken).to_string(),
        NotACode::Newer => tr!(NotACodeNewer).to_string(),
    }
}

pub fn attribute(attribute: Attribute) -> &'static str {
    match attribute {
        Attribute::Strength => tr!(Strength),
        Attribute::Agility => tr!(Agility),
        Attribute::Intelligence => tr!(Intelligence),
        Attribute::Universal => tr!(Universal),
    }
}

pub fn role(role: Role) -> &'static str {
    match role {
        Role::Carry => tr!(RoleCarry),
        Role::Mid => tr!(RoleMid),
        Role::Offlane => tr!(RoleOfflane),
        Role::SoftSupport => tr!(RoleSoftSupport),
        Role::HardSupport => tr!(RoleHardSupport),
    }
}

pub fn as_played(role: Role) -> &'static str {
    match role {
        Role::Carry => tr!(AsCarry),
        Role::Mid => tr!(InMid),
        Role::Offlane => tr!(InOfflane),
        Role::SoftSupport => tr!(AsSoftSupport),
        Role::HardSupport => tr!(AsHardSupport),
    }
}

pub fn player(role: Role) -> &'static str {
    match role {
        Role::Carry => tr!(CarryPlayer),
        Role::Mid => tr!(MidPlayer),
        Role::Offlane => tr!(OfflanePlayer),
        Role::SoftSupport => tr!(SoftSupportPlayer),
        Role::HardSupport => tr!(HardSupportPlayer),
    }
}

pub fn rank(rank: Rank) -> &'static str {
    match rank {
        Rank::Herald => tr!(Herald),
        Rank::Guardian => tr!(Guardian),
        Rank::Crusader => tr!(Crusader),
        Rank::Archon => tr!(Archon),
        Rank::Legend => tr!(Legend),
        Rank::Ancient => tr!(Ancient),
        Rank::Divine => tr!(Divine),
        Rank::Immortal => tr!(Immortal),
    }
}

pub fn style(style: Style) -> &'static str {
    match style {
        Style::Team => tr!(StyleTeam),
        Style::Lane => tr!(StyleLane),
    }
}

pub fn mode(mode: Mode) -> &'static str {
    match mode {
        Mode::AllPick => tr!(ModeAllPick),
        Mode::CaptainsMode => tr!(ModeCaptains),
    }
}

pub struct CatalogWords;

impl catalog::Words for CatalogWords {
    fn persona(&self, slot: &str) -> String {
        tr!(PersonaSlot, slot)
    }

    fn effects_of(&self, ability: &str, persona: bool) -> String {
        if persona { tr!(PersonaAbilityEffectsOf, ability) } else { tr!(AbilityEffectsOf, ability) }
    }

    fn level(&self, number: u32) -> String {
        tr!(LevelNumber, number)
    }

    fn style(&self, number: u32) -> String {
        tr!(StyleNumber, number)
    }

    fn no_emblem(&self) -> String {
        tr!(NoEmblem).to_string()
    }

    fn trees(&self) -> String {
        tr!(TreesSlot).to_string()
    }

    fn own_trees(&self) -> String {
        tr!(OwnTrees).to_string()
    }

    fn map_as_it_is(&self) -> String {
        tr!(MapAsItIs).to_string()
    }

    fn event_kind(&self, kind: &effects::Kind, game_name: Option<&str>) -> String {
        let own = match kind.id {
            "teleport" => Some(tr!(KindTeleport)),
            "attack" => Some(tr!(KindAttack)),
            "level_up" => Some(tr!(KindLevelUp)),
            "fountain" => Some(tr!(KindFountain)),
            "last_hit" => Some(tr!(KindLastHit)),
            "deny" => Some(tr!(KindDeny)),
            _ => None,
        };
        match (own, game_name) {
            (Some(own), _) => own.to_string(),
            (None, Some(game)) if super::current().code != "en" => game.to_string(),
            (None, _) => kind.name.to_string(),
        }
    }

    fn tree_kind(&self, kind: &trees::Kind) -> String {
        match kind.id {
            "autumn" => tr!(AutumnTrees).to_string(),
            "winter" => tr!(WinterTrees).to_string(),
            "spring" => tr!(SpringTrees).to_string(),
            _ => kind.name.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dme_core::catalog::Words;

    #[test]
    fn the_catalogs_words_in_english_are_those_it_had() {
        let (ours, english) = (CatalogWords, catalog::English);
        assert_eq!(ours.persona("Weapon"), english.persona("Weapon"));
        assert_eq!(ours.effects_of("Blade Fury", false), english.effects_of("Blade Fury", false));
        assert_eq!(ours.effects_of("Blade Fury", true), english.effects_of("Blade Fury", true));
        assert_eq!((ours.level(2), ours.style(3)), (english.level(2), english.style(3)));
        assert_eq!((ours.no_emblem(), ours.trees(), ours.own_trees(), ours.map_as_it_is()), (english.no_emblem(), english.trees(), english.own_trees(), english.map_as_it_is()));
        for kind in effects::KINDS {
            assert_eq!(ours.event_kind(kind, Some("A game name")), kind.name, "{}", kind.id);
        }
        for kind in trees::KINDS {
            assert_eq!(ours.tree_kind(kind), kind.name);
        }
    }

    #[test]
    fn errors_are_said_as_the_engine_says_them_in_english() {
        let dota = Path::new(r"D:\Games\dota 2 beta");
        let errors = [
            Error::io("write", &dota.join("game"), std::io::Error::from_raw_os_error(32)),
            Error::io("write", Path::new(r"D:\elsewhere"), std::io::Error::from_raw_os_error(32)),
            Error::io("write", &dota.join("game"), std::io::Error::from_raw_os_error(112)),
            Error::io("read", &dota.join("game"), std::io::Error::from_raw_os_error(3)),
            Error::DotaRunning,
            Error::SteamBusy,
            Error::DotaNotFound(Vec::new()),
            Error::DotaNotFound(vec![r"D:\a".to_string(), r"E:\b".to_string()]),
            Error::NotDota(dota.to_path_buf()),
            Error::NoSteam,
            Error::NotValves,
            Error::MissingModels(vec!["models/a.vmdl".to_string(), "models/b.vmdl".to_string()]),
            Error::Refused("something".to_string()),
        ];
        for error in &errors {
            assert_eq!(super::error(error, dota), error.plain(dota));
        }
        assert_eq!(not_a_code(NotACode::Missing), NotACode::Missing.to_string());
        assert_eq!(not_a_code(NotACode::Broken), NotACode::Broken.to_string());
        assert_eq!(not_a_code(NotACode::Newer), NotACode::Newer.to_string());
        assert_eq!(note(Note::BranchNotValves), Note::BranchNotValves.to_string());
        assert_eq!(note(Note::OtherToolsLines), Note::OtherToolsLines.to_string());
    }

    #[test]
    fn names_of_roles_ranks_and_attributes_in_english_are_the_engines() {
        for each in Role::ALL {
            assert_eq!(role(each), each.name());
            assert_eq!(as_played(each), each.as_played());
        }
        for each in Rank::ALL {
            assert_eq!(rank(each), each.name());
        }
        for each in Attribute::ALL {
            assert_eq!(attribute(each), each.name());
        }
        for each in Style::ALL {
            assert_eq!(style(each), each.name());
        }
    }
}
