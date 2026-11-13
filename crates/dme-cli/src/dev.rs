mod effects;

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use dme_core::apply::{self, Change, Pick};
use dme_core::catalog::{Catalog, Gap};
use dme_core::dota::DotaInstall;
use dme_core::heroes::HeroFiles;
use dme_core::items::{self, Item, ItemsGame};
use dme_core::signatures::{self, Signatures};
use dme_core::update::{self, Manifest};
use dme_core::vpk::Archive;
use dme_core::{engine, fsutil, map_effect, pictures, trees};

const ITEMS_GAME: &str = "scripts/items/items_game.txt";

#[derive(Subcommand)]
pub enum DevCommand {
    /// Recompute the checksums stored in a VPK (the game's archive by default).
    CheckVpk {
        /// A `*_dir.vpk` file.
        file: Option<PathBuf>,
    },
    /// Recompute the hash of every file declared in dota.signatures.
    CheckSignatures,
    /// Copy one file out of an archive (the game's by default).
    Extract {
        /// Path inside the archive, such as scripts/items/items_game.txt.
        path: String,
        /// Where to write the copy.
        out: PathBuf,
        /// A `*_dir.vpk` file to read instead of the game's archive.
        #[arg(long, value_name = "VPK")]
        from: Option<PathBuf>,
    },
    /// List exactly what differs between the game's item database and the one in a mod package.
    Compare {
        /// The package to compare with. Defaults to the installed mod package.
        package: Option<PathBuf>,
    },
    /// Print one item of the item database as the game stores it.
    Item {
        /// The item's numeric id.
        id: String,
        /// A `*_dir.vpk` file to read instead of the game's archive, such as the mod package.
        #[arg(long, value_name = "VPK")]
        from: Option<PathBuf>,
    },
    /// Count which keys default items and set items use.
    Keys,
    /// Count the keys a default item takes from, and keeps in spite of, the items that can be
    /// picked for its slot.
    CopiedKeys,
    /// Build the item database for every item in every slot, every style and every set, in
    /// memory, and report what happens.
    TryAll,
    /// Show how items describe their alternate styles.
    Styles,
    /// List the files in the game's archive whose path contains a text.
    Find {
        /// Part of a path, such as localization/items_.
        text: String,
        /// The most paths to print.
        #[arg(long, default_value_t = 60)]
        limit: usize,
        /// A VPK file to look in instead of the game's archive, such as a map or the mod package.
        #[arg(long, value_name = "VPK")]
        from: Option<PathBuf>,
    },
    /// Count what the app would offer, and try to read every picture it would show.
    Catalog,
    /// Show how the game describes ability effects that are worn apart from their item.
    Effects,
    /// Write one of the game's pictures as a PNG file.
    Picture {
        /// A path in the archive, or a picture's name such as econ/terrain/desert_terrain.
        image: String,
        /// Where to write the PNG.
        out: PathBuf,
    },
    /// Look a text up in one of the game's name files.
    Name {
        /// A token, such as #DOTA_Item_Desc_Pudge_Arcana_Base.
        token: String,
        /// The name file to look in.
        #[arg(long, default_value = "resource/localization/items_english.txt")]
        file: String,
    },
    /// Make a key to sign releases of the app with. Its secret half goes into a file, and its
    /// public half is printed, to be built into the app.
    UpdateKey {
        /// Where to write the secret key. Keep that file to yourself and out of the repository.
        out: PathBuf,
    },
    /// Describe a release of the app for the app's own updater: write the manifest that goes
    /// online beside the app's file.
    Release {
        /// The app's file of the new version: Dota2ModEnjoyer.exe.
        file: PathBuf,
        /// The https address the file will be downloaded from.
        #[arg(long)]
        url: String,
        /// The version of the release. Left out, it is the version of this tool, which is
        /// built together with the app.
        #[arg(long)]
        version: Option<String>,
        /// The file with the secret key to sign the release with, as `dme dev update-key`
        /// wrote it.
        #[arg(long, value_name = "FILE")]
        key: Option<PathBuf>,
        /// What is new, in a sentence or two, for players to read.
        #[arg(long)]
        notes: Option<String>,
        /// Where to write the manifest.
        #[arg(long, value_name = "FILE", default_value = "update.json")]
        out: PathBuf,
        /// For trying an update out on this computer: the address may then be a plain http
        /// one, or the path of a file. Never for a release that players are to get.
        #[arg(long)]
        trial: bool,
    },
}

impl DevCommand {
    pub fn needs_no_game(&self) -> bool {
        matches!(self, DevCommand::UpdateKey { .. } | DevCommand::Release { .. })
    }
}

pub fn run_without_game(command: DevCommand) -> Result<()> {
    match command {
        DevCommand::UpdateKey { out } => update_key(&out),
        DevCommand::Release { file, url, version, key, notes, out, trial } => release(&file, &url, version, key.as_deref(), notes, &out, trial),
        _ => bail!("this command needs the game's files"),
    }
}

fn update_key(out: &Path) -> Result<()> {
    if out.exists() {
        bail!("{} is there already. The key that releases were signed with must not get lost: move it away first if a new one is what you want", out.display());
    }
    let (public, secret) = update::new_key();
    std::fs::write(out, format!("{secret}\n")).with_context(|| format!("writing {}", out.display()))?;
    println!("The secret key is in {}.", out.display());
    println!("Keep that file to yourself and out of the repository: whoever has it can sign releases that the app takes.");
    println!("\nThe public key, to be the value of PUBLIC_KEY in crates/dme-core/src/update.rs:\n{public}");
    Ok(())
}

fn release(file: &Path, url: &str, version: Option<String>, key: Option<&Path>, notes: Option<String>, out: &Path, trial: bool) -> Result<()> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    if !update::may_fetch(url, trial) {
        bail!("{url} is not an https address, and the app fetches a new version from nothing else. To try an update out on this computer, add --trial");
    }
    let version = version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let notes = notes.map(|notes| notes.trim().to_string()).filter(|notes| !notes.is_empty());
    let mut manifest = Manifest { notes, ..Manifest::describe(&bytes, &version, url) };
    match key {
        Some(key) => {
            let secret = std::fs::read_to_string(key).with_context(|| format!("reading {}", key.display()))?;
            let public = manifest.sign(&secret)?;
            println!("Signed with the key whose public half is {public}");
            if !update::PUBLIC_KEY.is_empty() && !update::PUBLIC_KEY.trim().eq_ignore_ascii_case(&public) {
                bail!("that is not the key this build takes releases of: its PUBLIC_KEY is {}", update::PUBLIC_KEY);
            }
        }
        None if trial => println!("Not signed: an app that tries updates out asks for no signature."),
        None if !update::PUBLIC_KEY.is_empty() => bail!("the app is built to take signed releases only: give the secret key with --key"),
        None => println!("Not signed: the app is built to go by the checksum alone."),
    }
    let written = manifest.to_json();
    let read = Manifest::parse(written.as_bytes())?;
    read.check_file(&bytes)?;
    read.check_signature(if trial { "" } else { update::PUBLIC_KEY })?;
    fsutil::write_atomic(out, written.as_bytes())?;
    println!("{written}");
    if trial {
        println!("Written to {}. This is a release for trying updates out on this computer, not one for players:", out.display());
        println!("  the file has to be where the manifest says it is: {url}");
        println!("  an app built while DME_UPDATE_TRIAL named this manifest finds it within 10 seconds, and so does one started with --update-from");
        return Ok(());
    }
    println!("Written to {}. Put it online together with {}:", out.display(), file.display());
    println!("  the file where the manifest says it is: {url}");
    match update::MANIFEST_URL.is_empty() {
        true => println!("  the manifest where the app will ask for it. This build asks nobody: MANIFEST_URL in crates/dme-core/src/update.rs is empty."),
        false => println!("  the manifest where the app asks for it: {}", update::MANIFEST_URL),
    }
    Ok(())
}

