#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod logo;
mod pictures;
mod presets;
mod script;
mod settings;
mod status;
mod support;
mod theme;
mod update;
mod views;
mod worker;

use std::path::PathBuf;

use eframe::egui::{self, pos2};

use crate::script::Step;

#[derive(Debug, Default, PartialEq)]
pub struct Options {
    pub dota: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub ignore_running: bool,
    pub no_start: bool,
    pub screenshot: Option<PathBuf>,
    pub open: Option<String>,
    pub steps: Vec<Step>,
    pub size: Option<[f32; 2]>,
    pub update_from: Option<String>,
    pub pretend_version: Option<String>,
    pub no_restart: bool,
    pub updated_from: Option<String>,
}

impl Options {
    pub fn read(arguments: impl IntoIterator<Item = String>) -> Options {
        let mut options = Options::default();
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--ignore-running" => options.ignore_running = true,
                "--no-start" => options.no_start = true,
                "--no-restart" => options.no_restart = true,
                "--update-from" => options.update_from = arguments.next(),
                "--pretend-version" => options.pretend_version = arguments.next(),
                "--updated-from" => options.updated_from = arguments.next(),
                "--dota" => options.dota = arguments.next().map(PathBuf::from),
                "--data-dir" => options.data_dir = arguments.next().map(PathBuf::from),
                "--screenshot" => options.screenshot = arguments.next().map(PathBuf::from),
                "--open" => options.open = arguments.next(),
                "--size" => options.size = arguments.next().and_then(|size| read_numbers(&size, 'x')),
                "--click" => {
                    let at: Option<[f32; 2]> = arguments.next().and_then(|at| read_numbers(&at, ','));
                    options.steps.extend(at.map(|[x, y]| Step::Click(pos2(x, y))));
                }
                "--rest" => {
                    let at: Option<[f32; 2]> = arguments.next().and_then(|at| read_numbers(&at, ','));
                    options.steps.extend(at.map(|[x, y]| Step::Rest(pos2(x, y))));
                }
                "--scroll" => {
                    let turn: Option<[f32; 3]> = arguments.next().and_then(|turn| read_numbers(&turn, ','));
                    options.steps.extend(turn.map(|[x, y, down]| Step::Scroll(pos2(x, y), down)));
                }
                "--type" => options.steps.extend(arguments.next().map(Step::Type)),
                "--wait" => {
                    let seconds: Option<f64> = arguments.next().and_then(|seconds| seconds.trim().parse().ok());
                    options.steps.extend(seconds.filter(|seconds| seconds.is_finite() && *seconds > 0.0).map(Step::Wait));
                }
                _ => {}
            }
        }
        options.ignore_running &= options.dota.is_some();
        options
    }
}

fn read_numbers<const N: usize>(text: &str, between: char) -> Option<[f32; N]> {
    let numbers: Option<Vec<f32>> = text.split(between).map(|number| number.trim().parse().ok()).collect();
    numbers?.try_into().ok()
}

const LIKED_SIZE: [f32; 2] = [1280.0, 820.0];
const LEAST_SIZE: [f32; 2] = [760.0, 480.0];
const SCREEN_MARGIN: f32 = 8.0;

fn opening_size(room: Option<[f32; 2]>) -> [f32; 2] {
    let Some(room) = room else { return LIKED_SIZE };
    [0, 1].map(|side| LIKED_SIZE[side].min(room[side] - SCREEN_MARGIN).max(LEAST_SIZE[side]))
}

#[cfg(windows)]
fn screen_room() -> Option<[f32; 2]> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXFULLSCREEN, SM_CYFULLSCREEN};

    let (width, height, dots_per_inch) =
        unsafe { (GetSystemMetrics(SM_CXFULLSCREEN), GetSystemMetrics(SM_CYFULLSCREEN), GetDpiForSystem()) };
    let scale = dots_per_inch as f32 / 96.0;
    (width > 0 && height > 0 && scale > 0.0).then(|| [width as f32 / scale, height as f32 / scale])
}

#[cfg(not(windows))]
fn screen_room() -> Option<[f32; 2]> {
    None
}

