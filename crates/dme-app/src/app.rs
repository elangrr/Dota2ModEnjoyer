use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dme_core::apply::Pick;
use dme_core::catalog::{Catalog, Hero};
use dme_core::dota::DotaInstall;
use dme_core::engine::{self, Engine, ModState, Ready};
use dme_core::items;
use dme_core::loadout::Loadout;
use eframe::egui::text::LayoutJob;
use eframe::egui::{self, Align, Color32, CornerRadius, CursorIcon, FontId, Layout, Rect, RichText, Sense, Ui, pos2, vec2};

use crate::Options;
use crate::pictures::Pictures;
use crate::script::Script;
use crate::settings::Settings;
use crate::status::{Busy, Facts, Tone};
use crate::theme;
use crate::update::{self, News, Updater};
use crate::views::loadout::{Page, Row};
use crate::views::{self, dim, title_bar};
use crate::worker::{Failure, GameState, Job, Reply, Worker};

pub const TITLE: &str = "Dota 2 Mod Enjoyer";

const VERSION: &str = env!("CARGO_PKG_VERSION");

const SETTINGS_FILE: &str = "settings.json";
const STARTING_SECONDS: f64 = 90.0;

const NOTICE: [&str; 3] = [
    "This app changes how Dota 2 looks on your computer. It does that by editing two of the game's own files. Only you see the result.",
    "Editing game files may be against Valve's rules for Dota 2, so you use this app at your own risk. It is not made or approved by Valve.",
    "Nothing is changed while Dota 2 is running, and \"Back to normal Dota\" undoes everything.",
];

const LOW_END_HINT: &str = "Play starts Dota with its lightest video settings: no shadows, simple water and lighting, no fog, grass or other extras. \
     An experiment. Dota may keep these settings afterwards; you can change them back in Dota's video options.";

const HELP: [(&str, &str); 12] = [
    (
        "A pick does not show in Dota",
        "A pick takes the place of a default item. If you wear an item of your own in that slot, unequip it in your Dota loadout.",
    ),
    (
        "Other players wear it too",
        "Whoever uses the default item shows your pick as well. That is on your screen only: nobody else sees any of it.",
    ),
    (
        "Ability effects",
        "Under what a hero wears is a list of its abilities. Pick an item there to give the ability that item's effects, whatever the hero wears. Only effects that Dota itself lets players use apart from their item are listed.",
    ),
    (
        "Personas",
        "A persona is put on the hero's own items, so it shows without your owning it in Dota. It takes the place of what the hero wears: picks for those slots are taken off, and picking one of them takes the persona off. An experiment.",
    ),
    (
        "Effects",
        "A courier, and what a hero wears, can be given an effect on top of its own, the way a gem gives one in Dota. Pick the item first, then choose under \"Effect\". On a hero, the effects marked \"(courier)\" are an experiment: they are made for couriers.",
    ),
    (
        "Emblems, teleports and other effects",
        "On the World page, under \"Effects\", you can put an event's emblem around your heroes, and use the effect a past event had for a teleport, a Blink Dagger, attacks and more. They go on every hero, or on the heroes you choose in Settings. A hero shows them as long as its slots in Dota are on their default items.",
    ),
    (
        "A part of an item does not show",
        "An item with a small ! mark has a part that may not show: Dota no longer has some of its files, or some of its effects go by the level of an arcana, which Dota's servers give only to its owner, or it is known to be buggy, as the fog of Aghanim's Labyrinth's map effect is. The lightest video settings leave parts of effects out as well.",
    ),
    (
        "Video settings",
        "Do not change Dota's video settings during a match. That makes Dota show the heroes you picked for as \"ERROR\" until the next game. Change them before a match.",
    ),
    (
        "Experiments",
        "\"Trees\" and \"Map Effect\", at the bottom of the World page, and \"Low-end PC mode\", in Settings, are still being tried out. Low-end PC mode starts Dota with its lightest video settings. Dota may keep them afterwards; you can change them back in its video options.",
    ),
    ("After a Dota update", "Just press Play. Your picks are put back in the game before it starts."),
    (
        "Something is not in the lists",
        "Taunts and a few more cannot be changed: the game has no default item for them to take the place of.",
    ),
    (
        "Back to normal Dota",
        "Takes everything this app did out of the game's files and puts your picks away, so that nothing is picked. \"Bring back my picks\" picks it all again, unless you have started over with something new.",
    ),
];

const UPDATES_HELP: (&str, &str) = (
    "Updates",
    "When a newer version of this app is out, an Update button shows at the top. One click downloads it and starts the app again. Your picks stay as they are. In Settings you can have the app do this by itself.",
);

