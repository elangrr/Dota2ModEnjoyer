use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use dme_core::fsutil;
use dme_core::update::{self, Manifest};
use eframe::egui;

use crate::Options;
use crate::crash;
use crate::worker::Failure;

const TRIAL_SOURCE: Option<&str> = option_env!("DME_UPDATE_TRIAL");

const ASK_EVERY: Duration = Duration::from_secs(5 * 60);
const TRIAL_ASK_EVERY: Duration = if cfg!(test) { Duration::from_millis(40) } else { Duration::from_secs(10) };
const TRIAL_STEPS: u64 = 20;
const TRIAL_STEP: Duration = if cfg!(test) { Duration::from_millis(1) } else { Duration::from_millis(250) };

fn built_in_trial() -> Option<&'static str> {
    TRIAL_SOURCE.map(str::trim).filter(|source| !source.is_empty())
}

pub enum News {
    Offered(Manifest),
    Nothing(Option<Failure>),
    Progress(u64, u64),
    Installed,
    Failed(Failure),
}

pub struct Updater {
    source: String,
    trial: bool,
    version: String,
    inbox: Receiver<News>,
    outbox: Sender<News>,
    offer: Option<Manifest>,
    asking: bool,
    fetching: bool,
    progress: Option<(u64, u64)>,
    installed: bool,
    alive: Arc<()>,
}

impl Updater {
    pub fn start(options: &Options, ctx: &egui::Context) -> Option<Updater> {
        let built_in = built_in_trial().filter(|_| options.screenshot.is_none()).map(str::to_string);
        let trial_source = options.update_from.clone().or(built_in);
        let trial = trial_source.is_some();
        let source = trial_source.unwrap_or_else(|| update::MANIFEST_URL.to_string());
        let asks = !source.trim().is_empty() && update::may_fetch(&source, trial) && (trial || options.screenshot.is_none());
        if !asks {
            return None;
        }
        let version = options.pretend_version.clone().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        let (outbox, inbox) = mpsc::channel();
        let updater = Updater { source, trial, version, inbox, outbox, offer: None, asking: true, fetching: false, progress: None, installed: false, alive: Arc::new(()) };
        updater.watch(ctx);
        Some(updater)
    }

    pub fn offer(&self) -> Option<&Manifest> {
        self.offer.as_ref()
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn trial_source(&self) -> Option<&str> {
        self.trial.then_some(self.source.as_str())
    }

    pub fn is_working(&self) -> bool {
        self.asking || self.fetching
    }

    pub fn is_fetching(&self) -> bool {
        self.fetching
    }

    pub fn is_installed(&self) -> bool {
        self.installed
    }

    pub fn progress(&self) -> Option<f32> {
        let (come, all) = self.progress.filter(|_| self.fetching || self.installed)?;
        Some(if all == 0 { 1.0 } else { (come as f64 / all as f64).clamp(0.0, 1.0) as f32 })
    }

    pub fn poll(&mut self) -> Option<News> {
        let news = self.inbox.try_recv().ok()?;
        match &news {
            News::Offered(manifest) => {
                self.asking = false;
                if !self.fetching && !self.installed {
                    self.offer = Some(manifest.clone());
                }
            }
            News::Nothing(_) => self.asking = false,
            News::Progress(come, all) => self.progress = Some((*come, *all)),
            News::Installed => {
                self.fetching = false;
                self.installed = true;
            }
            News::Failed(_) => {
                self.fetching = false;
                self.progress = None;
            }
        }
        Some(news)
    }

    fn watch(&self, ctx: &egui::Context) {
        let (source, trial, version) = (self.source.clone(), self.trial, self.version.clone());
        let (outbox, ctx) = (self.outbox.clone(), ctx.clone());
        let every = if trial { TRIAL_ASK_EVERY } else { ASK_EVERY };
        let wanted = Arc::downgrade(&self.alive);
        spawn("update check", move || {
            let mut offered: Option<String> = None;
            let mut first = true;
            while wanted.upgrade().is_some() {
                let asked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| look(&source, trial, &version)));
                let news = asked.unwrap_or_else(|_| News::Nothing(trial.then(|| failure(&version, &source, "something went wrong inside the app"))));
                let worth_telling = match &news {
                    News::Offered(manifest) => offered.as_deref() != Some(manifest.version.as_str()),
                    _ => first,
                };
                if let News::Offered(manifest) = &news {
                    offered = Some(manifest.version.clone());
                }
                if worth_telling {
                    if outbox.send(news).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
                first = false;
                std::thread::sleep(every);
            }
        });
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        let Some(manifest) = self.offer.clone().filter(|_| !self.fetching && !self.installed) else { return };
        self.fetching = true;
        self.progress = None;
        let (trial, version) = (self.trial, self.version.clone());
        let (outbox, ctx) = (self.outbox.clone(), ctx.clone());
        spawn("update", move || {
            let mut hundredths = Hundredths { all: manifest.size, told: None };
            let mut come = |bytes: u64| {
                if let Some(news) = hundredths.news(bytes) {
                    let _ = outbox.send(news);
                    ctx.request_repaint();
                }
            };
            let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let file = fetch_counting(&manifest.url, update::RELEASE_LIMIT, trial, Some(&mut come)).map_err(|error| (error, None))?;
                manifest.check_file(&file).map_err(|error| (error.to_string(), None))?;
                let app = std::env::current_exe().map_err(|error| (format!("the app could not tell where its own file is: {error}"), None))?;
                update::install(&app, &file).map_err(|error| (error.to_string(), installing_in_plain_words(&error)))
            }));
            let news = match done {
                Ok(Ok(())) => News::Installed,
                Ok(Err((error, plain))) => {
                    let mut failed = failure(&version, &manifest.url, &error);
                    if let Some(plain) = plain {
                        failed.sentence = plain.to_string();
                    }
                    News::Failed(failed)
                }
                Err(_) => News::Failed(failure(&version, &manifest.url, &crash::last().unwrap_or_else(|| "something went wrong inside the app".to_string()))),
            };
            let _ = outbox.send(news);
            ctx.request_repaint();
        });
    }
}

