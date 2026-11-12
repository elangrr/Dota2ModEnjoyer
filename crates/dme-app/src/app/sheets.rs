use eframe::egui::{self, CornerRadius, RichText};

use super::{App, Banner, TITLE, VERSION, primary};
use crate::status::Tone;
use crate::support;
use crate::theme;
use crate::update::Updater;
use crate::views::{self, dim};

const NOTICE: [&str; 3] = [
    "This app changes how Dota 2 looks on your computer. It does that by editing two of the game's own files. Only you see the result.",
    "Editing game files may be against Valve's rules for Dota 2, so you use this app at your own risk. It is not made or approved by Valve.",
    "Nothing is changed while Dota 2 is running, and \"Back to normal Dota\" undoes everything.",
];

const TOUR: [(&str, &str); 4] = [
    ("1. Pick", "Choose a hero and click what it should wear: a whole set, or piece by piece. The courier, the ward and the rest are under World."),
    ("2. Play", "Press Play. Your picks are put in the game and Dota 2 starts. Close Dota first if it is open."),
    ("3. In Dota", "A pick takes the place of a default item. If you wear an item of your own in that slot, unequip it there for the pick to show."),
    ("Good to know", "Just type to search. \"My picks\" lists everything you picked. \"Back to normal Dota\" undoes it all. Help has the rest."),
];

const LOW_END_HINT: &str = "Play starts Dota with its lightest video settings: no shadows, simple water and lighting, no fog, grass or other extras. \
     An experiment. Dota may keep these settings afterwards; you can change them back in Dota's video options.";

const HELP: [(&str, &str); 14] = [
    (
        "Finding things",
        "Just type. On the hero grid that finds heroes and, from the third letter on, items and sets wherever they are. The chips above the heroes show only those of one attribute, or those with picks. Rest the pointer on an item or a set for a larger look at it and its styles.",
    ),
    (
        "My picks and presets",
        "\"My picks\", on the left, lists everything you picked. A click on a pick opens its page, and its cross takes it off. To keep what is picked as a preset, type a name there and press \"Save as preset\". A click on a preset picks it again, in the place of what is picked now.",
    ),
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
        "On the World page, under \"Effects\", you can put an event's emblem around your heroes, and use the effect a past event had for a teleport, a Blink Dagger, attacks and more. They go on the heroes you picked something for; Settings can make that every hero, or heroes you choose. A hero shows them as long as its slots in Dota are on their default items.",
    ),
    (
        "A part of an item does not show",
        "An item or a set with a small ! mark has a part that may not show: Dota no longer has some of its files, or some of its effects go by the level of an arcana, which Dota's servers give only to its owner, or it is known to be buggy, as the fog of Aghanim's Labyrinth's map effect is. The lightest video settings leave parts of effects out as well.",
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
    "When a newer version of this app is out, an Update button shows on the left. One click downloads it and starts the app again. Your picks stay as they are. In Settings you can have the app do this by itself.",
);

const AUTO_UPDATE_NOTE: &str = "On: when a newer version of this app is out, the app downloads it and starts again by itself. Off: an Update button shows on the left, and nothing happens until you press it. Your picks stay as they are either way.";

