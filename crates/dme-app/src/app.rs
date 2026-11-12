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
use eframe::egui::{self, Align, Color32, CornerRadius, CursorIcon, FontId, Layout, Rect, Sense, Ui, pos2, vec2};

use crate::Options;
use crate::pictures::Pictures;
use crate::presets::Presets;
use crate::script::Script;
use crate::settings::Settings;
use crate::status::{Busy, Facts, Tone};
use crate::theme;
use crate::update::{self, News, Updater};
use crate::views::icons::Icon;
use crate::views::loadout::{Page, Row};
use crate::views::{self, dim, title_bar};
use crate::worker::{Failure, GameState, Job, Reply, Worker};

mod sheets;

pub const TITLE: &str = "Dota 2 Mod Enjoyer";

const VERSION: &str = env!("CARGO_PKG_VERSION");

const SETTINGS_FILE: &str = "settings.json";
const PRESETS_FILE: &str = "presets.json";
const STARTING_SECONDS: f64 = 90.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Heroes,
    World,
    Picks,
}

struct Banner {
    tone: Tone,
    text: String,
    details: Option<String>,
    undo: Option<Loadout>,
    update: bool,
    since: Option<f64>,
}

impl Banner {
    fn new(tone: Tone, text: impl Into<String>) -> Banner {
        Banner { tone, text: text.into(), details: None, undo: None, update: false, since: None }
    }

