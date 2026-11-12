use std::collections::BTreeSet;

use dme_core::catalog::Rarity;
use eframe::egui::{self, Color32, CornerRadius, CursorIcon, FontId, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::dim;
use crate::theme;

pub fn color(rarities: &[Rarity], place: Option<usize>) -> Option<Color32> {
    let [red, green, blue] = rarities.get(place?)?.color;
    Some(Color32::from_rgb(red, green, blue))
}

pub fn named(rarities: &[Rarity], place: Option<usize>) -> Option<(&str, Color32)> {
    Some((rarities.get(place?)?.name.as_str(), color(rarities, place)?))
}

pub fn wanted(asked: &BTreeSet<usize>, place: Option<usize>) -> bool {
    asked.is_empty() || place.is_some_and(|place| asked.contains(&place))
}

pub fn chips(ui: &mut Ui, rarities: &[Rarity], present: &BTreeSet<usize>, asked: &mut BTreeSet<usize>) {
    let shown: Vec<usize> = present.union(asked).copied().filter(|&place| place < rarities.len()).collect();
    if shown.len() < 2 && asked.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        for place in shown {
            let on = asked.contains(&place);
            if chip(ui, &rarities[place], on).clicked() {
                match on {
                    true => asked.remove(&place),
                    false => asked.insert(place),
                };
            }
        }
        if !asked.is_empty() && ui.add(egui::Button::new(dim("Show all")).frame(false)).clicked() {
            asked.clear();
        }
    });
    ui.add_space(4.0);
}

fn chip(ui: &mut Ui, rarity: &Rarity, on: bool) -> Response {
    pill(ui, &rarity.name, Color32::from_rgb(rarity.color[0], rarity.color[1], rarity.color[2]), on)
}

pub fn pill(ui: &mut Ui, name: &str, color: Color32, on: bool) -> Response {
    let name = ui.painter().layout_no_wrap(name.to_string(), FontId::proportional(13.0), theme::text());
    let (rect, response) = ui.allocate_exact_size(vec2(name.size().x + 34.0, 26.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let (fill, line, ink) = if on {
            (color.gamma_multiply(0.3), color, theme::text())
        } else if response.hovered() {
            (theme::card_hover(), theme::line(), theme::text())
        } else {
            (theme::card(), theme::line(), theme::text_dim())
        };
        let painter = ui.painter();
        let round = CornerRadius::same(13);
        painter.rect_filled(rect, round, fill);
        painter.rect_stroke(rect, round, Stroke::new(1.0, line), StrokeKind::Inside);
        painter.circle_filled(pos2(rect.left() + 13.0, rect.center().y), 4.5, color);
        painter.galley_with_override_text_color(pos2(rect.left() + 24.0, rect.center().y - name.size().y / 2.0), name, ink);
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rarities() -> Vec<Rarity> {
        let rarity = |id: &str, color: [u8; 3]| Rarity { id: id.to_string(), name: id.to_string(), color };
        vec![rarity("common", [0xb0, 0xc3, 0xd9]), rarity("mythical", [0x88, 0x47, 0xff]), rarity("immortal", [0xe4, 0xae, 0x39])]
    }

    #[test]
    fn everything_is_shown_until_a_rarity_is_asked_for() {
        let mut asked = BTreeSet::new();
        assert!(wanted(&asked, Some(0)) && wanted(&asked, Some(2)) && wanted(&asked, None));

        asked.insert(2);
        assert!(wanted(&asked, Some(2)));
        assert!(!wanted(&asked, Some(1)));
        assert!(!wanted(&asked, None), "what has no rarity is not of the rarity asked for");

        asked.insert(1);
        assert!(wanted(&asked, Some(1)) && wanted(&asked, Some(2)) && !wanted(&asked, Some(0)), "several can be asked for");
    }

    #[test]
    fn a_rarity_has_the_colour_the_game_gives_it() {
        let rarities = rarities();
        assert_eq!(color(&rarities, Some(1)), Some(Color32::from_rgb(0x88, 0x47, 0xff)));
        assert_eq!(color(&rarities, None), None);
        assert_eq!(color(&rarities, Some(9)), None, "a place that is not in the list");
    }
}
