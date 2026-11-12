use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub const BACKGROUND: Color32 = Color32::from_rgb(0x12, 0x14, 0x18);
pub const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x23);
pub const CARD: Color32 = Color32::from_rgb(0x23, 0x27, 0x2f);
pub const CARD_HOVER: Color32 = Color32::from_rgb(0x2d, 0x32, 0x3c);
pub const CARD_PICKED: Color32 = Color32::from_rgb(0x3a, 0x2b, 0x27);
pub const LINE: Color32 = Color32::from_rgb(0x33, 0x39, 0x45);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xea, 0xee);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0xa3, 0xb2);
pub const ACCENT: Color32 = Color32::from_rgb(0xe5, 0x5a, 0x34);
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0xf6, 0x8a, 0x66);
pub const GOOD: Color32 = Color32::from_rgb(0x5c, 0xc2, 0x8a);
pub const WARN: Color32 = Color32::from_rgb(0xe6, 0xa8, 0x3c);

pub const ROUND: u8 = 8;

pub const BODY: f32 = 15.0;
pub const SMALL: f32 = 12.5;

pub fn apply(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(SMALL)),
            (TextStyle::Body, FontId::proportional(BODY)),
            (TextStyle::Button, FontId::proportional(BODY)),
            (TextStyle::Heading, FontId::proportional(22.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.scroll.bar_width = 10.0;

        let visuals = &mut style.visuals;
        visuals.override_text_color = Some(TEXT);
        visuals.panel_fill = PANEL;
        visuals.window_fill = PANEL;
        visuals.window_stroke = Stroke::new(1.0, LINE);
        visuals.extreme_bg_color = BACKGROUND;
        visuals.faint_bg_color = CARD;
        visuals.hyperlink_color = ACCENT_TEXT;
        visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
        visuals.selection.stroke = Stroke::new(1.0, TEXT);

        let widgets = &mut visuals.widgets;
        for widget in [&mut widgets.noninteractive, &mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
            widget.corner_radius = CornerRadius::same(ROUND);
        }
        widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        widgets.inactive.bg_fill = CARD;
        widgets.inactive.weak_bg_fill = CARD;
        widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        widgets.hovered.bg_fill = CARD_HOVER;
        widgets.hovered.weak_bg_fill = CARD_HOVER;
        widgets.hovered.bg_stroke = Stroke::new(1.0, TEXT_DIM);
        widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        widgets.active.bg_fill = LINE;
        widgets.active.weak_bg_fill = LINE;
        widgets.active.bg_stroke = Stroke::new(1.0, TEXT_DIM);
        widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
        widgets.open.bg_fill = CARD_HOVER;
        widgets.open.weak_bg_fill = CARD_HOVER;
    });
}
