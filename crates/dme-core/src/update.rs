use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_compact::{KeyPair, PublicKey, Seed, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::fsutil;

pub const MANIFEST_URL: &str = "https://github.com/elangrr/Dota2ModEnjoyer/releases/latest/download/update.json";

pub const PUBLIC_KEY: &str = "18869cb71458361275235598f55e20a12751bd1c578ebfdcd8590c0e4132799e";

pub const MANIFEST_LIMIT: usize = 64 * 1024;
pub const RELEASE_LIMIT: usize = 256 * 1024 * 1024;

const WHAT: &str = "the update's manifest";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Manifest> {
        let manifest: Manifest = serde_json::from_slice(bytes).map_err(|error| Error::format(WHAT, error.to_string()))?;
        let wrong = |detail: &str| Err(Error::format(WHAT, detail));
        if numbers(&manifest.version).is_none() {
            return wrong("its version is not numbers with dots between them");
        }
        if unhex(&manifest.sha256).is_none_or(|checksum| checksum.len() != 32) {
            return wrong("its checksum is not 64 hex digits");
        }
        if manifest.size == 0 || manifest.size > RELEASE_LIMIT as u64 {
            return wrong("its size is not that of an app");
        }
        if manifest.url.trim().is_empty() {
            return wrong("it does not say where the new version is");
        }
        Ok(manifest)
    }

    pub fn describe(file: &[u8], version: &str, url: &str) -> Manifest {
        Manifest { version: version.to_string(), url: url.to_string(), sha256: sha256_hex(file), size: file.len() as u64, signature: None, notes: None }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a manifest is plain data") + "\n"
    }

    pub fn is_newer_than(&self, version: &str) -> bool {
        is_newer(&self.version, version)
    }

    pub fn check_file(&self, file: &[u8]) -> Result<()> {
        if file.len() as u64 == self.size && sha256_hex(file).eq_ignore_ascii_case(self.sha256.trim()) {
            Ok(())
        } else {
            Err(Error::Refused("the file that was downloaded is not the one the new version's description names, so it was not used".to_string()))
        }
    }

    pub fn check_signature(&self, public_key: &str) -> Result<()> {
        if public_key.trim().is_empty() {
            return Ok(());
        }
        let refused = || Error::Refused("the new version is not signed by the makers of this app, so it was not used".to_string());
        let key = unhex(public_key).and_then(|key| PublicKey::from_slice(&key).ok());
        let key = key.ok_or_else(|| Error::format("this app's key for updates", "it is not 64 hex digits"))?;
        let signature = self.signature.as_deref().and_then(unhex).and_then(|signature| Signature::from_slice(&signature).ok());
        key.verify(self.signed(), &signature.ok_or_else(refused)?).map_err(|_| refused())
    }

    pub fn sign(&mut self, secret_key: &str) -> Result<String> {
        let seed = unhex(secret_key).and_then(|seed| Seed::from_slice(&seed).ok());
        let pair = KeyPair::from_seed(seed.ok_or_else(|| Error::format("the secret key", "it is not 64 hex digits"))?);
        self.signature = Some(hex(pair.sk.sign(self.signed(), None).as_ref()));
        Ok(hex(pair.pk.as_ref()))
    }

    fn signed(&self) -> String {
        format!("Dota2ModEnjoyer\n{}\n{}\n{}", self.version.trim(), self.sha256.trim().to_ascii_lowercase(), self.size)
    }
}

pub fn new_key() -> (String, String) {
    let pair = KeyPair::from_seed(Seed::generate());
    (hex(pair.pk.as_ref()), hex(pair.sk.seed().as_ref()))
}

pub fn may_fetch(address: &str, testing: bool) -> bool {
    testing || is_at(address, "https://")
}

pub fn is_at(address: &str, kind: &str) -> bool {
    address.get(..kind.len()).is_some_and(|start| start.eq_ignore_ascii_case(kind))
}

pub fn is_newer(version: &str, than: &str) -> bool {
    let (mut one, mut other) = (numbers(version).unwrap_or_default(), numbers(than).unwrap_or_default());
    let length = one.len().max(other.len());
    one.resize(length, 0);
    other.resize(length, 0);
    one > other
}

pub fn is_version(text: &str) -> bool {
    numbers(text).is_some()
}

fn numbers(version: &str) -> Option<Vec<u64>> {
    let version = version.trim();
    let version = version.strip_prefix(['v', 'V']).unwrap_or(version);
    let read = |part: &str| part.bytes().all(|digit| digit.is_ascii_digit()).then(|| part.parse().ok()).flatten();
    version.split('.').map(read).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let digits: Option<Vec<u8>> = text.trim().chars().map(|digit| digit.to_digit(16).map(|value| value as u8)).collect();
    let digits = digits?;
    digits.len().is_multiple_of(2).then(|| digits.chunks(2).map(|pair| pair[0] * 16 + pair[1]).collect())
}