struct Hundredths {
    all: u64,
    told: Option<u64>,
}

impl Hundredths {
    fn news(&mut self, bytes: u64) -> Option<News> {
        let come = bytes.min(self.all);
        let hundredths = come.saturating_mul(100).checked_div(self.all).unwrap_or(100);
        (self.told != Some(hundredths)).then(|| {
            self.told = Some(hundredths);
            News::Progress(come, self.all)
        })
    }
}

fn look(source: &str, trial: bool, version: &str) -> News {
    let found = fetch(source, update::MANIFEST_LIMIT, trial).and_then(|bytes| {
        let manifest = Manifest::parse(&bytes).map_err(|error| error.to_string())?;
        let key = if trial { "" } else { update::PUBLIC_KEY };
        manifest.check_signature(key).map_err(|error| error.to_string())?;
        Ok(manifest)
    });
    match found {
        Ok(manifest) if !manifest.is_newer_than(version) => News::Nothing(None),
        Ok(manifest) if !update::may_fetch(&manifest.url, trial) => News::Nothing(Some(failure(version, source, "the new version is not at an https address"))),
        Ok(manifest) => News::Offered(manifest),
        Err(error) => News::Nothing(trial.then(|| failure(version, source, &error))),
    }
}

pub fn start_again(options: &Options, from: &str) -> std::io::Result<()> {
    std::process::Command::new(std::env::current_exe()?).args(told_when_started_again(options, from)).spawn().map(|_| ())
}

fn told_when_started_again(options: &Options, from: &str) -> Vec<OsString> {
    let mut told: Vec<OsString> = vec!["--updated-from".into(), from.into()];
    if let Some(dota) = &options.dota {
        told.extend(["--dota".into(), dota.clone().into_os_string()]);
    }
    if let Some(data_dir) = &options.data_dir {
        told.extend(["--data-dir".into(), data_dir.clone().into_os_string()]);
    }
    if options.ignore_running {
        told.push("--ignore-running".into());
    }
    if options.no_start {
        told.push("--no-start".into());
    }
    if let Some([width, height]) = options.size {
        told.extend(["--size".into(), format!("{width}x{height}").into()]);
    }
    if let Some(shot) = &options.screenshot {
        told.extend(["--screenshot".into(), picture_after(shot).into_os_string()]);
    }
    told
}

fn picture_after(shot: &Path) -> PathBuf {
    shot.with_extension("restarted.png")
}