const AUTO_UPDATE_NOTE: &str = "On: when a newer version of this app is out, the app downloads it and starts again by itself. Off: an Update button shows at the top, and nothing happens until you press it. Your picks stay as they are either way.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Heroes,
    World,
}

struct Banner {
    tone: Tone,
    text: String,
    details: Option<String>,
    undo: Option<Loadout>,
    update: bool,
}

impl Banner {
    fn new(tone: Tone, text: impl Into<String>) -> Banner {
        Banner { tone, text: text.into(), details: None, undo: None, update: false }
    }

    fn failure(failure: Failure) -> Banner {
        Banner { details: Some(failure.details), ..Banner::new(Tone::Warn, failure.sentence) }
    }

    fn with_undo(text: String, before: Loadout) -> Banner {
        Banner { undo: Some(before), ..Banner::new(Tone::Plain, text) }
    }
}

pub struct App {
    options: Options,
    data_dir: PathBuf,
    settings: Settings,

    engine: Option<Engine>,
    worker: Option<Worker>,
    find_error: Option<String>,
    folder_text: String,

    catalog: Option<Arc<Catalog>>,
    load_failure: Option<Failure>,
    loadout: Loadout,
    saved: Loadout,
    picks_changed: bool,
    game: Option<GameState>,
    busy: Option<Busy>,
    starting_until: Option<f64>,
    banner: Option<Banner>,
    close_when_done: bool,

    tab: Tab,
    hero: Option<String>,
    hero_search: String,
    hero_row: Row,
    world_row: Row,
    item_search: String,
    rarities_asked: BTreeSet<usize>,
    help_open: bool,
    settings_open: bool,

    updater: Option<Updater>,
    start_again: bool,

    pictures: Pictures,
    script: Option<Script>,
}

impl App {
    pub fn new(creation: &eframe::CreationContext<'_>, options: Options) -> App {
        let ctx = &creation.egui_ctx;
        theme::apply(ctx);
        let data_dir = options.data_dir.clone().unwrap_or_else(engine::default_data_dir);
        let settings = Settings::load(&data_dir.join(SETTINGS_FILE));
        let script = options.screenshot.clone().map(|shot| Script::new(options.steps.clone(), shot));
        update::clean_up();
        let updater = Updater::start(&options, ctx);
        let updated = options.updated_from.as_deref().is_some_and(|before| before != VERSION);
        let banner = updated.then(|| Banner::new(Tone::Good, format!("The app was updated. This is version {VERSION} now.")));

        let found = match (&options.dota, &settings.dota_folder) {
            (Some(folder), _) => DotaInstall::at(folder),
            (None, Some(folder)) => DotaInstall::at(folder).or_else(|_| DotaInstall::find()),
            (None, None) => DotaInstall::find(),
        };
        let mut app = App {
            options,
            data_dir,
            settings,
            engine: None,
            worker: None,
            find_error: None,
            folder_text: String::new(),
            catalog: None,
            load_failure: None,
            loadout: Loadout::default(),
            saved: Loadout::default(),
            picks_changed: false,
            game: None,
            busy: None,
            starting_until: None,
            banner,
            close_when_done: false,
            tab: Tab::Heroes,
            hero: None,
            hero_search: String::new(),
            hero_row: Row::Sets,
            world_row: Row::Sets,
            item_search: String::new(),
            rarities_asked: BTreeSet::new(),
            help_open: false,
            settings_open: false,
            updater,
            start_again: false,
            pictures: Pictures::new(ctx),
            script,
        };
        match found {
            Ok(dota) => app.connect(dota, ctx),
            Err(error) => app.find_error = Some(error.to_string()),
        }
        app
    }

    fn connect(&mut self, dota: DotaInstall, ctx: &egui::Context) {
        let engine = Engine { dota, data_dir: self.data_dir.clone(), refuse_while_running: !self.options.ignore_running };
        self.loadout = engine.loadout();
        self.saved = self.loadout.clone();
        let never_saved = self.loadout != Loadout::default() && !engine.loadout_file().exists();
        let worker = Worker::start(engine.clone(), ctx.clone());
        worker.send(Job::Load);
        self.engine = Some(engine);
        self.worker = Some(worker);
        self.find_error = None;
        if never_saved {
            self.save_picks();
        }
    }

