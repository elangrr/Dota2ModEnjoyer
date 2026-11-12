mod dev;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use dme_core::apply::{self, EffectsFor, Pick, SetPicks};
use dme_core::dota::DotaInstall;
use dme_core::effects;
use dme_core::engine::{self, Engine, ITEMS_GAME, ModState, Ready};
use dme_core::heroes::HeroFiles;
use dme_core::items::{self, Item, ItemSet, ItemsGame, Style};
use dme_core::loadout::{self, Loadout};
use dme_core::names::Names;
use dme_core::vpk::Archive;
use dme_core::{catalog, launch, map_effect, process, trees};

const PICK_FORMS: &str = "A pick is written HERO=SET (axe=armor_of_the_shattered_vanguard), \
     HERO.SLOT=ITEM (axe.weapon=9040) or SLOT=ITEM (courier=10003)";

#[derive(Parser)]
#[command(name = "dme", about = "Dota 2 Mod Enjoyer, command-line edition", version)]
struct Cli {
    /// The Dota 2 folder ("dota 2 beta"). Found automatically when left out.
    #[arg(long, global = true, value_name = "FOLDER")]
    dota: Option<PathBuf>,

    /// Where settings and backups are kept. Used for testing on a copy of the game.
    #[arg(long, global = true, value_name = "FOLDER", hide = true)]
    data_dir: Option<PathBuf>,

    /// Write even while Dota is running. Only for testing on a copy of the game.
    #[arg(long, global = true, hide = true)]
    ignore_running: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show where Dota is and whether mods are on.
    Status,
    /// List the heroes.
    Heroes,
    /// List the sets of one hero.
    Sets {
        /// A hero, such as axe or crystal_maiden.
        hero: String,
    },
    /// List the slots of one hero (weapon, head ...) and what the hero has in each normally.
    Slots {
        /// A hero, such as axe or crystal_maiden.
        hero: String,
    },
    /// List the slots that are not a hero's: courier, ward, terrain ...
    World,
    /// List the effects that can be put on an item on top of its own.
    Effects,
    /// List the items that can go in a slot: `dme items axe weapon`, or `dme items courier`.
    Items {
        /// A hero, such as axe. Or a world slot, such as courier.
        #[arg(value_name = "HERO or SLOT")]
        place: String,
        /// The hero's slot, such as weapon. Left out for a world slot.
        slot: Option<String>,
    },
    /// Show the picked items in the game. This replaces whatever was applied before.
    ///
    /// A pick is written in one of three ways:
    ///
    ///   HERO=SET         a whole set          axe=armor_of_the_shattered_vanguard
    ///   HERO.SLOT=ITEM   one item on a hero   axe.weapon=9040
    ///   SLOT=ITEM        a world item         courier=10003
    ///
    /// Sets and items go by the id or name the listings show, or a part of the name that fits
    /// only one. Add :N to choose a style: axe.head=9500:1 is that item's second style.
    ///
    /// A hero's slots for ability effects take an item's effects for one ability and leave
    /// the item itself out: juggernaut.ability_effects_1=9984. `dme slots HERO` lists them.
    ///
    /// The world's fx_ slots take the effect an event had for one kind of thing, for every
    /// hero: fx_teleport=17482:2 is that event's third level. `dme world` lists them.
    /// emblem=12996 puts the glow of an event's emblem on every hero.
    ///
    /// An item can be given an effect on top of its own, the way a gem gives one in Dota:
    /// "courier=10003,effect=15". `dme effects` lists them.
    ///
    /// Experiments: trees=winter shows the leaves of Valve's winter terrain on the trees of
    /// the normal map, and map_effect=18380 draws the whole map another way. `dme items trees`
    /// and `dme items map_effect` list what there is.
    ///
    /// A later pick for a slot replaces an earlier one, so a set can be followed by single
    /// items that swap some of its pieces.
    #[command(verbatim_doc_comment)]
    Apply {
        #[arg(required = true, value_name = "PICK")]
        picks: Vec<String>,
    },
    /// Start Dota 2 with your picks. They are put in the game first if they are not there, for
    /// example after a Dota update, and brought back if you had gone back to normal Dota.
    Play {
        /// Get the game ready but do not start it. Used for testing.
        #[arg(long, hide = true)]
        no_start: bool,
        /// Start Dota with its lightest video settings, for a computer that struggles with
        /// the game. An experiment: Dota may keep the settings afterwards.
        #[arg(long)]
        low_end: bool,
    },
    /// Choose which heroes get the effects of events (fx_ picks) and an emblem. They go on
    /// the default items of those heroes, so they show on whoever plays one of them.
    ///
    ///   picked           the heroes you picked something for (what there is unless you choose)
    ///   every            every hero
    ///   axe,lina,...     these heroes
    ///
    /// Without a choice, says which one is in use. Takes effect at the next apply or play.
    #[command(verbatim_doc_comment)]
    EffectsFor {
        #[arg(value_name = "every, picked or HEROES")]
        choice: Option<String>,
    },
    /// Go back to normal Dota: undo the changes to the game's files and remove the mod package.
    /// Your picks are put away until `dme play` asks for them again.
    Restore {
        /// Forget the picks as well.
        #[arg(long)]
        forget: bool,
    },
    /// For whoever works on this tool: checks that help understand the game's files, which
    /// only read, and what it takes to release a version of the app.
    #[command(subcommand)]
    Dev(dev::DevCommand),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = match cli.command {
        Command::Dev(command) if command.needs_no_game() => return dev::run_without_game(command),
        command => command,
    };
    let dota = match &cli.dota {
        Some(folder) => DotaInstall::at(folder)?,
        None => DotaInstall::find()?,
    };
    let engine = Engine {
        dota,
        data_dir: cli.data_dir.unwrap_or_else(engine::default_data_dir),
        refuse_while_running: !cli.ignore_running,
    };
    match command {
        Command::Status => status(&engine),
        Command::Heroes => heroes(&engine.dota),
        Command::Sets { hero } => sets(&engine.dota, &hero),
        Command::Slots { hero } => slots(&engine.dota, &hero),
        Command::World => world(&engine.dota),
        Command::Effects => list_effects(&engine.dota),
        Command::Items { place, slot } => list_items(&engine.dota, &place, slot.as_deref()),
        Command::Apply { picks } => apply(&engine, &picks),
        Command::Play { no_start, low_end } => play(&engine, !no_start, low_end),
        Command::Restore { forget } => restore(&engine, forget),
        Command::EffectsFor { choice } => effects_for(&engine, choice.as_deref()),
        Command::Dev(command) => dev::run(&engine.dota, command),
    }
}