const INCOMING: &str = ".new";
const OUTGOING: &str = ".old";
const MOST_ASIDE: u32 = 9;

fn beside(app: &Path, ending: &str) -> PathBuf {
    let mut name = app.file_name().unwrap_or_default().to_os_string();
    name.push(ending);
    app.with_file_name(name)
}

fn further_aside(app: &Path, number: u32) -> PathBuf {
    beside(app, &format!("{OUTGOING}.{number}"))
}

fn is_gone(deleted: std::io::Result<()>) -> bool {
    match deleted {
        Ok(()) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

fn make_room(app: &Path) -> std::io::Result<()> {
    let outgoing = beside(app, OUTGOING);
    let error = match fs::remove_file(&outgoing) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => error,
        _ => return Ok(()),
    };
    let moved = (1..=MOST_ASIDE).map(|number| further_aside(app, number)).any(|further| is_gone(fs::remove_file(&further)) && fs::rename(&outgoing, &further).is_ok());
    if moved { Ok(()) } else { Err(error) }
}

pub fn install(app: &Path, new: &[u8]) -> Result<()> {
    let (incoming, outgoing) = (beside(app, INCOMING), beside(app, OUTGOING));
    let written = (|| {
        let mut file = fs::File::create(&incoming)?;
        file.write_all(new)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&incoming);
        return Err(Error::io("write", &incoming, error));
    }
    let give_up = |action: &'static str, path: &Path, error: std::io::Error| {
        let _ = fs::remove_file(&incoming);
        Err(Error::io(action, path, error))
    };
    if let Err(error) = make_room(app) {
        return give_up("delete", &outgoing, error);
    }
    if let Err(error) = fsutil::patiently(|| fs::rename(app, &outgoing)) {
        return give_up("move aside", app, error);
    }
    if let Err(error) = fsutil::patiently(|| fs::rename(&incoming, app)) {
        let _ = fs::rename(&outgoing, app);
        return give_up("replace", app, error);
    }
    Ok(())
}

pub fn clean_up(app: &Path) -> bool {
    let _ = fs::remove_file(beside(app, INCOMING));
    for number in 1..=MOST_ASIDE {
        let _ = fs::remove_file(further_aside(app, number));
    }
    clear_replaced(app)
}