pub fn clean_up() {
    const TRIES: u32 = 10;
    const PAUSE: Duration = Duration::from_millis(500);
    let Ok(app) = std::env::current_exe() else { return };
    if update::clean_up(&app) {
        return;
    }
    spawn("update cleanup", move || {
        for _ in 0..TRIES {
            std::thread::sleep(PAUSE);
            if update::clear_replaced(&app) {
                return;
            }
        }
    });
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new().name(name.to_string()).spawn(work).expect("a thread can be started");
}

fn installing_in_plain_words(error: &dme_core::Error) -> Option<&'static str> {
    let dme_core::Error::Io { action, source, .. } = error else { return None };
    if *action == "write" && source.raw_os_error() == Some(5) {
        Some("The update could not be put in place: Windows does not let the app write in its own folder. Move the app to a folder of your own, such as Documents or Downloads, then try again.")
    } else if fsutil::is_held(source) {
        Some("The update could not be put in place: another program, such as an antivirus, is holding the app's file. Try again in a moment. This version goes on working as it is.")
    } else if fsutil::is_disk_full(source) {
        Some("The update could not be put in place: the disk is full. Make some room on it, then try again.")
    } else {
        None
    }
}

fn failure(version: &str, address: &str, error: &str) -> Failure {
    Failure {
        sentence: format!("The update did not work: {error}. This version goes on working as it is."),
        details: format!("Dota 2 Mod Enjoyer {version}\nWhile updating from {address}: {error}"),
    }
}

fn fetch(address: &str, limit: usize, testing: bool) -> Result<Vec<u8>, String> {
    fetch_counting(address, limit, testing, None)
}

fn fetch_counting(address: &str, limit: usize, testing: bool, mut come: Option<&mut dyn FnMut(u64)>) -> Result<Vec<u8>, String> {
    if !update::may_fetch(address, testing) {
        return Err(format!("{address} is not an https address"));
    }
    if update::is_at(address, "https://") || update::is_at(address, "http://") {
        return http::get(address, limit, &mut |bytes| {
            if let Some(come) = come.as_mut() {
                come(bytes);
            }
        });
    }
    let file = std::fs::read(PathBuf::from(address)).map_err(|error| format!("{address} could not be read: {error}"))?;
    if file.len() > limit {
        return Err(too_large(limit));
    }
    if let Some(come) = come {
        for step in 1..=TRIAL_STEPS {
            std::thread::sleep(TRIAL_STEP);
            come(file.len() as u64 * step / TRIAL_STEPS);
        }
    }
    Ok(file)
}

fn too_large(limit: usize) -> String {
    format!("what came is larger than the {limit} bytes it could be at most")
}

#[cfg(windows)]
mod http {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinHttp::{
        URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_INTERNET_SCHEME_HTTPS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    };

    const PATIENCE: i32 = 20_000;

    struct Handle(*mut c_void);