pub fn run(dota: &DotaInstall, command: DevCommand) -> Result<()> {
    match command {
        DevCommand::CheckVpk { file } => check_vpk(&file.unwrap_or_else(|| dota.game_archive())),
        DevCommand::CheckSignatures => check_signatures(dota),
        DevCommand::Extract { path, out, from } => extract(&from.unwrap_or_else(|| dota.game_archive()), &path, &out),
        DevCommand::Compare { package } => compare(dota, &package.unwrap_or_else(|| dota.mod_package())),
        DevCommand::Item { id, from } => item(&from.unwrap_or_else(|| dota.game_archive()), &id),
        DevCommand::Keys => keys(dota),
        DevCommand::CopiedKeys => copied_keys(dota),
        DevCommand::TryAll => try_all(dota),
        DevCommand::Styles => styles(dota),
        DevCommand::Catalog => catalog(dota),
        DevCommand::Effects => effects::effects(dota),
        DevCommand::Picture { image, out } => picture(dota, &image, &out),
        DevCommand::Find { text, limit, from } => find(&from.unwrap_or_else(|| dota.game_archive()), &text, limit),
        DevCommand::Name { token, file } => name(dota, &token, &file),
        command @ (DevCommand::UpdateKey { .. } | DevCommand::Release { .. }) => run_without_game(command),
    }
}

fn picture(dota: &DotaInstall, image: &str, out: &Path) -> Result<()> {
    let archive = Archive::open(&dota.game_archive())?;
    let path = if image.ends_with(".vtex_c") { image.to_string() } else { pictures::archive_path(image) };
    let file = archive.read(&path)?.with_context(|| format!("{path} is not in the game's archive"))?;
    let texture = pictures::read_texture(&file)?;
    println!(
        "{path}: {} by {}, format {}{}, {} bytes of pixel data{}",
        texture.width,
        texture.height,
        texture.format,
        if texture.luma_chroma { " (brightness and color differences)" } else { "" },
        texture.data.len(),
        texture.true_size.map(|(width, height)| format!(", the picture in it is {width} by {height}")).unwrap_or_default()
    );
    let picture = pictures::decode(&file)?;
    std::fs::write(out, picture.to_png()).with_context(|| format!("writing {}", out.display()))?;
    println!("written to {}", out.display());
    Ok(())
}

