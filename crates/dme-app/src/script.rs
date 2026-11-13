use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use dme_core::pictures::Picture;
use eframe::egui::{self, Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase, vec2};

const STEP_QUIET: f64 = 0.3;
const SHOT_QUIET: f64 = 0.6;
const LATEST: f64 = 120.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Click(Pos2),
    RightClick(Pos2),
    Rest(Pos2),
    Type(String),
    Scroll(Pos2, f32),
    Wait(f64),
}

impl Step {
    fn input(&self) -> Vec<Vec<Event>> {
        match self {
            Step::Click(at) | Step::RightClick(at) => {
                let which = if matches!(self, Step::RightClick(_)) { PointerButton::Secondary } else { PointerButton::Primary };
                let button = |pressed| Event::PointerButton { pos: *at, button: which, pressed, modifiers: Modifiers::NONE };
                vec![vec![Event::PointerMoved(*at)], vec![button(true)], vec![button(false)]]
            }
            Step::Rest(at) => vec![vec![Event::PointerMoved(*at)]],
            Step::Type(text) => vec![vec![Event::Text(text.clone())]],
            Step::Scroll(at, down) => {
                let wheel = Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: vec2(0.0, -down),
                    phase: TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: Modifiers::NONE,
                };
                vec![vec![Event::PointerMoved(*at)], vec![wheel]]
            }
            Step::Wait(_) => Vec::new(),
        }
    }

    fn brings_the_pointer(&self) -> bool {
        matches!(self, Step::Click(_) | Step::RightClick(_) | Step::Scroll(..))
    }
}

pub struct Script {
    steps: VecDeque<Step>,
    shot: PathBuf,
    input: VecDeque<Vec<Event>>,
    pointer_in: bool,
    quiet_since: Option<f64>,
    waiting_until: Option<f64>,
    asked: bool,
    done: Arc<AtomicBool>,
}

impl Script {
    pub fn new(steps: Vec<Step>, shot: PathBuf) -> Script {
        Script {
            steps: steps.into(),
            shot,
            input: VecDeque::new(),
            pointer_in: false,
            quiet_since: None,
            waiting_until: None,
            asked: false,
            done: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn feed(&mut self, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.extend(self.input.pop_front().unwrap_or_default());
    }

    pub fn advance(&mut self, ctx: &egui::Context, settled: bool) {
        ctx.request_repaint();
        if self.done.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let now = ctx.input(|input| input.time);
        let late = now > LATEST;
        if self.waiting_until.is_some_and(|until| now < until) && !late {
            self.quiet_since = None;
            return;
        }
        self.waiting_until = None;
        if !settled || !self.input.is_empty() {
            self.quiet_since = None;
        }
        let quiet_for = now - *self.quiet_since.get_or_insert(now);

        if quiet_for > STEP_QUIET && !late {
            if std::mem::take(&mut self.pointer_in) {
                self.input.push_back(vec![Event::PointerGone]);
                self.quiet_since = None;
                return;
            }
            if let Some(step) = self.steps.pop_front() {
                if let Step::Wait(seconds) = step {
                    self.waiting_until = Some(now + seconds);
                }
                self.pointer_in = step.brings_the_pointer();
                self.input.extend(step.input());
                self.quiet_since = None;
                return;
            }
        }
        if !self.asked && (late || quiet_for > SHOT_QUIET) {
            self.asked = true;
            let (path, done) = (self.shot.clone(), Arc::clone(&self.done));
            ctx.request_screenshot(move |image| {
                let rgba = image.pixels.iter().flat_map(|pixel| pixel.to_array()).collect();
                let picture = Picture { width: image.width() as u32, height: image.height() as u32, rgba };
                let _ = std::fs::write(&path, picture.to_png());
                done.store(true, Ordering::Relaxed);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::pos2;

    use super::*;

    #[test]
    fn a_click_is_the_pointer_arriving_then_the_button_going_down_and_up() {
        let at = pos2(40.0, 50.0);
        let frames = Step::Click(at).input();
        assert_eq!(frames.len(), 3, "each in a frame of its own, as from a real mouse");
        assert_eq!(frames[0], [Event::PointerMoved(at)]);
        assert!(matches!(frames[1][..], [Event::PointerButton { pos, pressed: true, button: PointerButton::Primary, .. }] if pos == at));
        assert!(matches!(frames[2][..], [Event::PointerButton { pos, pressed: false, .. }] if pos == at));
        assert!(Step::Click(at).brings_the_pointer());
        let right = Step::RightClick(at).input();
        assert!(matches!(right[1][..], [Event::PointerButton { pos, pressed: true, button: PointerButton::Secondary, .. }] if pos == at));
        assert!(Step::RightClick(at).brings_the_pointer());
    }

    #[test]
    fn scrolling_down_moves_what_is_in_view_up() {
        let frames = Step::Scroll(pos2(1.0, 2.0), 300.0).input();
        assert_eq!(frames[0], [Event::PointerMoved(pos2(1.0, 2.0))]);
        assert!(matches!(frames[1][..], [Event::MouseWheel { unit: MouseWheelUnit::Point, delta, .. }] if delta == vec2(0.0, -300.0)));
    }

    #[test]
    fn typing_needs_no_pointer() {
        let step = Step::Type("axe".to_string());
        assert_eq!(step.input(), [[Event::Text("axe".to_string())]]);
        assert!(!step.brings_the_pointer());
    }

    #[test]
    fn waiting_gives_the_window_nothing_to_do() {
        let step = Step::Wait(12.0);
        assert!(step.input().is_empty());
        assert!(!step.brings_the_pointer());
    }
}
