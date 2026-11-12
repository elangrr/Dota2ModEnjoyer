use eframe::egui::{self, Align2, Color32, Pos2, Rect, Shape, Stroke, pos2};

use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Heroes,
    World,
    Picks,
    Support,
    Settings,
    Help,
    Update,
}

const UNITS: f32 = 24.0;

pub fn paint(painter: &egui::Painter, icon: Icon, room: Rect, color: Color32) {
    let side = room.width().min(room.height());
    let scale = side / UNITS;
    let origin = room.center() - egui::vec2(side, side) / 2.0;
    let at = |x: f32, y: f32| -> Pos2 { pos2(origin.x + x * scale, origin.y + y * scale) };
    let stroke = Stroke::new((1.8 * scale).max(1.2), color);
    let line = |points: &[[f32; 2]]| Shape::line(points.iter().map(|[x, y]| at(*x, *y)).collect(), stroke);
    let closed = |points: Vec<[f32; 2]>| Shape::closed_line(points.iter().map(|[x, y]| at(*x, *y)).collect(), stroke);
    let ring = |x: f32, y: f32, radius: f32| Shape::circle_stroke(at(x, y), radius * scale, stroke);
    let around = |x: f32, y: f32, steps: usize, radius: &dyn Fn(usize) -> f32, from: f32, sweep: f32| -> Vec<[f32; 2]> {
        (0..=steps)
            .map(|step| {
                let angle = from + sweep * step as f32 / steps as f32;
                [x + angle.cos() * radius(step), y + angle.sin() * radius(step)]
            })
            .collect()
    };
    let half = std::f32::consts::PI;

    match icon {
        Icon::Heroes => {
            let mut helmet = around(12.0, 12.0, 14, &|_| 7.0, half, half);
            helmet.extend([[19.0, 20.0], [14.5, 20.0], [14.5, 15.0], [9.5, 15.0], [9.5, 20.0], [5.0, 20.0]]);
            painter.add(closed(helmet));
            painter.add(line(&[[12.0, 5.0], [13.0, 3.4], [15.0, 2.6], [17.0, 3.0], [15.4, 4.0], [14.4, 5.4]]));
        }
        Icon::World => {
            painter.add(closed(vec![[3.0, 19.0], [8.5, 10.0], [12.0, 15.0], [14.5, 11.5], [21.0, 19.0]]));
            painter.add(ring(16.5, 6.5, 2.0));
        }
        Icon::Picks => {
            for row in [6.0, 12.0, 18.0] {
                painter.add(line(&[[9.5, row], [20.0, row]]));
                painter.add(line(&[[3.5, row], [4.8, row + 1.3], [7.0, row - 1.1]]));
            }
        }
        Icon::Support => {
            let (lobe, dip) = (4.3, 0.56);
            let mut heart = around(8.4, 9.2, 12, &|_| lobe, 0.78 * half, 1.04 * half);
            heart.extend(around(15.6, 9.2, 12, &|_| lobe, (1.0 + dip / 3.2) * half, 1.04 * half));
            heart.push([12.0, 20.2]);
            painter.add(closed(heart));
        }
        Icon::Settings => {
            let teeth = around(12.0, 12.0, 31, &|step| if step % 4 < 2 { 9.4 } else { 7.2 }, -0.2, 2.0 * half * 31.0 / 32.0);
            painter.add(closed(teeth));
            painter.add(ring(12.0, 12.0, 3.0));
        }
        Icon::Help => {
            painter.add(ring(12.0, 12.0, 9.0));
            painter.text(at(12.0, 12.4), Align2::CENTER_CENTER, "?", theme::heavy(13.5 * scale), color);
        }
        Icon::Update => {
            painter.add(ring(12.0, 12.0, 9.0));
            painter.add(line(&[[12.0, 16.5], [12.0, 8.0]]));
            painter.add(line(&[[8.6, 11.3], [12.0, 7.7], [15.4, 11.3]]));
        }
    }
}
