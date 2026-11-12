use dme_core::apply::EffectsFor;
use dme_core::catalog::Catalog;
use eframe::egui::{self, RichText, Ui};

use super::dim;

pub const EFFECTS_FOR_NOTE: &str = "The effects of events (Teleport, Blink Dagger, Attack and the rest) and an emblem ride on the default items of these heroes. \
     So they show on whoever plays one of them in a match, on your screen only: Dota cannot tell your hero from the others by its files.";

pub fn effects_for(ui: &mut Ui, catalog: Option<&Catalog>, current: &EffectsFor) -> Option<EffectsFor> {
    let mut chosen = None;
    let these = matches!(current, EffectsFor::These(_));
    if ui.radio(current.is_every(), "Every hero").clicked() && !current.is_every() {
        chosen = Some(EffectsFor::Every);
    }
    if ui.radio(*current == EffectsFor::Picked, "Only the heroes I picked something for").clicked() && *current != EffectsFor::Picked {
        chosen = Some(EffectsFor::Picked);
    }
    if ui.radio(these, "Only the heroes I choose").clicked() && !these {
        chosen = Some(EffectsFor::These(Vec::new()));
    }
    if let (EffectsFor::These(heroes), Some(catalog)) = (current, catalog) {
        ui.indent("chosen heroes", |ui| {
            ui.horizontal_wrapped(|ui| {
                for hero in heroes {
                    let name = catalog.hero(hero).map_or(hero.as_str(), |known| known.name.as_str());
                    let chip = ui.button(format!("{name}  ×")).on_hover_text("Take this hero off the list");
                    if chip.clicked() {
                        chosen = Some(EffectsFor::These(heroes.iter().filter(|other| *other != hero).cloned().collect()));
                    }
                }
                let others = catalog.heroes.iter().filter(|hero| !heroes.iter().any(|known| known.eq_ignore_ascii_case(&hero.id)));
                egui::ComboBox::from_id_salt("add a hero").selected_text("Add a hero").width(160.0).height(320.0).show_ui(ui, |ui| {
                    for hero in others {
                        if ui.selectable_label(false, hero.name.as_str()).clicked() {
                            let mut more = heroes.clone();
                            more.push(hero.id.clone());
                            chosen = Some(EffectsFor::These(more));
                        }
                    }
                });
            });
            if heroes.is_empty() {
                ui.label(dim("No hero yet: until you add one, no hero gets them."));
            }
        });
    }
    chosen
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text).size(17.0).strong());
    ui.add_space(2.0);
}
