use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, RichText, Stroke, StrokeKind, TextStyle, pos2, vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Night,
    Day,
    System,
}

impl Theme {
    pub fn is_night(&self) -> bool {
        *self == Theme::Night
    }

    fn is_day(self, system: Option<egui::Theme>) -> bool {
        match self {
            Theme::Night => false,
            Theme::Day => true,
            Theme::System => system == Some(egui::Theme::Light),
        }
    }
}

struct Palette {
    background: Color32,
    panel: Color32,
    card: Color32,
    card_hover: Color32,
    card_picked: Color32,
    line: Color32,
    text: Color32,
    text_dim: Color32,
    accent_text: Color32,
    good: Color32,
    warn: Color32,
}

const NIGHT: Palette = Palette {
    background: Color32::from_rgb(0x0e, 0x10, 0x14),
    panel: Color32::from_rgb(0x1a, 0x1d, 0x23),
    card: Color32::from_rgb(0x1f, 0x23, 0x2b),
    card_hover: Color32::from_rgb(0x2a, 0x2f, 0x39),
    card_picked: Color32::from_rgb(0x3a, 0x2b, 0x27),
    line: Color32::from_rgb(0x33, 0x39, 0x45),
    text: Color32::from_rgb(0xe8, 0xea, 0xee),
    text_dim: Color32::from_rgb(0x9a, 0xa3, 0xb2),
    accent_text: Color32::from_rgb(0xf6, 0x8a, 0x66),
    good: Color32::from_rgb(0x5c, 0xc2, 0x8a),
    warn: Color32::from_rgb(0xe6, 0xa8, 0x3c),
};

const DAY: Palette = Palette {
    background: Color32::from_rgb(0xec, 0xee, 0xf2),
    panel: Color32::from_rgb(0xfb, 0xfb, 0xfd),
    card: Color32::from_rgb(0xff, 0xff, 0xff),
    card_hover: Color32::from_rgb(0xf3, 0xf4, 0xf8),
    card_picked: Color32::from_rgb(0xfd, 0xe9, 0xe2),
    line: Color32::from_rgb(0xd3, 0xd8, 0xe1),
    text: Color32::from_rgb(0x1b, 0x20, 0x30),
    text_dim: Color32::from_rgb(0x5b, 0x64, 0x75),
    accent_text: Color32::from_rgb(0xc8, 0x42, 0x1c),
    good: Color32::from_rgb(0x17, 0x80, 0x4c),
    warn: Color32::from_rgb(0xa5, 0x65, 0x08),
};

static IS_DAY: AtomicBool = AtomicBool::new(false);

pub fn is_day() -> bool {
    IS_DAY.load(Ordering::Relaxed)
}

fn palette() -> &'static Palette {
    if is_day() { &DAY } else { &NIGHT }
}

pub fn background() -> Color32 {
    palette().background
}
pub fn panel() -> Color32 {
    palette().panel
}
pub fn card() -> Color32 {
    palette().card
}
pub fn card_hover() -> Color32 {
    palette().card_hover
}
pub fn card_picked() -> Color32 {
    palette().card_picked
}
pub fn line() -> Color32 {
    palette().line
}
pub fn text() -> Color32 {
    palette().text
}
pub fn text_dim() -> Color32 {
    palette().text_dim
}
pub fn accent_text() -> Color32 {
    palette().accent_text
}
pub fn good() -> Color32 {
    palette().good
}
pub fn warn() -> Color32 {
    palette().warn
}

pub const fn accent() -> Color32 {
    Color32::from_rgb(0xe5, 0x5a, 0x34)
}

pub const fn well() -> Color32 {
    Color32::from_rgb(0x12, 0x14, 0x18)
}

pub const ROUND: u8 = 10;
pub const GLASS_ROUND: u8 = 16;

pub const BODY: f32 = 15.0;
pub const SMALL: f32 = 12.5;

const HEAVY: &str = "heavy";

pub fn title(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(heavy(size)).color(self::text())
}

pub fn heavy(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HEAVY.into()))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.ttf")).into());
    fonts.font_data.insert("inter-heavy".into(), FontData::from_static(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf")).into());
    let behind = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    fonts.families.insert(FontFamily::Name(HEAVY.into()), std::iter::once("inter-heavy".to_string()).chain(behind).collect());
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, theme: Theme) {
    let day = theme.is_day(ctx.system_theme());
    IS_DAY.store(day, Ordering::Relaxed);
    ctx.set_theme(if day { egui::Theme::Light } else { egui::Theme::Dark });
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(SMALL)),
            (TextStyle::Body, FontId::proportional(BODY)),
            (TextStyle::Button, FontId::proportional(BODY)),
            (TextStyle::Heading, heavy(22.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.scroll.bar_width = 10.0;

        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(text());
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = panel();
        visuals.window_stroke = Stroke::new(1.0, line());
        visuals.extreme_bg_color = if day { Color32::WHITE } else { background() };
        visuals.faint_bg_color = card();
        visuals.hyperlink_color = accent_text();
        visuals.selection.bg_fill = accent().gamma_multiply(if day { 0.25 } else { 0.45 });
        visuals.selection.stroke = Stroke::new(1.0, text());

        let widgets = &mut visuals.widgets;
        for widget in [&mut widgets.noninteractive, &mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
            widget.corner_radius = CornerRadius::same(ROUND);
        }
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, line());
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, text());
        widgets.inactive.bg_fill = card();
        widgets.inactive.weak_bg_fill = card();
        widgets.inactive.bg_stroke = Stroke::new(1.0, line());
        widgets.inactive.fg_stroke = Stroke::new(1.0, text());
        widgets.hovered.bg_fill = card_hover();
        widgets.hovered.weak_bg_fill = card_hover();
        widgets.hovered.bg_stroke = Stroke::new(1.0, text_dim());
        widgets.hovered.fg_stroke = Stroke::new(1.0, text());
        widgets.active.bg_fill = line();
        widgets.active.weak_bg_fill = line();
        widgets.active.bg_stroke = Stroke::new(1.0, text_dim());
        widgets.active.fg_stroke = Stroke::new(1.0, text());
        widgets.open.bg_fill = card_hover();
        widgets.open.weak_bg_fill = card_hover();
    });
}

