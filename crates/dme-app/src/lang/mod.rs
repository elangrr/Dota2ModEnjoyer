use std::fmt::Display;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

macro_rules! tr {
    (@value $name:ident) => { $name };
    (@value $name:ident = $value:expr) => { $value };
    ($key:ident) => { $crate::lang::t($crate::lang::T::$key) };
    ($key:ident, $($name:ident $(= $value:expr)?),+ $(,)?) => {
        $crate::lang::fill($crate::lang::t($crate::lang::T::$key), &[$((stringify!($name), &tr!(@value $name $(= $value)?) as &dyn ::std::fmt::Display)),+])
    };
}

macro_rules! tr_en {
    ($key:ident) => { $crate::lang::text_in($crate::lang::LANGUAGES[0], $crate::lang::T::$key) };
    ($key:ident, $($name:ident $(= $value:expr)?),+ $(,)?) => {
        $crate::lang::fill_in($crate::lang::LANGUAGES[0], $crate::lang::text_in($crate::lang::LANGUAGES[0], $crate::lang::T::$key), &[$((stringify!($name), &tr!(@value $name $(= $value)?) as &dyn ::std::fmt::Display)),+])
    };
}

macro_rules! texts {
    ($($(#[$note:meta])* $key:ident => $text:expr,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum T {
            $($(#[$note])* $key,)*
        }

        pub const ENGLISH: &[(T, &str)] = &[$((T::$key, $text),)*];
    };
}

macro_rules! translated {
    ($($key:ident => $text:expr,)*) => {
        &[$((crate::lang::T::$key, $text),)*]
    };
}

macro_rules! languages {
    ($($code:ident,)+) => {
        $(mod $code;)+
        pub const LANGUAGES: &[&Language] = &[$(&$code::LANGUAGE),+];
    };
}

languages! {
    en,
    ru,
}

pub mod words;

pub use en::T;

pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
    pub windows: &'static [&'static str],
    pub game: &'static str,
    #[allow(dead_code, reason = "read by the test of the texts")]
    pub forms: usize,
    pub plural: fn(f64) -> usize,
    pub decimal: char,
    pub letters: Letters,
    pub texts: &'static [(T, &'static str)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Letters {
    InterHasThem,
    Han,
}

pub fn one_and_more(number: f64) -> usize {
    usize::from(number != 1.0)
}

pub fn slavic(number: f64) -> usize {
    if number.fract() != 0.0 {
        return 1;
    }
    let whole = number.abs() as u64;
    match (whole % 10, whole % 100) {
        (1, rest) if rest != 11 => 0,
        (2..=4, rest) if !(12..=14).contains(&rest) => 1,
        _ => 2,
    }
}

#[allow(dead_code, reason = "for the languages that need it")]
pub fn one_form(_number: f64) -> usize {
    0
}

static CURRENT: AtomicUsize = AtomicUsize::new(0);

fn tables() -> &'static [Vec<&'static str>] {
    static TABLES: OnceLock<Vec<Vec<&'static str>>> = OnceLock::new();
    TABLES.get_or_init(|| {
        LANGUAGES
            .iter()
            .map(|language| {
                let mut table: Vec<&'static str> = en::ENGLISH.iter().map(|(_, text)| *text).collect();
                for (key, text) in language.texts {
                    table[*key as usize] = text;
                }
                table
            })
            .collect()
    })
}

pub fn current() -> &'static Language {
    LANGUAGES[CURRENT.load(Ordering::Relaxed)]
}

pub fn choose(code: &str) {
    let place = LANGUAGES.iter().position(|language| language.code == code).unwrap_or(0);
    CURRENT.store(place, Ordering::Relaxed);
}

pub fn by_code(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().copied().find(|language| language.code == code)
}

pub fn t(key: T) -> &'static str {
    tables()[CURRENT.load(Ordering::Relaxed)][key as usize]
}

pub fn text_in(language: &Language, key: T) -> &'static str {
    let place = LANGUAGES.iter().position(|known| std::ptr::eq(*known, language)).unwrap_or(0);
    tables()[place][key as usize]
}