    fn stays(&self) -> Option<f64> {
        if self.tone == Tone::Warn || self.details.is_some() || self.update {
            None
        } else if self.undo.is_some() {
            Some(12.0)
        } else {
            Some(7.0)
        }
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
    found: views::heroes::Found,
    presets: Presets,
    preset_name: String,
    hero_filter: views::heroes::Filter,
    hero_row: Row,
    world_row: Row,
    item_search: String,
    rarities_asked: BTreeSet<usize>,
    help_open: bool,
    settings_open: bool,
    support_open: bool,
    look: (theme::Theme, Option<egui::Theme>),
    footer_top: f32,

    updater: Option<Updater>,
    start_again: bool,

    pictures: Pictures,
    script: Option<Script>,
}

impl App {
    pub fn new(creation: &eframe::CreationContext<'_>, options: Options) -> App {
        let ctx = &creation.egui_ctx;
        let data_dir = options.data_dir.clone().unwrap_or_else(engine::default_data_dir);
        let settings = Settings::load(&data_dir.join(SETTINGS_FILE));
        theme::install_fonts(ctx);
        theme::apply(ctx, settings.theme);
        let look = (settings.theme, ctx.system_theme());
        let presets = Presets::load(&data_dir.join(PRESETS_FILE));
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
            found: views::heroes::Found::default(),
            presets,
            preset_name: String::new(),
            hero_filter: views::heroes::Filter::default(),
            hero_row: Row::Sets,
            world_row: Row::Sets,
            item_search: String::new(),
            rarities_asked: BTreeSet::new(),
            help_open: false,
            settings_open: false,
            support_open: false,
            look,
            footer_top: 0.0,
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

    fn go(&mut self, open: views::Open) {
        match open.hero {
            Some(hero) => {
                self.tab = Tab::Heroes;
                self.hero = Some(hero);
                self.hero_row = open.row;
            }
            None => {
                self.tab = Tab::World;
                self.world_row = open.row;
            }
        }
        self.hero_search.clear();
        self.item_search = open.search;
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
            Some("picks") => self.tab = Tab::Picks,
            Some("help") => self.help_open = true,
            Some("settings") => self.settings_open = true,
            Some("support") => self.support_open = true,
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

    fn save_presets(&mut self) {
        if let Err(error) = self.presets.save(&self.data_dir.join(PRESETS_FILE)) {
            self.banner = Some(Banner::new(Tone::Warn, format!("Your presets could not be saved: {error}")));
        }
    }

    fn save_preset(&mut self) {
        let Some(name) = self.presets.keep(&self.preset_name, self.loadout.picks()) else { return };
        self.preset_name.clear();
        self.banner = Some(Banner::new(Tone::Good, format!("Your picks are saved as the preset \"{name}\".")));
        self.save_presets();
    }

    fn use_preset(&mut self, catalog: &Catalog, place: usize) {
        let Some(preset) = self.presets.get(place) else { return };
        let before = self.loadout.clone();
        let mut picked = Loadout::from_picks(preset.picks.iter().cloned());
        picked.set_effects_for(before.effects_for().clone());
        let gone = picked.picks().iter().filter(|pick| catalog.settled(pick).is_none()).count();
        picked.settle(|pick| catalog.settled(pick));
        let text = match gone {
            0 => format!("The preset \"{}\" is picked. Press Play to see it in Dota.", preset.name),
            1 => format!("The preset \"{}\" is picked, without one item that is not in the game any more.", preset.name),
            many => format!("The preset \"{}\" is picked, without {many} items that are not in the game any more.", preset.name),
        };
        self.loadout = picked;
        self.saved = self.loadout.clone();
        if self.save_picks() {
            self.banner = Some(Banner::with_undo(text, before));
        }
    }

    fn forget_preset(&mut self, place: usize) {
        if let Some(preset) = self.presets.forget(place) {
            self.banner = Some(Banner::new(Tone::Plain, format!("The preset \"{}\" is forgotten. Your picks stay as they are.", preset.name)));
            self.save_presets();
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
        if self.catalog.is_none() || !self.settings.notice_accepted || !self.settings.tour_seen || self.help_open || self.settings_open || self.support_open || self.tab == Tab::Picks {
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
        let look = (self.settings.theme, ctx.system_theme());
        if look != self.look {
            self.look = look;
            theme::apply(ctx, self.settings.theme);
        }
        self.listen(ctx);
        self.start_the_new_version(ctx);
        self.close_politely(ctx);
        self.run_script(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.pictures.begin_frame();
        theme::paint_backdrop(&ui.ctx().layer_painter(egui::LayerId::background()), ui.ctx().content_rect());
        if self.engine.is_none() {
            self.find_dota(ui);
        } else {
            self.type_to_search(ui.ctx());
            self.nav(ui);
            self.footer(ui);
            let frame = egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 16, top: 16, bottom: 6 });
            egui::CentralPanel::default().frame(frame).show(ui, |ui| self.content(ui));
            self.message(ui.ctx());
            if !self.settings.notice_accepted {
                self.notice(ui.ctx());
            } else if !self.settings.tour_seen {
                self.tour(ui.ctx());
            } else if self.help_open {
                self.help(ui.ctx());
            } else if self.settings_open {
                self.settings_sheet(ui.ctx());
            } else if self.support_open {
                self.support_sheet(ui.ctx());
            }
        }
        self.pictures.end_frame();
        if std::mem::take(&mut self.picks_changed) {
            self.picks_were_changed();
        }
    }
}

impl App {
    fn nav(&mut self, ui: &mut Ui) {
        const WIDTH: f32 = 92.0;
        let mut update = false;
        let mut look = None;
        let frame = egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 2, top: 10, bottom: 10 });
        egui::Panel::left("nav").exact_size(WIDTH).resizable(false).show_separator_line(false).frame(frame).show(ui, |ui| {
            theme::paint_glass(ui.painter(), ui.max_rect(), theme::GLASS_ROUND);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                let (mark, _) = ui.allocate_exact_size(vec2(34.0, 36.0), Sense::hover());
                crate::logo::paint(ui.painter(), mark);
                ui.add_space(10.0);
                let picks = self.loadout.len();
                for (tab, icon, name, count) in [(Tab::Heroes, Icon::Heroes, "Heroes", 0), (Tab::World, Icon::World, "World", 0), (Tab::Picks, Icon::Picks, "My picks", picks)] {
                    if nav_button(ui, icon, name, self.tab == tab, count).clicked() && self.tab != tab {
                        self.tab = tab;
                        self.item_search.clear();
                    }
                }
            });
            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                ui.add_space(10.0);
                look = views::theme_switch(ui);
                ui.add_space(8.0);
                if nav_button(ui, Icon::Help, "Help", self.help_open, 0).clicked() {
                    self.help_open = true;
                }
                if nav_button(ui, Icon::Settings, "Settings", self.settings_open, 0).clicked() {
                    self.settings_open = true;
                }
                if nav_button(ui, Icon::Support, "Support", self.support_open, 0).on_hover_text("Give the project a star, or donate").clicked() {
                    self.support_open = true;
                }
                if let Some(updater) = &self.updater
                    && let Some(offer) = updater.offer()
                {
                    let (label, waiting) = match (updater.is_installed(), updater.is_fetching()) {
                        (true, _) => ("Updated", false),
                        (false, true) => ("Updating…", false),
                        (false, false) => ("Update", true),
                    };
                    let hint = format!("Version {} of this app is out. One click downloads it and starts the app again. Your picks stay as they are.", offer.version);
                    let button = ui.add_enabled_ui(waiting, |ui| nav_button(ui, Icon::Update, label, true, 0)).inner;
                    update = button.on_hover_text(hint).clicked() && waiting;
                }
            });
        });
        if update {
            self.update(ui.ctx());
        }
        if let Some(look) = look {
            self.settings.theme = look;
            self.save_settings();
        }
    }

    fn footer(&mut self, ui: &mut Ui) {
        let facts = self.facts(ui.ctx());
        let frame = egui::Frame::new().outer_margin(egui::Margin { left: 12, right: 12, top: 4, bottom: 10 }).inner_margin(egui::Margin::symmetric(14, 10));
        let bar = egui::Panel::bottom("footer").show_separator_line(false).frame(frame).show(ui, |ui| {
            theme::paint_glass(ui.painter(), ui.max_rect().expand2(vec2(14.0, 10.0)), theme::GLASS_ROUND);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if play_button(ui, facts.play_label(), facts.can_play()).clicked() {
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
        self.footer_top = bar.response.rect.top();
    }

    fn message(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|input| input.time);
        let Some(banner) = &mut self.banner else { return };
        let since = *banner.since.get_or_insert(now);
        let (mut close, mut undo, mut update) = (false, false, false);
        if let Some(stays) = banner.stays() {
            close = now - since > stays;
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        let banner = &*banner;
        let room = ctx.content_rect();
        let width = (room.width() - 140.0).clamp(240.0, 460.0);
        let frame = egui::Frame::new()
            .fill(theme::panel())
            .stroke(egui::Stroke::new(1.0, theme::line()))
            .corner_radius(CornerRadius::same(theme::GLASS_ROUND - 2))
            .shadow(egui::Shadow { offset: [0, 8], blur: 26, spread: 0, color: Color32::from_black_alpha(if theme::is_day() { 40 } else { 110 }) })
            .inner_margin(egui::Margin::symmetric(14, 12));
        let above_the_bar = -(room.bottom() - self.footer_top + 10.0);
        egui::Area::new(egui::Id::unique("message")).anchor(egui::Align2::RIGHT_BOTTOM, vec2(-16.0, above_the_bar)).order(egui::Order::Foreground).show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(width);
                let color = match banner.tone {
                    Tone::Plain => theme::text(),
                    tone => tone_color(tone),
                };
                let mut job = LayoutJob::simple(banner.text.clone(), FontId::proportional(theme::BODY), color, width);
                job.wrap.max_rows = 6;
                job.wrap.overflow_character = Some('…');
                let text = ui.painter().layout_job(job);
                let (rect, _) = ui.allocate_exact_size(vec2(width, text.size().y), Sense::hover());
                ui.painter().galley(rect.min, text, color);
                ui.add_space(2.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    close |= ui.button(if banner.update { "Later" } else { "Close" }).clicked();
                    update = banner.update && ui.button("Update").on_hover_text("Download the new version and start the app again").clicked();
                    undo = banner.undo.is_some() && ui.button("Undo").on_hover_text("Put those picks back").clicked();
                    if let Some(details) = &banner.details
                        && ui.button("Copy details").on_hover_text("Copy what happened, to send to whoever helps you").clicked()
                    {
                        ui.ctx().copy_text(details.clone());
                    }
                });
            });
        });
        if undo {
            if let Some(before) = self.banner.take().and_then(|banner| banner.undo) {
                self.loadout = before;
                self.saved = self.loadout.clone();
                self.save_picks();
            }
        } else if update {
            self.update(ctx);
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
            Tab::Picks => {
                let answer = views::picks::show(ui, &catalog, &mut self.loadout, &mut self.pictures, self.presets.all(), &mut self.preset_name);
                self.picks_changed |= answer.changed;
                if answer.save {
                    self.save_preset();
                } else if let Some(place) = answer.use_preset {
                    self.use_preset(&catalog, place);
                } else if let Some(place) = answer.forget_preset {
                    self.forget_preset(place);
                }
                if let Some(open) = answer.open {
                    self.go(open);
                }
            }
            Tab::Heroes => match self.hero.as_deref().and_then(|id| catalog.hero(id)) {
                Some(hero) => self.hero_page(ui, &catalog, hero),
                None => {
                    let clicked = views::heroes::show(ui, &catalog, &self.loadout, &mut self.pictures, &mut self.hero_search, &mut self.found, &mut self.hero_filter);
                    if let Some(open) = clicked {
                        self.go(open);
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
            ui.label(theme::title("The game's files could not be read", 20.0));
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
            ui.add_space(6.0);
            let (portrait, _) = ui.allocate_exact_size(vec2(78.0, 44.0), Sense::hover());
            views::tiles::fill_with(ui.painter(), &mut self.pictures, hero.picture.as_deref(), portrait, 9, 1.0);
            ui.add_space(4.0);
            ui.label(theme::title(&hero.name, 24.0));
            if let Some(attribute) = hero.attribute {
                ui.label(dim(attribute.name()).small());
            }
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
        title_bar(ui, "The world", None, None, |ui| {
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
        let frame = egui::Frame::new().inner_margin(egui::Margin::same(24));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.25);
                ui.label(theme::title("Dota 2 was not found", 24.0));
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
}

fn said(ui: &mut Ui, facts: &Facts) {
    let (tone, sentence) = facts.sentence();
    let width = ui.available_width();
    let badge = facts.badge().filter(|_| width > 520.0).map(|(tone, words)| {
        let color = tone_color(tone);
        (ui.painter().layout_no_wrap(words.to_string(), FontId::proportional(theme::SMALL + 0.5), color), color)
    });
    let before = badge.as_ref().map_or(24.0, |(words, _)| words.size().x + 32.0);
    let lay_out = |text: &str, size: f32, color: Color32| {
        let mut job = LayoutJob::simple(text.to_string(), FontId::proportional(size), color, (width - before).max(80.0));
        job.wrap.max_rows = 2;
        job.wrap.overflow_character = Some('…');
        ui.painter().layout_job(job)
    };
    let sentence = lay_out(&sentence, 16.0, theme::text());
    let tip = facts.tip().map(|tip| lay_out(tip, theme::SMALL, theme::text_dim()));
    let first_line = sentence.rows.first().map_or(sentence.size().y, |placed| placed.row.height());
    let height = sentence.size().y + tip.as_ref().map_or(0.0, |tip| tip.size().y + 2.0);

    let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(46.0)), Sense::hover());
    let top = rect.center().y - height / 2.0;
    let mark = Rect::from_center_size(pos2(rect.left() + 8.0, top + first_line / 2.0), vec2(16.0, 16.0));
    if facts.working() {
        ui.put(mark, egui::Spinner::new().size(14.0));
    } else if let Some((words, color)) = badge {
        let plate = Rect::from_min_size(pos2(rect.left(), rect.center().y - 13.0), vec2(words.size().x + 20.0, 26.0));
        let round = CornerRadius::same(13);
        ui.painter().rect_filled(plate, round, color.gamma_multiply(0.16));
        ui.painter().rect_stroke(plate, round, egui::Stroke::new(1.0, color.gamma_multiply(0.55)), egui::StrokeKind::Inside);
        ui.painter().galley(plate.center() - words.size() / 2.0, words, color);
    } else {
        ui.painter().circle_filled(mark.center(), 4.0, tone_color(tone));
    }
    let painter = ui.painter();
    let tip_top = top + sentence.size().y + 2.0;
    painter.galley(pos2(rect.left() + before, top), sentence, theme::text());
    if let Some(tip) = tip {
        painter.galley(pos2(rect.left() + before, tip_top), tip, theme::text_dim());
    }
}

fn primary(text: &str) -> egui::Button<'static> {
    let label = theme::title(text.to_string(), 16.0).color(Color32::WHITE);
    egui::Button::new(label).fill(theme::accent()).min_size(vec2(150.0, 38.0))
}

fn nav_button(ui: &mut Ui, icon: Icon, name: &str, open: bool, count: usize) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(72.0, 56.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered() || open, 0.12);
        let enabled = ui.is_enabled();
        let color = match (open && enabled, enabled) {
            (true, _) => theme::accent_text(),
            (false, true) => theme::text_dim().lerp_to_gamma(theme::text(), lit),
            (false, false) => theme::text_dim().gamma_multiply(0.6),
        };
        let painter = ui.painter();
        if open && enabled {
            painter.rect_filled(rect.shrink2(vec2(3.0, 1.0)), CornerRadius::same(12), theme::accent().gamma_multiply(if theme::is_day() { 0.16 } else { 0.20 }));
        } else if lit > 0.0 {
            painter.rect_filled(rect.shrink2(vec2(3.0, 1.0)), CornerRadius::same(12), theme::text().gamma_multiply(0.07 * lit));
        }
        let drawing = Rect::from_center_size(pos2(rect.center().x, rect.top() + 20.0), vec2(23.0, 23.0));
        views::icons::paint(painter, icon, drawing, color);
        painter.text(pos2(rect.center().x, rect.bottom() - 13.0), egui::Align2::CENTER_CENTER, name, FontId::proportional(11.0), color);
        if count > 0 {
            let number = painter.layout_no_wrap(count.to_string(), theme::heavy(10.0), Color32::WHITE);
            let plate = Rect::from_center_size(pos2(drawing.right() + 5.0, drawing.top() + 2.0), vec2((number.size().x + 9.0).max(17.0), 17.0));
            painter.rect_filled(plate, CornerRadius::same(9), theme::accent());
            painter.galley(plate.center() - number.size() / 2.0, number, Color32::WHITE);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn play_button(ui: &mut Ui, label: &str, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(176.0, 46.0), sense);
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), enabled && response.hovered(), 0.12);
        let painter = ui.painter();
        let round = CornerRadius::same(12);
        let strength = if enabled { 1.0 } else { 0.45 };
        if enabled {
            for step in 1..=3_u8 {
                let grown = rect.expand(f32::from(step) * 2.0).translate(vec2(0.0, 3.0));
                painter.rect_filled(grown, CornerRadius::same(12 + step * 2), theme::accent().gamma_multiply(0.10 + 0.05 * lit));
            }
        }
        painter.rect_filled(rect, round, theme::accent().gamma_multiply(strength));
        let top = Rect::from_min_max(rect.min, pos2(rect.right(), rect.center().y));
        painter.rect_filled(top, CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, Color32::from_white_alpha((22.0 + 16.0 * lit) as u8).gamma_multiply(strength));
        painter.rect_stroke(rect, round, egui::Stroke::new(1.0, Color32::from_white_alpha(46).gamma_multiply(strength)), egui::StrokeKind::Inside);
        let ink = Color32::WHITE.gamma_multiply(if enabled { 1.0 } else { 0.7 });
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, theme::heavy(18.0), ink);
    }
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

fn tone_color(tone: Tone) -> Color32 {
    match tone {
        Tone::Plain => theme::text_dim(),
        Tone::Good => theme::good(),
        Tone::Warn => theme::warn(),
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
