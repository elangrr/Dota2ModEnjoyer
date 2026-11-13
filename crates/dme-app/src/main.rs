#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod changelog;
mod crash;
mod glass;
mod logo;
mod one_copy;
mod pictures;
mod presets;
mod report;
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
                "--right-click" => {
                    let at: Option<[f32; 2]> = arguments.next().and_then(|at| read_numbers(&at, ','));
                    options.steps.extend(at.map(|[x, y]| Step::RightClick(pos2(x, y))));
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

const TRY_NO_GLASS: &str = "DME_TRY_NO_GLASS";
const TRY_ONE_COPY: &str = "DME_TRY_ONE_COPY";

fn main() -> std::process::ExitCode {
    let options = Options::read(std::env::args().skip(1));
    let data_dir = options.data_dir.clone().unwrap_or_else(dme_core::engine::default_data_dir);
    let picture_only = options.screenshot.is_some();
    crash::watch(&data_dir);
    let trying_copies = std::env::var_os(TRY_ONE_COPY).is_some();
    let held = match picture_only && !trying_copies {
        true => None,
        false => {
            let wait = if options.updated_from.is_some() { one_copy::AFTER_AN_UPDATE } else { one_copy::AT_A_START };
            match one_copy::take(&data_dir, wait) {
                one_copy::Taken::Alone(held) => Some(held),
                one_copy::Taken::Other => {
                    if !picture_only {
                        one_copy::bring_forward();
                    }
                    return std::process::ExitCode::SUCCESS;
                }
            }
        }
    };
    let settings_file = data_dir.join(app::SETTINGS_FILE);
    let mut settings = settings::Settings::load(&settings_file);
    let trying = std::env::var_os(TRY_NO_GLASS).is_some();
    let glass = !settings.no_glass && (!picture_only || trying);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(app::TITLE)
        .with_inner_size(options.size.unwrap_or(LIKED_SIZE))
        .with_min_inner_size(LEAST_SIZE)
        .with_icon(logo::icon());
    if picture_only {
        viewport = viewport.with_position([-8000.0, -8000.0]).with_active(false).with_taskbar(false);
    }
    if glass {
        viewport = viewport.with_transparent(true);
    }
    let fit_the_screen: eframe::WindowBuilderHook = Box::new(|viewport| viewport.with_inner_size(opening_size(screen_room())));
    let native = eframe::NativeOptions { viewport, window_builder: options.size.is_none().then_some(fit_the_screen), ..Default::default() };
    crash::making_window(true);
    let shown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eframe::run_native(
            app::TITLE,
            native,
            Box::new(move |creation| {
                if trying && glass {
                    panic!("{TRY_NO_GLASS} is set: the window is taken not to be see-through on this computer");
                }
                crash::making_window(false);
                Ok(Box::new(app::App::new(creation, options, glass)))
            }),
        )
    }));
    let outcome = match shown {
        Ok(Ok(())) => Shown::Closed,
        Ok(Err(error)) => Shown::Failed(error.to_string()),
        Err(_) => Shown::Crashed,
    };
    match afterwards(outcome, crash::is_making_window(), glass) {
        Afterwards::Done => std::process::ExitCode::SUCCESS,
        Afterwards::WithoutGlass => {
            settings.no_glass = true;
            let _ = settings.save(&settings_file);
            drop(held);
            let again = std::env::current_exe().and_then(|app| std::process::Command::new(app).args(std::env::args_os().skip(1)).spawn());
            if again.is_err() && !picture_only {
                crash::tell("The app could not start with its see-through window. Start it again: it starts without it from now on.");
            }
            std::process::ExitCode::SUCCESS
        }
        Afterwards::NoWindow(why) => {
            if !picture_only {
                crash::tell(&format!("The app's window could not be made on this computer: {why}\n\nUpdating the driver of the graphics card may help."));
            }
            std::process::ExitCode::FAILURE
        }
        Afterwards::Crashed => {
            if !picture_only {
                let written = Some(data_dir.join(crash::FILE)).filter(|file| file.exists());
                crash::tell(&crash::closed_words(written.as_deref()));
            }
            std::process::ExitCode::FAILURE
        }
    }
}

#[derive(Debug, PartialEq)]
enum Shown {
    Closed,
    Failed(String),
    Crashed,
}

#[derive(Debug, PartialEq)]
enum Afterwards {
    Done,
    WithoutGlass,
    NoWindow(String),
    Crashed,
}

fn afterwards(shown: Shown, making: bool, glass: bool) -> Afterwards {
    match shown {
        Shown::Closed => Afterwards::Done,
        _ if making && glass => Afterwards::WithoutGlass,
        Shown::Failed(why) => Afterwards::NoWindow(why),
        Shown::Crashed if making => Afterwards::NoWindow(crash::last().unwrap_or_else(|| "no reason was given".to_string())),
        Shown::Crashed => Afterwards::Crashed,
    }
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
        let options = read(&["--click", "640,260", "--type", "baby roshan", "--scroll", "700, 400, 900", "--wait", "2.5", "--right-click", "30,40", "--click", "12.5,7"]);
        assert_eq!(
            options.steps,
            [
                Step::Click(pos2(640.0, 260.0)),
                Step::Type("baby roshan".to_string()),
                Step::Scroll(pos2(700.0, 400.0), 900.0),
                Step::Wait(2.5),
                Step::RightClick(pos2(30.0, 40.0)),
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
    fn a_window_that_cannot_be_see_through_is_made_again_without_and_a_crash_is_told() {
        assert_eq!(afterwards(Shown::Closed, false, true), Afterwards::Done);
        assert_eq!(afterwards(Shown::Crashed, true, true), Afterwards::WithoutGlass);
        assert_eq!(afterwards(Shown::Failed("no pixel format".to_string()), true, true), Afterwards::WithoutGlass);
        assert_eq!(afterwards(Shown::Failed("no OpenGL".to_string()), true, false), Afterwards::NoWindow("no OpenGL".to_string()));
        assert!(matches!(afterwards(Shown::Crashed, true, false), Afterwards::NoWindow(_)));
        assert_eq!(afterwards(Shown::Crashed, false, true), Afterwards::Crashed);
        assert_eq!(afterwards(Shown::Crashed, false, false), Afterwards::Crashed);
        assert_eq!(afterwards(Shown::Failed("lost".to_string()), false, false), Afterwards::NoWindow("lost".to_string()));
    }

    #[test]
    fn numbers_are_read_only_if_there_are_as_many_as_asked_for() {
        assert_eq!(read_numbers::<2>("1180x760", 'x'), Some([1180.0, 760.0]));
        assert_eq!(read_numbers::<2>("wide", 'x'), None);
        assert_eq!(read_numbers::<2>("1x2x3", 'x'), None);
        assert_eq!(read_numbers::<3>("1, 2 ,3", ','), Some([1.0, 2.0, 3.0]));
    }
}