fn catalog(dota: &DotaInstall) -> Result<()> {
    let started = Instant::now();
    let archive = Archive::open(&dota.game_archive())?;
    let catalog = Catalog::load(&archive)?;
    println!("catalog read in {} ms", started.elapsed().as_millis());

    let mut kinds: BTreeMap<&str, (usize, usize, Vec<String>)> = BTreeMap::new();
    let mut wanted: BTreeSet<&str> = BTreeSet::new();
    let mut count = |kind: &'static str, name: &str, picture: Option<&String>| {
        let (all, with_picture, without) = kinds.entry(kind).or_default();
        *all += 1;
        match picture {
            Some(_) => *with_picture += 1,
            None if without.len() < 4 => without.push(name.to_string()),
            None => {}
        }
    };
    let mut styled = 0;
    let mut slots = |kind: &'static str, slots: &[dme_core::catalog::Slot], count: &mut dyn FnMut(&'static str, &str, Option<&String>)| {
        for slot in slots {
            let (default, kind, styles) = if slot.is_for_effects() { ("abilities", "effects", "effect styles") } else { ("defaults", kind, "styles") };
            count(default, &slot.default.name, slot.default.picture.as_ref());
            for entry in &slot.items {
                count(kind, &entry.name, entry.picture.as_ref());
                styled += usize::from(!entry.styles.is_empty() && !slot.is_for_effects());
                for style in &entry.styles {
                    count(styles, &format!("{} / {}", entry.name, style.name), style.picture.as_ref());
                }
            }
        }
    };
    for hero in &catalog.heroes {
        count("heroes", &hero.name, hero.picture.as_ref());
        for set in &hero.sets {
            count("sets", &set.name, set.picture.as_ref());
        }
        slots("hero items", &hero.slots, &mut count);
    }
    slots("world items", &catalog.world, &mut count);
    for hero in &catalog.heroes {
        wanted.extend(hero.picture.as_deref());
        wanted.extend(hero.sets.iter().filter_map(|set| set.picture.as_deref()));
    }
    let all_slots = catalog.heroes.iter().flat_map(|hero| &hero.slots).chain(&catalog.world);
    for slot in all_slots {
        wanted.extend(slot.default.picture.as_deref());
        for entry in &slot.items {
            wanted.extend(entry.picture.as_deref());
            wanted.extend(entry.styles.iter().filter_map(|style| style.picture.as_deref()));
        }
    }

    let hero_slots = || catalog.heroes.iter().flat_map(|hero| &hero.slots);
    let for_effects = hero_slots().filter(|slot| slot.is_for_effects()).count();
    let heroes_with_effects = catalog.heroes.iter().filter(|hero| hero.slots.iter().any(|slot| slot.is_for_effects())).count();
    println!(
        "{} heroes with {} slots, {} world slots, {styled} items with styles",
        catalog.heroes.len(),
        hero_slots().count() - for_effects,
        catalog.world.len()
    );
    let effects = || hero_slots().filter(|slot| slot.is_for_effects()).flat_map(|slot| &slot.items);
    println!(
        "{for_effects} slots for the effects of abilities, of {heroes_with_effects} heroes, with {} choices, {} of them with styles",
        effects().count(),
        effects().filter(|entry| !entry.styles.is_empty()).count()
    );
    let hero_effects = hero_slots().map(|slot| slot.effects.len()).max().unwrap_or(0);
    let courier_effects = catalog.world.iter().map(|slot| slot.effects.len()).max().unwrap_or(0);
    let made_for_couriers = catalog.world.iter().flat_map(|slot| &slot.effects).filter(|effect| effect.for_couriers).count();
    println!("{hero_effects} effects to put on what a hero wears, {courier_effects} on a courier; {made_for_couriers} of them are made for couriers");
    let entries = || hero_slots().chain(&catalog.world).flat_map(|slot| &slot.items);
    let with_gap = |gap: Gap| entries().filter(|entry| entry.gaps.contains(&gap)).count();
    let event_gaps = catalog.world.iter().filter(|slot| slot.is_for_an_event()).flat_map(|slot| &slot.items).filter(|entry| entry.gaps.contains(&Gap::Files)).count();
    println!(
        "{} choices name a file the game no longer has, or one such a file is made of ({event_gaps} of them effects of events); {} have effects that go by an arcana's level; {} map effects have checkered fog",
        with_gap(Gap::Files),
        with_gap(Gap::ArcanaLevel),
        with_gap(Gap::Checkered)
    );
    let owners = std::iter::once((None, catalog.world.as_slice())).chain(catalog.heroes.iter().map(|hero| (Some(hero), hero.slots.as_slice())));
    for (hero, slots) in owners {
        let owner = hero.map_or("World", |hero| hero.name.as_str());
        for slot in slots {
            for entry in slot.items.iter().filter(|entry| !entry.gaps.is_empty()) {
                let sets = hero.map_or(Vec::new(), |hero| hero.sets.iter().filter(|set| set.pieces.iter().any(|piece| piece.slot == slot.id && piece.item == entry.id)).map(|set| set.name.as_str()).collect());
                let in_sets = if sets.is_empty() { String::new() } else { format!(", in the set {}", sets.join(", ")) };
                println!("  {owner} · {}: {} {:?}{in_sets}", slot.name, entry.name, entry.gaps);
            }
        }
    }
    let mut lost_kinds = Counter::default();
    for file in &catalog.lost_files {
        lost_kinds.add(file.rsplit('.').next().unwrap_or_default().as_bytes());
    }
    println!("{} files they name are gone from the game:", catalog.lost_files.len());
    lost_kinds.print("of the kinds");
    for file in &catalog.lost_files {
        println!("  {file}");
    }
    println!("{} files they name are in another folder than the one named, and go in the package under the name they are asked for by:", catalog.moved_files.len());
    for (named, there) in &catalog.moved_files {
        println!("  {named}  <-  {there}");
    }
    for hero in &catalog.heroes {
        for slot in hero.slots.iter().filter(|slot| !slot.shared_with.is_empty()) {
            let others: Vec<&str> = slot.shared_with.iter().map(|other| other.name.as_str()).collect();
            println!("{}'s {} has a default item ({}) that it shares with: {}", hero.name, slot.name, slot.default.id, others.join(", "));
        }
    }
    let names: Vec<&str> = catalog.heroes.iter().map(|hero| hero.name.as_str()).collect();
    println!("heroes: {} ... {}", names.iter().take(6).copied().collect::<Vec<_>>().join(", "), names.last().copied().unwrap_or_default());
    let world: Vec<String> = catalog.world.iter().map(|slot| format!("{} ({})", slot.name, slot.items.len())).collect();
    println!("world: {}", world.join(", "));

    let mut slot_names: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    let mut twice = Vec::new();
    for hero in &catalog.heroes {
        let mut seen = BTreeSet::new();
        for slot in &hero.slots {
            if !slot.is_for_effects() {
                *slot_names.entry((&slot.id, &slot.name)).or_default() += 1;
            }
            if !seen.insert(slot.name.as_str()) {
                twice.push(format!("{} has two called {}", hero.name, slot.name));
            }
        }
    }
    let listed: Vec<String> = slot_names.iter().map(|((id, name), heroes)| format!("{id} is {name:?} ({heroes})")).collect();
    println!("\nhero slots: {}", listed.join(", "));
    println!("slot names a hero has twice: {}", if twice.is_empty() { "none".to_string() } else { twice.join("; ") });

    let mut rare: Vec<[usize; 3]> = vec![[0; 3]; catalog.rarities.len() + 1];
    let mut tally = |rarity: Option<usize>, kind: usize| rare[rarity.unwrap_or(catalog.rarities.len())][kind] += 1;
    for slot in catalog.heroes.iter().flat_map(|hero| &hero.slots).chain(&catalog.world) {
        let kind = if slot.is_for_effects() { 2 } else { 0 };
        slot.items.iter().for_each(|entry| tally(entry.rarity, kind));
    }
    catalog.heroes.iter().flat_map(|hero| &hero.sets).for_each(|set| tally(set.rarity, 1));
    println!("\n{:<12} {:>7} {:>7} {:>8}   colour", "rarity", "items", "sets", "effects");
    for (place, [items, sets, effects]) in rare.iter().enumerate() {
        let (name, colour) = match catalog.rarities.get(place) {
            Some(rarity) => (rarity.name.as_str(), format!("#{:02x}{:02x}{:02x}", rarity.color[0], rarity.color[1], rarity.color[2])),
            None => ("(none)", String::new()),
        };
        println!("{name:<12} {items:>7} {sets:>7} {effects:>8}   {colour}");
    }

    println!("\n{:<12} {:>7} {:>13}   examples without one", "", "listed", "with picture");
    for (kind, (all, with_picture, without)) in &kinds {
        println!("{kind:<12} {all:>7} {with_picture:>13}   {}", without.join("; "));
    }

    let wanted: Vec<&str> = wanted.into_iter().collect();
    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let results = Mutex::new((Counter::default(), Counter::default(), Vec::new()));
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(path) = wanted.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let outcome = archive.read(path).map_err(anyhow::Error::from).and_then(|file| {
                        let file = file.context("the file is not in the archive")?;
                        let texture = pictures::read_texture(&file)?;
                        let stored = format!("format {}{}", texture.format, if texture.luma_chroma { " as brightness and color differences" } else { "" });
                        let picture = pictures::decode(&file).map_err(|error| anyhow::anyhow!("{stored}: {error}"))?;
                        Ok((stored, picture.width, picture.height))
                    });
                    let mut results = results.lock().unwrap();
                    match outcome {
                        Ok((stored, width, height)) => {
                            results.0.add(stored.as_bytes());
                            results.1.add(format!("{width}x{height}").as_bytes());
                        }
                        Err(error) => results.2.push(format!("{path}: {error}")),
                    }
                }
            });
        }
    });
    let (formats, sizes, mut failures) = results.into_inner().unwrap();
    failures.sort();
    println!(
        "\n{} different pictures read in {:.1} s: {} could not be read",
        wanted.len(),
        started.elapsed().as_secs_f32(),
        failures.len()
    );
    formats.print("how the pictures are stored");
    sizes.print_top("their sizes", 12);
    let mut reasons = Counter::default();
    for failure in &failures {
        reasons.add(failure.rsplit(": ").next().unwrap_or_default().as_bytes());
    }
    if !failures.is_empty() {
        reasons.print("why pictures could not be read");
        for failure in failures.iter().take(12) {
            println!("  {failure}");
        }
    }
    Ok(())
}

fn find(vpk: &Path, text: &str, limit: usize) -> Result<()> {
    let archive = Archive::open(vpk)?;
    let needle = text.to_ascii_lowercase().replace('\\', "/");
    let part = |entry: &dme_core::vpk::Entry| if entry.is_embedded() { "dir".to_string() } else { format!("{:03}", entry.archive_index()) };
    let mut found: Vec<(String, usize, String)> = archive
        .entries()
        .iter()
        .map(|entry| (archive.path_of(entry), entry.size(), part(entry)))
        .filter(|(path, ..)| path.to_ascii_lowercase().contains(&needle))
        .collect();
    found.sort();
    for (path, size, part) in found.iter().take(limit) {
        println!("{size:>10}  {part}  {path}");
    }
    let parts: BTreeSet<&str> = found.iter().map(|(.., part)| part.as_str()).collect();
    println!("{} of {} files in {} have {text:?} in their path", found.len(), archive.len(), vpk.display());
    println!("they are in these parts of the archive: {}", parts.into_iter().collect::<Vec<_>>().join(" "));
    Ok(())
}