    fn listen(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        while let Some(reply) = self.worker.as_ref().and_then(Worker::poll) {
            match reply {
                Reply::Loaded(Ok(loaded)) => {
                    self.pictures.use_archive(loaded.archive);
                    self.take_catalog(loaded.catalog);
                }
                Reply::Loaded(Err(failure)) => self.load_failure = Some(failure),
                Reply::Game(game) => {
                    if game.running {
                        self.starting_until = None;
                    }
                    self.game = Some(game);
                }
                Reply::Played(Ok(played)) => {
                    self.busy = None;
                    if played.started {
                        self.starting_until = Some(now + STARTING_SECONDS);
                    } else {
                        let text = match played.ready {
                            Ready::Normal(_) => "Dota 2 is as Valve ships it.",
                            Ready::Already | Ready::Applied(_) => "Your picks are in the game.",
                        };
                        self.banner = Some(Banner::new(Tone::Good, text));
                    }
                }
                Reply::Restored(Ok(restored)) => {
                    self.busy = None;
                    let text = if !restored.notes.is_empty() {
                        restored.notes.join(" ")
                    } else if self.loadout.is_put_away() {
                        "Dota 2 is back to normal. Your picks are put away: \"Bring back my picks\" shows them again.".to_string()
                    } else {
                        "Dota 2 is back to normal.".to_string()
                    };
                    let tone = if restored.notes.is_empty() { Tone::Good } else { Tone::Warn };
                    self.banner = Some(Banner::new(tone, text));
                }
                Reply::Played(Err(failure)) | Reply::Restored(Err(failure)) => {
                    self.busy = None;
                    self.banner = Some(Banner::failure(failure));
                }
            }
        }
        if self.starting_until.is_some_and(|until| now >= until) {
            self.starting_until = None;
        }
        if self.starting_until.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        while let Some(news) = self.updater.as_mut().and_then(Updater::poll) {
            let under_way = self.updater.as_ref().is_some_and(|updater| updater.is_fetching() || updater.is_installed());
            match news {
                News::Offered(_) if !under_way && self.settings.auto_update => self.update(ctx),
                News::Offered(manifest) if !under_way && self.banner.as_ref().is_none_or(|banner| banner.update) => {
                    let notes = manifest.notes.as_deref().map(|notes| format!(" {}", notes.trim())).unwrap_or_default();
                    let text = format!("Version {} of this app is out.{notes}", manifest.version);
                    self.banner = Some(Banner { update: true, ..Banner::new(Tone::Plain, text) });
                }
                News::Offered(_) | News::Nothing(None) => {}
                News::Nothing(Some(failure)) | News::Failed(failure) => self.banner = Some(Banner::failure(failure)),
                News::Installed => self.start_again = true,
            }
        }
    }

    fn update(&mut self, ctx: &egui::Context) {
        if let Some(updater) = &mut self.updater {
            updater.update(ctx);
        }
        if self.banner.as_ref().is_some_and(|banner| banner.update) {
            self.banner = None;
        }
    }

    fn start_the_new_version(&mut self, ctx: &egui::Context) {
        if !self.start_again || self.busy.is_some() {
            return;
        }
        self.start_again = false;
        let from = self.updater.as_ref().map_or(VERSION, Updater::version).to_string();
        let started = !self.options.no_restart && update::start_again(&self.options, &from).is_ok();
        if started {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            let version = self.updater.as_ref().and_then(Updater::offer).map(|manifest| manifest.version.clone()).unwrap_or_default();
            self.banner = Some(Banner::new(Tone::Good, format!("Version {version} is in place. Close the app and start it again to use it.")));
        }
    }

    fn take_catalog(&mut self, catalog: Arc<Catalog>) {
        let before = self.loadout.clone();
        let gone = self.loadout.picks().iter().filter(|pick| catalog.settled(pick).is_none()).count();
        self.loadout.settle(|pick| catalog.settled(pick));
        if self.loadout != before {
            self.saved = self.loadout.clone();
            match gone {
                0 => {}
                1 => self.banner = Some(Banner::new(Tone::Plain, "One of your picks is not in the game any more and was taken off.")),
                many => {
                    let text = format!("{many} of your picks are not in the game any more and were taken off.");
                    self.banner = Some(Banner::new(Tone::Plain, text));
                }
            }
            self.save_picks();
        }
        if let Some(target) = self.options.open.clone() {
            self.open(&catalog, &target);
        }
        self.rarities_asked.clear();
        self.catalog = Some(catalog);
    }

    fn open(&mut self, catalog: &Catalog, target: &str) {
        let mut parts = target.split(':');
        let row = |slot: Option<&str>| match slot {
            None | Some("sets") => Row::Sets,
            Some(slot) => Row::Slot(slot.to_string()),
        };
        match parts.next() {
            Some("world") => {
                self.tab = Tab::World;
                self.world_row = row(parts.next());
            }
            Some("hero") => {
                let id = parts.next().map(items::hero_id).filter(|id| catalog.hero(id).is_some());
                self.hero_row = row(parts.next());
                self.hero = id;
            }
            Some("help") => self.help_open = true,
            Some("settings") => self.settings_open = true,
            _ => {}
        }
    }

