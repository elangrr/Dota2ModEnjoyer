use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use dme_core::apply::{EffectsFor, Pick};
use dme_core::catalog::Catalog;
use dme_core::engine::{Engine, ModState, Ready, Restored, Status};
use dme_core::loadout::same_picks;
use dme_core::vpk::Archive;
use dme_core::{launch, process};
use eframe::egui;

const WATCH: Duration = Duration::from_millis(1500);

pub enum Job {
    Load,
    Play { picks: Vec<Pick>, effects_for: EffectsFor, start: bool, low_end: bool },
    Restore,
}

pub enum Reply {
    Loaded(Result<Loaded, Failure>),
    Game(GameState),
    Played(Result<Played, Failure>),
    Restored(Result<Restored, Failure>),
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
}

impl Worker {
    pub fn start(engine: Engine, ctx: egui::Context) -> Worker {
        let (jobs, job_inbox) = mpsc::channel();
        let (reply_outbox, replies) = mpsc::channel();
        std::thread::Builder::new()
            .name("game files".to_string())
            .spawn(move || run(&engine, &job_inbox, &reply_outbox, &ctx))
            .expect("a thread can be started");
        Worker { jobs, replies }
    }

    pub fn send(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn poll(&self) -> Option<Reply> {
        self.replies.try_recv().ok()
    }
}

fn run(engine: &Engine, jobs: &Receiver<Job>, replies: &Sender<Reply>, ctx: &egui::Context) {
    let mut running = None;
    let tell_state = |running: &mut Option<bool>| {
        if let Ok(status) = engine.status() {
            *running = Some(status.running);
            let _ = replies.send(Reply::Game(status.into()));
        }
    };
    tell_state(&mut running);
    ctx.request_repaint();

    loop {
        match jobs.recv_timeout(WATCH) {
            Ok(Job::Load) => {
                let _ = replies.send(Reply::Loaded(load(engine).map_err(|error| failure(engine, "reading the game's files", &error))));
            }
            Ok(Job::Play { picks, effects_for, start, low_end }) => {
                let played = play(engine, &picks, &effects_for, start, low_end).map_err(|error| failure(engine, "putting the picks in the game", &error));
                let _ = replies.send(Reply::Played(played));
                tell_state(&mut running);
            }
            Ok(Job::Restore) => {
                let restored = engine.restore().map_err(|error| failure(engine, "going back to normal Dota", &error));
                let _ = replies.send(Reply::Restored(restored));
                tell_state(&mut running);
            }
            Err(RecvTimeoutError::Timeout) => {
                if Some(process::is_dota_running()) == running {
                    continue;
                }
                tell_state(&mut running);
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
        ctx.request_repaint();
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
        sentence: error.to_string(),
        details: format!(
            "Dota 2 Mod Enjoyer {}\nWhile {doing}: {error}\n{error:?}\nDota 2 build {build} in {}\nData in {}",
            env!("CARGO_PKG_VERSION"),
            engine.dota.root().display(),
            engine.data_dir.display(),
        ),
    }
}