struct GameData {
    items_game: Vec<u8>,
    names: Names,
    heroes: HeroFiles,
}

impl GameData {
    fn load(dota: &DotaInstall) -> Result<GameData> {
        let archive = Archive::open(&dota.game_archive())?;
        let items_game = archive
            .read(ITEMS_GAME)?
            .with_context(|| format!("{ITEMS_GAME} is not in the game's archive"))?;
        Ok(GameData { items_game, names: catalog::read_names(&archive), heroes: HeroFiles::read(&archive) })
    }

    fn ability_name(&self, hero: &str, slot: &str) -> Option<&str> {
        let ability = self.heroes.ability(hero, slot)?;
        Some(self.names.get(&format!("DOTA_Tooltip_ability_{ability}")).unwrap_or(ability))
    }

    fn set_name<'a>(&'a self, set: &ItemSet<'a>) -> &'a str {
        set.name_token.and_then(|token| self.names.get(token)).unwrap_or(set.key)
    }

    fn item_name<'a>(&'a self, item: &Item<'a>) -> &'a str {
        let token = item.block.str("item_name").and_then(|token| std::str::from_utf8(token).ok());
        token.and_then(|token| self.names.get(token)).or(item.name).unwrap_or(item.id)
    }

    fn style_names(&self, item: &Item<'_>) -> Vec<String> {
        item.styles_to_choose().iter().map(|style| self.style_name(style)).collect()
    }

    fn style_name(&self, style: &Style<'_>) -> String {
        match style.name_token.and_then(|token| self.names.get(token)) {
            Some(name) => name.to_string(),
            None => format!("style {}", style.index),
        }
    }
}

struct Place {
    hero: Option<String>,
    slot: String,
}

impl Place {
    fn new(game: &ItemsGame<'_>, hero: Option<&str>, slot: &str) -> Result<Place> {
        let slot = slot.trim().to_ascii_lowercase();
        let hero = hero.map(|hero| known_hero(game, hero)).transpose()?;
        let for_effects = hero.as_deref().is_some_and(|hero| items::effects_slot(&slot).is_some() && game.effects_slots(hero).contains(&slot));
        let of_ours = hero.is_none() && (effects::kind_of(&slot).is_some() || [trees::SLOT, effects::EMBLEM_SLOT, map_effect::SLOT].contains(&slot.as_str()));
        if for_effects || of_ours || game.default_item(hero.as_deref(), &slot).is_some() {
            return Ok(Place { hero, slot });
        }
        match &hero {
            Some(hero) => {
                let short = items::hero_short_name(hero);
                bail!("{short} has no {slot} slot. See its slots with: dme slots {short}")
            }
            None if is_world_slot(game, &slot) => {
                bail!("the game has no default item for the {slot} slot, so nothing can be shown there")
            }
            None => bail!("there is no {slot} slot. See the slots with: dme world"),
        }
    }

    fn written(&self, separator: &str) -> String {
        match &self.hero {
            Some(hero) => format!("{}{separator}{}", items::hero_short_name(hero), self.slot),
            None => self.slot.clone(),
        }
    }

    fn words(&self) -> String {
        match &self.hero {
            Some(hero) => format!("{}'s {} slot", items::hero_short_name(hero), self.slot),
            None => format!("the {} slot", self.slot),
        }
    }

