use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use dme_core::apply::{EffectsFor, Pick};
use dme_core::catalog::Catalog;
use dme_core::engine::{Engine, ModState, Ready, Restored, Status};
use dme_core::loadout::same_picks;
use dme_core::vpk::Archive;
use dme_core::{launch, process};
use eframe::egui;

use crate::crash;

const WATCH: Duration = Duration::from_millis(1500);
const LOOK_AGAIN: u32 = 6;

pub const WENT_WRONG: &str = "Something went wrong inside the app, and it stopped what it was doing. Try again. If it happens again, please report it.";

pub enum Job {
    Load,
    Play { picks: Vec<Pick>, effects_for: EffectsFor, start: bool, low_end: bool },
    Restore,
    Tidy,
}

pub enum Reply {
    Loaded(Result<Loaded, Failure>),
    Game(GameState),
    Played(Result<Played, Failure>),
    Restored(Result<Restored, Failure>),
    Tidied(Result<Restored, Failure>),
    Stopped(Failure),
}

pub struct Loaded {
    pub catalog: Arc<Catalog>,
    pub archive: Arc<Archive>,
}

#[derive(Clone, Debug)]
pub struct GameState {
    pub running: bool,
    pub mods: ModState,
    pub applied: Vec<Pick>,
    pub effects_for: EffectsFor,
    pub package_is_current: bool,
    pub steam_busy: bool,
}

impl GameState {
    pub fn shows(&self, picks: &[Pick], effects_for: &EffectsFor) -> bool {
        self.mods == ModState::On && self.package_is_current && same_picks(&self.applied, picks) && self.effects_for == *effects_for
    }
}

impl From<Status> for GameState {
    fn from(status: Status) -> GameState {
        GameState {
            running: status.running,
            mods: status.state,
            applied: status.picks,
            effects_for: status.effects_for,
            package_is_current: status.package_is_current,
            steam_busy: status.steam_busy,
        }
    }
}

pub struct Played {
    pub ready: Ready,
    pub started: bool,
}

#[derive(Clone, Debug)]
pub struct Failure {
    pub sentence: String,
    pub details: String,
}

pub struct Worker {
    jobs: Sender<Job>,
    replies: Receiver<Reply>,
    told_gone: Cell<bool>,
}

impl Worker {
    pub fn start(engine: Engine, ctx: egui::Context) -> Worker {
        let (jobs, job_inbox) = mpsc::channel();
        let (reply_outbox, replies) = mpsc::channel();
        std::thread::Builder::new()
            .name("game files".to_string())
            .spawn(move || run(&engine, &job_inbox, &reply_outbox, &ctx))
            .expect("a thread can be started");
        Worker { jobs, replies, told_gone: Cell::new(false) }
    }

    pub fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn poll(&self) -> Option<Reply> {
        match self.replies.try_recv() {
            Ok(reply) => Some(reply),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) if !self.told_gone.replace(true) => {
                let details = format!("While working on the game's files, the thread for it stopped.\n{}", crash::last().unwrap_or_default());
                Some(Reply::Stopped(Failure { sentence: WENT_WRONG.to_string(), details }))
            }
            Err(TryRecvError::Disconnected) => None,
        }
    }
}