    impl Handle {
        fn of(handle: *mut c_void, what: &str) -> Result<Handle, String> {
            if handle.is_null() { Err(failed(what)) } else { Ok(Handle(handle)) }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { WinHttpCloseHandle(self.0) };
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn failed(what: &str) -> String {
        format!("{what} did not work (Windows error {})", unsafe { GetLastError() })
    }

    pub fn get(address: &str, limit: usize, come: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
        let address_wide = wide(address);
        let (mut host, mut path, mut query) = ([0u16; 256], [0u16; 2048], [0u16; 2048]);
        let mut parts: URL_COMPONENTS = unsafe { std::mem::zeroed() };
        parts.dwStructSize = size_of::<URL_COMPONENTS>() as u32;
        (parts.lpszHostName, parts.dwHostNameLength) = (host.as_mut_ptr(), host.len() as u32);
        (parts.lpszUrlPath, parts.dwUrlPathLength) = (path.as_mut_ptr(), path.len() as u32);
        (parts.lpszExtraInfo, parts.dwExtraInfoLength) = (query.as_mut_ptr(), query.len() as u32);
        if unsafe { WinHttpCrackUrl(address_wide.as_ptr(), 0, 0, &mut parts) } == 0 {
            return Err(failed("reading the address"));
        }
        let mut object: Vec<u16> = path.iter().copied().take_while(|&unit| unit != 0).chain(query.iter().copied().take_while(|&unit| unit != 0)).collect();
        object.push(0);
        let secure = if parts.nScheme == WINHTTP_INTERNET_SCHEME_HTTPS { WINHTTP_FLAG_SECURE } else { 0 };

        let agent = wide(concat!("Dota2ModEnjoyer/", env!("CARGO_PKG_VERSION")));
        unsafe {
            let session = Handle::of(WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, null(), null(), 0), "getting ready to reach the internet")?;
            WinHttpSetTimeouts(session.0, PATIENCE, PATIENCE, PATIENCE, PATIENCE);
            let connection = Handle::of(WinHttpConnect(session.0, host.as_ptr(), parts.nPort, 0), "reaching the server")?;
            let request = Handle::of(WinHttpOpenRequest(connection.0, wide("GET").as_ptr(), object.as_ptr(), null(), null(), null(), secure), "asking the server")?;
            if WinHttpSendRequest(request.0, null(), 0, null(), 0, 0, 0) == 0 {
                return Err(failed("reaching the server"));
            }
            if WinHttpReceiveResponse(request.0, null_mut()) == 0 {
                return Err(failed("waiting for the server's answer"));
            }
            let (mut status, mut size) = (0u32, size_of::<u32>() as u32);
            let level = WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER;
            if WinHttpQueryHeaders(request.0, level, null(), (&raw mut status).cast(), &mut size, null_mut()) == 0 {
                return Err(failed("reading the server's answer"));
            }
            if status != 200 {
                return Err(format!("the server answered {status} where 200 says that all is well"));
            }
            let mut body = Vec::new();
            loop {
                let mut coming = 0u32;
                if WinHttpQueryDataAvailable(request.0, &mut coming) == 0 {
                    return Err(failed("receiving"));
                }
                if coming == 0 {
                    return Ok(body);
                }
                let have = body.len();
                if have + coming as usize > limit {
                    return Err(super::too_large(limit));
                }
                body.resize(have + coming as usize, 0);
                let mut got = 0u32;
                if WinHttpReadData(request.0, body[have..].as_mut_ptr().cast(), coming, &mut got) == 0 {
                    return Err(failed("receiving"));
                }
                body.truncate(have + got as usize);
                come(body.len() as u64);
            }
        }
    }
}

#[cfg(not(windows))]
mod http {
    pub fn get(_address: &str, _limit: usize, _come: &mut dyn FnMut(u64)) -> Result<Vec<u8>, String> {
        Err("this app fetches its updates on Windows only".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_that_cannot_be_put_in_place_says_what_to_do_in_plain_words() {
        let app = Path::new(r"C:\Program Files\Dota2ModEnjoyer\Dota2ModEnjoyer.exe");
        let io = |action: &'static str, code: i32| dme_core::Error::io(action, app, std::io::Error::from_raw_os_error(code));
        assert!(installing_in_plain_words(&io("write", 5)).is_some_and(|said| said.contains("Move the app to a folder of your own")));
        assert!(installing_in_plain_words(&io("replace", 32)).is_some_and(|said| said.contains("antivirus")));
        assert!(installing_in_plain_words(&io("move aside", 5)).is_some_and(|said| said.contains("antivirus")));
        assert!(installing_in_plain_words(&io("write", 112)).is_some_and(|said| said.contains("disk is full")));
        assert_eq!(installing_in_plain_words(&io("write", 3)), None, "said as it is");
        assert_eq!(installing_in_plain_words(&dme_core::Error::Refused("no".to_string())), None);
    }

    #[test]
    fn a_test_may_fetch_from_a_file_and_a_player_may_not() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update.json");
        std::fs::write(&file, b"twelve bytes").unwrap();
        let address = file.to_string_lossy().into_owned();
        assert_eq!(fetch(&address, 64, true).unwrap(), b"twelve bytes");
        assert_eq!(fetch(&address, 12, true).unwrap().len(), 12);
        assert!(fetch(&address, 11, true).unwrap_err().contains("larger"), "more than was asked for is refused");
        assert!(fetch(&address, 64, false).unwrap_err().contains("not an https address"));
        assert!(fetch("http://127.0.0.1:1/update.json", 64, false).unwrap_err().contains("not an https address"), "nothing is asked of the network");
        assert!(fetch(&dir.path().join("missing.json").to_string_lossy(), 64, true).is_err());
    }

    #[test]
    fn a_build_that_asks_nobody_has_no_updater() {
        let ctx = egui::Context::default();
        if update::MANIFEST_URL.is_empty() && built_in_trial().is_none() {
            assert!(Updater::start(&Options::default(), &ctx).is_none());
        }
        let shot = Options { screenshot: Some(PathBuf::from("shot.png")), ..Options::default() };
        assert!(Updater::start(&shot, &ctx).is_none());
    }

    fn asking_at(manifest: &Path, believes: &str) -> Updater {
        let options = Options { update_from: Some(manifest.to_string_lossy().into_owned()), pretend_version: Some(believes.to_string()), ..Options::default() };
        Updater::start(&options, &egui::Context::default()).unwrap()
    }

    fn next(updater: &mut Updater) -> News {
        let started = std::time::Instant::now();
        loop {
            if let Some(news) = updater.poll() {
                return news;
            }
            assert!(started.elapsed().as_secs() < 20, "the thread never answered");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_newer_version_in_a_manifest_is_offered_and_an_older_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("update.json");
        let release = Manifest::describe(b"the new app", "0.5.0", &dir.path().join("new.exe").to_string_lossy());
        std::fs::write(&manifest, release.to_json()).unwrap();
        let told = |believes: &str| {
            let mut updater = asking_at(&manifest, believes);
            assert!(updater.is_working() && !updater.is_fetching() && updater.offer().is_none());
            assert_eq!((updater.version(), updater.trial_source()), (believes, Some(&*manifest.to_string_lossy())));
            let news = next(&mut updater);
            assert!(!updater.is_working());
            (news, updater.offer().cloned())
        };
        assert!(matches!(told("0.4.9"), (News::Offered(offered), Some(kept)) if offered == release && kept == release));
        assert!(matches!(told("0.5.0"), (News::Nothing(None), None)), "the same version is no news");
        assert!(matches!(told("0.6.0"), (News::Nothing(None), None)));

        std::fs::write(&manifest, b"<html>404</html>").unwrap();
        assert!(matches!(told("0.1.0"), (News::Nothing(Some(failure)), None) if failure.details.contains("manifest")));
    }

    #[test]
    fn a_release_that_comes_out_while_the_app_is_open_is_offered_once() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("update.json");
        let release = |version: &str| Manifest::describe(b"the new app", version, &dir.path().join("new.exe").to_string_lossy());
        let publish = |version: &str| std::fs::write(&manifest, release(version).to_json()).unwrap();
        let quiet_for_a_while = |updater: &mut Updater| {
            std::thread::sleep(TRIAL_ASK_EVERY * 6);
            updater.poll().is_none()
        };

        publish("0.5.0");
        let mut updater = asking_at(&manifest, "0.5.0");
        assert!(matches!(next(&mut updater), News::Nothing(None)));
        assert!(quiet_for_a_while(&mut updater), "that nothing is new is said once");

        publish("0.6.0");
        assert!(matches!(next(&mut updater), News::Offered(offered) if offered.version == "0.6.0"));
        assert_eq!(updater.offer().map(|offer| offer.version.as_str()), Some("0.6.0"));
        assert!(quiet_for_a_while(&mut updater), "and hears of it once, however often it asks");

        std::fs::write(&manifest, b"half a manif").unwrap();
        assert!(quiet_for_a_while(&mut updater));
        assert!(updater.offer().is_some());
        publish("0.7.0");
        assert!(matches!(next(&mut updater), News::Offered(offered) if offered.version == "0.7.0"));
        assert_eq!(updater.offer().map(|offer| offer.version.as_str()), Some("0.7.0"));
    }

    #[test]
    fn how_much_of_a_new_version_has_come_is_told_as_it_comes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("new.exe");
        std::fs::write(&file, vec![7u8; 1000]).unwrap();
        let address = file.to_string_lossy().into_owned();
        let mut told = Vec::new();
        let fetched = fetch_counting(&address, 4096, true, Some(&mut |bytes| told.push(bytes))).unwrap();
        assert_eq!(fetched.len(), 1000);
        assert_eq!(told.len() as u64, TRIAL_STEPS);
        assert!(told.windows(2).all(|pair| pair[0] < pair[1]) && told.last() == Some(&1000), "{told:?}");
        assert_eq!(fetch(&address, 4096, true).unwrap(), fetched);

        let mut hundredths = Hundredths { all: 1000, told: None };
        let progress = |news: Option<News>| match news {
            Some(News::Progress(come, all)) => Some((come, all)),
            _ => None,
        };
        assert_eq!(progress(hundredths.news(0)), Some((0, 1000)));
        assert_eq!(progress(hundredths.news(9)), None, "less than a hundredth more");
        assert_eq!(progress(hundredths.news(10)), Some((10, 1000)));
        assert_eq!(progress(hundredths.news(999)), Some((999, 1000)));
        assert_eq!(progress(hundredths.news(1200)), Some((1000, 1000)), "never more than there is");
        assert_eq!(progress(Hundredths { all: 0, told: None }.news(0)), Some((0, 0)));
    }