    fn event_kind(&self) -> Option<&'static effects::Kind> {
        effects::kind_of(&self.slot).filter(|_| self.hero.is_none())
    }

    fn is_for_trees(&self) -> bool {
        self.hero.is_none() && self.slot == trees::SLOT
    }

    fn is_for_an_emblem(&self) -> bool {
        self.hero.is_none() && self.slot == effects::EMBLEM_SLOT
    }

    fn is_for_the_map(&self) -> bool {
        self.hero.is_none() && self.slot == map_effect::SLOT
    }

    fn shows_an_item(&self) -> bool {
        let of_its_own_kind = self.event_kind().is_some() || self.is_for_trees() || self.is_for_an_emblem() || self.is_for_the_map();
        !of_its_own_kind && !(self.hero.is_some() && items::effects_slot(&self.slot).is_some())
    }
}

fn level_name(level: u32) -> String {
    format!("Level {}", level + 1)
}

fn events_for<'g, 'a>(game: &'g ItemsGame<'a>, kind: &effects::Kind) -> Vec<&'g Item<'a>> {
    game.items().iter().filter(|item| effects::is_pack(game, item) && !effects::variants(item, kind).is_empty()).collect()
}

fn emblems<'g, 'a>(game: &'g ItemsGame<'a>) -> Vec<&'g Item<'a>> {
    game.items().iter().filter(|item| !effects::emblem_blocks(game, item).is_empty()).collect()
}

fn map_effects<'g, 'a>(game: &'g ItemsGame<'a>) -> Vec<&'g Item<'a>> {
    game.items().iter().filter(|item| map_effect::material(game, item).is_some()).collect()
}

fn known_hero(game: &ItemsGame<'_>, name: &str) -> Result<String> {
    let hero = items::hero_id(name);
    if game.hero_slots(&hero).is_empty() {
        bail!("there is no hero called {}. See the hero names with: dme heroes", items::hero_short_name(&hero));
    }
    Ok(hero)
}

fn is_world_slot(game: &ItemsGame<'_>, name: &str) -> bool {
    let name = name.trim();
    game.world_slots().iter().any(|slot| slot.eq_ignore_ascii_case(name)) || effects::kind_of(name).is_some() || name.eq_ignore_ascii_case(trees::SLOT)
}

pub(crate) fn written(pick: &Pick) -> String {
    let place = match &pick.hero {
        Some(hero) => format!("{}.{}", items::hero_short_name(hero), pick.slot),
        None => pick.slot.clone(),
    };
    let style = if pick.style == 0 { String::new() } else { format!(":{}", pick.style) };
    let effect = pick.effect.map(|effect| format!(",effect={effect}")).unwrap_or_default();
    format!("{place}={}{style}{effect}", pick.item)
}

fn status(engine: &Engine) -> Result<()> {
    let status = engine.status()?;
    println!("Dota 2:   {}", engine.dota.root().display());
    println!("Version:  build {} ({})", status.version.build, status.version.date);
    println!("Running:  {}", if status.running { "yes" } else { "no" });
    let mods = match &status.state {
        ModState::Off => "off (the game's files are as Valve shipped them)".to_string(),
        ModState::On => "on".to_string(),
        ModState::NeedsRepair(why) => format!("needs repair: {why}. `dme play` puts it right."),
        ModState::ChangedByOther(why) => format!("not usable: {why}"),
    };
    println!("Mods:     {mods}");
    for pick in &status.picks {
        println!("Pick:     {}", written(pick));
    }
    let saved = engine.loadout();
    if !saved.effects_for().is_usual() {
        println!("Effects:  the effects of events and an emblem go on {}", effects_for_words(saved.effects_for()));
    }
    if saved.is_put_away() {
        println!("Away:     {} picks are put away. `dme play` brings them back.", saved.kept_away().len());
    } else if !saved.is_empty() {
        let same = loadout::same_picks(saved.picks(), &status.picks) && *saved.effects_for() == status.effects_for;
        if !(status.state == ModState::On && same) {
            println!("Waiting:  {} saved picks are not in the game as it is now. `dme play` puts them there.", saved.len());
        } else if !status.package_is_current {
            println!("Waiting:  the package in the game is from before an update of Dota or of this tool. `dme play` makes it again.");
        }
    }
    Ok(())
}

fn heroes(dota: &DotaInstall) -> Result<()> {
    let data = GameData::load(dota)?;
    let game = ItemsGame::parse(&data.items_game)?;
    let heroes = game.heroes();
    println!("{:<24} {:>5} {:>6}", "hero", "sets", "items");
    for hero in &heroes {
        let sets = game.sets_for(hero).len();
        let items: usize =
            game.hero_slots(hero).iter().map(|(slot, _)| game.items_for(Some(hero), slot).len()).sum();
        println!("{:<24} {sets:>5} {items:>6}", items::hero_short_name(hero));
    }
    println!("\n{} heroes. See a hero's sets with `dme sets <hero>` and its slots with `dme slots <hero>`.", heroes.len());
    Ok(())
}

