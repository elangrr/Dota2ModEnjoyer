pub mod heroes;
pub mod icons;
pub mod loadout;
pub mod picks;
pub mod rarity;
pub mod settings;
pub mod tiles;

use eframe::egui::{self, Align, Align2, FontId, Layout, RichText, Sense, Stroke, Ui, vec2};

use crate::theme;

pub fn search_id() -> egui::Id {
    egui::Id::unique("search")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub hero: Option<String>,
    pub row: loadout::Row,
    pub search: String,
}

pub fn title_bar(ui: &mut Ui, title: &str, note: Option<&str>, search: Option<(&mut String, &str)>, on_the_right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(crate::theme::title(title, 22.0));
        if let Some(note) = note {
            question_mark(ui, note);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            on_the_right(ui);
            if let Some((text, hint)) = search {
                ui.add(egui::TextEdit::singleline(text).id(search_id()).hint_text(hint).desired_width(220.0));
            }
        });
    });
    ui.add_space(4.0);
}

pub fn question_mark(ui: &mut Ui, note: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
    let color = if response.hovered() { theme::text() } else { theme::text_dim() };
    ui.painter().circle_stroke(rect.center(), 9.0, Stroke::new(1.2, color));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "?", FontId::proportional(12.5), color);
    response.on_hover_ui(|ui| {
        ui.set_max_width(380.0);
        ui.label(note);
    });
}

pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(theme::text_dim())
}

pub fn fits(name: &str, wanted: &str) -> bool {
    wanted.is_empty() || name.to_lowercase().contains(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searching_ignores_upper_and_lower_case() {
        assert!(fits("Axe of Phractos", "phrac"));
        assert!(fits("Axe of Phractos", ""));
        assert!(!fits("Axe of Phractos", "helm"));
    }
}