    #[test]
    fn the_window_is_told_how_far_the_new_version_has_come_while_it_comes() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("update.json");
        let release = Manifest::describe(b"the new app", "0.5.0", &dir.path().join("new.exe").to_string_lossy());
        std::fs::write(&manifest, release.to_json()).unwrap();
        let mut updater = asking_at(&manifest, "0.4.9");
        assert!(matches!(next(&mut updater), News::Offered(_)));
        assert_eq!(updater.progress(), None, "nothing is coming yet");

        updater.fetching = true;
        assert_eq!(updater.progress(), None, "how much there is to come is not known yet");
        updater.outbox.send(News::Progress(250, 1000)).unwrap();
        assert!(matches!(next(&mut updater), News::Progress(250, 1000)));
        assert_eq!(updater.progress(), Some(0.25));
        updater.outbox.send(News::Failed(failure("0.4.9", "here", "it broke"))).unwrap();
        assert!(matches!(next(&mut updater), News::Failed(_)));
        assert_eq!(updater.progress(), None);
        assert!(!updater.is_fetching());
    }

    #[test]
    fn a_build_for_trying_updates_out_asks_where_it_was_told_to_when_it_was_made() {
        let Some(built_in) = built_in_trial() else { return };
        let ctx = egui::Context::default();
        let updater = Updater::start(&Options::default(), &ctx).expect("such a build asks");
        assert_eq!(updater.trial_source(), Some(built_in));
        let elsewhere = Options { update_from: Some("C:\\elsewhere\\update.json".to_string()), ..Options::default() };
        assert_eq!(Updater::start(&elsewhere, &ctx).unwrap().trial_source(), Some("C:\\elsewhere\\update.json"));
    }

