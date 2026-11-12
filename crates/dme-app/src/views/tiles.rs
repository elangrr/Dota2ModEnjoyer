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
    pub name_over: bool,
}

impl Look {
    pub fn size(self) -> Vec2 {
        if self.name_over {
            return self.picture;
        }
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
        let around = if self.name_over { 0.0 } else { 2.0 * PAD };
        let stretch = ((room - around) / self.picture.x).clamp(1.0, MOST_STRETCH);
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
    pub detail: Option<&'a Detail<'a>>,
    pub mark: Option<Color32>,
}

#[derive(Default)]
pub struct Detail<'a> {
    pub rarity: Option<(&'a str, Color32)>,
    pub looks: Vec<(&'a str, Option<&'a str>)>,
    pub facts: Vec<String>,
}

const LARGER: f32 = 2.0;
const STYLE_PICTURE: Vec2 = vec2(96.0, 64.0);

pub fn tile(ui: &mut Ui, pictures: &mut Pictures, look: Look, tile: &Tile<'_>) -> Response {
    if look.name_over {
        return card(ui, pictures, look, tile);
    }
    let (rect, response) = ui.allocate_exact_size(look.size(), Sense::click());
    let mut name_cut_short = false;
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered(), 0.12);
        let fill = if tile.picked { theme::card_picked() } else { theme::card().lerp_to_gamma(theme::card_hover(), lit) };
        painter.rect_filled(rect, CornerRadius::same(theme::ROUND), fill);

        let well = Rect::from_min_size(rect.min + vec2(PAD, PAD), look.picture);
        painter.rect_filled(well, CornerRadius::same(5), theme::well());
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
        if let Some(color) = tile.mark {
            let at = pos2(well.left() + 11.0, well.top() + 11.0);
            painter.circle_filled(at, 6.5, Color32::from_black_alpha(190));
            painter.circle_filled(at, 4.5, color);
        }
        if let Some(badge) = tile.badge {
            let text = painter.layout_no_wrap(badge.to_string(), FontId::proportional(theme::SMALL), theme::text());
            let plate = Rect::from_min_size(pos2(well.right() - text.size().x - 14.0, well.top() + 4.0), text.size() + vec2(10.0, 4.0));
            painter.rect_filled(plate, CornerRadius::same(4), Color32::from_black_alpha(200));
            painter.galley(plate.min + vec2(5.0, 2.0), text, theme::text());
        }

        let color = if tile.faint { theme::text_dim() } else { theme::text() };
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
            painter.text(place, Align2::CENTER_TOP, note, FontId::proportional(theme::SMALL), theme::text_dim());
        }
        if tile.picked {
            painter.rect_stroke(rect, CornerRadius::same(theme::ROUND), Stroke::new(2.0, theme::accent()), StrokeKind::Inside);
        }
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if let Some(detail) = tile.detail {
        return response.on_hover_ui(|ui| larger_look(ui, pictures, look, tile, detail));
    }
    match (tile.warning, name_cut_short) {
        (Some(warning), true) => response.on_hover_text(format!("{}\n\n{warning}", tile.name)),
        (Some(warning), false) => response.on_hover_text(warning),
        (None, true) => response.on_hover_text(tile.name),
        (None, false) => response,
    }
}

pub fn fill_with(painter: &egui::Painter, pictures: &mut Pictures, picture: Option<&str>, room: Rect, round: u8, closer: f32) {
    let corners = CornerRadius::same(round);
    painter.rect_filled(room, corners, theme::well());
    if let Some((texture, size)) = picture.and_then(|path| pictures.get(path)) {
        let (wide, high) = (room.width() / size.x.max(1.0), room.height() / size.y.max(1.0));
        let fill = wide.max(high) * closer;
        let part = Rect::from_center_size(pos2(0.5, 0.5), vec2(wide / fill, high / fill));
        painter.add(egui::epaint::RectShape::filled(room, corners, Color32::WHITE).with_texture(texture, part));
    }
}