pub fn fill(text: &str, values: &[(&str, &dyn Display)]) -> String {
    fill_in(current(), text, values)
}

pub fn fill_in(language: &Language, text: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}').map(|close| open + close) else { break };
        let inside = &rest[open + 1..close];
        let mut parts = inside.split('|');
        let name = parts.next().unwrap_or_default();
        let forms: Vec<&str> = parts.collect();
        match values.iter().find(|(known, _)| *known == name) {
            Some((_, value)) if forms.is_empty() => out.push_str(&value.to_string()),
            Some((_, value)) => {
                let number = value.to_string().parse::<f64>().unwrap_or(0.0);
                let form = (language.plural)(number).min(forms.len() - 1);
                out.push_str(forms[form]);
            }
            None => out.push_str(&rest[open..=close]),
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

pub fn number(value: f64, digits: usize) -> String {
    let written = format!("{value:.digits$}");
    match current().decimal {
        '.' => written,
        other => written.replace('.', &other.to_string()),
    }
}

pub fn signed(value: f32) -> String {
    let written = format!("{value:+.1}");
    match current().decimal {
        '.' => written,
        other => written.replace('.', &other.to_string()),
    }
}

pub fn for_windows(preferred: &[String], offered: impl Fn(&Language) -> bool) -> &'static Language {
    let fits = |wanted: &str, name: &str| {
        let (wanted, name) = (wanted.to_ascii_lowercase(), name.to_ascii_lowercase());
        name == wanted || (!wanted.contains('-') && name.starts_with(&format!("{wanted}-")))
    };
    preferred
        .iter()
        .find_map(|name| LANGUAGES.iter().copied().find(|language| offered(language) && language.windows.iter().any(|wanted| fits(wanted, name))))
        .unwrap_or(LANGUAGES[0])
}