fn name(dota: &DotaInstall, token: &str, file: &str) -> Result<()> {
    let bytes = Archive::open(&dota.game_archive())?
        .read(file)?
        .with_context(|| format!("{file} is not in the game's archive"))?;
    let names = dme_core::names::Names::parse(&bytes);
    match names.get(token) {
        Some(text) => println!("{token} = {text:?}"),
        None => {
            let found = names.search(token.trim_start_matches('#'));
            for (token, text) in found.iter().take(60) {
                println!("{token} = {text:?}");
            }
            println!("{} of the {} names of {file} have {token:?} in their token", found.len(), names.len());
        }
    }
    Ok(())
}

fn styles(dota: &DotaInstall) -> Result<()> {
    let bytes = read_items_game(dota)?;
    let game = ItemsGame::parse(&bytes)?;
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();

    let mut key_patterns = Counter::default();
    let mut style_keys = Counter::default();
    let mut modifier_style_values = Counter::default();
    let mut modifier_keys = Counter::default();
    let mut styled_modifier_types = Counter::default();
    let (mut with_styles, mut modifier_style_out_of_range) = (0, 0);
    let mut odd = Vec::new();
    for item in game.items() {
        let Some(visuals) = item.block.block("visuals") else { continue };
        let Some(styles) = visuals.block("styles") else { continue };
        with_styles += 1;
        let keys: Vec<String> = styles.pairs().map(|p| text(p.key)).collect();
        let in_order = keys.iter().enumerate().all(|(i, key)| *key == i.to_string());
        key_patterns.add(if in_order { format!("{} in order", keys.len()) } else { keys.join(",") }.as_bytes());
        if !in_order && odd.len() < 6 {
            odd.push(format!("{} {:?}: {}", item.id, item.name.unwrap_or("?"), keys.join(",")));
        }
        for style in styles.pairs().filter_map(|p| p.value.as_block()) {
            for pair in style.pairs() {
                style_keys.add(pair.key);
            }
        }
        for pair in visuals.pairs() {
            let Some(modifier) = pair.value.as_block() else { continue };
            if pair.key.eq_ignore_ascii_case(b"styles") {
                continue;
            }
            for inner in modifier.pairs() {
                modifier_keys.add(inner.key);
            }
            if let Some(style) = modifier.str("style") {
                modifier_style_values.add(style);
                styled_modifier_types.add(modifier.str("type").unwrap_or(b"(no type)"));
                if !keys.iter().any(|key| key.parse::<u32>().ok() == text(style).parse::<u32>().ok()) {
                    modifier_style_out_of_range += 1;
                }
            }
        }
    }
    println!("{with_styles} items have styles");
    key_patterns.print("names of the style blocks");
    for line in odd {
        println!("  odd: {line}");
    }
    style_keys.print("keys inside a style block");
    modifier_keys.print("keys of the other blocks in visuals (items with styles only)");
    modifier_style_values.print("values of \"style\" on those blocks");
    styled_modifier_types.print("types of the blocks that carry a \"style\"");
    println!("\nblocks whose \"style\" names no style block: {modifier_style_out_of_range}");
    Ok(())
}