fn sets(dota: &DotaInstall, hero: &str) -> Result<()> {
    let data = GameData::load(dota)?;
    let game = ItemsGame::parse(&data.items_game)?;
    let hero = known_hero(&game, hero)?;
    let short = items::hero_short_name(&hero);
    let sets = game.sets_for(&hero);
    if sets.is_empty() {
        bail!("{short} has no sets. See its single items with: dme slots {short}");
    }
    for set in &sets {
        let styles = game.set_items(set).iter().flatten().map(|item| item.styles_to_choose().len()).max().unwrap_or(0);
        let styles = if styles > 1 { format!(", {styles} styles") } else { String::new() };
        let rarity = set.bundle.and_then(|bundle| game.item_named(bundle)).and_then(|bundle| game.rarity(bundle)).unwrap_or("-");
        println!("{:<44} {rarity:<10} {} ({} pieces{styles})", set.key, data.set_name(set), set.item_names.len());
    }
    println!("\n{} sets. Put one on with: dme apply {short}=<name from the left column>", sets.len());
    Ok(())
}

fn slots(dota: &DotaInstall, hero: &str) -> Result<()> {
    let data = GameData::load(dota)?;
    let game = ItemsGame::parse(&data.items_game)?;
    let hero = known_hero(&game, hero)?;
    let short = items::hero_short_name(&hero);
    println!("{:<28} {:<44} items to pick from", "slot", "what the hero has there normally");
    let (live, dead): (Vec<_>, Vec<_>) = game.hero_slots(&hero).into_iter().partition(|(slot, _)| data.heroes.has_slot(&hero, slot));
    for (slot, default) in live {
        println!("{slot:<28} {:<44} {}", data.item_name(default), game.items_for(Some(&hero), slot).len());
    }
    for slot in game.effects_slots(&hero) {
        let ability = data.ability_name(&hero, &slot).unwrap_or("an ability");
        println!("{slot:<28} {:<44} {}", format!("the effects of {ability}"), game.items_for(Some(&hero), &slot).len());
    }
    println!("\nList a slot's items with: dme items {short} <slot>");
    if !dead.is_empty() {
        let names: Vec<&str> = dead.iter().map(|(slot, _)| *slot).collect();
        println!("The hero has these slots no more, so what is picked for them shows nowhere: {}", names.join(", "));
    }
    Ok(())
}

fn world(dota: &DotaInstall) -> Result<()> {
    let data = GameData::load(dota)?;
    let game = ItemsGame::parse(&data.items_game)?;
    println!("{:<28} {:<44} items to pick from", "slot", "what the game shows there normally");
    let mut fixed = Vec::new();
    for &slot in game.world_slots() {
        match game.default_item(None, slot) {
            Some(default) => {
                println!("{slot:<28} {:<44} {}", data.item_name(default), game.items_for(None, slot).len());
            }
            None if slot.eq_ignore_ascii_case(effects::EMBLEM_SLOT) || slot.eq_ignore_ascii_case(map_effect::SLOT) => {}
            None => fixed.push(slot),
        }
    }
    println!("{:<28} {:<44} {}", effects::EMBLEM_SLOT, "no emblem", emblems(&game).len());
    for kind in effects::KINDS {
        let events = events_for(&game, kind).len();
        if events > 0 {
            println!("{:<28} {:<44} {events}", kind.slot(), format!("the effect of: {}", kind.name));
        }
    }
    println!("{:<28} {:<44} {}", trees::SLOT, "the trees of the normal map (an experiment)", trees::KINDS.len());
    println!("{:<28} {:<44} {}", map_effect::SLOT, "the map as it is (an experiment)", map_effects(&game).len());
    println!("\nList a slot's items with: dme items <slot>");
    if !fixed.is_empty() {
        println!("The game has no default item for these slots, so they cannot be changed: {}", fixed.join(", "));
    }
    Ok(())
}

fn effect_name<'a>(data: &'a GameData, effect: &items::AttachedEffect<'_>) -> Option<&'a str> {
    data.names.get(&format!("Attrib_Particle{}", effect.id))
}

fn list_effects(dota: &DotaInstall) -> Result<()> {
    let data = GameData::load(dota)?;
    let game = ItemsGame::parse(&data.items_game)?;
    println!("{:<7} {:<10} name", "number", "made for");
    let mut listed = 0;
    for effect in game.attached_effects() {
        let Some(name) = effect_name(&data, effect) else { continue };
        let made_for = if effect.is_for_couriers() { "couriers" } else { "anything" };
        println!("{:<7} {made_for:<10} {name}", effect.id);
        listed += 1;
    }
    println!("\n{listed} effects. Put one on an item by writing it after it: dme apply \"courier=<item>,effect=<number or name>\"");
    println!("On what a hero wears, the effects made for couriers are an experiment.");
    Ok(())
}

