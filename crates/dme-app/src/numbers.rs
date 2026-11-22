use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, SystemTime};

use dme_core::builds::{self, Builds};
use dme_core::draft::{self, Numbers};
use dme_core::{fsutil, update};
use eframe::egui;

use crate::Options;
use crate::worker::Failure;

pub const FILE: &str = "draft.json";

const FRESH_FOR: Duration = Duration::from_secs(24 * 3600);

type Parse<T> = fn(&[u8]) -> dme_core::Result<T>;

struct Kind<T> {
    failed: crate::lang::T,
    limit: usize,
    parse: Parse<T>,
}

pub struct Fetcher<T> {
    kind: Kind<T>,
    source: Option<String>,
    testing: bool,
    kept: PathBuf,
    read: Option<Arc<T>>,
    failure: Option<Failure>,
    fetching: bool,
    looked: bool,
    inbox: Receiver<Result<T, String>>,
    outbox: Sender<Result<T, String>>,
}

const TRIAL_SOURCE: Option<&str> = option_env!("DME_NUMBERS_TRIAL");

fn numbers_source(options: &Options) -> (Option<String>, bool) {
    source_of(options, TRIAL_SOURCE)
}

fn source_of(options: &Options, trial: Option<&str>) -> (Option<String>, bool) {
    let trial = trial.map(str::trim).filter(|trial| !trial.is_empty());
    let testing = options.numbers_from.is_some() || trial.is_some();
    let built_in = options.screenshot.is_none().then(|| trial.unwrap_or(draft::NUMBERS_URL).to_string());
    let source = options.numbers_from.clone().or(built_in).filter(|source| !source.trim().is_empty() && update::may_fetch(source, testing));
    (source, testing)
}

fn beside(address: &str, name: &str) -> String {
    let folder = address.rfind(['/', '\\']).map_or("", |end| &address[..=end]);
    format!("{folder}{name}")
}

pub fn draft_numbers(options: &Options, data_dir: &Path) -> Fetcher<Numbers> {
    let (source, testing) = numbers_source(options);
    Fetcher::new(Kind { failed: crate::lang::T::NumbersNotFetched, limit: draft::NUMBERS_LIMIT, parse: Numbers::parse }, source, testing, data_dir.join(FILE))
}

pub fn hero_builds(options: &Options, data_dir: &Path) -> Fetcher<Builds> {
    let (source, testing) = numbers_source(options);
    let source = source.map(|numbers| beside(&numbers, builds::BUILDS_FILE));
    Fetcher::new(Kind { failed: crate::lang::T::BuildsNotFetched, limit: builds::BUILDS_LIMIT, parse: Builds::parse }, source, testing, data_dir.join(builds::BUILDS_FILE))
}

