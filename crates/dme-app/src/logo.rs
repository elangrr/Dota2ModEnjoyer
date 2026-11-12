use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, pos2};

const UNITS: f32 = 256.0;
const ROUND: f32 = 56.0;
const BACK: [[u8; 3]; 2] = [[0x26, 0x2b, 0x33], [0x14, 0x16, 0x1a]];

struct Facet {
    corners: &'static [[f32; 2]],
    color: [u8; 3],
}

const FACETS: [Facet; 4] = [
    Facet { corners: &[[128.0, 34.0], [208.0, 81.0], [168.0, 128.0], [106.0, 92.0], [48.0, 81.0]], color: [0xf9, 0xa1, 0x83] },
    Facet { corners: &[[48.0, 81.0], [106.0, 92.0], [106.0, 164.0], [48.0, 175.0]], color: [0xe5, 0x5a, 0x34] },
    Facet { corners: &[[208.0, 81.0], [208.0, 175.0], [128.0, 222.0], [48.0, 175.0], [106.0, 164.0], [168.0, 128.0]], color: [0xb8, 0x43, 0x1f] },
    Facet { corners: &[[106.0, 92.0], [168.0, 128.0], [106.0, 164.0]], color: [0x14, 0x16, 0x1a] },
];
const PLAY_MARK: usize = 3;
const DARK: usize = 2;
const DARK_IN_PIECES: [&[[f32; 2]]; 3] = [
    &[[208.0, 81.0], [208.0, 175.0], [128.0, 222.0], [168.0, 128.0]],
    &[[168.0, 128.0], [128.0, 222.0], [106.0, 164.0]],
    &[[106.0, 164.0], [128.0, 222.0], [48.0, 175.0]],
];

fn inside(corners: &[[f32; 2]], x: f32, y: f32) -> bool {
    let mut crossings = 0;
    for (place, &[ax, ay]) in corners.iter().enumerate() {
        let [bx, by] = corners[(place + 1) % corners.len()];
        if (ay > y) != (by > y) && x < ax + (y - ay) / (by - ay) * (bx - ax) {
            crossings += 1;
        }
    }
    crossings % 2 == 1
}

fn at(x: f32, y: f32) -> Option<[u8; 3]> {
    let nearest = |along: f32| along.clamp(ROUND, UNITS - ROUND);
    let (dx, dy) = (x - nearest(x), y - nearest(y));
    if dx * dx + dy * dy > ROUND * ROUND {
        return None;
    }
    let facet = FACETS.iter().rev().find(|facet| inside(facet.corners, x, y));
    let back = |channel: usize| (f32::from(BACK[0][channel]) + (f32::from(BACK[1][channel]) - f32::from(BACK[0][channel])) * y / UNITS).round() as u8;
    Some(facet.map_or([back(0), back(1), back(2)], |facet| facet.color))
}

pub fn icon() -> egui::IconData {
    const SIZE: u32 = 64;
    const FINER: u32 = 4;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (mut sum, mut covered) = ([0.0_f32; 3], 0.0_f32);
            for point in 0..FINER * FINER {
                let place = |pixel: u32, part: u32| (pixel as f32 + (part as f32 + 0.5) / FINER as f32) * UNITS / SIZE as f32;
                if let Some(color) = at(place(x, point % FINER), place(y, point / FINER)) {
                    covered += 1.0;
                    for (total, part) in sum.iter_mut().zip(color) {
                        *total += f32::from(part);
                    }
                }
            }
            let mean = |total: f32| if covered > 0.0 { (total / covered).round() as u8 } else { 0 };
            rgba.extend([mean(sum[0]), mean(sum[1]), mean(sum[2]), (covered / (FINER * FINER) as f32 * 255.0).round() as u8]);
        }
    }
    egui::IconData { rgba, width: SIZE, height: SIZE }
}