fn list_items(dota: &DotaInstall, place: &str, slot: Option<&str>) -> Result<()> {
    let data = GameData::load(dota)?;
    let game = ItemsGame::parse(&data.items_game)?;
    let place = match slot {
        Some(slot) => Place::new(&game, Some(place), slot)?,
        None if !is_world_slot(&game, place) && !game.hero_slots(&items::hero_id(place)).is_empty() => {
            let short = items::hero_id(place);
            let short = items::hero_short_name(&short);
            bail!("{short} is a hero: name one of its slots too, as in `dme items {short} weapon`. See its slots with: dme slots {short}")
        }
        None => Place::new(&game, None, place)?,
    };

    if place.is_for_trees() {
        for kind in trees::KINDS {
            println!("{:<7} {}", kind.id, kind.name);
        }
        println!("\nAn experiment: these are the leaves of Valve's seasonal terrains. Put them on the trees of the normal map with: dme apply trees=<name from the left column>");
        return Ok(());
    }
    if let Some(kind) = place.event_kind() {
        let found = events_for(&game, kind);
        for event in &found {
            let levels: Vec<String> = (0..effects::variants(event, kind).len()).map(|level| format!("{level} {}", level_name(level as u32))).collect();
            let levels = if levels.len() > 1 { format!("   [levels: {}]", levels.join(", ")) } else { String::new() };
            println!("{:<7} {}{levels}", event.id, data.item_name(event));
        }
        println!(
            "\n{} events have an effect for this: {}. Use one, for every hero, with: dme apply {}=<number from the left column>",
            found.len(),
            kind.name,
            place.slot
        );
        return Ok(());
    }
    if place.is_for_an_emblem() || place.is_for_the_map() {
        let found = if place.is_for_an_emblem() { emblems(&game) } else { map_effects(&game) };
        for item in &found {
            println!("{:<7} {:<10} {}", item.id, game.rarity(item).unwrap_or("-"), data.item_name(item));
        }
        match place.is_for_an_emblem() {
            true => println!("\n{} emblems. Put one on every hero with: dme apply emblem=<number from the left column>", found.len()),
            false => println!("\nAn experiment: {} looks for the whole map. Use one with: dme apply map_effect=<number from the left column>", found.len()),
        }
        return Ok(());
    }

    let found = game.items_for(place.hero.as_deref(), &place.slot);
    let effects = items::effects_slot(&place.slot);
    for item in &found {
        let styles = match effects {
            Some(effects) => item.effects_styles(effects.number),
            None => item.styles_to_choose(),
        };
        let styles = if styles.len() > 1 || styles.first().is_some_and(|style| style.index > 0) {
            let numbered: Vec<String> = styles.iter().map(|style| format!("{} {}", style.index, data.style_name(style))).collect();
            format!("   [styles: {}]", numbered.join(", "))
        } else {
            String::new()
        };
        println!("{:<7} {:<10} {}{styles}", item.id, game.rarity(item).unwrap_or("-"), data.item_name(item));
    }
    let (count, verb) = match found.len() {
        1 => ("1 item".to_string(), "goes"),
        many => (format!("{many} items"), "go"),
    };
    let short = place.written(".");
    match effects.and(place.hero.as_deref()) {
        Some(hero) => {
            let ability = data.ability_name(hero, &place.slot).unwrap_or("this ability");
            let (have, one) = if found.len() == 1 { ("has", "its") } else { ("have", "one's") };
            println!("\n{count} {have} effects for {ability}. Use {one} effects, whatever the hero wears, with: dme apply {short}=<number from the left column>");
        }
        None => println!("\n{count} {verb} in {}. Show one in the game with: dme apply {short}=<number from the left column>", place.words()),
    }
    Ok(())
}

fn apply(engine: &Engine, wanted: &[String]) -> Result<()> {
    let data = GameData::load(&engine.dota)?;
    let game = ItemsGame::parse(&data.items_game)?;

    let mut picks: Vec<Pick> = Vec::new();
    let mut left_out = Vec::new();
    for text in wanted {
        let read = read_pick(&game, &data, text)?;
        for pick in read.picks {
            picks.retain(|earlier| !(earlier.hero == pick.hero && earlier.slot == pick.slot));
            picks.push(pick);
        }
        left_out.extend(read.skipped);
    }
    let mut owners: Vec<Option<String>> = Vec::new();
    for pick in &picks {
        if !owners.contains(&pick.hero) {
            owners.push(pick.hero.clone());
        }
    }
    picks.sort_by_key(|pick| owners.iter().position(|owner| *owner == pick.hero));

    let mut saved = Loadout::from_picks(picks.clone());
    let picks = saved.picks().to_vec();
    let effects_for = engine.loadout().effects_for().clone();
    let applied = engine.apply_for(&picks, &effects_for)?;
    saved.set_effects_for(effects_for);
    saved.save(&engine.loadout_file())?;

    let mut owner = None;
    for (pick, change) in picks.iter().zip(&applied.changes) {
        if owner != Some(&change.hero) {
            owner = Some(&change.hero);
            println!("{}", change.hero.as_deref().map_or("world", items::hero_short_name));
        }
        let item = game.item(&change.item_id);
        let name = item.map_or(change.item_name.as_str(), |item| data.item_name(item));
        let style = match effects::kind_of(&change.slot).filter(|_| change.hero.is_none()) {
            Some(kind) => item.filter(|item| effects::variants(item, kind).len() > 1).map(|_| level_name(change.style)),
            None => item.filter(|_| change.style > 0).and_then(|item| data.style_names(item).into_iter().nth(change.style as usize)),
        };
        let style = style.map(|name| format!(" ({name})")).unwrap_or_default();
        let effect = pick.effect.and_then(|id| game.attached_effect(id));
        let effect = effect.map(|effect| format!(" + {}", effect_name(&data, effect).unwrap_or(effect.system))).unwrap_or_default();
        println!("  {:<26} {name}{style}{effect}", change.slot);
    }
    for skipped in &left_out {
        println!("left out: {} ({})", skipped.item_name, skipped.reason);
    }
    if !applied.missing_files.is_empty() {
        let count = match applied.missing_files.len() {
            1 => "a file".to_string(),
            many => format!("{many} files"),
        };
        println!("note: these items name {count} that this copy of Dota does not have, so a part of them may not show:");
        for file in applied.missing_files.iter().take(8) {
            println!("  {file}");
        }
        if applied.missing_files.len() > 8 {
            println!("  ... and {} more", applied.missing_files.len() - 8);
        }
    }
    println!(
        "\nWrote the mod package ({:.1} MB) and set the game up to load it.",
        applied.package_len as f64 / 1_048_576.0
    );
    println!("Start Dota 2 to see it, or let `dme play` start it. To go back to normal Dota: dme restore");
    Ok(())
}

