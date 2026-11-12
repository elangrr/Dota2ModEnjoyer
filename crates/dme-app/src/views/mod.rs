pub mod heroes;
pub mod loadout;
pub mod rarity;
pub mod settings;
pub mod tiles;

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::theme;

pub fn search_id() -> egui::Id {
    egui::Id::unique("search")
}

pub fn title_bar(ui: &mut Ui, title: &str, search: Option<(&mut String, &str)>, on_the_right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(22.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            on_the_right(ui);
            if let Some((text, hint)) = search {
                ui.add(egui::TextEdit::singleline(text).id(search_id()).hint_text(hint).desired_width(220.0));
            }
        });
    });
    ui.add_space(4.0);
}

pub fn dim(text: impl Into<String>) -> RichText {
    RichText::new(text).color(theme::TEXT_DIM)
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