fn try_all(dota: &DotaInstall) -> Result<()> {
    let archive = Archive::open(&dota.game_archive())?;
    let bytes = archive.read(ITEMS_GAME)?.with_context(|| format!("{ITEMS_GAME} is not in the game's archive"))?;
    let game = ItemsGame::parse(&bytes)?;
    let heroes = game.heroes();
    let mut failed = 0;

    let in_every_style = |pick: Pick, item: &Item<'_>| -> Vec<Vec<Pick>> {
        (0..item.styles_to_choose().len().max(1) as u32).map(|style| vec![pick.clone().styled(style)]).collect()
    };
    let mut slots: BTreeMap<&str, Vec<Vec<Pick>>> = BTreeMap::new();
    let (mut hero_slots, mut world_slots) = (0, 0);
    for hero in &heroes {
        for (slot, default) in game.hero_slots(hero) {
            hero_slots += 1;
            if slot.eq_ignore_ascii_case(apply::PERSONA_SELECTOR) {
                continue;
            }
            let candidates = slots.entry(default.id).or_default();
            for item in game.items_for(Some(hero), slot) {
                candidates.extend(in_every_style(Pick::hero(hero, slot, item.id), item));
            }
        }
    }
    for &slot in game.world_slots() {
        let Some(default) = game.default_item(None, slot) else { continue };
        world_slots += 1;
        let candidates = slots.entry(default.id).or_default();
        for item in game.items_for(None, slot) {
            candidates.extend(in_every_style(Pick::world(slot, item.id), item));
        }
    }
    let slots: Vec<Vec<Vec<Pick>>> = slots.into_values().collect();
    let picks = slots.iter().flatten().count();
    let styled = slots.iter().flatten().flatten().filter(|pick| pick.style > 0).count();
    println!("SINGLE ITEMS: {hero_slots} hero slots and {world_slots} world slots");
    println!("{picks} picks, {styled} of them for a style other than the first");
    failed += run_rounds(&game, &bytes, &in_rounds(&slots)).report(dota, &archive);

    let mut personas: Vec<Vec<Vec<Pick>>> = Vec::new();
    for hero in &heroes {
        let chosen = game.items_for(Some(hero), apply::PERSONA_SELECTOR);
        let Some(first) = chosen.first() else { continue };
        let mut candidates: Vec<Vec<Pick>> = chosen.iter().flat_map(|item| in_every_style(Pick::hero(hero, apply::PERSONA_SELECTOR, item.id), item)).collect();
        let persona = Pick::hero(hero, apply::PERSONA_SELECTOR, first.id);
        for (slot, _) in game.hero_slots(hero).into_iter().filter(|(slot, _)| !items::persona_of(slot).is_empty() && items::effects_slot(slot).is_none()) {
            for item in game.items_for(Some(hero), slot) {
                candidates.push(vec![persona.clone(), Pick::hero(hero, slot, item.id)]);
            }
        }
        personas.push(candidates);
    }
    println!("\nPERSONAS ON THE HERO'S OWN ITEMS: {} heroes, {} picks", personas.len(), personas.iter().flatten().count());
    failed += run_rounds(&game, &bytes, &in_rounds(&personas)).report(dota, &archive);

    let mut reasons = Counter::default();
    let (mut sets, mut styled_sets, mut empty_sets) = (0, 0, 0);
    let mut per_hero: Vec<Vec<Vec<Pick>>> = Vec::new();
    for hero in &heroes {
        let mut candidates = Vec::new();
        for set in game.sets_for(hero) {
            sets += 1;
            let styles = game.set_items(set).iter().flatten().map(|item| item.styles_to_choose().len()).max().unwrap_or(0).max(1);
            styled_sets += usize::from(styles > 1);
            for style in 0..styles as u32 {
                let found = apply::set_picks(&game, hero, set.key, style)?;
                if style == 0 {
                    empty_sets += usize::from(found.picks.is_empty());
                    for skipped in &found.skipped {
                        reasons.add(skipped.reason.as_bytes());
                    }
                }
                candidates.push(found.picks);
            }
        }
        per_hero.push(candidates);
    }
    println!("\nSETS: {sets} sets of {} heroes", heroes.len());
    println!("{styled_sets} have pieces with styles; {empty_sets} have no piece that can be shown");
    failed += run_rounds(&game, &bytes, &in_rounds(&per_hero)).report(dota, &archive);
    reasons.print("set pieces left out, by reason");

    let mut effect_slots: Vec<Vec<Vec<Pick>>> = Vec::new();
    let mut carriers: Vec<BTreeMap<&str, (Vec<Pick>, String)>> = Vec::new();
    let (mut effect_heroes, mut in_no_style) = (0, Vec::new());
    for hero in &heroes {
        let mut of_hero: BTreeMap<&str, (Vec<Pick>, String)> = BTreeMap::new();
        for slot in game.effects_slots(hero) {
            let Some(effects) = items::effects_slot(&slot) else { continue };
            let mut candidates = Vec::new();
            for item in game.items_for(Some(hero), &slot) {
                if item.effects_picked(effects.number, item.first_effects_style(effects.number)).is_empty() {
                    in_no_style.push(format!("{}.{slot}={}", items::hero_short_name(hero), item.id));
                    continue;
                }
                let styles: Vec<u32> = item.effects_styles(effects.number).iter().map(|style| style.index).collect();
                let (Some(carrier), Some(worn)) = (game.carrier(hero, item), game.slot(item)) else { continue };
                for style in if styles.is_empty() { vec![0] } else { styles } {
                    let pick = Pick::hero(hero, &slot, item.id).styled(style);
                    of_hero.entry(carrier.id).or_insert_with(|| (Vec::new(), worn.clone())).0.push(pick.clone());
                    candidates.push(vec![pick]);
                }
            }
            effect_slots.push(candidates);
        }
        effect_heroes += usize::from(!of_hero.is_empty());
        carriers.push(of_hero);
    }
    let picks = effect_slots.iter().flatten().count();
    let styled = effect_slots.iter().flatten().flatten().filter(|pick| pick.style > 0).count();
    println!("\nABILITY EFFECTS: {} slots of {effect_heroes} heroes", effect_slots.len());
    println!("{picks} picks, {styled} of them for a style other than the first");
    println!("left out, for having effects in none of their styles: {}", in_no_style.len());
    for line in in_no_style.iter().take(8) {
        println!("  {line}");
    }
    failed += run_rounds(&game, &bytes, &in_rounds(&effect_slots)).report(dota, &archive);

    let mut pairs: Vec<Vec<Vec<Pick>>> = Vec::new();
    for (hero, of_hero) in heroes.iter().zip(&carriers) {
        let mut of_this_hero = Vec::new();
        for (effects, worn) in of_hero.values() {
            let looks = game.items_for(Some(hero), worn);
            for turn in 0..looks.len().max(effects.len()).min(looks.len() * effects.len()) {
                let look = Pick::hero(hero, worn, looks[turn % looks.len()].id);
                of_this_hero.push(vec![look, effects[turn % effects.len()].clone()]);
            }
        }
        pairs.push(of_this_hero);
    }
    println!("\nABILITY EFFECTS WITH AN ITEM WORN WHERE THEY RIDE: {} pairs", pairs.iter().flatten().count());
    failed += run_rounds(&game, &bytes, &in_rounds(&pairs)).report(dota, &archive);

    let catalog = Catalog::load(&archive)?;
    let mut gem_slots: BTreeMap<&str, Vec<Vec<Pick>>> = BTreeMap::new();
    let owners = catalog.heroes.iter().map(|hero| (Some(hero.id.as_str()), hero.slots.as_slice())).chain([(None, catalog.world.as_slice())]);
    for (hero, slots) in owners {
        let taking: Vec<&dme_core::catalog::Slot> = slots.iter().filter(|slot| !slot.is_for_effects() && !slot.is_for_an_event() && !slot.effects.is_empty()).collect();
        for (place, slot) in taking.iter().enumerate() {
            let pick = |entry: &dme_core::catalog::Entry| match hero {
                Some(hero) => Pick::hero(hero, &slot.id, &entry.id).styled(entry.first_style()),
                None => Pick::world(&slot.id, &entry.id).styled(entry.first_style()),
            };
            let mut candidates = Vec::new();
            for (turn, effect) in slot.effects.iter().enumerate().filter(|(turn, _)| turn % taking.len() == place) {
                let entry = &slot.items[turn % slot.items.len()];
                candidates.push(vec![pick(entry).with_effect(Some(effect.id))]);
            }
            if !candidates.is_empty() {
                gem_slots.entry(slot.default.id.as_str()).or_default().extend(candidates);
            }
        }
    }
    let gem_slots: Vec<Vec<Vec<Pick>>> = gem_slots.into_values().collect();
    println!("\nEFFECTS PUT ON ITEMS: {} slots, {} picks", gem_slots.len(), gem_slots.iter().flatten().count());
    failed += run_rounds(&game, &bytes, &in_rounds(&gem_slots)).report(dota, &archive);

    let mut event_slots: Vec<Vec<Vec<Pick>>> = Vec::new();
    for slot in catalog.world.iter().filter(|slot| slot.is_for_an_event()) {
        let mut candidates = Vec::new();
        for entry in &slot.items {
            let levels: Vec<u32> = if entry.styles.is_empty() { vec![0] } else { entry.styles.iter().map(|style| style.index).collect() };
            candidates.extend(levels.into_iter().map(|level| vec![Pick::world(&slot.id, &entry.id).styled(level)]));
        }
        event_slots.push(candidates);
    }
    println!("\nEFFECTS OF EVENTS: {} kinds, {} picks", event_slots.len(), event_slots.iter().flatten().count());
    failed += run_rounds(&game, &bytes, &in_rounds(&event_slots)).report(dota, &archive);

    let ability_rounds = in_rounds(&effect_slots);
    let longest = event_slots.iter().map(Vec::len).max().unwrap_or(0);
    let together: Vec<Vec<Vec<Pick>>> = (0..longest)
        .map(|round| {
            let events = event_slots.iter().filter(|candidates| !candidates.is_empty()).map(|candidates| candidates[round % candidates.len()].clone());
            ability_rounds.get(round % ability_rounds.len().max(1)).into_iter().flatten().cloned().chain(events).collect()
        })
        .collect();
    println!("\nABILITY EFFECTS WITH THE EFFECTS OF EVENTS: {} rounds", together.len());
    failed += run_rounds(&game, &bytes, &together).report(dota, &archive);

    let mut socket_slots: BTreeMap<&str, Vec<Vec<Pick>>> = BTreeMap::new();
    let (mut most, mut held) = (0, 0);
    for hero in &heroes {
        for (slot, default) in game.hero_slots(hero) {
            for item in game.items_for(Some(hero), slot).into_iter().filter(|item| !item.static_sockets().is_empty()) {
                let candidates = socket_slots.entry(default.id).or_default();
                most = most.max(item.static_sockets().len());
                held += item.static_sockets().len();
                candidates.push(vec![Pick::hero(hero, slot, item.id)]);
            }
        }
    }
    let socket_slots: Vec<Vec<Vec<Pick>>> = socket_slots.into_values().collect();
    println!("\nITEMS THAT COME WITH A GEM: {} picks", socket_slots.iter().flatten().count());
    println!("{held} gems in those items, at most {most} in one");
    failed += run_rounds(&game, &bytes, &in_rounds(&socket_slots)).report(dota, &archive);

    let hero_files = HeroFiles::read(&archive);
    let live = |hero: &str, slot: &str| hero_files.has_slot(hero, slot);
    let carriers = apply::emblem_carriers(&game, &[], &live);
    let emblems: Vec<&str> = catalog.world.iter().filter(|slot| slot.is_for_an_emblem()).flat_map(|slot| &slot.items).map(|entry| entry.id.as_str()).collect();
    println!("\nEMBLEMS: {} emblems", emblems.len());
    let always = carriers.iter().filter(|carrier| carrier.always_worn()).count();
    println!("{} of {} heroes have a default item to carry one; {always} of them wear it whatever the player owns", carriers.len(), heroes.len());
    let mut carrying = Counter::default();
    for carrier in &carriers {
        carrying.add(format!("{}{}", carrier.slot, if carrier.always_worn() { "" } else { " (has items to pick)" }).as_bytes());
    }
    carrying.print("the slots whose default item carries it");
    let without: Vec<&str> = heroes.iter().filter(|hero| !carriers.iter().any(|carrier| carrier.hero == **hero)).map(|hero| items::hero_short_name(hero)).collect();
    println!("heroes with nothing to carry one: {}", if without.is_empty() { "none".to_string() } else { without.join(", ") });
    let unsure: Vec<String> =
        carriers.iter().filter(|carrier| !carrier.always_worn()).map(|carrier| format!("{} ({}, {} items)", items::hero_short_name(carrier.hero), carrier.slot, carrier.others)).collect();
    println!("heroes that show it only while a slot with items to pick is on its default: {}", if unsure.is_empty() { "none".to_string() } else { unsure.join(", ") });
    let started = Instant::now();
    let mut outcome = Outcome { rounds: emblems.len(), ..Outcome::default() };
    for id in &emblems {
        match apply::patch_with(&game, &bytes, &[Pick::world(dme_core::effects::EMBLEM_SLOT, id)], &live) {
            Ok(patched) => outcome.changes.extend(patched.changes),
            Err(error) => outcome.failures.push(format!("emblem={id}: {error}")),
        }
    }
    outcome.seconds = started.elapsed().as_secs_f32();
    failed += outcome.report(dota, &archive);

    println!("\nTREES: {} kinds", trees::KINDS.len());
    let listed = |name: &str, files: dme_core::Result<Vec<dme_core::vpk::NewFile>>| match files {
        Ok(files) => {
            let size: usize = files.iter().map(|(_, data)| data.len()).sum();
            let names: Vec<&str> = files.iter().map(|(path, _)| path.rsplit('/').next().unwrap_or(path).trim_end_matches(".vmat_c")).collect();
            println!("{name}: {} files, {} KB, in the place of {}", files.len(), size / 1024, names.join(", "));
        }
        Err(error) => println!("  FAILED {name}: {error}"),
    };
    for kind in trees::KINDS {
        listed(kind.name, kind.files(&archive));
    }
    let looks: Vec<&Item<'_>> = game.items().iter().filter(|item| map_effect::material(&game, item).is_some()).collect();
    println!("\nLOOKS FOR THE MAP: {} items", looks.len());
    for item in looks {
        listed(&format!("{} ({})", item.name.unwrap_or(item.id), item.id), map_effect::files(&game, &archive, item));
    }
    if failed > 0 {
        bail!("{failed} picks FAILED");
    }
    Ok(())
}