pub fn paint_backdrop(painter: &egui::Painter, room: Rect) {
    painter.rect_filled(room, CornerRadius::ZERO, background());
    let reach = room.width().max(room.height());
    let (warm, cool) = if is_day() { (0.13, 0.12) } else { (0.20, 0.09) };
    glow(painter, room.left_top() + vec2(room.width() * 0.10, room.height() * 0.06), reach * 0.42, accent().gamma_multiply(warm));
    glow(painter, room.right_bottom() - vec2(room.width() * 0.06, room.height() * 0.02), reach * 0.46, Color32::from_rgb(0x78, 0x8c, 0xbe).gamma_multiply(cool));
}

fn glow(painter: &egui::Painter, middle: Pos2, reach: f32, color: Color32) {
    const SPOKES: u32 = 48;
    const RINGS: u32 = 6;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(middle, color);
    for ring in 1..=RINGS {
        let part = ring as f32 / RINGS as f32;
        let faded = color.gamma_multiply((1.0 - part).powf(1.6));
        for spoke in 0..SPOKES {
            let angle = spoke as f32 / SPOKES as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(pos2(middle.x + angle.cos() * reach * part, middle.y + angle.sin() * reach * part), faded);
        }
    }
    for spoke in 0..SPOKES {
        let next = (spoke + 1) % SPOKES;
        mesh.add_triangle(0, 1 + spoke, 1 + next);
        for ring in 1..RINGS {
            let (inner, outer) = (1 + (ring - 1) * SPOKES, 1 + ring * SPOKES);
            mesh.add_triangle(inner + spoke, outer + spoke, outer + next);
            mesh.add_triangle(inner + spoke, outer + next, inner + next);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

pub fn paint_glass(painter: &egui::Painter, rect: Rect, round: u8) {
    let corners = CornerRadius::same(round);
    let day = is_day();
    for step in 1..=4_u8 {
        let grown = rect.expand(f32::from(step) * 2.0).translate(vec2(0.0, f32::from(step) * 1.5));
        let shade = Color32::from_black_alpha(if day { 7 } else { 16 } / step);
        painter.rect_filled(grown, CornerRadius::same(round.saturating_add(step * 2)), shade);
    }
    let fill = if day { Color32::from_white_alpha(170) } else { Color32::from_white_alpha(13) };
    painter.rect_filled(rect, corners, fill);
    let edge = if day { Color32::from_white_alpha(235) } else { Color32::from_white_alpha(28) };
    painter.rect_stroke(rect, corners, Stroke::new(1.0, edge), StrokeKind::Inside);
    let inset = f32::from(round) * 0.8;
    let light = if day { Color32::WHITE } else { Color32::from_white_alpha(46) };
    painter.line_segment([pos2(rect.left() + inset, rect.top() + 0.5), pos2(rect.right() - inset, rect.top() + 0.5)], Stroke::new(1.0, light));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_is_the_one_chosen_or_the_one_windows_has() {
        assert!(!Theme::Night.is_day(Some(egui::Theme::Light)));
        assert!(Theme::Day.is_day(Some(egui::Theme::Dark)));
        assert!(Theme::System.is_day(Some(egui::Theme::Light)));
        assert!(!Theme::System.is_day(Some(egui::Theme::Dark)));
        assert!(!Theme::System.is_day(None), "night where Windows does not say");
        assert_eq!(Theme::default(), Theme::Night);
        assert_eq!(serde_json::to_string(&Theme::System).unwrap(), "\"system\"");
    }

    #[test]
    fn text_is_readable_on_what_is_behind_it_by_night_and_by_day() {
        let light = |color: Color32| {
            let part = |value: u8| {
                let value = f32::from(value) / 255.0;
                if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * part(color.r()) + 0.7152 * part(color.g()) + 0.0722 * part(color.b())
        };
        let contrast = |a: Color32, b: Color32| {
            let (a, b) = (light(a), light(b));
            (a.max(b) + 0.05) / (a.min(b) + 0.05)
        };
        for look in [&NIGHT, &DAY] {
            for behind in [look.background, look.panel, look.card, look.card_picked] {
                assert!(contrast(look.text, behind) >= 7.0);
                assert!(contrast(look.text_dim, behind) >= 4.5);
                assert!(contrast(look.accent_text, behind) >= 4.0);
            }
            assert!(contrast(look.warn, look.card) >= 3.5 && contrast(look.good, look.card) >= 3.5);
        }
    }
}