fn main() -> eframe::Result {
    let options = Options::read(std::env::args().skip(1));
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(app::TITLE)
        .with_inner_size(options.size.unwrap_or(LIKED_SIZE))
        .with_min_inner_size(LEAST_SIZE)
        .with_icon(logo::icon());
    if options.screenshot.is_some() {
        viewport = viewport.with_position([-8000.0, -8000.0]).with_active(false).with_taskbar(false);
    }
    let fit_the_screen: eframe::WindowBuilderHook = Box::new(|viewport| viewport.with_inner_size(opening_size(screen_room())));
    let native = eframe::NativeOptions { viewport, window_builder: options.size.is_none().then_some(fit_the_screen), ..Default::default() };
    eframe::run_native(app::TITLE, native, Box::new(move |creation| Ok(Box::new(app::App::new(creation, options)))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(arguments: &[&str]) -> Options {
        Options::read(arguments.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn a_player_starts_the_app_with_nothing() {
        assert_eq!(read(&[]), Options::default());
    }

    #[test]
    fn testing_options_are_read() {
        let options = read(&[
            "--dota", "C:\\copy", "--data-dir", "D:\\data", "--ignore-running", "--no-start", "--screenshot", "shot.png", "--open", "hero:axe",
            "--size", "1000x700", "--unknown",
        ]);
        assert_eq!(options.dota, Some(PathBuf::from("C:\\copy")));
        assert_eq!(options.data_dir, Some(PathBuf::from("D:\\data")));
        assert!(options.ignore_running && options.no_start);
        assert_eq!(options.screenshot, Some(PathBuf::from("shot.png")));
        assert_eq!(options.open.as_deref(), Some("hero:axe"));
        assert_eq!(options.size, Some([1000.0, 700.0]));
        assert!(options.steps.is_empty());
    }

    #[test]
    fn options_for_testing_updates_are_read() {
        let options = read(&["--update-from", "C:\\release\\update.json", "--pretend-version", "0.0.1", "--no-restart"]);
        assert_eq!(options.update_from.as_deref(), Some("C:\\release\\update.json"));
        assert_eq!(options.pretend_version.as_deref(), Some("0.0.1"));
        assert!(options.no_restart);
        let plain = read(&[]);
        assert!(plain.update_from.is_none() && plain.pretend_version.is_none() && !plain.no_restart);
        assert_eq!(read(&["--updated-from", "0.1.0"]).updated_from.as_deref(), Some("0.1.0"));
        assert_eq!(read(&["--updated-from"]).updated_from, None);
    }

    #[test]
    fn steps_are_kept_in_the_order_they_were_given() {
        let options = read(&["--click", "640,260", "--type", "baby roshan", "--scroll", "700, 400, 900", "--wait", "2.5", "--click", "12.5,7"]);
        assert_eq!(
            options.steps,
            [
                Step::Click(pos2(640.0, 260.0)),
                Step::Type("baby roshan".to_string()),
                Step::Scroll(pos2(700.0, 400.0), 900.0),
                Step::Wait(2.5),
                Step::Click(pos2(12.5, 7.0)),
            ]
        );
        assert!(read(&["--click", "640", "--scroll", "1,2", "--click", "a,b", "--wait", "long", "--wait", "-3", "--type"]).steps.is_empty());
    }

    #[test]
    fn writing_while_dota_runs_is_only_for_a_copy_that_is_pointed_out() {
        assert!(!read(&["--ignore-running"]).ignore_running);
        assert!(read(&["--ignore-running", "--dota", "C:\\copy"]).ignore_running);
        assert!(!read(&["--dota", "C:\\copy"]).ignore_running);
    }

    #[test]
    fn the_window_opens_no_larger_than_the_screen_has_room_for() {
        assert_eq!(opening_size(None), LIKED_SIZE);
        assert_eq!(opening_size(Some([1920.0, 1009.0])), LIKED_SIZE, "a large screen");
        assert_eq!(opening_size(Some([1366.0, 697.0])), [1280.0, 689.0]);
        assert_eq!(opening_size(Some([1280.0, 649.0])), [1272.0, 641.0]);
        assert_eq!(opening_size(Some([640.0, 400.0])), LEAST_SIZE, "never smaller than can be used");
    }

    #[test]
    fn the_screen_says_how_much_room_it_has() {
        assert!(screen_room().is_none_or(|[width, height]| width >= 320.0 && height >= 200.0));
    }

    #[test]
    fn numbers_are_read_only_if_there_are_as_many_as_asked_for() {
        assert_eq!(read_numbers::<2>("1180x760", 'x'), Some([1180.0, 760.0]));
        assert_eq!(read_numbers::<2>("wide", 'x'), None);
        assert_eq!(read_numbers::<2>("1x2x3", 'x'), None);
        assert_eq!(read_numbers::<3>("1, 2 ,3", ','), Some([1.0, 2.0, 3.0]));
    }
}