fn card(ui: &mut Ui, pictures: &mut Pictures, look: Look, tile: &Tile<'_>) -> Response {
    const ROUND: u8 = 12;
    let (rect, response) = ui.allocate_exact_size(look.size(), Sense::click());
    if ui.is_rect_visible(rect) {
        let lit = ui.ctx().animate_bool_with_time(response.id.with("lit"), response.hovered(), 0.14);
        let painter = ui.painter_at(rect.expand(2.0));
        let corners = CornerRadius::same(ROUND);
        fill_with(&painter, pictures, tile.picture, rect, ROUND, 1.0 + 0.05 * lit);
        let dark = Color32::from_black_alpha(215);
        let top = rect.bottom() - rect.height() * 0.55;
        let flat = rect.bottom() - f32::from(ROUND);
        let mut shade = egui::Mesh::default();
        shade.colored_vertex(pos2(rect.left(), top), Color32::TRANSPARENT);
        shade.colored_vertex(pos2(rect.right(), top), Color32::TRANSPARENT);
        shade.colored_vertex(pos2(rect.right(), flat), dark);
        shade.colored_vertex(pos2(rect.left(), flat), dark);
        shade.add_triangle(0, 1, 2);
        shade.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(shade));
        painter.rect_filled(Rect::from_min_max(pos2(rect.left(), flat), rect.max), CornerRadius { nw: 0, ne: 0, sw: ROUND, se: ROUND }, dark);

        let mut name = LayoutJob::simple(tile.name.to_string(), theme::heavy(14.0), Color32::WHITE, rect.width() - 20.0);
        name.wrap.max_rows = 1;
        name.wrap.overflow_character = Some('…');
        let name = painter.layout_job(name);
        painter.galley(pos2(rect.left() + 10.0, rect.bottom() - 8.0 - name.size().y), name, Color32::WHITE);

        if let Some(color) = tile.mark {
            let at = pos2(rect.left() + 13.0, rect.top() + 13.0);
            painter.circle_filled(at, 6.5, Color32::from_black_alpha(170));
            painter.circle_filled(at, 4.5, color);
        }
        if let Some(badge) = tile.badge {
            let text = painter.layout_no_wrap(badge.to_string(), theme::heavy(11.0), Color32::WHITE);
            let plate = Rect::from_min_size(pos2(rect.right() - text.size().x - 22.0, rect.top() + 8.0), text.size() + vec2(14.0, 6.0));
            painter.rect_filled(plate, CornerRadius::same(10), theme::accent());
            painter.galley(plate.min + vec2(7.0, 3.0), text, Color32::WHITE);
        }
        if tile.picked {
            painter.rect_stroke(rect, corners, Stroke::new(2.0, theme::accent()), StrokeKind::Inside);
        } else {
            painter.rect_stroke(rect, corners, Stroke::new(1.0, Color32::from_white_alpha((26.0 + 60.0 * lit) as u8)), StrokeKind::Inside);
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn larger_look(ui: &mut Ui, pictures: &mut Pictures, look: Look, tile: &Tile<'_>, detail: &Detail<'_>) {
    let room = look.picture * LARGER;
    ui.set_max_width(room.x.max(STYLE_PICTURE.x * 3.0 + 16.0));
    let (well, _) = ui.allocate_exact_size(room, Sense::hover());
    ui.painter().rect_filled(well, CornerRadius::same(6), theme::well());
    if let Some((texture, size)) = tile.picture.and_then(|path| pictures.get(path)) {
        ui.painter().image(texture, fit(size, well), WHOLE, Color32::WHITE);
    }
    if let Some(color) = tile.band {
        rarity_band(ui.painter(), well, color, 6);
    }
    ui.add_space(4.0);
    ui.label(crate::theme::title(tile.name, 16.0));
    if let Some((rarity, color)) = detail.rarity {
        ui.label(egui::RichText::new(rarity).size(theme::SMALL).color(color));
    }
    for fact in &detail.facts {
        ui.label(egui::RichText::new(fact).size(theme::SMALL).color(theme::text_dim()));
    }
    if detail.looks.len() > 1 {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            for (name, picture) in &detail.looks {
                ui.vertical(|ui| {
                    ui.set_width(STYLE_PICTURE.x);
                    let (well, _) = ui.allocate_exact_size(STYLE_PICTURE, Sense::hover());
                    ui.painter().rect_filled(well, CornerRadius::same(4), theme::well());
                    if let Some((texture, size)) = picture.and_then(|path| pictures.get(path)) {
                        ui.painter().image(texture, fit(size, well), WHOLE, Color32::WHITE);
                    }
                    ui.label(egui::RichText::new(*name).size(theme::SMALL).color(theme::text_dim()));
                });
            }
        });
    }
    if let Some(warning) = tile.warning {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(warning).size(theme::SMALL).color(theme::warn()));
    }
}

pub fn flow(ui: &mut Ui, count: usize, look: Look, mut draw: impl FnMut(&mut Ui, usize, Look)) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = vec2(GAP, GAP);
        let (columns, look) = look.to_fill(ui.available_width() - SCROLL_BAR);
        for row in 0..count.div_ceil(columns) {
            ui.horizontal(|ui| {
                for index in row * columns..((row + 1) * columns).min(count) {
                    draw(ui, index, look);
                }
            });
        }
    });
}

pub fn warning_mark(painter: &egui::Painter, at: egui::Pos2) {
    painter.circle_filled(at, 8.0, theme::warn());
    painter.text(at, Align2::CENTER_CENTER, "!", FontId::proportional(12.0), theme::well());
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
        let plain = Look { picture: vec2(150.0, 100.0), name_lines: 2, note: false, name_over: false };
        assert_eq!(plain.size(), vec2(166.0, 158.0));
        let noted = Look { note: true, ..plain };
        assert_eq!(noted.size().y - plain.size().y, NOTE_HEIGHT);
        let one_line = Look { name_lines: 1, ..plain };
        assert_eq!(plain.size().y - one_line.size().y, NAME_LINE);
    }

    #[test]
    fn tiles_grow_a_little_to_fill_the_width() {
        let look = Look { picture: vec2(150.0, 100.0), name_lines: 2, note: false, name_over: false };
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
