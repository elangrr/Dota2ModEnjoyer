use dme_core::apply::Pick;
use dme_core::share::{self, LONGEST_NAME};
use eframe::egui::{self, CornerRadius, RichText};

use super::{App, Banner, TITLE, VERSION, primary};
use crate::lang::{self, T};
use crate::report;
use crate::status::Tone;
use crate::support;
use crate::theme;
use crate::update::Updater;
use crate::views::{self, dim};

const NOTICE: [T; 3] = [T::Notice1, T::Notice2, T::Notice3];

const TOUR: [(T, T); 4] = [(T::Tour1, T::Tour1Text), (T::Tour2, T::Tour2Text), (T::Tour3, T::Tour3Text), (T::Tour4, T::Tour4Text)];

const HELP: [(T, T); 18] = [
    (T::HelpFinding, T::HelpFindingText),
    (T::HelpPresets, T::HelpPresetsText),
    (T::HelpDraft, T::HelpDraftText),
    (T::HelpBuilds, T::HelpBuildsText),
    (T::HelpNotShowing, T::HelpNotShowingText),
    (T::HelpOthersWear, T::HelpOthersWearText),
    (T::HelpAbilityEffects, T::HelpAbilityEffectsText),
    (T::HelpPersonas, T::HelpPersonasText),
    (T::HelpEffects, T::HelpEffectsText),
    (T::HelpEmblems, T::HelpEmblemsText),
    (T::HelpPartMissing, T::HelpPartMissingText),
    (T::HelpVideo, T::HelpVideoText),
    (T::HelpExperiments, T::HelpExperimentsText),
    (T::HelpAfterUpdate, T::HelpAfterUpdateText),
    (T::HelpVac, T::HelpVacText),
    (T::HelpNotListed, T::HelpNotListedText),
    (T::HelpBackToNormal, T::HelpBackToNormalText),
    (T::HelpReporting, T::HelpReportingText),
];

const UPDATES_HELP: (T, T) = (T::HelpUpdates, T::HelpUpdatesText);