fn effects_for(engine: &Engine, choice: Option<&str>) -> Result<()> {
    let mut saved = engine.loadout();
    let Some(choice) = choice else {
        println!("The effects of events and an emblem go on {}.", effects_for_words(saved.effects_for()));
        return Ok(());
    };
    let wanted = match choice.trim().to_ascii_lowercase().as_str() {
        "every" | "all" => EffectsFor::Every,
        "picked" => EffectsFor::Picked,
        _ => {
            let game_data = GameData::load(&engine.dota)?;
            let game = ItemsGame::parse(&game_data.items_game)?;
            let heroes: Result<Vec<String>> = choice.split(',').filter(|hero| !hero.trim().is_empty()).map(|hero| known_hero(&game, hero.trim())).collect();
            let heroes = heroes?;
            if heroes.is_empty() {
                bail!("name the heroes, as in: dme effects-for axe,lina");
            }
            EffectsFor::These(heroes)
        }
    };
    saved.set_effects_for(wanted);
    saved.save(&engine.loadout_file())?;
    println!("The effects of events and an emblem go on {}. They are put in the game at the next apply or play.", effects_for_words(saved.effects_for()));
    Ok(())
}

fn effects_for_words(effects_for: &EffectsFor) -> String {
    match effects_for {
        EffectsFor::Every => "every hero".to_string(),
        EffectsFor::Picked => "the heroes you picked something for".to_string(),
        EffectsFor::These(heroes) => heroes.iter().map(|hero| items::hero_short_name(hero)).collect::<Vec<_>>().join(", "),
    }
}

fn play(engine: &Engine, start: bool, low_end: bool) -> Result<()> {
    let mut saved = engine.loadout();
    if saved.is_put_away() {
        saved.bring_back();
        saved.save(&engine.loadout_file())?;
        println!("Brought back the picks you had put away.");
    }
    match engine.make_ready(saved.picks(), saved.effects_for())? {
        Ready::Already => println!("Your {} picks are in the game already.", saved.len()),
        Ready::Applied(applied) => println!(
            "Put your {} picks in the game ({:.1} MB written).",
            applied.changes.len(),
            applied.package_len as f64 / 1_048_576.0
        ),
        Ready::Normal(_) => println!("You have no picks, so Dota 2 is as Valve ships it. Pick something with: dme apply"),
    }
    if !start {
        return Ok(());
    }
    if process::is_dota_running() {
        println!("Dota 2 is running already.");
        return Ok(());
    }
    launch::start_dota(launch::settings(low_end))?;
    match low_end {
        true => println!("Asked Steam to start Dota 2 with its lightest video settings."),
        false => println!("Asked Steam to start Dota 2."),
    }
    Ok(())
}

fn read_pick(game: &ItemsGame<'_>, data: &GameData, text: &str) -> Result<SetPicks> {
    let (place, what) = text.split_once('=').with_context(|| format!("{text:?} is not a pick. {PICK_FORMS}."))?;
    let (what, effect, color) = split_gem(what.trim());
    let (what, style) = split_style(what);
    if what.is_empty() {
        bail!("{text:?} names nothing after the = sign. {PICK_FORMS}.");
    }
    if color.is_some() {
        bail!("colours are gone: Dota's servers decide the colour of a gem, so it never showed in a match. Leave out color=...");
    }

    let one_item = |place: Place| -> Result<SetPicks> {
        if place.is_for_trees() {
            let wanted = what.to_lowercase();
            let kind = trees::KINDS.iter().find(|kind| kind.id == wanted || kind.name.to_lowercase() == wanted);
            let kind = kind.with_context(|| format!("there are no trees called {what:?}. See them with: dme items trees"))?;
            if style.is_some() || effect.is_some() {
                bail!("trees have no styles, and take no effect");
            }
            return Ok(SetPicks { picks: vec![Pick::world(trees::SLOT, kind.id)], skipped: Vec::new() });
        }
        let item = find_item(game, data, &place, what)?;
        let first = items::effects_slot(&place.slot).filter(|_| place.hero.is_some()).map_or(0, |effects| item.first_effects_style(effects.number));
        if effect.is_some() && !place.shows_an_item() {
            bail!("an effect goes on an item that takes the place of a default item, and nothing in {} does", place.words());
        }
        if style.is_some() && (place.is_for_an_emblem() || place.is_for_the_map()) {
            bail!("what goes in {} has no styles", place.words());
        }
        let effect = effect.map(|wanted| find_effect(game, data, wanted)).transpose()?;
        let pick = Pick { hero: place.hero, slot: place.slot, item: item.id.to_string(), style: style.unwrap_or(first), effect };
        Ok(SetPicks { picks: vec![pick], skipped: Vec::new() })
    };
    if let Some((hero, slot)) = place.split_once('.') {
        one_item(Place::new(game, Some(hero), slot)?)
    } else if is_world_slot(game, place) {
        one_item(Place::new(game, None, place)?)
    } else {
        let hero = known_hero(game, place)?;
        if effect.is_some() {
            bail!("an effect goes on one item, not on a set: write it as {}.SLOT=ITEM,effect=...", items::hero_short_name(&hero));
        }
        let set = find_set(game, data, &hero, what)?;
        let style = style.unwrap_or(0);
        let found = apply::set_picks(game, &hero, set.key, style)?;
        if style > 0 && found.picks.iter().all(|pick| pick.style == 0) {
            bail!("no piece of {} has a style {style}. See the styles with: dme sets {}", data.set_name(set), items::hero_short_name(&hero));
        }
        Ok(found)
    }
}