    fn facts(&self, ctx: &egui::Context) -> Facts {
        let now = ctx.input(|input| input.time);
        let (running, mods, in_game) = match &self.game {
            Some(game) => (game.running && !self.options.ignore_running, game.mods.clone(), game.shows(self.loadout.picks(), self.loadout.effects_for())),
            None => (false, ModState::Off, false),
        };
        Facts {
            loaded: self.catalog.is_some(),
            load_failed: self.load_failure.is_some(),
            busy: self.busy,
            starting: !running && self.starting_until.is_some_and(|until| now < until),
            running,
            mods,
            picks: self.loadout.len(),
            away: self.loadout.is_put_away(),
            in_game,
        }
    }

    fn play(&mut self) {
        let Some(worker) = &self.worker else { return };
        self.busy = Some(Busy::Playing);
        self.banner = None;
        let effects_for = self.loadout.effects_for().clone();
        worker.send(Job::Play { picks: self.loadout.picks().to_vec(), effects_for, start: !self.options.no_start, low_end: self.settings.low_end });
    }

    fn back_to_normal(&mut self) {
        let Some(worker) = &self.worker else { return };
        self.busy = Some(Busy::Restoring);
        self.banner = None;
        worker.send(Job::Restore);
        self.loadout.put_away();
        self.saved = self.loadout.clone();
        self.save_picks();
    }

    fn bring_back(&mut self) {
        self.banner = None;
        self.loadout.bring_back();
        self.saved = self.loadout.clone();
        self.save_picks();
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save(&self.data_dir.join(SETTINGS_FILE)) {
            self.banner = Some(Banner::new(Tone::Warn, error.to_string()));
        }
    }

    fn save_picks(&mut self) -> bool {
        let Some(engine) = &self.engine else { return false };
        match self.loadout.save(&engine.loadout_file()) {
            Ok(()) => true,
            Err(error) => {
                self.banner = Some(Banner::new(Tone::Warn, format!("Your picks could not be saved: {error}")));
                false
            }
        }
    }

    fn picks_were_changed(&mut self) {
        let removed = taken_off(&self.saved, &self.loadout);
        let before = std::mem::replace(&mut self.saved, self.loadout.clone());
        if !self.save_picks() {
            return;
        }
        let dropped = if self.loadout.is_put_away() { 0 } else { before.kept_away().len() };
        match removed {
            Some(count) if count > 1 => {
                self.banner = Some(Banner::with_undo(format!("{count} picks were taken off."), before));
            }
            _ if dropped > 0 => {
                let text = match dropped {
                    1 => "You have started over. The pick you had put away is gone.".to_string(),
                    many => format!("You have started over. The {many} picks you had put away are gone."),
                };
                self.banner = Some(Banner::with_undo(text, before));
            }
            _ if self.banner.as_ref().is_some_and(|banner| banner.undo.is_some()) => self.banner = None,
            _ => {}
        }
    }