pub fn windows_languages() -> Vec<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::{GetUserPreferredUILanguages, MUI_LANGUAGE_NAME};
        let (mut count, mut size) = (0_u32, 0_u32);
        unsafe {
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut size) == 0 || size == 0 {
                return Vec::new();
            }
            let mut buffer = vec![0_u16; size as usize];
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buffer.as_mut_ptr(), &mut size) == 0 {
                return Vec::new();
            }
            buffer.split(|&unit| unit == 0).filter(|name| !name.is_empty()).map(String::from_utf16_lossy).collect()
        }
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};

    fn places(text: &str) -> BTreeSet<(String, usize)> {
        let mut found = BTreeSet::new();
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}').map(|close| open + close) else { break };
            let mut parts = rest[open + 1..close].split('|');
            let name = parts.next().unwrap_or_default().to_string();
            found.insert((name, parts.count()));
            rest = &rest[close + 1..];
        }
        found
    }

    #[test]
    fn every_language_has_every_text_once_with_the_same_places() {
        let english: HashMap<T, &str> = en::ENGLISH.iter().copied().collect();
        assert_eq!(english.len(), en::ENGLISH.len());
        let mut wrong = Vec::new();
        for (place, (key, text)) in en::ENGLISH.iter().enumerate() {
            assert_eq!(*key as usize, place, "the English texts are in the order of the keys");
            for (name, forms) in places(text) {
                if forms != 0 && forms != LANGUAGES[0].forms {
                    wrong.push(format!("en {key:?}: {{{name}|...}} has {forms} forms, not {}", LANGUAGES[0].forms));
                }
            }
        }
        for language in &LANGUAGES[1..] {
            let mut seen = HashMap::new();
            for (key, text) in language.texts {
                if seen.insert(*key, *text).is_some() {
                    wrong.push(format!("{} has {key:?} twice", language.code));
                }
            }
            let missing: Vec<String> = en::ENGLISH.iter().filter(|(key, _)| !seen.contains_key(key)).map(|(key, _)| format!("{key:?}")).collect();
            if !missing.is_empty() {
                wrong.push(format!("{} lacks {} texts: {}", language.code, missing.len(), missing.join(", ")));
            }
            for (key, text) in language.texts {
                let source: BTreeSet<String> = places(english[key]).into_iter().map(|(name, _)| name).collect();
                let here = places(text);
                let names: BTreeSet<String> = here.iter().map(|(name, _)| name.clone()).collect();
                for lost in source.difference(&names) {
                    wrong.push(format!("{} {key:?} lost the place {{{lost}}}", language.code));
                }
                for made_up in names.difference(&source) {
                    wrong.push(format!("{} {key:?} has a place English does not: {{{made_up}}}", language.code));
                }
                for (name, forms) in here {
                    if forms != 0 && forms != language.forms {
                        wrong.push(format!("{} {key:?}: {{{name}|...}} has {forms} forms, not {}", language.code, language.forms));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn every_language_has_a_code_and_a_name_of_its_own() {
        let codes: BTreeSet<&str> = LANGUAGES.iter().map(|language| language.code).collect();
        assert_eq!(codes.len(), LANGUAGES.len());
        assert_eq!(LANGUAGES[0].code, "en", "English comes first: it is the fallback");
        for language in LANGUAGES {
            assert!(!language.name.is_empty() && !language.game.is_empty() && language.forms >= 1, "{}", language.code);
            for number in [0.0, 1.0, 2.0, 5.0, 11.0, 21.0, 2.5] {
                assert!((language.plural)(number) < language.forms, "{} {number}", language.code);
            }
        }
    }

    #[test]
    fn places_are_filled_in_with_the_forms_of_the_language() {
        let english = LANGUAGES[0];
        let text = "{count} {count|pick is|picks are} ready for {hero}";
        assert_eq!(fill_in(english, text, &[("count", &1), ("hero", &"Axe")]), "1 pick is ready for Axe");
        assert_eq!(fill_in(english, text, &[("count", &12), ("hero", &"Axe")]), "12 picks are ready for Axe");
        assert_eq!(fill_in(english, "{missing} stays", &[]), "{missing} stays", "for the test to find");
        let russian = by_code("ru").unwrap();
        let text = "{count} {count|предмет|предмета|предметов}";
        let said = |count: u32| fill_in(russian, text, &[("count", &count)]);
        assert_eq!([said(1), said(3), said(5), said(11), said(21), said(22), said(112)], ["1 предмет", "3 предмета", "5 предметов", "11 предметов", "21 предмет", "22 предмета", "112 предметов"]);
    }

    #[test]
    fn the_first_language_is_the_first_of_windows_that_the_app_has() {
        let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<_>>();
        let all = |_: &Language| true;
        assert_eq!(for_windows(&names(&["ru-RU", "en-US"]), all).code, "ru");
        assert_eq!(for_windows(&names(&["de-DE", "ru-UA"]), all).code, "ru", "the first the app has");
        assert_eq!(for_windows(&names(&["de-DE", "en-GB"]), all).code, "en");
        assert_eq!(for_windows(&names(&["ru-RU"]), |language: &Language| language.code != "ru").code, "en", "nor one this computer cannot show");
        assert_eq!(for_windows(&names(&["de-DE"]), all).code, "en", "English where the app has none of them");
        assert_eq!(for_windows(&[], all).code, "en");
        assert_eq!(for_windows(&names(&["eng"]), all).code, "en", "a name is matched whole, or up to its dash");
    }

    #[test]
    fn a_text_is_had_in_any_language_whatever_the_window_is_in() {
        let russian = by_code("ru").unwrap();
        assert_eq!(text_in(LANGUAGES[0], T::Play), "Play");
        assert_eq!(text_in(russian, T::Play), "Играть");
        let ready = |count: usize| fill_in(russian, text_in(russian, T::PicksReady), &[("count", &count)]);
        assert_eq!(ready(1), "1 предмет готов. Нажмите «Играть».");
        assert_eq!(ready(3), "3 предмета готовы. Нажмите «Играть».");
        assert_eq!(ready(21), "21 предмет готов. Нажмите «Играть».");
        assert_eq!(ready(25), "25 предметов готовы. Нажмите «Играть».");
    }

    #[test]
    fn numbers_are_written_with_the_point_of_the_language() {
        assert_eq!(number(3.04, 1), "3.0");
        assert_eq!(number(52.25, 1), "52.2");
    }
}
