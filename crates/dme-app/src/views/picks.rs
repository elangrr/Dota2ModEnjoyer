use dme_core::apply::Pick;
use dme_core::catalog::Catalog;
use dme_core::loadout::Loadout;
use eframe::egui::{self, Align, Color32, CornerRadius, CursorIcon, FontId, Layout, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::loadout::{ROW_HEIGHT, Row, RowLook, gaps_note, in_words, row};
use super::{Open, dim, rarity, title_bar};
use crate::pictures::Pictures;
use crate::presets::{LONGEST_NAME, Preset};
use crate::theme;

const PRESETS_NOTE: &str = "Everything you picked. A click on a pick opens its page; its cross takes it off. \
     To keep what is picked as a preset, type a name and press \"Save as preset\". A click on a preset picks it again, in the place of what is picked now. \
     \"Share\" copies a code of your picks, for a friend or another computer; \"Use a code\" takes one in. A right-click on a preset shares it.";

pub const SHARE_HINT: &str = "Copy a code of these picks, to paste anywhere: a friend with the app uses it under My picks, \"Use a code\". It holds only the items' numbers.";

const PICK_WIDTH: f32 = 300.0;
const GAP: f32 = 8.0;

#[derive(Default)]
pub struct Answer {
    pub changed: bool,
    pub open: Option<Open>,
    pub save: bool,
    pub use_preset: Option<usize>,
    pub forget_preset: Option<usize>,
    pub share: bool,
    pub share_preset: Option<usize>,
    pub take_code: bool,
}

pub fn show(ui: &mut Ui, catalog: &Catalog, loadout: &mut Loadout, pictures: &mut Pictures, presets: &[Preset], name: &mut String) -> Answer {
    let mut answer = Answer::default();
    let mut clear_all = false;
    title_bar(ui, "My picks", Some(PRESETS_NOTE), None, |ui| {
        let button = ui.add_enabled(!loadout.is_empty(), egui::Button::new("Clear all"));
        clear_all = button.on_hover_text("Take off every pick").clicked();
        let share = ui.add_enabled(!loadout.is_empty(), egui::Button::new("Share"));
        answer.share = share.on_hover_text(SHARE_HINT).on_disabled_hover_text("Pick something first").clicked();
        answer.take_code = ui.button("Use a code").on_hover_text("Paste a share code that someone sent you, to use its picks").clicked();
    });
    if clear_all {
        loadout.clear_all();
        answer.changed = true;
        return answer;
    }
    preset_bar(ui, presets, name, loadout, &mut answer);

    if loadout.is_empty() {
        ui.add_space(ui.available_height() * 0.2);
        ui.vertical_centered(|ui| {
            ui.label(crate::theme::title("Nothing is picked yet", 20.0));
            ui.add_space(4.0);
            ui.label(dim(match loadout.kept_away().len() {
                0 => "Choose a hero under Heroes, or a courier, a ward and the rest under World. What you pick is listed here.".to_string(),
                1 => "One pick is put away. \"Bring back my picks\", beside Play, picks it again.".to_string(),
                many => format!("{many} picks are put away. \"Bring back my picks\", beside Play, picks them again."),
            }));
        });
        return answer;
    }

    let owners = std::iter::once(None).chain(catalog.heroes.iter().map(Some));
    egui::ScrollArea::vertical().id_salt("all picks").auto_shrink(false).show(ui, |ui| {
        for owner in owners {
            let id = owner.map(|hero| hero.id.as_str());
            let picks: Vec<Pick> = loadout.of(id).cloned().collect();
            if picks.is_empty() {
                continue;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(owner.map_or("The world", |hero| hero.name.as_str())).size(17.0).strong());
                ui.label(dim(count(picks.len())).small());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(14.0);
                    let hint = if owner.is_some() { "Take off every pick for this hero" } else { "Take off every pick that is not a hero's" };
                    if ui.add(egui::Button::new(dim("Reset")).frame(false)).on_hover_text(hint).clicked() {
                        loadout.clear_owner(id);
                        answer.changed = true;
                    }
                });
            });
            ui.add_space(2.0);

            let room = ui.available_width() - 14.0;
            let columns = (((room + GAP) / (PICK_WIDTH + GAP)).floor() as usize).max(1);
            let width = (room + GAP) / columns as f32 - GAP;
            for line in picks.chunks(columns) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(GAP, 4.0);
                    for pick in line {
                        let slot = catalog.slot(id, &pick.slot);
                        let entry = slot.and_then(|slot| slot.entry(&pick.item));
                        let (value, picture) = match (slot, entry) {
                            (Some(slot), Some(entry)) => in_words(slot, pick, entry),
                            _ => (pick.item.clone(), None),
                        };
                        let warning = entry.and_then(|entry| gaps_note(&entry.gaps));
                        let look = RowLook {
                            label: slot.map_or(pick.slot.as_str(), |slot| slot.name.as_str()),
                            value: &value,
                            picked: true,
                            picture,
                            band: entry.and_then(|entry| rarity::color(&catalog.rarities, entry.rarity)),
                            open: false,
                            clear_hint: "Take this pick off",
                            warning: warning.as_deref(),
                        };
                        let said = ui.allocate_ui(vec2(width, ROW_HEIGHT), |ui| row(ui, pictures, &look)).inner;
                        if said.cleared {
                            loadout.clear(id, &pick.slot);
                            answer.changed = true;
                        }
                        if said.clicked {
                            answer.open = Some(Open { hero: id.map(str::to_string), row: Row::Slot(pick.slot.clone()), search: String::new() });
                        }
                    }
                });
            }
        }
    });
    answer
}