    fn close_politely(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested()) && self.busy.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_when_done = true;
        }
        if self.close_when_done && self.busy.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn type_to_search(&mut self, ctx: &egui::Context) {
        if self.catalog.is_none() || !self.settings.notice_accepted || self.help_open || self.settings_open {
            return;
        }
        let (typed, erased, escaped) = ctx.input(|input| {
            let typed = input.events.iter().any(|event| matches!(event, egui::Event::Text(text) if !text.trim().is_empty()));
            (typed, input.key_pressed(egui::Key::Backspace), input.key_pressed(egui::Key::Escape))
        });
        let search = if self.tab == Tab::Heroes && self.hero.is_none() { &mut self.hero_search } else { &mut self.item_search };
        if escaped {
            search.clear();
        }
        let for_the_box = typed || (erased && !search.is_empty());
        if for_the_box && !ctx.memory(|memory| memory.has_focus(views::search_id())) {
            ctx.memory_mut(|memory| memory.request_focus(views::search_id()));
        }
    }

    fn run_script(&mut self, ctx: &egui::Context) {
        let loaded = self.engine.is_none() || self.catalog.is_some() || self.load_failure.is_some();
        let updating = self.updater.as_ref().is_some_and(Updater::is_working) || self.start_again;
        let settled = loaded && self.busy.is_none() && self.pictures.loading() == 0 && !updating;
        if let Some(script) = &mut self.script {
            script.advance(ctx, settled);
        }
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(script) = &mut self.script {
            script.feed(raw_input);
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.listen(ctx);
        self.start_the_new_version(ctx);
        self.close_politely(ctx);
        self.run_script(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.pictures.begin_frame();
        if self.engine.is_none() {
            self.find_dota(ui);
        } else {
            self.type_to_search(ui.ctx());
            self.header(ui);
            self.footer(ui);
            self.banner(ui);
            let frame = egui::Frame::new().fill(theme::BACKGROUND).inner_margin(egui::Margin::same(16));
            egui::CentralPanel::default().frame(frame).show(ui, |ui| self.content(ui));
            if !self.settings.notice_accepted {
                self.notice(ui.ctx());
            } else if self.help_open {
                self.help(ui.ctx());
            } else if self.settings_open {
                self.settings_sheet(ui.ctx());
            }
        }
        self.pictures.end_frame();
        if std::mem::take(&mut self.picks_changed) {
            self.picks_were_changed();
        }
    }
}

impl App {
    fn header(&mut self, ui: &mut Ui) {
        let mut update = false;
        egui::Panel::top("header").frame(bar()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(TITLE).size(18.0).strong());
                ui.add_space(20.0);
                for (tab, name) in [(Tab::Heroes, "Heroes"), (Tab::World, "World")] {
                    if tab_button(ui, name, self.tab == tab).clicked() && self.tab != tab {
                        self.tab = tab;
                        self.item_search.clear();
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Help").clicked() {
                        self.help_open = true;
                    }
                    if ui.button("Settings").clicked() {
                        self.settings_open = true;
                    }
                    if let Some(updater) = &self.updater
                        && let Some(offer) = updater.offer()
                    {
                        let (label, waiting) = match (updater.is_installed(), updater.is_fetching()) {
                            (true, _) => ("Updated", false),
                            (false, true) => ("Updating…", false),
                            (false, false) => ("Update", true),
                        };
                        let button = egui::Button::new(RichText::new(label).strong().color(Color32::WHITE)).fill(theme::ACCENT);
                        let hint = format!("Version {} of this app is out. One click downloads it and starts the app again. Your picks stay as they are.", offer.version);
                        update = ui.add_enabled(waiting, button).on_hover_text(hint).clicked();
                    }
                    if self.loadout.is_empty() {
                        return;
                    }
                    ui.add_space(8.0);
                    let clear = ui.add(egui::Button::new(dim("Clear all")).frame(false));
                    if clear.on_hover_text("Take off every pick").clicked() {
                        self.loadout.clear_all();
                        self.picks_changed = true;
                    }
                    ui.label(dim(picks_count(self.loadout.len())));
                });
            });
        });
        if update {
            self.update(ui.ctx());
        }
    }

    fn footer(&mut self, ui: &mut Ui) {
        let facts = self.facts(ui.ctx());
        egui::Panel::bottom("footer").frame(bar()).show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = RichText::new(facts.play_label()).size(18.0).strong().color(Color32::WHITE);
                let play = egui::Button::new(label).fill(theme::ACCENT).min_size(vec2(176.0, 44.0));
                if ui.add_enabled(facts.can_play(), play).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    self.play();
                }
                let switch = egui::Button::new(facts.switch_label()).min_size(vec2(0.0, 44.0));
                let hint = match facts.brings_back() {
                    true => "Pick again what you had picked before going back to normal Dota. Play puts it in the game.",
                    false => "Take every change out of the game's files and put your picks away. They are kept, to be brought back.",
                };
                let switch = ui.add_enabled(facts.can_switch(), switch).on_hover_text(hint).on_disabled_hover_text(facts.why_no_switch());
                if switch.clicked() {
                    match facts.brings_back() {
                        true => self.bring_back(),
                        false => self.back_to_normal(),
                    }
                }
                ui.add_space(8.0);
                said(ui, &facts);
            });
        });
    }

    fn banner(&mut self, ui: &mut Ui) {
        let Some(banner) = &self.banner else { return };
        let (mut close, mut undo, mut update) = (false, false, false);
        let frame = egui::Frame::new().fill(theme::CARD).inner_margin(egui::Margin::symmetric(16, 10));
        egui::Panel::bottom("banner").frame(frame).show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                close = ui.button(if banner.update { "Later" } else { "Close" }).clicked();
                update = banner.update && ui.button("Update").on_hover_text("Download the new version and start the app again").clicked();
                undo = banner.undo.is_some() && ui.button("Undo").on_hover_text("Put those picks back").clicked();
                if let Some(details) = &banner.details
                    && ui.button("Copy details").on_hover_text("Copy what happened, to send to whoever helps you").clicked()
                {
                    ui.ctx().copy_text(details.clone());
                }
                ui.add_space(8.0);
                let (width, color) = (ui.available_width(), tone_color(banner.tone));
                let mut job = LayoutJob::simple(banner.text.clone(), FontId::proportional(theme::BODY), color, width);
                job.wrap.max_rows = 6;
                job.wrap.overflow_character = Some('…');
                let text = ui.painter().layout_job(job);
                let (rect, _) = ui.allocate_exact_size(vec2(width, text.size().y), Sense::hover());
                ui.painter().galley(rect.min, text, color);
            });
        });
        if undo {
            if let Some(before) = self.banner.take().and_then(|banner| banner.undo) {
                self.loadout = before;
                self.saved = self.loadout.clone();
                self.save_picks();
            }
        } else if update {
            self.update(ui.ctx());
        } else if close {
            self.banner = None;
        }
    }

    fn content(&mut self, ui: &mut Ui) {
        let Some(catalog) = self.catalog.clone() else {
            return self.loading(ui);
        };
        match self.tab {
            Tab::World => self.world_page(ui, &catalog),
            Tab::Heroes => match self.hero.as_deref().and_then(|id| catalog.hero(id)) {
                Some(hero) => self.hero_page(ui, &catalog, hero),
                None => {
                    let clicked = views::heroes::show(ui, &catalog, &self.loadout, &mut self.pictures, &mut self.hero_search);
                    if let Some(id) = clicked {
                        self.hero = Some(id);
                        self.hero_row = Row::Sets;
                        self.hero_search.clear();
                        self.item_search.clear();
                    }
                }
            },
        }
    }

    fn loading(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            let Some(failure) = &self.load_failure else {
                ui.spinner();
                ui.label(dim("Reading your game's files…"));
                return;
            };
            ui.label(RichText::new("The game's files could not be read").size(20.0).strong());
            ui.add_space(4.0);
            ui.label("This can happen while Steam is updating Dota 2. Let it finish, then try again.");
            ui.add_space(10.0);
            let again = ui.add(primary("Try again")).clicked();
            ui.add_space(16.0);
            ui.label(dim(&failure.sentence).small());
            if ui.button("Copy details").clicked() {
                ui.ctx().copy_text(failure.details.clone());
            }
            if again {
                self.load_failure = None;
                if let Some(worker) = &self.worker {
                    worker.send(Job::Load);
                }
            }
        });
    }

    fn hero_page(&mut self, ui: &mut Ui, catalog: &Catalog, hero: &Hero) {
        let hero_id = Some(hero.id.as_str());
        ui.horizontal(|ui| {
            if ui.button("<  All heroes").clicked() {
                self.hero = None;
                self.item_search.clear();
            }
            ui.add_space(8.0);
            ui.label(RichText::new(&hero.name).size(22.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let any = self.loadout.of(hero_id).next().is_some();
                let reset = ui.add_enabled(any, egui::Button::new("Reset hero"));
                if reset.on_hover_text("Take off every pick for this hero").clicked() {
                    self.loadout.clear_owner(hero_id);
                    self.picks_changed = true;
                }
            });
        });
        ui.add_space(8.0);
        let page = Page {
            hero: Some(hero),
            slots: &hero.slots,
            loadout: &mut self.loadout,
            pictures: &mut self.pictures,
            row: &mut self.hero_row,
            search: &mut self.item_search,
            rarities: &catalog.rarities,
            asked: &mut self.rarities_asked,
        };
        self.picks_changed |= views::loadout::show(ui, page);
    }

    fn world_page(&mut self, ui: &mut Ui, catalog: &Catalog) {
        let any = self.loadout.of(None).next().is_some();
        let mut reset = false;
        title_bar(ui, "The world", None, |ui| {
            let button = ui.add_enabled(any, egui::Button::new("Reset the world"));
            reset = button.on_hover_text("Take off every pick that is not a hero's").clicked();
        });
        if reset {
            self.loadout.clear_owner(None);
            self.picks_changed = true;
        }
        ui.add_space(4.0);
        let page = Page {
            hero: None,
            slots: &catalog.world,
            loadout: &mut self.loadout,
            pictures: &mut self.pictures,
            row: &mut self.world_row,
            search: &mut self.item_search,
            rarities: &catalog.rarities,
            asked: &mut self.rarities_asked,
        };
        self.picks_changed |= views::loadout::show(ui, page);
    }

    fn find_dota(&mut self, ui: &mut Ui) {
        let frame = egui::Frame::new().fill(theme::BACKGROUND).inner_margin(egui::Margin::same(24));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.25);
                ui.label(RichText::new("Dota 2 was not found").size(24.0).strong());
                ui.add_space(6.0);
                ui.label(dim("Paste the path of your Dota 2 folder here. In Steam: right-click Dota 2, Manage, Browse local files."));
                ui.add_space(10.0);
                let hint = r"C:\Program Files (x86)\Steam\steamapps\common\dota 2 beta";
                ui.add(egui::TextEdit::singleline(&mut self.folder_text).hint_text(hint).desired_width(560.0));
                ui.add_space(6.0);
                if ui.add(primary("Use this folder")).clicked() {
                    let folder = PathBuf::from(self.folder_text.trim().trim_matches('"'));
                    match DotaInstall::at(&folder) {
                        Ok(dota) => {
                            self.settings.dota_folder = Some(folder);
                            self.save_settings();
                            self.connect(dota, ui.ctx());
                        }
                        Err(error) => self.find_error = Some(error.to_string()),
                    }
                }
                if let Some(error) = &self.find_error {
                    ui.add_space(8.0);
                    ui.label(dim(error));
                }
            });
        });
    }

    fn help(&mut self, ctx: &egui::Context) {
        let room = (ctx.content_rect().height() - 200.0).max(120.0);
        let updates = self.updater.is_some().then_some(UPDATES_HELP);
        let version = match self.updater.as_ref().and_then(Updater::trial_source) {
            Some(source) => format!("Version {VERSION}. Trying updates out: this copy looks for them at {source}"),
            None => format!("Version {VERSION}"),
        };
        let shown = egui::Modal::new(egui::Id::unique("help")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.label(RichText::new("Good to know").size(22.0).strong());
            ui.add_space(8.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                for (heading, answer) in HELP.into_iter().chain(updates) {
                    ui.label(RichText::new(heading).strong());
                    ui.label(dim(answer));
                    ui.add_space(6.0);
                }
                ui.label(dim(version.as_str()).small());
            });
            ui.add_space(6.0);
            ui.vertical_centered(|ui| ui.add(primary("Close")).clicked()).inner
        });
        if shown.inner || shown.should_close() {
            self.help_open = false;
        }
    }

    fn settings_sheet(&mut self, ctx: &egui::Context) {
        let looks = self.updater.is_some();
        let mut auto_update = self.settings.auto_update;
        let mut low_end = self.settings.low_end;
        let mut effects_for = None;
        let room = (ctx.content_rect().height() - 200.0).max(160.0);
        let dota = self.engine.as_ref().map(|engine| engine.dota.root().display().to_string());
        let kept = self.data_dir.display().to_string();
        let catalog = self.catalog.clone();
        let current = self.loadout.effects_for().clone();
        let shown = egui::Modal::new(egui::Id::unique("settings")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(RichText::new("Settings").size(22.0).strong());
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                views::settings::heading(ui, "Effects of events and emblems");
                ui.label(dim("Which heroes get them:"));
                effects_for = views::settings::effects_for(ui, catalog.as_deref(), &current);
                ui.label(dim(views::settings::EFFECTS_FOR_NOTE).small());

                views::settings::heading(ui, "Play");
                ui.checkbox(&mut low_end, RichText::new("Low-end PC mode (experimental)").strong());
                ui.label(dim(LOW_END_HINT).small());

                views::settings::heading(ui, "Updates");
                ui.add_enabled(looks, egui::Checkbox::new(&mut auto_update, RichText::new("Update automatically").strong()));
                ui.label(dim(if looks { AUTO_UPDATE_NOTE } else { "Automatic updates are not set up yet. Get a newer version the same way you got this one." }).small());

                views::settings::heading(ui, "Dota 2 and your files");
                if let Some(dota) = &dota {
                    ui.label(dim(format!("Dota 2 is in {dota}")).small());
                }
                ui.label(dim(format!("Your picks, these settings and the copies of the two game files this app edits are kept in {kept}")).small());

                views::settings::heading(ui, "About");
                ui.label(dim(format!("{TITLE}, version {VERSION}. Copyright (C) 2026 Fluxen. Free software under the GNU General Public License, version 3 or later. Not made or approved by Valve.")).small());
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| ui.add(primary("Close")).clicked()).inner
        });
        if let Some(effects_for) = effects_for {
            self.loadout.set_effects_for(effects_for);
            self.saved.set_effects_for(self.loadout.effects_for().clone());
            self.save_picks();
        }
        if low_end != self.settings.low_end {
            self.settings.low_end = low_end;
            self.save_settings();
        }
        if auto_update != self.settings.auto_update {
            self.settings.auto_update = auto_update;
            self.save_settings();
            if auto_update && self.updater.as_ref().is_some_and(|updater| updater.offer().is_some()) {
                self.update(ctx);
            }
        }
        if shown.inner || shown.should_close() {
            self.settings_open = false;
        }
    }

    fn notice(&mut self, ctx: &egui::Context) {
        let mut accepted = false;
        egui::Modal::new(egui::Id::unique("notice")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(RichText::new("Before you start").size(22.0).strong());
            ui.add_space(8.0);
            for paragraph in NOTICE {
                ui.label(paragraph);
                ui.add_space(6.0);
            }
            ui.add_space(6.0);
            ui.vertical_centered(|ui| accepted = ui.add(primary("I understand")).clicked());
        });
        if accepted {
            self.settings.notice_accepted = true;
            self.save_settings();
        }
    }
}