impl App {
    pub(super) fn help(&mut self, ctx: &egui::Context) {
        let room = (ctx.content_rect().height() - 200.0).max(120.0);
        let updates = self.updater.is_some().then_some(UPDATES_HELP);
        let version = match self.updater.as_ref().and_then(Updater::trial_source) {
            Some(source) => tr!(VersionTrying, version = VERSION, source),
            None => tr!(VersionIs, version = VERSION),
        };
        let shown = egui::Modal::new(egui::Id::unique("help")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(540.0);
            ui.label(theme::title(tr!(GoodToKnow), 22.0));
            ui.add_space(8.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                for (heading, answer) in HELP.into_iter().chain(updates) {
                    egui::CollapsingHeader::new(RichText::new(lang::t(heading)).strong()).id_salt(heading).show(ui, |ui| {
                        ui.label(dim(lang::t(answer)));
                        ui.add_space(4.0);
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(dim(version.as_str()).small());
                    if ui.small_button(tr!(ReportProblem)).on_hover_text(tr!(ReportHint)).clicked() {
                        self.open_page(&report::address("", None));
                    }
                });
            });
            ui.add_space(6.0);
            ui.vertical_centered(|ui| ui.add(primary(tr!(Close))).clicked()).inner
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
            ui.label(theme::title(tr!(SupportTitle), 22.0));
            ui.add_space(6.0);
            ui.label(dim(tr!(SupportWhy, app = TITLE)));
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                views::settings::heading(ui, tr!(StarIt));
                ui.label(dim(tr!(StarWhy)).small());
                ui.add_space(4.0);
                if ui.add(primary(tr!(StarButton))).on_hover_text(support::REPOSITORY).clicked() && !self.open_page(support::REPOSITORY) {
                    said = Some(tr!(PageNotOpened, address = support::REPOSITORY));
                }
                if !support::takes_donations() {
                    return;
                }
                views::settings::heading(ui, tr!(Donate));
                ui.label(dim(tr!(DonateWhy)).small());
                if !support::PAYPAL.is_empty() {
                    ui.add_space(4.0);
                    if ui.button(tr!(DonatePaypal)).on_hover_text(support::PAYPAL).clicked() && !self.open_page(support::PAYPAL) {
                        said = Some(tr!(PageNotOpened, address = support::PAYPAL));
                    }
                }
                for wallet in support::WALLETS {
                    ui.add_space(6.0);
                    let coin = coin_words(wallet.coin);
                    ui.label(RichText::new(&coin).strong());
                    ui.horizontal(|ui| {
                        if ui.button(tr!(Copy)).on_hover_text(tr!(CopyAddress)).clicked() {
                            self.copy(ui.ctx(), wallet.address);
                            said = Some(tr!(AddressCopied, coin));
                        }
                        ui.add(egui::Label::new(RichText::new(wallet.address).monospace().size(theme::SMALL)).wrap_mode(egui::TextWrapMode::Wrap));
                    });
                }
                if !support::WALLETS.is_empty() {
                    ui.add_space(6.0);
                    ui.label(dim(tr!(SendOnly)).small());
                }
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| ui.add(primary(tr!(Close))).clicked()).inner
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
        let offered = self.offered_languages();
        let mut language = None;
        let mut pool = None;
        let to_pool = std::mem::take(&mut self.to_pool);
        let shown = egui::Modal::new(egui::Id::unique("settings")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(theme::title(tr!(SettingsTitle), 22.0));
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                views::settings::heading(ui, tr!(Language));
                language = views::settings::language(ui, &offered);
                ui.label(dim(tr!(LanguageNote)).small());

                if to_pool {
                    ui.scroll_to_cursor(Some(egui::Align::TOP));
                }
                views::settings::heading(ui, tr!(PoolHeading));
                ui.label(dim(tr!(PoolNote)));
                ui.add_space(4.0);
                pool = views::settings::hero_pool(ui, catalog.as_deref(), &mut self.pictures, &self.settings.draft_heroes, &mut self.pool);

                views::settings::heading(ui, tr!(EffectsHeading));
                ui.label(dim(tr!(WhichHeroes)));
                effects_for = views::settings::effects_for(ui, catalog.as_deref(), &current);
                ui.label(dim(tr!(EffectsForNote)).small());

                views::settings::heading(ui, tr!(PlayHeading));
                ui.checkbox(&mut low_end, RichText::new(tr!(LowEnd)).strong());
                ui.label(dim(tr!(LowEndHint)).small());

                views::settings::heading(ui, tr!(UpdatesHeading));
                ui.add_enabled(looks, egui::Checkbox::new(&mut auto_update, RichText::new(tr!(AutoUpdate)).strong()));
                ui.label(dim(if looks { tr!(AutoUpdateNote) } else { tr!(NoAutoUpdate) }).small());

                views::settings::heading(ui, tr!(FilesHeading));
                if let Some(dota) = &dota {
                    ui.label(dim(tr!(DotaIn, folder = dota)).small());
                }
                ui.label(dim(tr!(KeptIn, folder = kept)).small());

                views::settings::heading(ui, tr!(About));
                ui.label(dim(tr!(AboutText, app = TITLE, version = VERSION)).small());
            });
            ui.add_space(10.0);
            ui.vertical_centered(|ui| ui.add(primary(tr!(Close))).clicked()).inner
        });
        if let Some(language) = language {
            self.use_language(ctx, language);
        }
        if let Some(pool) = pool {
            self.settings.draft_heroes = pool;
            self.save_settings();
        }
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
        let escaped = self.pool.choosing.is_some() && ctx.input(|input| input.key_pressed(egui::Key::Escape));
        if escaped {
            self.pool.close();
        } else if shown.inner || shown.should_close() {
            self.settings_open = false;
            self.pool = Default::default();
        }
    }

    pub(super) fn whats_new_sheet(&mut self, ctx: &egui::Context) {
        let Some(news) = self.whats_new.clone() else { return };
        let room = (ctx.content_rect().height() - 220.0).max(140.0);
        let shown = egui::Modal::new(egui::Id::unique("whats new")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(500.0);
            ui.label(theme::title(tr!(UpdatedTo, version = VERSION), 22.0));
            ui.add_space(4.0);
            ui.label(dim(tr!(WhatsNew)));
            ui.add_space(10.0);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
                for version in &news {
                    if news.len() > 1 {
                        ui.label(RichText::new(tr!(VersionNumber, version = version.number)).strong().color(theme::accent_text()));
                        ui.add_space(4.0);
                    }
                    for change in &version.changes {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new("•").strong().color(theme::accent_text()));
                            ui.add(egui::Label::new(change.as_str()).wrap_mode(egui::TextWrapMode::Wrap));
                        });
                        ui.add_space(4.0);
                    }
                    ui.add_space(6.0);
                }
            });
            ui.add_space(6.0);
            ui.vertical_centered(|ui| ui.add(primary(tr!(GotIt))).clicked()).inner
        });
        if shown.inner || shown.should_close() {
            self.whats_new = None;
            self.settings.seen_version = Some(VERSION.to_string());
            self.save_settings();
        }
    }

    pub(super) fn share_sheet(&mut self, ctx: &egui::Context) {
        let Some(catalog) = self.catalog.clone() else { return };
        let read = (!self.share_text.trim().is_empty()).then(|| share::read(&self.share_text));
        let taken = read.as_ref().and_then(|read| read.as_ref().ok()).map(|shared| {
            let settled: Vec<Pick> = shared.picks.iter().filter_map(|pick| catalog.settled(pick)).collect();
            let left_out = shared.picks.len() - settled.len();
            (shared.name.clone(), settled, left_out)
        });
        let (mut use_them, mut keep) = (false, false);
        let shown = egui::Modal::new(egui::Id::unique("share")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(theme::title(tr!(UseCodeTitle), 22.0));
            ui.add_space(4.0);
            ui.label(dim(tr!(PasteCode, prefix = share::PREFIX)));
            ui.add_space(8.0);
            let hint = format!("{}…", share::PREFIX);
            let typed = ui.add(egui::TextEdit::multiline(&mut self.share_text).hint_text(hint).desired_rows(3).desired_width(f32::INFINITY));
            if self.share_text.is_empty() && !typed.has_focus() {
                typed.request_focus();
            }
            ui.add_space(8.0);
            match (&read, &taken) {
                (Some(Err(why)), _) => {
                    ui.label(RichText::new(lang::words::not_a_code(*why)).color(theme::warn()));
                }
                (_, Some((name, picks, left_out))) => {
                    if let Some(name) = name {
                        ui.label(RichText::new(format!("\"{name}\"")).strong());
                    }
                    ui.label(code_in_words(picks, *left_out, |hero| catalog.hero(hero).map(|hero| hero.name.clone())));
                    if !picks.is_empty() {
                        ui.add_space(8.0);
                        use_them = ui.add(primary(tr!(UseThese))).on_hover_text(tr!(UseTheseHint)).clicked();
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let hint = name.clone().unwrap_or_else(|| tr!(PresetNameHint).to_string());
                            let typed = ui.add(egui::TextEdit::singleline(&mut self.share_name).hint_text(hint).char_limit(LONGEST_NAME).desired_width(220.0));
                            let entered = typed.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                            let named = !self.share_name.trim().is_empty() || name.is_some();
                            let button = ui.add_enabled(named, egui::Button::new(tr!(SaveAsPreset)));
                            keep = named && (entered || button.on_hover_text(tr!(KeepCodeHint)).clicked());
                        });
                    }
                }
                _ => {}
            }
            ui.add_space(10.0);
            ui.vertical_centered(|ui| ui.add(primary(tr!(Close))).clicked()).inner
        });
        if let Some((name, picks, left_out)) = taken.filter(|_| use_them || keep) {
            if use_them {
                self.take_in(&catalog, &picks, left_out);
            } else {
                let typed = self.share_name.trim().to_string();
                self.keep_code(if typed.is_empty() { name.as_deref().unwrap_or_default() } else { &typed }, &picks);
            }
            self.share_text.clear();
            self.share_name.clear();
            self.share_open = false;
        } else if shown.inner || shown.should_close() {
            self.share_open = false;
        }
    }

    pub(super) fn tour(&mut self, ctx: &egui::Context) {
        let shown = egui::Modal::new(egui::Id::unique("tour")).frame(sheet()).show(ctx, |ui| {
            ui.set_width(480.0);
            ui.label(theme::title(tr!(HowItWorks), 22.0));
            ui.add_space(8.0);
            for (heading, sentence) in TOUR {
                ui.label(RichText::new(lang::t(heading)).strong().color(theme::accent_text()));
                ui.label(lang::t(sentence));
                ui.add_space(8.0);
            }
            ui.add_space(4.0);
            ui.vertical_centered(|ui| ui.add(primary(tr!(GotIt))).clicked()).inner
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
            ui.label(theme::title(tr!(BeforeYouStart), 22.0));
            ui.add_space(8.0);
            for paragraph in NOTICE {
                ui.label(lang::t(paragraph));
                ui.add_space(6.0);
            }
            ui.add_space(6.0);
            ui.vertical_centered(|ui| accepted = ui.add(primary(tr!(IUnderstand))).clicked());
        });
        if accepted {
            self.settings.notice_accepted = true;
            self.save_settings();
        }
    }
}