fn split_style(what: &str) -> (&str, Option<u32>) {
    match what.rsplit_once(':') {
        Some((name, style)) => match style.trim().parse() {
            Ok(style) => (name.trim(), Some(style)),
            Err(_) => (what, None),
        },
        None => (what, None),
    }
}

fn split_gem(what: &str) -> (&str, Option<&str>, Option<&str>) {
    let (mut rest, mut effect, mut color) = (what, None, None);
    while let Some((before, last)) = rest.rsplit_once(',') {
        let Some((key, value)) = last.split_once('=') else { break };
        match key.trim().to_ascii_lowercase().as_str() {
            "effect" => effect = Some(value.trim()),
            "color" | "colour" => color = Some(value.trim()),
            _ => break,
        }
        rest = before;
    }
    (rest.trim(), effect, color)
}

fn find_effect(game: &ItemsGame<'_>, data: &GameData, wanted: &str) -> Result<u32> {
    if let Ok(number) = wanted.parse::<u32>() {
        return match game.attached_effect(number) {
            Some(effect) => Ok(effect.id),
            None => bail!("there is no effect numbered {number}. See the effects with: dme effects"),
        };
    }
    let named: Vec<(u32, &str)> = game.attached_effects().iter().filter_map(|effect| Some((effect.id, effect_name(data, effect)?))).collect();
    if let Some((id, _)) = named.iter().find(|(_, name)| name.eq_ignore_ascii_case(wanted)) {
        return Ok(*id);
    }
    let needle = wanted.to_lowercase();
    let partial: Vec<&(u32, &str)> = named.iter().filter(|(_, name)| name.to_lowercase().contains(&needle)).collect();
    match partial.as_slice() {
        [(id, _)] => Ok(*id),
        [] => bail!("there is no effect like {wanted:?}. See the effects with: dme effects"),
        many => {
            let names: Vec<String> = many.iter().take(6).map(|(id, name)| format!("{name} ({id})")).collect();
            bail!("{wanted:?} fits more than one effect: {}{}", names.join(", "), if many.len() > names.len() { " ..." } else { "" })
        }
    }
}

fn find_set<'g, 'a>(game: &'g ItemsGame<'a>, data: &'a GameData, hero: &str, wanted: &str) -> Result<&'g ItemSet<'a>> {
    let short = items::hero_short_name(hero);
    let sets = game.sets_for(hero);
    if sets.is_empty() {
        bail!("{short} has no sets. See its single items with: dme slots {short}");
    }
    let exact = sets
        .iter()
        .copied()
        .find(|set| set.key.eq_ignore_ascii_case(wanted) || data.set_name(set).eq_ignore_ascii_case(wanted));
    if let Some(set) = exact {
        return Ok(set);
    }
    let needle = wanted.to_lowercase();
    let as_key = needle.replace(' ', "_");
    let partial: Vec<&ItemSet<'a>> = sets
        .iter()
        .copied()
        .filter(|set| set.key.to_lowercase().contains(&as_key) || data.set_name(set).to_lowercase().contains(&needle))
        .collect();
    match partial.as_slice() {
        [set] => Ok(*set),
        [] => bail!("{short} has no set like {wanted:?}. See its sets with: dme sets {short}"),
        many => {
            let keys: Vec<&str> = many.iter().map(|set| set.key).collect();
            bail!("{wanted:?} fits more than one set of {short}: {}", keys.join(", "))
        }
    }
}