pub fn clear_replaced(app: &Path) -> bool {
    is_gone(fs::remove_file(beside(app, OUTGOING)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn release() -> Manifest {
        Manifest::describe(b"abc", "0.2.0", "https://example.org/releases/Dota2ModEnjoyer.exe")
    }

    #[test]
    fn a_release_is_described_by_its_version_its_size_and_its_checksum() {
        assert_eq!(sha256_hex(b"abc"), ABC);
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let manifest = release();
        assert_eq!((manifest.version.as_str(), manifest.size, manifest.sha256.as_str()), ("0.2.0", 3, ABC));
        assert_eq!((manifest.signature.as_ref(), manifest.notes.as_ref()), (None, None));

        let json = manifest.to_json();
        assert_eq!(
            json,
            format!("{{\n  \"version\": \"0.2.0\",\n  \"url\": \"https://example.org/releases/Dota2ModEnjoyer.exe\",\n  \"sha256\": \"{ABC}\",\n  \"size\": 3\n}}\n")
        );
        assert_eq!(Manifest::parse(json.as_bytes()).unwrap(), manifest);
        let with_notes = Manifest { notes: Some("Emblems, and a fix for the trees.".to_string()), ..manifest };
        assert_eq!(Manifest::parse(with_notes.to_json().as_bytes()).unwrap(), with_notes);
        let more = json.replacen('{', "{ \"min_windows\": 10,", 1);
        assert_eq!(Manifest::parse(more.as_bytes()).unwrap(), release());
    }

    #[test]
    fn a_manifest_that_does_not_say_what_a_release_has_to_say_is_refused() {
        let with = |change: &dyn Fn(&mut Manifest)| {
            let mut manifest = release();
            change(&mut manifest);
            Manifest::parse(manifest.to_json().as_bytes())
        };
        assert!(with(&|_| {}).is_ok());
        for version in ["", "new", "1.x", "1..2", "-1.0", "1.0-beta"] {
            assert!(matches!(with(&|manifest| manifest.version = version.to_string()), Err(Error::Format { .. })), "{version:?}");
        }
        assert!(with(&|manifest| manifest.version = "v1.0".to_string()).is_ok());
        assert!(with(&|manifest| manifest.sha256 = "abc".to_string()).is_err());
        assert!(with(&|manifest| manifest.sha256 = "z".repeat(64)).is_err());
        assert!(with(&|manifest| manifest.sha256 = ABC.to_uppercase()).is_ok());
        assert!(with(&|manifest| manifest.size = 0).is_err());
        assert!(with(&|manifest| manifest.size = RELEASE_LIMIT as u64 + 1).is_err());
        assert!(with(&|manifest| manifest.url = " ".to_string()).is_err());
        for odd in ["", "not json", "[]", "{}", "{\"version\": \"1.0\"}", "<html>404</html>"] {
            assert!(matches!(Manifest::parse(odd.as_bytes()), Err(Error::Format { .. })), "{odd:?}");
        }
    }

    #[test]
    fn a_version_is_newer_by_its_numbers() {
        let offers = |offered: &str, has: &str| Manifest { version: offered.to_string(), ..release() }.is_newer_than(has);
        assert!(offers("0.2.0", "0.1.0"));
        assert!(offers("0.10.0", "0.9.9"), "by number, not by letter");
        assert!(offers("1.0", "0.99.99"));
        assert!(offers("0.1.0.1", "0.1.0"));
        assert!(offers("v0.2.0", "0.1.9"));
        assert!(!offers("0.1.0", "0.1.0"), "the same version is no news");
        assert!(!offers("0.1", "0.1.0") && !offers("0.1.0", "0.1"));
        assert!(!offers("0.2.2.0.0", "0.2.2") && !offers("0.2.2", "0.2.2.0.0"), "however many numbers are missing");
        assert!(!offers("0.1.0", "0.2.0"), "an older one is not put in the place of a newer one");
        assert!(offers("0.0.1", "what is this"), "a version that cannot be read is older than any");
        assert_eq!(numbers(" v1.20.3 "), Some(vec![1, 20, 3]));
        assert!(is_newer("0.2.3", "0.2.2") && !is_newer("0.2.2", "0.2.3") && !is_newer("0.2.3", "0.2.3"));
        assert!(is_version("0.2.3") && is_version("v1") && !is_version("") && !is_version("0.2.x"));
    }

    #[test]
    fn a_downloaded_file_has_to_be_the_one_the_manifest_describes() {
        let manifest = release();
        assert!(manifest.check_file(b"abc").is_ok());
        assert!(Manifest { sha256: ABC.to_uppercase(), ..release() }.check_file(b"abc").is_ok());
        for other in [b"abd".as_slice(), b"abcd", b"ab", b""] {
            assert!(matches!(manifest.check_file(other), Err(Error::Refused(_))), "{other:?}");
        }
        assert!(Manifest { size: 4, ..release() }.check_file(b"abc").is_err(), "the right bytes, and not as many as it says");
    }

    #[test]
    fn a_release_is_signed_with_a_key_that_the_app_has_the_public_half_of() {
        let (public, secret) = new_key();
        assert_eq!((public.len(), secret.len()), (64, 64));
        let mut manifest = release();
        assert_eq!(manifest.sign(&secret).unwrap(), public, "signing tells the public half");
        assert_eq!(manifest.signature.as_ref().unwrap().len(), 128);
        assert!(manifest.check_signature(&public).is_ok());
        assert!(manifest.check_signature(&public.to_uppercase()).is_ok());
        let mut again = release();
        again.sign(&secret).unwrap();
        assert_eq!(again, manifest);
        assert!(Manifest::parse(manifest.to_json().as_bytes()).unwrap().check_signature(&public).is_ok());

        let (other_public, other_secret) = new_key();
        assert!(matches!(manifest.check_signature(&other_public), Err(Error::Refused(_))));
        assert!(matches!(release().check_signature(&public), Err(Error::Refused(_))));
        assert!(Manifest { signature: Some("00".repeat(64)), ..release() }.check_signature(&public).is_err());
        assert!(Manifest { signature: Some("signed, honestly".to_string()), ..release() }.check_signature(&public).is_err());
        let mut forged = release();
        forged.sign(&other_secret).unwrap();
        assert!(forged.check_signature(&public).is_err());
        assert!(Manifest { version: "0.3.0".to_string(), ..manifest.clone() }.check_signature(&public).is_err(), "an old file as a new version");
        assert!(Manifest { sha256: sha256_hex(b"abd"), ..manifest.clone() }.check_signature(&public).is_err());
        assert!(Manifest { size: 4, ..manifest.clone() }.check_signature(&public).is_err());
        assert!(Manifest { url: "https://elsewhere.example/app.exe".to_string(), notes: Some("New.".to_string()), ..manifest.clone() }.check_signature(&public).is_ok());

        assert!(release().check_signature("").is_ok());
        assert!(manifest.check_signature(" ").is_ok());
        assert!(matches!(manifest.check_signature("abc"), Err(Error::Format { .. })));
        assert!(matches!(release().sign("abc"), Err(Error::Format { .. })));
    }

    #[test]
    fn a_build_for_players_fetches_over_https_only() {
        assert!(may_fetch("https://example.org/update.json", false));
        assert!(may_fetch("HTTPS://example.org/update.json", false));
        for other in ["http://example.org/update.json", "C:\\updates\\update.json", "file:///C:/update.json", "ftp://example.org/x", "", "https"] {
            assert!(!may_fetch(other, false), "{other:?}");
            assert!(may_fetch(other, true), "when testing: {other:?}");
        }
        assert!(is_at("http://127.0.0.1:8000/x", "http://") && !is_at("https://x", "http://"));
    }

    #[test]
    fn what_is_built_in_for_updates_can_be_used() {
        assert!(MANIFEST_URL.is_empty() || may_fetch(MANIFEST_URL, false));
        assert!(PUBLIC_KEY.is_empty() || unhex(PUBLIC_KEY).is_some_and(|key| PublicKey::from_slice(&key).is_ok()));
    }

    #[test]
    fn the_new_version_takes_the_place_of_the_file_the_app_runs_from() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Dota2ModEnjoyer.exe");
        let (old, new) = (dir.path().join("Dota2ModEnjoyer.exe.old"), dir.path().join("Dota2ModEnjoyer.exe.new"));
        fs::write(&app, b"version one").unwrap();

        install(&app, b"version two").unwrap();
        assert_eq!(fs::read(&app).unwrap(), b"version two");
        assert_eq!(fs::read(&old).unwrap(), b"version one", "the version that was running is kept until the new one has started");
        assert!(!new.exists());

        fs::write(&new, b"half of version three").unwrap();
        assert!(clean_up(&app));
        assert!(!old.exists() && !new.exists() && app.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(clean_up(&app) && clear_replaced(&app), "nothing is wrong if there is nothing");

        install(&app, b"version three").unwrap();
        install(&app, b"version four").unwrap();
        assert_eq!(fs::read(&app).unwrap(), b"version four");
        assert_eq!(fs::read(&old).unwrap(), b"version three");
        assert!(clear_replaced(&app) && !old.exists());

        let gone = dir.path().join("elsewhere").join("Dota2ModEnjoyer.exe");
        assert!(matches!(install(&gone, b"x"), Err(Error::Io { .. })));
        let missing = dir.path().join("Missing.exe");
        assert!(matches!(install(&missing, b"x"), Err(Error::Io { .. })));
        assert!(!dir.path().join("Missing.exe.new").exists() && !missing.exists());
    }

    #[test]
    fn a_version_still_running_in_another_window_does_not_stand_in_the_way() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Dota2ModEnjoyer.exe");
        let beside = |ending: &str| dir.path().join(format!("Dota2ModEnjoyer.exe{ending}"));
        let (old, new) = (beside(".old"), beside(".new"));
        let running = |path: &Path| {
            let _ = fs::remove_file(path);
            fs::create_dir(path).unwrap();
            fs::write(path.join("held"), b"").unwrap();
        };
        let stopped = |path: &Path| {
            fs::remove_dir_all(path).unwrap();
            fs::write(path, b"a version nobody runs any more").unwrap();
        };
        fs::write(&app, b"version one").unwrap();

        running(&old);
        assert!(!clear_replaced(&app) && !clean_up(&app));
        install(&app, b"version two").unwrap();
        assert_eq!(fs::read(&app).unwrap(), b"version two");
        assert_eq!(fs::read(&old).unwrap(), b"version one");
        assert!(beside(".old.1").join("held").exists() && !new.exists());

        running(&old);
        install(&app, b"version three").unwrap();
        assert_eq!(fs::read(&app).unwrap(), b"version three");
        assert_eq!(fs::read(&old).unwrap(), b"version two");
        assert!(beside(".old.1").is_dir() && beside(".old.2").is_dir());

        stopped(&beside(".old.1"));
        assert!(clean_up(&app), "the version replaced last is gone");
        assert!(!beside(".old.1").exists() && beside(".old.2").is_dir() && !old.exists());
        stopped(&beside(".old.2"));
        assert!(clean_up(&app));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "only the app is left");

        running(&old);
        for number in 1..=MOST_ASIDE {
            running(&further_aside(&app, number));
        }
        assert!(matches!(install(&app, b"version four"), Err(Error::Io { .. })));
        assert_eq!(fs::read(&app).unwrap(), b"version three");
        assert!(!new.exists() && old.is_dir());
    }
}