fn in_rounds(slots: &[Vec<Vec<Pick>>]) -> Vec<Vec<Vec<Pick>>> {
    let longest = slots.iter().map(Vec::len).max().unwrap_or(0);
    (0..longest).map(|round| slots.iter().filter_map(|candidates| candidates.get(round)).cloned().collect()).collect()
}

#[derive(Default)]
struct Outcome {
    rounds: usize,
    seconds: f32,
    changes: Vec<Change>,
    failures: Vec<String>,
}

fn run_rounds<'a>(game: &ItemsGame<'a>, bytes: &'a [u8], rounds: &[Vec<Vec<Pick>>]) -> Outcome {
    let started = Instant::now();
    let next = AtomicUsize::new(0);
    let outcome = Mutex::new(Outcome::default());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(round) = rounds.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let (mut changes, mut failures) = (Vec::new(), Vec::new());
                    match apply::patch(game, bytes, &round.concat()) {
                        Ok(patched) => changes = patched.changes,
                        Err(together) => {
                            for candidate in round {
                                match apply::patch(game, bytes, candidate) {
                                    Ok(patched) => changes.extend(patched.changes),
                                    Err(error) => {
                                        let written: Vec<String> = candidate.iter().map(crate::written).collect();
                                        failures.push(format!("{}: {error}", written.join(" ")));
                                    }
                                }
                            }
                            if failures.is_empty() {
                                failures.push(format!("{} picks that are fine one at a time fail together: {together}", round.len()));
                            }
                        }
                    }
                    let mut outcome = outcome.lock().unwrap();
                    outcome.changes.extend(changes);
                    outcome.failures.extend(failures);
                }
            });
        }
    });
    let mut outcome = outcome.into_inner().unwrap();
    outcome.rounds = rounds.len();
    outcome.seconds = started.elapsed().as_secs_f32();
    outcome.failures.sort();
    outcome
}

impl Outcome {
    fn report(&self, dota: &DotaInstall, archive: &Archive) -> usize {
        println!(
            "{} rounds in {:.1} s: {} default items rewritten, {} picks FAILED",
            self.rounds,
            self.seconds,
            self.changes.len(),
            self.failures.len()
        );
        for line in self.failures.iter().take(25) {
            println!("  FAILED {line}");
        }

        let mut models: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        let mut others: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for change in &self.changes {
            let missing = engine::missing_files(dota, archive, std::slice::from_ref(change));
            for model in missing.models {
                models.entry(model).or_default().insert(&change.item_id);
            }
            for file in missing.others {
                others.entry(file).or_default().insert(&change.item_id);
            }
        }
        let items = |files: &BTreeMap<String, BTreeSet<&str>>| files.values().flatten().collect::<BTreeSet<_>>().len();
        println!("models the game lacks: {} (the engine refuses the {} items made of them)", models.len(), items(&models));
        for (model, items) in models.iter().take(12) {
            println!("  {model}   item {}", items.iter().copied().collect::<Vec<_>>().join(", "));
        }
        let mut kinds = Counter::default();
        for file in others.keys() {
            kinds.add(file.rsplit('.').next().unwrap_or_default().as_bytes());
        }
        println!("other files the game lacks: {} (named by {} items; their effects will not show)", others.len(), items(&others));
        if !others.is_empty() {
            kinds.print("those files by kind");
            for (file, items) in &others {
                println!("  {file}   item {}", items.iter().copied().take(4).collect::<Vec<_>>().join(", "));
            }
        }
        self.failures.len()
    }
}

fn read_items_game(dota: &DotaInstall) -> Result<Vec<u8>> {
    Archive::open(&dota.game_archive())?
        .read(ITEMS_GAME)?
        .with_context(|| format!("{ITEMS_GAME} is not in the game's archive"))
}