fn run(engine: &Engine, jobs: &Receiver<Job>, replies: &Sender<Reply>, ctx: &egui::Context) {
    let mut running = None;
    let mut quiet: u32 = 0;
    let tell_state = |running: &mut Option<bool>| {
        if let Ok(Ok(status)) = catch_unwind(AssertUnwindSafe(|| engine.status())) {
            *running = Some(status.running);
            let _ = replies.send(Reply::Game(status.into()));
        }
    };
    tell_state(&mut running);
    ctx.request_repaint();

    loop {
        match jobs.recv_timeout(WATCH) {
            Ok(Job::Load) => {
                let _ = replies.send(Reply::Loaded(guarded(engine, "reading the game's files", || load(engine))));
            }
            Ok(Job::Play { picks, effects_for, start, low_end }) => {
                let played = guarded(engine, "putting the picks in the game", || play(engine, &picks, &effects_for, start, low_end));
                let _ = replies.send(Reply::Played(played));
                tell_state(&mut running);
            }
            Ok(Job::Restore) => {
                let restored = guarded(engine, "going back to normal Dota", || engine.restore());
                let _ = replies.send(Reply::Restored(restored));
                tell_state(&mut running);
            }
            Ok(Job::Tidy) => {
                let tidied = guarded(engine, "putting a game that an update left half undone back to normal", || engine.restore());
                let _ = replies.send(Reply::Tidied(tidied));
                tell_state(&mut running);
            }
            Err(RecvTimeoutError::Timeout) => {
                quiet = quiet.wrapping_add(1);
                let look_again = quiet.is_multiple_of(LOOK_AGAIN);
                if catch_unwind(process::is_dota_running).ok() == running && !look_again {
                    continue;
                }
                tell_state(&mut running);
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
        ctx.request_repaint();
    }
}

fn guarded<T>(engine: &Engine, doing: &str, work: impl FnOnce() -> dme_core::Result<T>) -> Result<T, Failure> {
    match catch_unwind(AssertUnwindSafe(work)) {
        Ok(done) => done.map_err(|error| failure(engine, doing, &error)),
        Err(_) => Err(Failure { sentence: WENT_WRONG.to_string(), details: format!("While {doing}:\n{}", crash::last().unwrap_or_default()) }),
    }
}

fn load(engine: &Engine) -> dme_core::Result<Loaded> {
    let archive = Arc::new(Archive::open(&engine.dota.game_archive())?);
    let catalog = Arc::new(Catalog::load(&archive)?);
    Ok(Loaded { catalog, archive })
}

fn play(engine: &Engine, picks: &[Pick], effects_for: &EffectsFor, start: bool, low_end: bool) -> dme_core::Result<Played> {
    let ready = engine.make_ready(picks, effects_for)?;
    let started = start && !process::is_dota_running();
    if started {
        launch::start_dota(launch::settings(low_end))?;
    }
    Ok(Played { ready, started })
}

fn failure(engine: &Engine, doing: &str, error: &dme_core::Error) -> Failure {
    let build = engine.dota.version().map_or_else(|_| "unknown".to_string(), |version| version.build);
    Failure {
        sentence: error.plain(engine.dota.root()),
        details: format!(
            "Dota 2 Mod Enjoyer {}\nWhile {doing}: {error}\n{error:?}\nDota 2 build {build} in {}\nData in {}",
            env!("CARGO_PKG_VERSION"),
            engine.dota.root().display(),
            engine.data_dir.display(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dme_core::dota::DotaInstall;

    fn engine(root: &std::path::Path) -> Engine {
        let dota = root.join("dota 2 beta");
        std::fs::create_dir_all(dota.join(r"game\bin\win64")).unwrap();
        std::fs::write(dota.join(r"game\bin\win64\dota2.exe"), b"").unwrap();
        Engine { dota: DotaInstall::at(&dota).unwrap(), data_dir: root.join("data"), refuse_while_running: true }
    }

    #[test]
    fn work_that_goes_wrong_inside_the_app_ends_in_a_failure_and_not_in_silence() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        crash::watch(dir.path());
        let failure = std::thread::scope(|scope| {
            let worked = scope.spawn(|| guarded(&engine, "putting the picks in the game", || -> dme_core::Result<()> { panic!("a test of what happens") }));
            worked.join().expect("the thread goes on").unwrap_err()
        });
        assert_eq!(failure.sentence, WENT_WRONG);
        assert!(failure.details.starts_with("While putting the picks in the game:\n"), "{}", failure.details);
        assert!(failure.details.contains("a test of what happens"), "{}", failure.details);
        assert!(!dir.path().join(crash::FILE).exists(), "the app goes on: that is no crash");

        let held = dme_core::Error::io("write", &engine.dota.signatures(), std::io::Error::from_raw_os_error(32));
        let failure = guarded(&engine, "putting the picks in the game", || Err::<(), _>(held)).unwrap_err();
        assert!(failure.sentence.contains("antivirus") && !failure.sentence.contains("os error"), "{}", failure.sentence);
        assert!(failure.details.contains("os error 32"), "{}", failure.details);
    }

    #[test]
    fn a_thread_that_is_gone_is_told_of_once() {
        let (jobs, _) = mpsc::channel();
        let (outbox, replies) = mpsc::channel::<Reply>();
        drop(outbox);
        let worker = Worker { jobs, replies, told_gone: Cell::new(false) };
        assert!(matches!(worker.poll(), Some(Reply::Stopped(failure)) if failure.sentence == WENT_WRONG));
        assert!(worker.poll().is_none());
    }
}