fn find_item<'g, 'a>(game: &'g ItemsGame<'a>, data: &'a GameData, place: &Place, wanted: &str) -> Result<&'g Item<'a>> {
    let hero = place.hero.as_deref();
    let listing = format!("dme items {}", place.written(" "));
    let candidates = match place.event_kind() {
        Some(kind) => events_for(game, kind),
        None if place.is_for_an_emblem() => emblems(game),
        None if place.is_for_the_map() => map_effects(game),
        None => game.items_for(hero, &place.slot),
    };
    if let Some(item) = game.item(wanted) {
        if !candidates.iter().any(|known| known.id == item.id) {
            bail!("{} does not go in {}. See what does with: {listing}", data.item_name(item), place.words());
        }
        return Ok(item);
    }
    let named = |item: &Item<'a>, matches: &dyn Fn(&str) -> bool| item.name.is_some_and(matches) || matches(data.item_name(item));
    let exact = candidates.iter().copied().find(|item| named(item, &|name| name.eq_ignore_ascii_case(wanted)));
    if let Some(item) = exact {
        return Ok(item);
    }
    let needle = wanted.to_lowercase();
    let partial: Vec<&Item<'a>> =
        candidates.iter().copied().filter(|item| named(item, &|name| name.to_lowercase().contains(&needle))).collect();
    match partial.as_slice() {
        [item] => Ok(*item),
        [] => bail!("nothing like {wanted:?} goes in {}. See what does with: {listing}", place.words()),
        many => {
            let names: Vec<String> = many.iter().take(6).map(|item| format!("{} ({})", data.item_name(item), item.id)).collect();
            let more = if many.len() > names.len() { " ..." } else { "" };
            bail!("{wanted:?} fits more than one item for {}: {}{more}", place.words(), names.join(", "))
        }
    }
}

fn restore(engine: &Engine, forget: bool) -> Result<()> {
    let mut saved = engine.loadout();
    let restored = engine.restore()?;
    if forget && !(saved.is_empty() && saved.kept_away().is_empty()) {
        saved.forget();
        println!("Forgot your picks.");
    }
    saved.put_away();
    if saved.is_put_away() || engine.loadout_file().exists() {
        saved.save(&engine.loadout_file())?;
    }
    let nothing = !(restored.gameinfo_restored || restored.signatures_restored || restored.package_removed);
    if nothing && restored.notes.is_empty() {
        println!("Nothing to undo: the game's files are already as Valve shipped them.");
    }
    if restored.gameinfo_restored {
        println!("Put gameinfo_branchspecific.gi back.");
    }
    if restored.signatures_restored {
        println!("Put dota.signatures back.");
    }
    if restored.package_removed {
        println!("Removed {}.", engine.dota.mod_dir().display());
    }
    for note in &restored.notes {
        println!("Note: {note}");
    }
    if !nothing && restored.notes.is_empty() {
        println!("Dota 2 is back to normal.");
    }
    if saved.is_put_away() {
        println!(
            "Your {} picks are put away: `dme play` brings them back, `dme restore --forget` drops them.",
            saved.kept_away().len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_is_a_number_after_the_last_colon() {
        assert_eq!(split_style("9500"), ("9500", None));
        assert_eq!(split_style("9500:1"), ("9500", Some(1)));
        assert_eq!(split_style("9500:0"), ("9500", Some(0)));
        assert_eq!(split_style("Crested Helm : 2"), ("Crested Helm", Some(2)));
        assert_eq!(split_style("Dota: The Item"), ("Dota: The Item", None), "a colon inside a name");
        assert_eq!(split_style("Dota: The Item:3"), ("Dota: The Item", Some(3)));
        assert_eq!(split_style(":1"), ("", Some(1)));
    }

    #[test]
    fn picks_are_written_the_way_apply_reads_them() {
        assert_eq!(written(&Pick::hero("npc_dota_hero_axe", "weapon", "9040")), "axe.weapon=9040");
        assert_eq!(written(&Pick::world("courier", "10003").styled(2)), "courier=10003:2");
        let gem = Pick::world("courier", "10003").styled(1).with_effect(Some(15));
        assert_eq!(written(&gem), "courier=10003:1,effect=15");
        assert_eq!(written(&Pick::world("fx_teleport", "17482").styled(2)), "fx_teleport=17482:2");
        assert_eq!(level_name(2), "Level 3", "a pick counts levels from 0");

        let (what, effect, color) = split_gem("10003:1,effect=15");
        assert_eq!((split_style(what), effect, color), (("10003", Some(1)), Some("15"), None));
    }

    #[test]
    fn an_effect_and_a_colour_of_old_are_written_after_the_item() {
        assert_eq!(split_gem("Baby Roshan"), ("Baby Roshan", None, None));
        assert_eq!(split_gem("Baby Roshan, effect = Ethereal Flame"), ("Baby Roshan", Some("Ethereal Flame"), None));
        assert_eq!(split_gem("Baby Roshan,COLOUR=red"), ("Baby Roshan", None, Some("red")));
        assert_eq!(split_gem("9500:1,color=Ships in the Night,effect=15"), ("9500:1", Some("15"), Some("Ships in the Night")), "in either order");
        assert_eq!(split_gem("Hat, Cape and Boots"), ("Hat, Cape and Boots", None, None), "a comma inside a name");
        assert_eq!(split_gem("Hat, Cape and Boots,effect=15"), ("Hat, Cape and Boots", Some("15"), None));
        assert_eq!(split_gem("One,size=2"), ("One,size=2", None, None), "nothing that is known");
        assert_eq!(split_gem(",effect="), ("", Some(""), None));
    }
}