fn bar() -> egui::Frame {
    egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(16, 10))
}

fn sheet() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(26))
}

fn said(ui: &mut Ui, facts: &Facts) {
    const MARK: f32 = 24.0;
    let (tone, sentence) = facts.sentence();
    let width = ui.available_width();
    let lay_out = |text: &str, size: f32, color: Color32| {
        let mut job = LayoutJob::simple(text.to_string(), FontId::proportional(size), color, (width - MARK).max(80.0));
        job.wrap.max_rows = 2;
        job.wrap.overflow_character = Some('…');
        ui.painter().layout_job(job)
    };
    let sentence = lay_out(&sentence, 16.0, theme::TEXT);
    let tip = facts.tip().map(|tip| lay_out(tip, theme::SMALL, theme::TEXT_DIM));
    let first_line = sentence.rows.first().map_or(sentence.size().y, |placed| placed.row.height());
    let height = sentence.size().y + tip.as_ref().map_or(0.0, |tip| tip.size().y + 2.0);

    let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(46.0)), Sense::hover());
    let top = rect.center().y - height / 2.0;
    let mark = Rect::from_center_size(pos2(rect.left() + 8.0, top + first_line / 2.0), vec2(16.0, 16.0));
    if facts.working() {
        ui.put(mark, egui::Spinner::new().size(14.0));
    } else {
        ui.painter().circle_filled(mark.center(), 4.0, tone_color(tone));
    }
    let painter = ui.painter();
    let tip_top = top + sentence.size().y + 2.0;
    painter.galley(pos2(rect.left() + MARK, top), sentence, theme::TEXT);
    if let Some(tip) = tip {
        painter.galley(pos2(rect.left() + MARK, tip_top), tip, theme::TEXT_DIM);
    }
}

