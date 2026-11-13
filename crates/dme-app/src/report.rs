use crate::support;

const LONGEST: usize = 6000;
const PROFILE: &str = "%USERPROFILE%";

pub fn address(title: &str, details: Option<&str>) -> String {
    let start = format!("{}/issues/new?title={}&body=", support::REPOSITORY, encode(title));
    let details = details.map(without_the_profile);
    let mut lines: Vec<&str> = details.as_deref().map(|details| details.lines().collect()).unwrap_or_default();
    loop {
        let cut = details.as_deref().is_some_and(|details| lines.len() < details.lines().count());
        let address = format!("{start}{}", encode(&body(&lines, cut)));
        if address.len() <= LONGEST || lines.is_empty() {
            return address;
        }
        lines.truncate(lines.len().saturating_sub(5));
    }
}

fn body(details: &[&str], cut: bool) -> String {
    let mut body = String::from("**What happened?**\nWrite here what you did, and what you saw.\n\n");
    if details.is_empty() {
        body.push_str(&format!("**Version:** {}\n", env!("CARGO_PKG_VERSION")));
    } else {
        body.push_str("**Details from the app**\n```\n");
        body.push_str(&details.join("\n"));
        if cut {
            body.push_str("\n… cut short: \"Copy details\" in the app has all of it.");
        }
        body.push_str("\n```\n");
    }
    body
}

pub fn without_the_profile(text: &str) -> String {
    match std::env::var("USERPROFILE") {
        Ok(profile) => profile_out(text, &profile),
        Err(_) => text.to_string(),
    }
}

fn profile_out(text: &str, profile: &str) -> String {
    let profile = profile.trim_end_matches(['\\', '/']);
    if profile.len() < 4 {
        return text.to_string();
    }
    let mut out = text.to_string();
    for written in [profile.to_string(), profile.replace('\\', "/")] {
        let wanted: Vec<char> = written.to_lowercase().chars().collect();
        let mut rest = out.as_str();
        let mut changed = String::with_capacity(out.len());
        while !rest.is_empty() {
            let here: Vec<char> = rest.chars().take(wanted.len()).flat_map(char::to_lowercase).collect();
            if here == wanted {
                changed.push_str(PROFILE);
                let skipped: usize = rest.chars().take(wanted.len()).map(char::len_utf8).sum();
                rest = &rest[skipped..];
            } else {
                let letter = rest.chars().next().expect("rest is not empty");
                changed.push(letter);
                rest = &rest[letter.len_utf8()..];
            }
        }
        out = changed;
    }
    out
}

fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => encoded.push(char::from(byte)),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub fn title_of(said: &str) -> String {
    let first = said.split_inclusive(". ").next().unwrap_or(said).trim().trim_end_matches('.');
    let mut title: String = first.chars().take(90).collect();
    if first.chars().count() > 90 {
        title.push('…');
    }
    title
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE_PATH: &str = r"D:\People\Анна Smith";

    #[test]
    fn the_players_own_folders_are_not_named_by_their_path() {
        let details = "Data in D:\\People\\Анна Smith\\AppData\\Local\\Dota2ModEnjoyer\nWhile reading d:\\people\\анна smith\\Downloads\\x and D:/People/Анна Smith/y";
        let out = profile_out(details, PROFILE_PATH);
        assert_eq!(out, "Data in %USERPROFILE%\\AppData\\Local\\Dota2ModEnjoyer\nWhile reading %USERPROFILE%\\Downloads\\x and %USERPROFILE%/y");
        assert!(!out.to_lowercase().contains("анна"));
        assert_eq!(profile_out(r"D:\People\Анна Smith\x", r"D:\People\Анна Smith\"), r"%USERPROFILE%\x");
        assert_eq!(profile_out(r"C:\Games\Dota", "C:"), r"C:\Games\Dota");
        assert_eq!(profile_out("nothing of theirs", PROFILE_PATH), "nothing of theirs");
    }

    #[test]
    fn an_address_has_only_what_an_address_may_have() {
        assert_eq!(encode("Az09-_.~"), "Az09-_.~");
        assert_eq!(encode("a b/c?&=#\n"), "a%20b%2Fc%3F%26%3D%23%0A");
        assert_eq!(encode("é…"), "%C3%A9%E2%80%A6");
    }

    #[test]
    fn a_report_is_a_new_issue_on_the_projects_page_with_what_happened_written_in() {
        let address = address("Play did not work", Some("Dota 2 Mod Enjoyer 0.2.3\nWhile putting the picks in the game: could not write"));
        assert!(address.starts_with(&format!("{}/issues/new?title=Play%20did%20not%20work&body=", support::REPOSITORY)));
        assert!(address.contains(&encode("While putting the picks in the game: could not write")));
        assert!(address.contains(&encode("**What happened?**")));
        assert!(!address.contains(' ') && !address.contains('\n'));
        assert!(super::address("A problem", None).contains(&encode(&format!("**Version:** {}", env!("CARGO_PKG_VERSION")))));
    }

    #[test]
    fn long_details_are_cut_short_to_fit_an_address() {
        let long: Vec<String> = (0..2000).map(|number| format!("   {number}: some::function::in::the::code")).collect();
        let address = address("Something went wrong inside the app", Some(&long.join("\n")));
        assert!(address.len() <= LONGEST, "{}", address.len());
        assert!(address.contains(&encode("   0: some::function")) && !address.contains(&encode("   1999: some")));
        assert!(address.contains(&encode("cut short")));
    }

    #[test]
    fn a_title_is_the_first_sentence_of_what_was_said() {
        assert_eq!(title_of("Windows did not let the app change one of Dota's files. Another program may be using it."), "Windows did not let the app change one of Dota's files");
        assert_eq!(title_of("Dota 2 is running. Close it and try again."), "Dota 2 is running");
        let long = "a".repeat(200);
        assert_eq!(title_of(&long).chars().count(), 91);
    }
}