fn coin_words(coin: &str) -> String {
    if coin.starts_with("Solana") {
        tr!(WalletSolana).to_string()
    } else if coin.starts_with("Ethereum") {
        tr!(WalletEthereum).to_string()
    } else {
        coin.to_string()
    }
}

fn sheet() -> egui::Frame {
    egui::Frame::new()
        .fill(theme::panel())
        .stroke(egui::Stroke::new(1.0, theme::line()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(26))
}

fn code_in_words(picks: &[Pick], left_out: usize, name_of: impl Fn(&str) -> Option<String>) -> String {
    let mut owners: Vec<(Option<&str>, usize)> = Vec::new();
    for pick in picks {
        match owners.iter_mut().find(|(owner, _)| *owner == pick.hero.as_deref()) {
            Some((_, count)) => *count += 1,
            None => owners.push((pick.hero.as_deref(), 1)),
        }
    }
    let named: Vec<String> = owners
        .iter()
        .map(|(owner, count)| {
            let who = owner.map_or_else(|| tr!(CodeWorld).to_string(), |hero| name_of(hero).unwrap_or_else(|| dme_core::items::hero_short_name(hero).to_string()));
            tr!(CodeOwner, who, count)
        })
        .collect();
    let whose = match named.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [first @ .., last] => tr!(CodeAnd, first = first.join(", "), last),
    };
    let has = match picks.len() {
        0 if left_out > 0 => tr!(CodeNothingInGame).to_string(),
        0 => tr!(CodeNoPicks).to_string(),
        count => tr!(CodeHas, count, whose),
    };
    match left_out {
        0 => has,
        _ if picks.is_empty() => has,
        1 => tr!(CodeOneLeftOut, has),
        many => tr!(CodeLeftOut, has, count = many),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_share_code_has_is_said_in_words() {
        let name_of = |hero: &str| (hero == "npc_dota_hero_axe").then(|| "Axe".to_string());
        let picks = [
            Pick::hero("npc_dota_hero_axe", "weapon", "1"),
            Pick::world("courier", "2"),
            Pick::hero("npc_dota_hero_axe", "head", "3"),
            Pick::hero("npc_dota_hero_dark_willow", "head", "4"),
        ];
        assert_eq!(code_in_words(&picks, 0, name_of), "It has 4 picks: Axe (2), the world (1) and dark_willow (1).");
        assert_eq!(code_in_words(&picks[..1], 0, name_of), "It has 1 pick: Axe (1).");
        assert_eq!(code_in_words(&picks[1..2], 2, name_of), "It has 1 pick: the world (1). 2 more items in it are not in your game, and are left out.");
        assert_eq!(code_in_words(&picks[..2], 1, name_of), "It has 2 picks: Axe (1) and the world (1). One more item in it is not in your game, and is left out.");
        assert_eq!(code_in_words(&[], 3, name_of), "Nothing in this code is in your game.");
        assert_eq!(code_in_words(&[], 0, name_of), "This code has no picks.");
    }
}