fn primary(text: &str) -> egui::Button<'static> {
    let label = RichText::new(text.to_string()).size(16.0).strong().color(Color32::WHITE);
    egui::Button::new(label).fill(theme::ACCENT).min_size(vec2(150.0, 38.0))
}

fn tab_button(ui: &mut Ui, name: &str, open: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(name.to_string(), FontId::proportional(16.0), theme::TEXT_DIM);
    let (rect, response) = ui.allocate_exact_size(vec2(galley.size().x + 28.0, 34.0), Sense::click());
    let color = if open || response.hovered() { theme::TEXT } else { theme::TEXT_DIM };
    let painter = ui.painter();
    painter.galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
    if open {
        let line = Rect::from_min_max(pos2(rect.left() + 10.0, rect.bottom() - 3.0), pos2(rect.right() - 10.0, rect.bottom()));
        painter.rect_filled(line, CornerRadius::same(2), theme::ACCENT);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn tone_color(tone: Tone) -> Color32 {
    match tone {
        Tone::Plain => theme::TEXT_DIM,
        Tone::Good => theme::GOOD,
        Tone::Warn => theme::WARN,
    }
}

fn picks_count(picks: usize) -> String {
    match picks {
        1 => "1 pick".to_string(),
        many => format!("{many} picks"),
    }
}

fn taken_off(before: &Loadout, after: &Loadout) -> Option<usize> {
    let kept = |pick: &Pick| before.get(pick.hero.as_deref(), &pick.slot) == Some(pick);
    after.picks().iter().all(kept).then(|| before.len() - after.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXE: &str = "npc_dota_hero_axe";

    #[test]
    fn taking_picks_off_is_told_from_changing_them() {
        let before = Loadout::from_picks([Pick::hero(AXE, "weapon", "1"), Pick::hero(AXE, "head", "2"), Pick::world("courier", "3")]);
        assert_eq!(taken_off(&before, &before), Some(0));
        assert_eq!(taken_off(&before, &Loadout::default()), Some(3), "cleared");

        let mut fewer = before.clone();
        fewer.clear_owner(Some(AXE));
        assert_eq!(taken_off(&before, &fewer), Some(2), "one hero reset");

        let mut other_set = before.clone();
        other_set.clear(Some(AXE), "head");
        other_set.set(Pick::hero(AXE, "weapon", "9"));
        assert_eq!(taken_off(&before, &other_set), None);
        let mut restyled = before.clone();
        restyled.set(Pick::hero(AXE, "weapon", "1").styled(1));
        assert_eq!(taken_off(&before, &restyled), None);

        assert_eq!(taken_off(&Loadout::default(), &before), None, "picks were added");
    }
}