fn item(vpk: &Path, id: &str) -> Result<()> {
    let bytes = Archive::open(vpk)?
        .read(ITEMS_GAME)?
        .with_context(|| format!("{ITEMS_GAME} is not in {}", vpk.display()))?;
    let game = ItemsGame::parse(&bytes)?;
    let item = game.item(id).with_context(|| format!("there is no item {id}"))?;
    let block = item.block;
    println!("\"{id}\"\n{{{}}}", String::from_utf8_lossy(&bytes[block.start()..block.end()]));
    println!("slot in effect: {}", game.slot(item).as_deref().unwrap_or("(none)"));
    println!("world slot: {}", game.world_slot(item).as_deref().unwrap_or("(none)"));
    println!("base item: {}", if game.is_base(item) { "yes" } else { "no" });
    println!("styles: {}", item.styles().len());
    Ok(())
}

fn keys(dota: &DotaInstall) -> Result<()> {
    let bytes = read_items_game(dota)?;
    let started = Instant::now();
    let game = ItemsGame::parse(&bytes)?;
    println!("parsed {} items and {} sets in {} ms", game.items().len(), game.sets().len(), started.elapsed().as_millis());

    let mut default_keys = Counter::default();
    let mut default_visuals = Counter::default();
    let mut default_slots: HashSet<(&str, Option<String>)> = HashSet::new();
    for item in game.items().iter().filter(|i| i.is_default()) {
        for pair in item.block.pairs() {
            default_keys.add(pair.key);
        }
        count_visuals(item.block, &mut default_visuals);
        for &hero in &item.heroes {
            default_slots.insert((hero, game.slot(item)));
        }
    }

    let mut set_item_keys = Counter::default();
    let mut set_item_visuals = Counter::default();
    let mut static_attributes = Counter::default();
    let mut prefabs = Counter::default();
    let (mut set_items, mut no_default_slot) = (0, Vec::new());
    for set in game.sets() {
        for item in game.set_items(set).into_iter().flatten() {
            set_items += 1;
            prefabs.add(item.prefab.unwrap_or("(none)").as_bytes());
            for pair in item.block.pairs() {
                set_item_keys.add(pair.key);
            }
            count_visuals(item.block, &mut set_item_visuals);
            if let Some(attributes) = item.block.block("static_attributes") {
                for pair in attributes.pairs() {
                    static_attributes.add(pair.key);
                }
            }
            let slot = game.slot(item);
            for &hero in &item.heroes {
                if !default_slots.contains(&(hero, slot.clone())) {
                    no_default_slot.push(format!("{} / {} / slot {:?} / hero {hero}", set.key, item.name.unwrap_or("?"), slot));
                }
            }
        }
    }

    default_keys.print("keys of default items");
    default_visuals.print("visuals of default items");
    println!("\n{set_items} items are in sets");
    prefabs.print("prefabs of set items");
    set_item_keys.print("keys of set items");
    set_item_visuals.print("visuals of set items");
    static_attributes.print("static_attributes of set items");
    println!("\nset items with no default item in the same slot: {}", no_default_slot.len());
    for line in no_default_slot {
        println!("  {line}");
    }
    Ok(())
}

fn copied_keys(dota: &DotaInstall) -> Result<()> {
    let bytes = read_items_game(dota)?;
    let game = ItemsGame::parse(&bytes)?;
    let listed = |list: &[&str], key: &[u8]| list.iter().any(|known| key.eq_ignore_ascii_case(known.as_bytes()));

    let mut slots: Vec<(Option<&str>, String)> = Vec::new();
    for hero in game.heroes() {
        slots.extend(game.hero_slots(hero).into_iter().map(|(slot, _)| (Some(hero), slot.to_string())));
    }
    slots.extend(game.world_slots().iter().map(|slot| (None, slot.to_ascii_lowercase())));

    let (mut looks, mut undecided, mut kept) = (Counter::default(), Counter::default(), Counter::default());
    let mut examples: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = HashSet::new();
    for (hero, slot) in &slots {
        let Some(default) = game.default_item(*hero, slot) else { continue };
        for pair in default.block.pairs().filter(|pair| !listed(apply::LOOKS, pair.key)) {
            kept.add(pair.key);
        }
        for item in game.items_for(*hero, slot) {
            if !seen.insert(item.id) {
                continue;
            }
            for pair in item.block.pairs().filter(|pair| !listed(apply::NOT_COPIED, pair.key)) {
                if listed(apply::LOOKS, pair.key) {
                    looks.add(pair.key);
                } else {
                    undecided.add(pair.key);
                    let key = String::from_utf8_lossy(pair.key).to_ascii_lowercase();
                    let value = pair.value.as_str().map_or("{...}".into(), |value| String::from_utf8_lossy(value).into_owned());
                    examples.entry(key).or_insert_with(|| format!("item {}: {value}", item.id));
                }
            }
        }
    }
    println!("{} items can be picked", seen.len());
    looks.print("keys copied because they are looks");
    undecided.print("keys copied that are on neither list");
    for (key, example) in &examples {
        println!("  {key:<40} {example}");
    }
    kept.print("keys default items keep (unless the picked item has the key too)");
    Ok(())
}

fn count_visuals(item: dme_core::kv::Block<'_>, counter: &mut Counter) {
    let Some(visuals) = item.block("visuals") else { return };
    for pair in visuals.pairs() {
        match pair.value.as_block() {
            Some(modifier) if pair.key.starts_with(b"asset_modifier") => {
                let kind = modifier.str("type").unwrap_or(b"(no type)");
                counter.add(&[b"asset_modifier: ", kind].concat());
            }
            _ => counter.add(pair.key),
        }
    }
}

#[derive(Default)]
struct Counter(std::collections::HashMap<String, usize>);

impl Counter {
    fn add(&mut self, key: &[u8]) {
        *self.0.entry(String::from_utf8_lossy(key).to_ascii_lowercase()).or_default() += 1;
    }

    fn print(&self, title: &str) {
        self.print_top(title, usize::MAX);
    }

    fn print_top(&self, title: &str, most: usize) {
        let mut rows: Vec<(&String, &usize)> = self.0.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        println!("\n{title}:");
        let line: Vec<String> = rows.iter().take(most).map(|(key, count)| format!("{key}={count}")).collect();
        let more = if rows.len() > most { format!(", and {} more", rows.len() - most) } else { String::new() };
        println!("  {}{more}", line.join(", "));
    }
}

fn check_vpk(file: &Path) -> Result<()> {
    let started = Instant::now();
    let archive = Archive::open(file)?;
    println!("{}", file.display());
    println!("  version {}, {} files, opened in {} ms", archive.version(), archive.len(), started.elapsed().as_millis());
    match archive.verify_checksums() {
        None => println!("  no checksum section"),
        Some(sums) => {
            let word = |ok: bool| if ok { "matches" } else { "DOES NOT MATCH" };
            println!("  checksum of the file list:        {}", word(sums.tree));
            println!("  checksum of the archive section:  {}", word(sums.archive_section));
            println!("  checksum of the whole file:       {}", word(sums.whole_file));
            if !sums.all_ok() {
                bail!("the checksums do not match what the spec says they cover");
            }
        }
    }
    Ok(())
}

