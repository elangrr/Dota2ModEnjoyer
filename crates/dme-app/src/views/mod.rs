pub mod heroes;
pub mod icons;
pub mod loadout;
pub mod picks;
pub mod rarity;
pub mod settings;
pub mod tiles;

use eframe::egui::{self, Align, Align2, CornerRadius, CursorIcon, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, vec2};

use crate::theme::{self, Theme};

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

pub fn theme_switch(ui: &mut Ui) -> Option<Theme> {
    const HALF: egui::Vec2 = vec2(25.0, 22.0);
    let (rect, _) = ui.allocate_exact_size(vec2(HALF.x * 2.0 + 4.0, HALF.y + 4.0), Sense::hover());
    let day = theme::is_day();
    ui.painter().rect_filled(rect, CornerRadius::same(13), theme::text().gamma_multiply(if day { 0.08 } else { 0.10 }));
    let mut asked = None;
    for (index, (look, icon, hint)) in [(Theme::Day, icons::Icon::Day, "The day look"), (Theme::Night, icons::Icon::Night, "The night look")].into_iter().enumerate() {
        let half = Rect::from_min_size(rect.min + vec2(2.0 + HALF.x * index as f32, 2.0), HALF);
        let response = ui.interact(half, egui::Id::unique("look switch").with(index), Sense::click());
        let on = (look == Theme::Day) == day;
        if on {
            ui.painter().rect_filled(half, CornerRadius::same(11), theme::card());
            ui.painter().rect_stroke(half, CornerRadius::same(11), Stroke::new(1.0, theme::line()), StrokeKind::Inside);
        }
        let color = match (on, response.hovered()) {
            (true, _) => theme::text(),
            (false, true) => theme::text(),
            (false, false) => theme::text_dim(),
        };
        icons::paint(ui.painter(), icon, Rect::from_center_size(half.center(), vec2(14.0, 14.0)), color);
        if response.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(hint).clicked() && !on {
            asked = Some(look);
        }
    }
    asked
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
