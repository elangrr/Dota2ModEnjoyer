use std::fmt::Debug;
use std::hash::Hash;

use eframe::egui::text::LayoutJob;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

use crate::pictures::Pictures;
use crate::theme;

const PAD: f32 = 8.0;
const NAME_SIZE: f32 = 14.0;
const NAME_LINE: f32 = 18.0;
const NOTE_HEIGHT: f32 = 17.0;
const GAP: f32 = 10.0;
const SCROLL_BAR: f32 = 14.0;
const MOST_STRETCH: f32 = 1.3;
const BAND: f32 = 3.0;

pub const WHOLE: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1.0, 1.0) };

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub picture: Vec2,
    pub name_lines: usize,
    pub note: bool,
}

impl Look {
    pub fn size(self) -> Vec2 {
        let name = NAME_LINE * self.name_lines as f32;
        let note = if self.note { NOTE_HEIGHT } else { 0.0 };
        vec2(self.picture.x + 2.0 * PAD, PAD + self.picture.y + 6.0 + name + note + PAD)
    }

    fn stretched(self, by: f32) -> Look {
        Look { picture: self.picture * by, ..self }
    }

    fn to_fill(self, width: f32) -> (usize, Look) {
        let tile = self.size().x;
        let columns = (((width + GAP) / (tile + GAP)).floor() as usize).max(1);
        let room = (width + GAP) / columns as f32 - GAP;
        let stretch = ((room - 2.0 * PAD) / self.picture.x).clamp(1.0, MOST_STRETCH);
        (columns, self.stretched(stretch))
    }
}

pub struct Tile<'a> {
    pub picture: Option<&'a str>,
    pub name: &'a str,
    pub note: Option<&'a str>,
    pub badge: Option<&'a str>,
    pub band: Option<Color32>,
    pub picked: bool,
    pub faint: bool,
    pub warning: Option<&'a str>,
}

pub fn tile(ui: &mut Ui, pictures: &mut Pictures, look: Look, tile: &Tile<'_>) -> Response {
    let (rect, response) = ui.allocate_exact_size(look.size(), Sense::click());
    let mut name_cut_short = false;
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let fill = if tile.picked {
            theme::CARD_PICKED
        } else if response.hovered() {
            theme::CARD_HOVER
        } else {
            theme::CARD
        };
        painter.rect_filled(rect, CornerRadius::same(theme::ROUND), fill);

        let well = Rect::from_min_size(rect.min + vec2(PAD, PAD), look.picture);
        painter.rect_filled(well, CornerRadius::same(5), theme::BACKGROUND);
        if let Some((texture, size)) = tile.picture.and_then(|path| pictures.get(path)) {
            let tint = if tile.faint { Color32::from_gray(150) } else { Color32::WHITE };
            painter.image(texture, fit(size, well), WHOLE, tint);
        }
        if let Some(color) = tile.band {
            rarity_band(&painter, well, color, 5);
        }
        if tile.warning.is_some() {
            warning_mark(&painter, pos2(well.left() + 13.0, well.top() + 13.0));
        }
        if let Some(badge) = tile.badge {
            let text = painter.layout_no_wrap(badge.to_string(), FontId::proportional(theme::SMALL), theme::TEXT);
            let plate = Rect::from_min_size(pos2(well.right() - text.size().x - 14.0, well.top() + 4.0), text.size() + vec2(10.0, 4.0));
            painter.rect_filled(plate, CornerRadius::same(4), Color32::from_black_alpha(200));
            painter.galley(plate.min + vec2(5.0, 2.0), text, theme::TEXT);
        }

        let color = if tile.faint { theme::TEXT_DIM } else { theme::TEXT };
        let mut name = LayoutJob::simple(tile.name.to_string(), FontId::proportional(NAME_SIZE), color, look.picture.x);
        name.wrap.max_rows = look.name_lines;
        name.wrap.overflow_character = Some('…');
        name.halign = Align::Center;
        let name = painter.layout_job(name);
        name_cut_short = name.elided;
        let name_bottom = well.bottom() + 6.0 + name.size().y;
        painter.galley(pos2(rect.center().x, well.bottom() + 6.0), name, color);

        if let Some(note) = tile.note.filter(|_| look.note) {
            let place = pos2(rect.center().x, name_bottom + 2.0);
            painter.text(place, Align2::CENTER_TOP, note, FontId::proportional(theme::SMALL), theme::TEXT_DIM);
        }
        if tile.picked {
            painter.rect_stroke(rect, CornerRadius::same(theme::ROUND), Stroke::new(2.0, theme::ACCENT), StrokeKind::Inside);
        }
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    match (tile.warning, name_cut_short) {
        (Some(warning), true) => response.on_hover_text(format!("{}\n\n{warning}", tile.name)),
        (Some(warning), false) => response.on_hover_text(warning),
        (None, true) => response.on_hover_text(tile.name),
        (None, false) => response,
    }
}