pub fn paint(painter: &egui::Painter, room: Rect) {
    let (left, top, side) = (48.0, 34.0, 188.0);
    let scale = room.width().min(room.height()) / side;
    let place = |[x, y]: [f32; 2]| -> Pos2 { pos2(room.center().x + (x - left - 80.0) * scale, room.center().y + (y - top - 94.0) * scale) };
    for (number, facet) in FACETS[..PLAY_MARK].iter().enumerate() {
        let [red, green, blue] = facet.color;
        let color = Color32::from_rgb(red, green, blue);
        let pieces: &[&[[f32; 2]]] = if number == DARK { &DARK_IN_PIECES } else { &[facet.corners] };
        for piece in pieces {
            painter.add(Shape::convex_polygon(piece.iter().copied().map(place).collect(), color, Stroke::NONE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_the_gem_on_a_rounded_square() {
        let icon = icon();
        assert_eq!((icon.width, icon.height, icon.rgba.len()), (64, 64, 64 * 64 * 4));
        let pixel = |x: usize, y: usize| <[u8; 4]>::try_from(&icon.rgba[(y * 64 + x) * 4..][..4]).unwrap();
        assert_eq!(pixel(0, 0)[3], 0, "the corners are see-through");
        assert_eq!(pixel(32, 2)[3], 255, "the square");
        assert_eq!(pixel(32, 14), [0xf9, 0xa1, 0x83, 255], "the lit face");
        assert_eq!(pixel(16, 32), [0xe5, 0x5a, 0x34, 255], "the face in the shade");
        assert_eq!(pixel(40, 46), [0xb8, 0x43, 0x1f, 0xff], "the dark face");
        assert_eq!(pixel(31, 32), [0x14, 0x16, 0x1a, 255], "the play mark");
    }

    #[test]
    fn the_numbers_here_are_those_of_the_drawing() {
        let svg = include_str!("../../../assets/logo.svg");
        let polygons: Vec<&str> = svg.lines().filter(|line| line.contains("<polygon")).collect();
        assert_eq!(polygons.len(), FACETS.len());
        for (line, facet) in polygons.iter().zip(&FACETS) {
            let points: Vec<String> = facet.corners.iter().map(|[x, y]| format!("{x},{y}")).collect();
            let [red, green, blue] = facet.color;
            assert!(line.contains(&format!("points=\"{}\"", points.join(" "))), "{line}");
            assert!(line.contains(&format!("fill=\"#{red:02x}{green:02x}{blue:02x}\"")), "{line}");
        }
        assert!(svg.contains(&format!("rx=\"{ROUND}\"")));
        for [red, green, blue] in BACK {
            assert!(svg.contains(&format!("stop-color=\"#{red:02x}{green:02x}{blue:02x}\"")));
        }
    }

    #[test]
    fn the_pieces_of_the_dark_face_make_up_all_of_it_and_no_more() {
        let area = |corners: &[[f32; 2]]| -> f32 {
            let around = corners.iter().zip(corners.iter().cycle().skip(1));
            around.map(|([ax, ay], [bx, by])| ax * by - bx * ay).sum()
        };
        let pieces: f32 = DARK_IN_PIECES.iter().map(|piece| area(piece)).sum();
        assert_eq!(pieces, area(FACETS[DARK].corners));
        for piece in DARK_IN_PIECES {
            assert!(area(piece) > 0.0, "each goes around the way the face does");
            let count = piece.len() as f32;
            let (x, y) = piece.iter().fold((0.0, 0.0), |(x, y), corner| (x + corner[0] / count, y + corner[1] / count));
            assert!(inside(FACETS[DARK].corners, x, y));
        }
    }

    #[test]
    fn a_point_is_told_to_be_inside_a_face_or_not() {
        let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(inside(&square, 5.0, 5.0));
        assert!(!inside(&square, 15.0, 5.0) && !inside(&square, 5.0, -1.0));
        assert!(inside(FACETS[2].corners, 190.0, 150.0) && !inside(FACETS[2].corners, 120.0, 128.0));
    }
}