impl<T: Send + 'static> Fetcher<T> {
    fn new(kind: Kind<T>, source: Option<String>, testing: bool, kept: PathBuf) -> Fetcher<T> {
        let read = std::fs::read(&kept).ok().and_then(|bytes| (kind.parse)(&bytes).ok()).map(Arc::new);
        let (outbox, inbox) = mpsc::channel();
        Fetcher { kind, source, testing, kept, read, failure: None, fetching: false, looked: false, inbox, outbox }
    }

    pub fn get(&self) -> Option<&Arc<T>> {
        self.read.as_ref()
    }

    pub fn is_fetching(&self) -> bool {
        self.fetching
    }

    pub fn failure(&self) -> Option<&Failure> {
        self.failure.as_ref()
    }

    pub fn wanted(&mut self, ctx: &egui::Context) {
        if std::mem::replace(&mut self.looked, true) {
            return;
        }
        let fresh = self.read.is_some() && !self.testing && is_fresh(&self.kept, SystemTime::now());
        if !fresh {
            self.fetch(ctx);
        }
    }

    pub fn fetch(&mut self, ctx: &egui::Context) {
        let Some(source) = self.source.clone().filter(|_| !self.fetching) else { return };
        self.fetching = true;
        self.failure = None;
        let (testing, kept, limit, parse) = (self.testing, self.kept.clone(), self.kind.limit, self.kind.parse);
        let (outbox, ctx) = (self.outbox.clone(), ctx.clone());
        crate::update::spawn("fetching", move || {
            let fetched = crate::update::fetch(&source, limit, testing).and_then(|bytes| {
                let read = parse(&bytes).map_err(|error| error.to_string())?;
                let _ = keep(&kept, &bytes);
                Ok(read)
            });
            let _ = outbox.send(fetched);
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self) {
        while let Ok(fetched) = self.inbox.try_recv() {
            self.fetching = false;
            match fetched {
                Ok(read) => self.read = Some(Arc::new(read)),
                Err(error) => {
                    let source = self.source.as_deref().unwrap_or_default();
                    self.failure = Some(Failure {
                        sentence: crate::lang::t(self.kind.failed).to_string(),
                        english: crate::lang::text_in(crate::lang::LANGUAGES[0], self.kind.failed).to_string(),
                        details: format!("Dota 2 Mod Enjoyer {}\nWhile fetching {source}: {error}", env!("CARGO_PKG_VERSION")),
                    });
                }
            }
        }
    }
}

fn keep(path: &Path, bytes: &[u8]) -> dme_core::Result<()> {
    if let Some(folder) = path.parent() {
        fsutil::create_dir_all(folder)?;
    }
    fsutil::write_atomic(path, bytes)
}

fn is_fresh(kept: &Path, now: SystemTime) -> bool {
    let written = std::fs::metadata(kept).and_then(|file| file.modified());
    written.ok().and_then(|written| now.duration_since(written).ok()).is_some_and(|age| age < FRESH_FOR)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn file(made: u64) -> String {
        let rank = r#"{"played":[[9],[0],[0],[0],[0]],"won":[[4],[0],[0],[0],[0]]}"#;
        let pair = r#"{"against":[[0]],"with":[[0]]}"#;
        let (ranks, pairs) = ([rank; 8].join(","), [pair; 4].join(","));
        format!(r#"{{"format":1,"made":{made},"source":"STRATZ","patch":"7.41f","weeks":4,"heroes":["npc_dota_hero_axe"],"ranks":[{ranks}],"pairs":[{pairs}]}}"#)
    }

    fn options(source: &Path) -> Options {
        Options { numbers_from: Some(source.display().to_string()), ..Options::default() }
    }

    fn wait<T: Send + 'static>(fetcher: &mut Fetcher<T>) {
        let started = Instant::now();
        while fetcher.is_fetching() && started.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
            fetcher.poll();
        }
        assert!(!fetcher.is_fetching(), "the fetching came to an end");
    }

    #[test]
    fn numbers_are_fetched_when_they_are_wanted_and_kept_for_the_next_run() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.json");
        std::fs::write(&source, file(7)).unwrap();
        let data = dir.path().join("data");
        let ctx = egui::Context::default();

        let mut fetcher = draft_numbers(&options(&source), &data);
        assert!(fetcher.get().is_none() && !fetcher.is_fetching(), "nothing is fetched before the page is opened");
        fetcher.wanted(&ctx);
        assert!(fetcher.is_fetching());
        wait(&mut fetcher);
        assert_eq!(fetcher.get().map(|numbers| numbers.made), Some(7));
        assert!(fetcher.failure().is_none());
        assert_eq!(std::fs::read_to_string(data.join(FILE)).unwrap(), file(7), "kept as it came");

        fetcher.wanted(&ctx);
        assert!(!fetcher.is_fetching());

        std::fs::write(&source, file(8)).unwrap();
        let mut next = draft_numbers(&options(&source), &data);
        assert_eq!(next.get().map(|numbers| numbers.made), Some(7));
        next.wanted(&ctx);
        wait(&mut next);
        assert_eq!(next.get().map(|numbers| numbers.made), Some(8), "and newer ones once they are fetched");
    }

    #[test]
    fn numbers_that_do_not_come_leave_the_kept_ones_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.json");
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join(FILE), file(3)).unwrap();
        let ctx = egui::Context::default();

        for broken in [None, Some("nonsense")] {
            if let Some(text) = broken {
                std::fs::write(&source, text).unwrap();
            }
            let mut fetcher = draft_numbers(&options(&source), &data);
            fetcher.wanted(&ctx);
            wait(&mut fetcher);
            assert_eq!(fetcher.get().map(|numbers| numbers.made), Some(3));
            let failure = fetcher.failure().expect("it is said that they did not come");
            assert!(failure.sentence.starts_with("The numbers for the draft could not be fetched."));
            assert!(failure.details.contains("source.json"), "{}", failure.details);
            assert_eq!(std::fs::read_to_string(data.join(FILE)).unwrap(), file(3), "what is kept stays as it is");
        }

        std::fs::write(&source, file(4)).unwrap();
        let mut fetcher = draft_numbers(&options(&source), &data);
        fetcher.fetch(&ctx);
        wait(&mut fetcher);
        assert!(fetcher.failure().is_none());
        assert_eq!(fetcher.get().map(|numbers| numbers.made), Some(4));
    }

    #[test]
    fn a_window_that_asks_nobody_fetches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let quiet = Options { screenshot: Some(dir.path().join("shot.png")), ..Options::default() };
        let mut fetcher = draft_numbers(&quiet, dir.path());
        fetcher.wanted(&ctx);
        assert!(!fetcher.is_fetching() && fetcher.get().is_none() && fetcher.failure().is_none());
        assert!(hero_builds(&quiet, dir.path()).source.is_none());
        assert_eq!(source_of(&Options::default(), None), (Some(draft::NUMBERS_URL.to_string()), false));
        assert!(update::may_fetch(draft::NUMBERS_URL, false));
        assert_eq!(beside(draft::NUMBERS_URL, builds::BUILDS_FILE), "https://raw.githubusercontent.com/elangrr/Dota2ModEnjoyer/data/builds.json");
        assert_eq!(hero_builds(&Options::default(), dir.path()).kept, dir.path().join("builds.json"));
    }

    #[test]
    fn a_build_that_is_tried_out_fetches_from_where_it_was_built_to() {
        let trial = Some(r"C:\numbers\draft.json");
        assert_eq!(source_of(&Options::default(), trial), (Some(r"C:\numbers\draft.json".to_string()), true));
        let told = Options { numbers_from: Some("elsewhere.json".to_string()), ..Options::default() };
        assert_eq!(source_of(&told, trial), (Some("elsewhere.json".to_string()), true));
        let quiet = Options { screenshot: Some("shot.png".into()), ..Options::default() };
        assert_eq!(source_of(&quiet, trial).0, None);
        assert_eq!(source_of(&Options::default(), Some(" ")), (Some(draft::NUMBERS_URL.to_string()), false));
    }

    #[test]
    fn a_version_with_these_pages_takes_no_update_of_one_without_them() {
        assert!(!update::is_newer("0.2.99", env!("CARGO_PKG_VERSION")), "this is version {}", env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_builds_are_fetched_from_beside_the_numbers() {
        assert_eq!(beside("https://example.org/data/draft.json", "builds.json"), "https://example.org/data/builds.json");
        assert_eq!(beside(r"C:\numbers\draft.json", "builds.json"), r"C:\numbers\builds.json");
        assert_eq!(beside("draft.json", "builds.json"), "builds.json");

        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("draft.json");
        std::fs::write(dir.path().join("builds.json"), r#"{"format":2,"made":9,"patch":"7.41f","items":[],"skills":[],"heroes":{}}"#).unwrap();
        let data = dir.path().join("data");
        let mut fetcher = hero_builds(&options(&source), &data);
        fetcher.wanted(&egui::Context::default());
        wait(&mut fetcher);
        assert_eq!(fetcher.get().map(|builds| builds.made), Some(9));
        assert!(data.join("builds.json").is_file(), "kept under their own name");

        std::fs::remove_file(dir.path().join("builds.json")).unwrap();
        let mut fetcher = hero_builds(&options(&source), &dir.path().join("other"));
        fetcher.wanted(&egui::Context::default());
        wait(&mut fetcher);
        assert!(fetcher.failure().unwrap().sentence.starts_with("The builds could not be fetched."));
    }

    #[test]
    fn kept_numbers_are_new_enough_for_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let kept = dir.path().join(FILE);
        let now = SystemTime::now();
        assert!(!is_fresh(&kept, now), "no file");
        std::fs::write(&kept, "x").unwrap();
        assert!(is_fresh(&kept, now + Duration::from_secs(3600)));
        assert!(!is_fresh(&kept, now + Duration::from_secs(25 * 3600)));
        assert!(!is_fresh(&kept, now - Duration::from_secs(3600)), "a file from after now");
    }
}