fn preset_bar(ui: &mut Ui, presets: &[Preset], name: &mut String, loadout: &Loadout, answer: &mut Answer) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        ui.label(dim("Presets"));
        for (place, preset) in presets.iter().enumerate() {
            let worn = !loadout.is_empty() && dme_core::loadout::same_picks(loadout.picks(), &preset.picks);
            let said = preset_chip(ui, &preset.name, preset.picks.len(), worn);
            if said.used && !worn {
                answer.use_preset = Some(place);
            }
            if said.forgotten {
                answer.forget_preset = Some(place);
            }
            if said.shared {
                answer.share_preset = Some(place);
            }
        }
        if presets.is_empty() {
            ui.label(dim("none yet").small());
        }
        ui.add_space(10.0);
        let can_save = !loadout.is_empty();
        let hint = if can_save { "Name for a new preset" } else { "Pick something first" };
        let typed = ui.add_enabled(can_save, egui::TextEdit::singleline(name).hint_text(hint).char_limit(LONGEST_NAME).desired_width(200.0));
        let entered = typed.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let save = ui.add_enabled(can_save && !name.trim().is_empty(), egui::Button::new("Save as preset"));
        let known = presets.iter().any(|preset| preset.name.eq_ignore_ascii_case(name.trim()));
        let save = save.on_hover_text(if known { "Keep what is picked now under this name, in the place of what was kept there" } else { "Keep what is picked now under this name, to pick it again later with one click" });
        answer.save = can_save && !name.trim().is_empty() && (save.clicked() || entered);
    });
    ui.add_space(6.0);
}

struct ChipAnswer {
    used: bool,
    forgotten: bool,
    shared: bool,
}

fn preset_chip(ui: &mut Ui, name: &str, picks: usize, worn: bool) -> ChipAnswer {
    const CROSS: f32 = 24.0;
    let text = ui.painter().layout_no_wrap(format!("{name}  ·  {picks}"), FontId::proportional(13.5), theme::text());
    let (rect, response) = ui.allocate_exact_size(vec2(text.size().x + 22.0 + CROSS, 28.0), Sense::click());
    let cross = egui::Rect::from_center_size(pos2(rect.right() - CROSS / 2.0 - 3.0, rect.center().y), vec2(20.0, 20.0));
    let on_cross = ui.interact(cross, response.id.with("forget"), Sense::click());
    if ui.is_rect_visible(rect) {
        let (fill, line, ink) = if worn {
            (theme::card_picked(), theme::accent(), theme::text())
        } else if response.hovered() || on_cross.hovered() {
            (theme::card_hover(), theme::line(), theme::text())
        } else {
            (theme::card(), theme::line(), theme::text_dim())
        };
        let painter = ui.painter();
        let round = CornerRadius::same(14);
        painter.rect_filled(rect, round, fill);
        painter.rect_stroke(rect, round, Stroke::new(1.0, line), StrokeKind::Inside);
        painter.galley_with_override_text_color(pos2(rect.left() + 12.0, rect.center().y - text.size().y / 2.0), text, ink);
        if on_cross.hovered() {
            painter.circle_filled(cross.center(), 10.0, theme::line());
        }
        let stroke = Stroke::new(1.4, if on_cross.hovered() { theme::text() } else { Color32::from_gray(120) });
        let (center, arm) = (cross.center(), 3.5);
        painter.line_segment([center + vec2(-arm, -arm), center + vec2(arm, arm)], stroke);
        painter.line_segment([center + vec2(-arm, arm), center + vec2(arm, -arm)], stroke);
    }
    let mut forgotten = on_cross.clicked();
    on_cross.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Forget this preset. Your picks stay as they are.");
    let used = response.clicked() && !forgotten;
    let hint = if worn { "This is what is picked now. A right-click shares it." } else { "Pick this, in the place of what is picked now. A right-click shares it." };
    let mut shared = false;
    response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).context_menu(|ui| {
        if ui.button("Share this preset").on_hover_text(SHARE_HINT).clicked() {
            shared = true;
            ui.close();
        }
        if ui.button("Forget this preset").clicked() {
            forgotten = true;
            ui.close();
        }
    });
    ChipAnswer { used, forgotten, shared }
}

fn count(picks: usize) -> String {
    match picks {
        1 => "1 pick".to_string(),
        many => format!("{many} picks"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picks_of_an_owner_are_counted_in_words() {
        assert_eq!(count(1), "1 pick");
        assert_eq!(count(4), "4 picks");
    }
}