    #[test]
    fn the_version_an_update_starts_is_told_what_it_replaced_and_nothing_else_for_a_player() {
        let told = |options: &Options| -> Vec<String> {
            told_when_started_again(options, "0.1.0").iter().map(|word| word.to_string_lossy().into_owned()).collect()
        };
        assert_eq!(told(&Options::default()), ["--updated-from", "0.1.0"]);
        let read = |options: &Options| Options::read(told(options));
        assert_eq!(read(&Options::default()), Options { updated_from: Some("0.1.0".to_string()), ..Options::default() });

        let testing = Options {
            dota: Some(PathBuf::from("C:\\a copy\\dota 2 beta")),
            data_dir: Some(PathBuf::from("D:\\data")),
            ignore_running: true,
            no_start: true,
            size: Some([1280.0, 820.0]),
            screenshot: Some(PathBuf::from("shots\\update.png")),
            open: Some("help".to_string()),
            steps: vec![crate::script::Step::Wait(3.0)],
            update_from: Some("C:\\release\\update.json".to_string()),
            pretend_version: Some("0.0.1".to_string()),
            ..Options::default()
        };
        let passed_on = Options {
            dota: testing.dota.clone(),
            data_dir: testing.data_dir.clone(),
            ignore_running: true,
            no_start: true,
            size: testing.size,
            screenshot: Some(PathBuf::from("shots\\update.restarted.png")),
            updated_from: Some("0.1.0".to_string()),
            ..Options::default()
        };
        assert_eq!(read(&testing), passed_on);
        assert_eq!(picture_after(Path::new("update")), PathBuf::from("update.restarted.png"));
    }
}
