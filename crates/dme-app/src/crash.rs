use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::thread::ThreadId;

pub const FILE: &str = "crash.txt";
const SHORT_TRACE: usize = 60;

struct Watch {
    data_dir: PathBuf,
    main: ThreadId,
}

static WATCH: OnceLock<Watch> = OnceLock::new();
static LAST: Mutex<Option<String>> = Mutex::new(None);
static MAKING_WINDOW: AtomicBool = AtomicBool::new(false);

pub fn watch(data_dir: &Path) {
    let _ = WATCH.set(Watch { data_dir: data_dir.to_path_buf(), main: std::thread::current().id() });
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let place = info.location().map(|place| format!("{}:{}", place.file(), place.line())).unwrap_or_else(|| "a place not known".to_string());
        let message = info.payload().downcast_ref::<&str>().map(|text| text.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned());
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        let text = describe(thread.name().unwrap_or("unnamed"), &place, message.as_deref().unwrap_or("no message"), &trace);
        *LAST.lock().unwrap_or_else(PoisonError::into_inner) = Some(text.clone());
        let on_main = WATCH.get().is_some_and(|watch| watch.main == thread.id());
        if on_main && !is_making_window() {
            let _ = write(&text);
        }
        before(info);
    }));
}

pub fn making_window(making: bool) {
    MAKING_WINDOW.store(making, Ordering::SeqCst);
}

pub fn is_making_window() -> bool {
    MAKING_WINDOW.load(Ordering::SeqCst)
}

pub fn last() -> Option<String> {
    let last = LAST.lock().unwrap_or_else(PoisonError::into_inner).clone()?;
    Some(shortened(&last))
}

fn describe(thread: &str, place: &str, message: &str, trace: &str) -> String {
    format!(
        "Dota 2 Mod Enjoyer {}\nSomething went wrong inside the app, on its thread \"{thread}\", at {place}: {message}\n\nWhere in the code:\n{trace}",
        env!("CARGO_PKG_VERSION")
    )
}

fn shortened(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    let Some(trace) = lines.iter().position(|line| *line == "Where in the code:") else { return text.to_string() };
    let cut = lines.len() > trace + 1 + SHORT_TRACE;
    lines.truncate((trace + 1 + SHORT_TRACE).min(lines.len()));
    let mut short = lines.join("\n");
    if cut {
        short.push_str("\n…");
    }
    short
}

pub fn write(text: &str) -> Option<PathBuf> {
    let watch = WATCH.get()?;
    let path = watch.data_dir.join(FILE);
    std::fs::create_dir_all(&watch.data_dir).ok()?;
    std::fs::write(&path, text).ok()?;
    Some(path)
}

pub fn left_from_last_time(data_dir: &Path) -> Option<String> {
    let path = data_dir.join(FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let text = text.trim();
    (!text.is_empty()).then(|| shortened(text))
}

#[cfg(windows)]
pub fn tell(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW};
    let wide = |text: &str| text.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide(crate::app::TITLE));
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR | MB_SETFOREGROUND) };
}

#[cfg(not(windows))]
pub fn tell(_text: &str) {}

pub fn closed_words(written: Option<&Path>) -> String {
    let kept = match written {
        Some(path) => format!("What happened is written in {}. Start the app again to report it, so that it gets fixed.", path.display()),
        None => "Start the app again, and if it happens again, report it in Help.".to_string(),
    };
    format!("Something went wrong inside the app, and it had to close. Your picks are kept.\n\n{kept}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_went_wrong_names_the_version_the_thread_and_the_place() {
        let text = describe("game files", r"crates\dme-app\src\worker.rs:161", "index out of bounds", "   0: one\n   1: two");
        assert!(text.starts_with(&format!("Dota 2 Mod Enjoyer {}\n", env!("CARGO_PKG_VERSION"))));
        assert!(text.contains("\"game files\"") && text.contains(r"worker.rs:161: index out of bounds"));
        assert!(text.ends_with("Where in the code:\n   0: one\n   1: two"));
    }

    #[test]
    fn where_in_the_code_is_cut_short_for_a_report() {
        let long: Vec<String> = (0..200).map(|number| format!("  {number}: frame")).collect();
        let text = describe("main", "a.rs:1", "boom", &long.join("\n"));
        let short = shortened(&text);
        assert_eq!(short.lines().count(), 4 + SHORT_TRACE + 1, "{short}");
        assert!(short.ends_with('…') && short.contains("  0: frame") && !short.contains(" 60: frame"));
        let brief = describe("main", "a.rs:1", "boom", "  0: frame");
        assert_eq!(shortened(&brief), brief);
        assert_eq!(shortened("no trace in it"), "no trace in it");
    }

    #[test]
    fn what_made_the_app_close_is_offered_once() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(left_from_last_time(dir.path()), None);
        std::fs::write(dir.path().join(FILE), describe("main", "a.rs:1", "boom", "  0: frame")).unwrap();
        assert!(left_from_last_time(dir.path()).is_some_and(|text| text.contains("boom")));
        assert_eq!(left_from_last_time(dir.path()), None, "it was taken away");
        std::fs::write(dir.path().join(FILE), "  \n").unwrap();
        assert_eq!(left_from_last_time(dir.path()), None, "an empty file says nothing");
    }

    #[test]
    fn the_box_after_a_crash_says_that_the_picks_are_kept_and_where_to_look() {
        let said = closed_words(Some(Path::new(r"C:\data\crash.txt")));
        assert!(said.contains("Your picks are kept") && said.contains(r"C:\data\crash.txt") && said.contains("report"));
        assert!(closed_words(None).contains("report it in Help"));
    }
}