impl App {
    pub(super) fn help(&mut self, ctx: &egui::Context) {
        let room = (ctx.content_rect().height() - 200.0).max(120.0);
        let updates = self.updater.is_some().then_some(UPDATES_HELP);
        let version = match self.updater.as_ref().and_then(Updater::trial_source) {
            Some(source) => format!("Version {VERSION}. Trying updates out: this copy looks for them at {source}"),
            None => format!("Version {VERSION}"),
        };
        let shown = egui::Modal::new(egui::Id::unique("help")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.label(theme::title("Good to know", 22.0));
            ui.add_space(8.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                for (heading, answer) in HELP.into_iter().chain(updates) {
                    egui::CollapsingHeader::new(RichText::new(heading).strong()).show(ui, |ui| {
                        ui.label(dim(answer));
                        ui.add_space(4.0);
                    });
                }
                ui.add_space(6.0);
                ui.label(dim(version.as_str()).small());
            });
            ui.add_space(6.0);
            ui.vertical_centered(|ui| ui.add(primary("Close")).clicked()).inner
        });
        if shown.inner || shown.should_close() {
            self.help_open = false;
        }
    }

    pub(super) fn support_sheet(&mut self, ctx: &egui::Context) {
        let room = (ctx.content_rect().height() - 200.0).max(160.0);
        let mut said = None;
        let shown = egui::Modal::new(egui::Id::unique("support")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(500.0);
            ui.label(theme::title("Support the project", 22.0));
            ui.add_space(6.0);
            ui.label(dim(format!("{TITLE} is free, and its code is open. If you like it, this is how to help.")));
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                views::settings::heading(ui, "Star it on GitHub");
                ui.label(dim("A star costs nothing and helps other players find the project.").small());
                ui.add_space(4.0);
                if ui.add(primary("Star on GitHub")).on_hover_text(support::REPOSITORY).clicked() && !support::open(support::REPOSITORY) {
                    said = Some(format!("The page could not be opened. It is at {}", support::REPOSITORY));
                }
                if !support::takes_donations() {
                    return;
                }
                views::settings::heading(ui, "Donate");
                ui.label(dim("Donations go to the maker, Fluxen, and are never asked for. The app works the same without one.").small());
                if !support::PAYPAL.is_empty() {
                    ui.add_space(4.0);
                    if ui.button("Donate with PayPal").on_hover_text(support::PAYPAL).clicked() && !support::open(support::PAYPAL) {
                        said = Some(format!("The page could not be opened. It is at {}", support::PAYPAL));
                    }
                }
                for wallet in support::WALLETS {
                    ui.add_space(6.0);
                    ui.label(RichText::new(wallet.coin).strong());
                    ui.horizontal(|ui| {
                        if ui.button("Copy").on_hover_text("Copy this address").clicked() {
                            ui.ctx().copy_text(wallet.address.to_string());
                            said = Some(format!("The {} address is copied.", wallet.coin));
                        }
                        ui.add(egui::Label::new(RichText::new(wallet.address).monospace().size(theme::SMALL)).wrap_mode(egui::TextWrapMode::Wrap));
                    });
                }
                if !support::WALLETS.is_empty() {
                    ui.add_space(6.0);
                    ui.label(dim("Send only the currency named, on the network named: anything else sent to an address is lost.").small());
                }
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| ui.add(primary("Close")).clicked()).inner
        });
        if let Some(said) = said {
            self.banner = Some(Banner::new(Tone::Plain, said));
        }
        if shown.inner || shown.should_close() {
            self.support_open = false;
        }
    }

    pub(super) fn settings_sheet(&mut self, ctx: &egui::Context) {
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
            ui.label(theme::title("Settings", 22.0));
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

    pub(super) fn tour(&mut self, ctx: &egui::Context) {
        let shown = egui::Modal::new(egui::Id::unique("tour")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(theme::title("How it works", 22.0));
            ui.add_space(8.0);
            for (heading, sentence) in TOUR {
                ui.label(RichText::new(heading).strong().color(theme::accent_text()));
                ui.label(sentence);
                ui.add_space(8.0);
            }
            ui.add_space(4.0);
            ui.vertical_centered(|ui| ui.add(primary("Got it")).clicked()).inner
        });
        if shown.inner || shown.should_close() {
            self.settings.tour_seen = true;
            self.save_settings();
        }
    }

    pub(super) fn notice(&mut self, ctx: &egui::Context) {
        let mut accepted = false;
        egui::Modal::new(egui::Id::unique("notice")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(theme::title("Before you start", 22.0));
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

fn sheet() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::panel())
        .stroke(egui::Stroke::new(1.0, theme::line()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(26))
}
