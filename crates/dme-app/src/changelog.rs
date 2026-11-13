use dme_core::update;

const TEXT: &str = include_str!("../../../CHANGELOG.md");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub number: String,
    pub changes: Vec<String>,
}

pub fn read(text: &str) -> Vec<Version> {
    let mut versions: Vec<Version> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(number) = trimmed.strip_prefix("## ") {
            versions.push(Version { number: number.trim().to_string(), changes: Vec::new() });
            continue;
        }
        let Some(version) = versions.last_mut() else { continue };
        if let Some(change) = trimmed.strip_prefix("- ") {
            version.changes.push(change.trim().to_string());
        } else if !trimmed.is_empty()
            && line.starts_with([' ', '\t'])
            && let Some(change) = version.changes.last_mut()
        {
            change.push(' ');
            change.push_str(trimmed);
        }
    }
    versions
}

pub fn since(before: &str, now: &str) -> Vec<Version> {
    since_in(TEXT, before, now)
}

fn since_in(text: &str, before: &str, now: &str) -> Vec<Version> {
    let same = |one: &str, other: &str| !update::is_newer(one, other) && !update::is_newer(other, one);
    let wanted = |number: &str| match update::is_version(before) {
        true => update::is_newer(number, before) && !update::is_newer(number, now),
        false => same(number, now),
    };
    read(text).into_iter().filter(|version| !version.changes.is_empty() && wanted(&version.number)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# What is new

Some words about the file.

## 0.2.4

- Something not out yet.

## 0.2.3

- A change in one line.
- A change that goes on
  over a second line.

## 0.2.2

- An older change.

The end, which is no change.
";

    fn numbers(versions: &[Version]) -> Vec<&str> {
        versions.iter().map(|version| version.number.as_str()).collect()
    }

    #[test]
    fn a_changelog_is_read_as_versions_each_with_its_changes() {
        let versions = read(SAMPLE);
        assert_eq!(numbers(&versions), ["0.2.4", "0.2.3", "0.2.2"]);
        assert_eq!(versions[1].changes, ["A change in one line.", "A change that goes on over a second line."]);
        assert_eq!(versions[2].changes, ["An older change."], "the words after the list are no change");
        assert!(read("# Nothing yet\n\n- A change of no version.\n").is_empty());
    }

    #[test]
    fn what_is_new_is_what_came_after_the_version_before_up_to_this_one() {
        assert_eq!(numbers(&since_in(SAMPLE, "0.2.2", "0.2.3")), ["0.2.3"]);
        assert_eq!(numbers(&since_in(SAMPLE, "0.2.1", "0.2.3")), ["0.2.3", "0.2.2"], "versions that were skipped too");
        assert_eq!(numbers(&since_in(SAMPLE, "0.2.2", "0.2.4")), ["0.2.4", "0.2.3"]);
        assert!(since_in(SAMPLE, "0.2.3", "0.2.3").is_empty(), "nothing is new in the same version");
        assert!(since_in(SAMPLE, "0.2.4", "0.2.3").is_empty(), "nor in going back to an older one");
        assert_eq!(numbers(&since_in(SAMPLE, "", "0.2.3")), ["0.2.3"]);
        assert!(since_in(SAMPLE, "", "0.2.5").is_empty(), "a version the changelog says nothing of");
    }

    #[test]
    fn the_projects_changelog_tells_what_is_new_in_this_version_or_a_later_one() {
        let versions = read(TEXT);
        assert!(!versions.is_empty());
        for version in &versions {
            assert!(update::is_version(&version.number), "{} is no version", version.number);
            assert!(!version.changes.is_empty(), "{} says nothing", version.number);
            assert!(version.changes.iter().all(|change| !change.is_empty() && change.len() < 300), "{}", version.number);
        }
        assert!(versions.windows(2).all(|pair| update::is_newer(&pair[0].number, &pair[1].number)));
        assert!(!update::is_newer(env!("CARGO_PKG_VERSION"), &versions[0].number), "CHANGELOG.md says nothing of {}", env!("CARGO_PKG_VERSION"));
    }
}