pub fn warning_mark(painter: &egui::Painter, at: egui::Pos2) {
    painter.circle_filled(at, 8.0, theme::WARN);
    painter.text(at, Align2::CENTER_CENTER, "!", FontId::proportional(12.0), theme::BACKGROUND);
}

pub fn rarity_band(painter: &egui::Painter, picture: Rect, color: Color32, round: u8) {
    let band = Rect::from_min_max(pos2(picture.left(), picture.bottom() - BAND), picture.max);
    let glow = Rect::from_min_max(pos2(picture.left(), picture.bottom() - picture.height() * 0.3), band.right_top());
    let mut mesh = egui::Mesh::default();
    let (clear, tinted) = (Color32::TRANSPARENT, color.gamma_multiply(0.38));
    mesh.colored_vertex(glow.left_top(), clear);
    mesh.colored_vertex(glow.right_top(), clear);
    mesh.colored_vertex(glow.right_bottom(), tinted);
    mesh.colored_vertex(glow.left_bottom(), tinted);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_filled(band, CornerRadius { nw: 0, ne: 0, sw: round, se: round }, color);
}

pub fn fit(size: Vec2, room: Rect) -> Rect {
    if size.x <= 0.0 || size.y <= 0.0 {
        return room;
    }
    let scale = (room.width() / size.x).min(room.height() / size.y);
    Rect::from_center_size(room.center(), size * scale)
}

pub fn grid(ui: &mut Ui, id: impl Hash + Debug, count: usize, look: Look, mut draw: impl FnMut(&mut Ui, usize, Look)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        let (columns, look) = look.to_fill(ui.available_width() - SCROLL_BAR);
        let rows = count.div_ceil(columns);
        egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show_rows(ui, look.size().y, rows, |ui, visible| {
            for row in visible {
                ui.horizontal(|ui| {
                    for index in row * columns..((row + 1) * columns).min(count) {
                        draw(ui, index, look);
                    }
                });
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_keep_their_shape_and_sit_in_the_middle() {
        let room = Rect::from_min_size(pos2(10.0, 20.0), vec2(150.0, 100.0));
        assert_eq!(fit(vec2(256.0, 170.67), room).size().x.round(), 150.0);
        let wide = fit(vec2(256.0, 144.0), room);
        assert_eq!((wide.width(), wide.height().round()), (150.0, 84.0));
        assert_eq!(wide.center(), room.center());
        let tall = fit(vec2(112.0, 170.0), room);
        assert!((tall.height() - 100.0).abs() < 0.01);
        assert!((tall.width() - 65.9).abs() < 0.1);
        assert_eq!(fit(vec2(15.0, 10.0), room).size(), vec2(150.0, 100.0));
        assert_eq!(fit(vec2(0.0, 10.0), room), room);
    }

    #[test]
    fn a_tile_is_its_picture_plus_room_for_the_name() {
        let plain = Look { picture: vec2(150.0, 100.0), name_lines: 2, note: false };
        assert_eq!(plain.size(), vec2(166.0, 158.0));
        let noted = Look { note: true, ..plain };
        assert_eq!(noted.size().y - plain.size().y, NOTE_HEIGHT);
        let one_line = Look { name_lines: 1, ..plain };
        assert_eq!(plain.size().y - one_line.size().y, NAME_LINE);
    }

    #[test]
    fn tiles_grow_a_little_to_fill_the_width() {
        let look = Look { picture: vec2(150.0, 100.0), name_lines: 2, note: false };
        assert_eq!(look.to_fill(694.0), (4, look));

        let (columns, wider) = look.to_fill(800.0);
        assert_eq!(columns, 4);
        let filled = columns as f32 * wider.size().x + (columns - 1) as f32 * GAP;
        assert!((filled - 800.0).abs() < 0.01, "{filled}");
        assert!((wider.picture.y / wider.picture.x - 100.0 / 150.0).abs() < 0.001, "the picture keeps its shape");

        assert_eq!(look.to_fill(100.0), (1, look));
        let (columns, widest) = look.to_fill(260.0);
        assert_eq!(columns, 1);
        assert_eq!(widest.picture.x, 150.0 * MOST_STRETCH);
    }
}