fn check_signatures(dota: &DotaInstall) -> Result<()> {
    let path = dota.signatures();
    let bytes = std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let sig = Signatures::parse(&bytes)?;
    let folder = path.parent().context("dota.signatures has no parent folder")?;

    let (mut ok, mut wrong, mut missing) = (0, 0, 0);
    for line in sig.valve_lines() {
        let file = resolve(folder, line.path);
        match std::fs::read(&file) {
            Err(_) => {
                missing += 1;
                println!("  missing:   {}", line.path);
            }
            Ok(contents) if signatures::matches(&line, &contents) => ok += 1,
            Ok(_) => {
                wrong += 1;
                println!("  different: {}", line.path);
            }
        }
    }
    println!("Valve's declarations: {ok} match the file on disk, {wrong} differ, {missing} name a file that is not there");
    println!("Lines after DIGEST: {}", sig.extra.lines().count());
    for line in sig.extra_lines() {
        let state = match std::fs::read(resolve(folder, line.path)) {
            Ok(contents) if signatures::matches(&line, &contents) => "matches the file on disk",
            Ok(_) => "differs from the file on disk",
            Err(_) => "names a file that is not there",
        };
        println!("  {}: {state}", line.path);
    }
    Ok(())
}

fn resolve(signatures_folder: &Path, declared: &str) -> PathBuf {
    let mut path = signatures_folder.to_path_buf();
    for part in declared.split(['\\', '/']) {
        match part {
            "" | "." | "..." => {}
            ".." => {
                path.pop();
            }
            name => path.push(name),
        }
    }
    path.components().filter(|c| !matches!(c, Component::CurDir)).collect()
}

fn extract(vpk: &Path, path: &str, out: &Path) -> Result<()> {
    let started = Instant::now();
    let archive = Archive::open(vpk)?;
    let opened = started.elapsed();
    let entry = archive.find(path).with_context(|| format!("{path} is not in {}", vpk.display()))?;
    let contents = archive.read_entry(entry)?;
    let read = started.elapsed() - opened;
    std::fs::write(out, &contents).with_context(|| format!("writing {}", out.display()))?;
    let stored = if entry.is_embedded() {
        "the directory file".to_string()
    } else {
        format!("data file {:03}", entry.archive_index())
    };
    println!(
        "{path}: {} bytes from {stored}, written to {} (archive opened in {} ms, file read in {} ms)",
        contents.len(),
        out.display(),
        opened.as_millis(),
        read.as_millis()
    );
    Ok(())
}

fn compare(dota: &DotaInstall, package: &Path) -> Result<()> {
    let vanilla_bytes = read_items_game(dota)?;
    let modded_bytes = Archive::open(package)?
        .read(ITEMS_GAME)?
        .with_context(|| format!("{ITEMS_GAME} is not in {}", package.display()))?;
    let vanilla = ItemsGame::parse(&vanilla_bytes)?;
    let modded = ItemsGame::parse(&modded_bytes)?;
    println!(
        "game: {} bytes, {} items; package: {} bytes, {} items",
        vanilla_bytes.len(),
        vanilla.items().len(),
        modded_bytes.len(),
        modded.items().len()
    );

    let text = |bytes: &[u8], block: dme_core::kv::Block<'_>| bytes[block.start()..block.end()].to_vec();
    let mut changed = Vec::new();
    for before in vanilla.items() {
        match modded.item(before.id) {
            None => println!("  item {} is missing from the package", before.id),
            Some(after) if text(&vanilla_bytes, before.block) != text(&modded_bytes, after.block) => {
                println!(
                    "  item {:<6} {:<34} model {} -> {}",
                    before.id,
                    before.name.unwrap_or("?"),
                    before.model.unwrap_or("(none)"),
                    after.model.unwrap_or("(none)")
                );
                changed.push((before.block, after.block));
            }
            Some(_) => {}
        }
    }
    for after in modded.items().iter().filter(|item| vanilla.item(item.id).is_none()) {
        println!("  item {} exists only in the package", after.id);
    }

    let without = |bytes: &[u8], mut blocks: Vec<dme_core::kv::Block<'_>>| {
        blocks.sort_by_key(|block| block.start());
        let mut rest = Vec::with_capacity(bytes.len());
        let mut at = 0;
        for block in blocks {
            rest.extend_from_slice(&bytes[at..block.start()]);
            at = block.end();
        }
        rest.extend_from_slice(&bytes[at..]);
        rest
    };
    let same = without(&vanilla_bytes, changed.iter().map(|pair| pair.0).collect())
        == without(&modded_bytes, changed.iter().map(|pair| pair.1).collect());
    println!("{} items differ; everything else is {}", changed.len(), if same { "identical byte for byte" } else { "DIFFERENT" });
    if !same {
        bail!("the package differs from the game outside the listed items");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use dme_core::vpk::{self, PackFile};

    use super::*;

    #[test]
    fn a_release_to_try_updates_out_with_needs_no_key() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Dota2ModEnjoyer.exe");
        std::fs::write(&app, b"the new app").unwrap();
        let out = dir.path().join("update.json");
        release(&app, &app.to_string_lossy(), Some("9.9.9".to_string()), None, None, &out, true).unwrap();
        let manifest = Manifest::parse(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(manifest.version, "9.9.9");
        assert!(manifest.check_file(b"the new app").is_ok());
        if !update::PUBLIC_KEY.is_empty() {
            assert!(release(&app, "https://example.org/Dota2ModEnjoyer.exe", None, None, None, &out, false).is_err());
        }
    }

    const PERSONA_WITHOUT_ROOM: &str = r#""items_game" { "items" {
        "1" { "name" "Pudge's Hook" "prefab" "default_item" "item_slot" "weapon" "used_by_heroes" { "npc_dota_hero_pudge" "1" } }
        "5" { "name" "Pudge's Persona" "prefab" "default_item" "item_slot" "persona_selector" "used_by_heroes" { "npc_dota_hero_pudge" "1" } }
        "6" { "name" "Pudge Persona Weapon" "prefab" "default_item" "item_slot" "weapon_persona_1" "used_by_heroes" { "npc_dota_hero_pudge" "1" } }
        "8" { "name" "Pudge Persona Costume" "prefab" "default_item" "item_slot" "armor_persona_1" "used_by_heroes" { "npc_dota_hero_pudge" "1" } }
        "20" { "name" "The Toy Butcher" "prefab" "wearable" "item_slot" "persona_selector" "used_by_heroes" { "npc_dota_hero_pudge" "1" }
            "visuals" { "asset_modifier" { "type" "persona" "persona" "1" } } }
    } }"#;

    fn try_all_on(items_game: &str) -> Result<()> {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(game.join("bin").join("win64")).unwrap();
        std::fs::create_dir_all(game.join("dota")).unwrap();
        std::fs::write(game.join("bin").join("win64").join("dota2.exe"), b"").unwrap();
        let archive = vpk::build(&[PackFile { path: ITEMS_GAME, data: items_game.as_bytes() }]).unwrap();
        std::fs::write(game.join("dota").join("pak01_dir.vpk"), archive).unwrap();
        try_all(&DotaInstall::at(dir.path()).unwrap())
    }

    #[test]
    fn try_all_fails_where_a_pick_fails() {
        assert!(try_all_on(&PERSONA_WITHOUT_ROOM.replace("armor_persona_1", "weapon_persona_2")).is_ok());
        assert!(try_all_on(PERSONA_WITHOUT_ROOM).is_err(), "a pick FAILED, and try-all said all was well");
    }
}
