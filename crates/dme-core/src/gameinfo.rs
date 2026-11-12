use crate::error::{Error, Result};
use crate::kv::{self, Block};

pub const BEGIN_MARK: &str = "// >>> Dota 2 Mod Enjoyer: the lines down to the next marker were added by the app";
pub const END_MARK: &str = "// <<< Dota 2 Mod Enjoyer: end of added lines";

const BRANCH: &str = "gameinfo_branchspecific.gi";
const GAMEINFO: &str = "gameinfo.gi";

pub fn has_our_lines(branch: &str) -> bool {
    branch.contains(BEGIN_MARK) || branch.contains(END_MARK)
}

pub fn remove_our_lines(branch: &str) -> Result<String> {
    let (Some(begin), Some(end)) = (branch.find(BEGIN_MARK), branch.find(END_MARK)) else {
        return if has_our_lines(branch) {
            Err(Error::format(BRANCH, "only one of the app's two marker lines is present"))
        } else {
            Ok(branch.to_string())
        };
    };
    if end < begin {
        return Err(Error::format(BRANCH, "the app's marker lines are in the wrong order"));
    }
    let from = line_start(branch, begin);
    let to = branch[end..].find('\n').map_or(branch.len(), |i| end + i + 1);
    Ok(format!("{}{}", &branch[..from], &branch[to..]))
}

pub fn add_our_lines(branch: &str, gameinfo: &str, folder: &str) -> Result<String> {
    if has_our_lines(branch) {
        return Err(Error::format(BRANCH, "the app's lines are already in the file"));
    }
    kv::validate(branch.as_bytes(), BRANCH)?;
    kv::validate(gameinfo.as_bytes(), GAMEINFO)?;

    let file_system = file_system_block(branch.as_bytes(), BRANCH)?;
    if file_system.get("SearchPaths").is_some() {
        return Err(Error::format(BRANCH, "the file already has a SearchPaths block"));
    }
    let close = file_system.end();
    let insert_at = line_start(branch, close);
    let brace_indent = &branch[insert_at..close];
    if !brace_indent.chars().all(|c| c == ' ' || c == '\t') {
        return Err(Error::format(BRANCH, "the closing brace of FileSystem is not on a line of its own"));
    }

    let paths = search_paths_with(gameinfo, folder)?;
    let newline = if branch.contains("\r\n") { "\r\n" } else { "\n" };
    let indent = format!("{brace_indent}\t");

    let mut added = String::new();
    let mut line = |text: &str| {
        added.push_str(&indent);
        added.push_str(text);
        added.push_str(newline);
    };
    line(BEGIN_MARK);
    line("SearchPaths");
    line("{");
    for (kind, path) in &paths {
        line(&format!("\t{}\t{}", word(kind), word(path)));
    }
    line("}");
    line(END_MARK);

    Ok(format!("{}{}{}", &branch[..insert_at], added, &branch[insert_at..]))
}

fn search_paths_with(gameinfo: &str, folder: &str) -> Result<Vec<(String, String)>> {
    let list = file_system_block(gameinfo.as_bytes(), GAMEINFO)?
        .block("SearchPaths")
        .ok_or_else(|| Error::format(GAMEINFO, "there is no SearchPaths block"))?;

    let mut paths = Vec::new();
    let (mut game_added, mut mod_added) = (false, false);
    for pair in list.pairs() {
        let Some(value) = pair.value.as_str() else {
            return Err(Error::format(GAMEINFO, "the SearchPaths block has a nested block"));
        };
        let (kind, path) = (kv::unescape(pair.key), kv::unescape(value));
        if path.eq_ignore_ascii_case("dota") {
            if kind.eq_ignore_ascii_case("Game") && !game_added {
                paths.push(("Game".to_string(), folder.to_string()));
                game_added = true;
            } else if kind.eq_ignore_ascii_case("Mod") && !mod_added {
                paths.push(("Mod".to_string(), folder.to_string()));
                mod_added = true;
            }
        }
        paths.push((kind, path));
    }
    if !game_added {
        return Err(Error::format(GAMEINFO, "the SearchPaths block has no \"Game dota\" entry"));
    }
    Ok(paths)
}

fn file_system_block<'a>(bytes: &'a [u8], what: &str) -> Result<Block<'a>> {
    Block::document(bytes)
        .block("GameInfo")
        .ok_or_else(|| Error::format(what, "there is no GameInfo block"))?
        .block("FileSystem")
        .ok_or_else(|| Error::format(what, "there is no FileSystem block"))
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

fn word(text: &str) -> String {
    let bare = !text.is_empty() && !text.contains(|c: char| c.is_whitespace() || matches!(c, '{' | '}' | '"'));
    if bare { text.to_string() } else { format!("\"{text}\"") }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const BRANCH_TEXT: &str = "\"GameInfo\"\r\n{\r\n\t//\r\n\t// Branch-varying info.\r\n\t//\r\n\r\n\tgame \t\t\"Dota 2\"\r\n\ttitle \t\t\"Dota 2\"\r\n\r\n\tFileSystem\r\n\t{\r\n\t\tSteamAppId\t\t\t\t570\r\n\t\tBreakpadAppId\t\t\t373300\t// Report crashes under beta DLC, not the S1 game.\r\n\t}\r\n}\r\n";

    pub(crate) const GAMEINFO_TEXT: &str = "\"GameInfo\"\r\n{\r\n\tgame\t\"Dota 2\"\r\n\tFileSystem\r\n\t{\r\n\t\t//\r\n\t\t// Search paths are relative to the exe directory\\..\\\r\n\t\t//\r\n\t\tSearchPaths\r\n\t\t{\r\n\t\t\t// These are optional language paths.\r\n\t\t\tGame_AudioLanguage\tdota_*LANGUAGE*\r\n\r\n\t\t\tGame_LowViolence\tdota_lv\r\n\r\n\t\t\tGame\t\t\t\tdota\r\n\t\t\tGame\t\t\t\tcore\r\n\r\n\t\t\tMod\t\t\t\t\tdota\r\n\r\n\t\t\tWrite\t\t\t\tdota\r\n\r\n\t\t\tAddonRoot\t\t\tdota_addons\r\n\r\n\t\t\tPublicContent\t\tdota_core\r\n\t\t\tPublicContent\t\tcore\r\n\t\t}\r\n\t\t\r\n\t\t\"UserSettingsPathID\"\t\"USRLOCAL\"\r\n\t}\r\n}\r\n";

    #[test]
    fn adds_the_full_search_path_list_with_our_folder_first() {
        let edited = add_our_lines(BRANCH_TEXT, GAMEINFO_TEXT, "mod_enjoyer").unwrap();
        kv::validate(edited.as_bytes(), "edited").unwrap();

        let paths = file_system_block(edited.as_bytes(), "edited").unwrap().block("SearchPaths").unwrap();
        let listed: Vec<(String, String)> = paths
            .pairs()
            .map(|p| (kv::unescape(p.key), kv::unescape(p.value.as_str().unwrap())))
            .collect();
        let expected = [
            ("Game_AudioLanguage", "dota_*LANGUAGE*"),
            ("Game_LowViolence", "dota_lv"),
            ("Game", "mod_enjoyer"),
            ("Game", "dota"),
            ("Game", "core"),
            ("Mod", "mod_enjoyer"),
            ("Mod", "dota"),
            ("Write", "dota"),
            ("AddonRoot", "dota_addons"),
            ("PublicContent", "dota_core"),
            ("PublicContent", "core"),
        ];
        assert_eq!(listed.len(), expected.len());
        for ((kind, path), (want_kind, want_path)) in listed.iter().zip(expected) {
            assert_eq!((kind.as_str(), path.as_str()), (want_kind, want_path));
        }

        let fs = file_system_block(edited.as_bytes(), "edited").unwrap();
        assert_eq!(fs.str("SteamAppId").unwrap(), b"570");
        assert_eq!(fs.str("BreakpadAppId").unwrap(), b"373300");
    }

    #[test]
    fn removing_our_lines_gives_back_the_original_bytes() {
        let edited = add_our_lines(BRANCH_TEXT, GAMEINFO_TEXT, "mod_enjoyer").unwrap();
        assert_ne!(edited, BRANCH_TEXT);
        assert!(has_our_lines(&edited));
        assert_eq!(remove_our_lines(&edited).unwrap(), BRANCH_TEXT);
    }

    #[test]
    fn works_with_unix_line_breaks_too() {
        let branch = BRANCH_TEXT.replace("\r\n", "\n");
        let gameinfo = GAMEINFO_TEXT.replace("\r\n", "\n");
        let edited = add_our_lines(&branch, &gameinfo, "mod_enjoyer").unwrap();
        assert!(!edited.contains('\r'));
        assert_eq!(remove_our_lines(&edited).unwrap(), branch);
    }

    #[test]
    fn an_untouched_file_is_left_alone_by_remove() {
        assert!(!has_our_lines(BRANCH_TEXT));
        assert_eq!(remove_our_lines(BRANCH_TEXT).unwrap(), BRANCH_TEXT);
    }

    #[test]
    fn refuses_layouts_it_does_not_understand() {
        let edited = add_our_lines(BRANCH_TEXT, GAMEINFO_TEXT, "mod_enjoyer").unwrap();
        assert!(add_our_lines(&edited, GAMEINFO_TEXT, "mod_enjoyer").is_err(), "already edited");

        let no_fs = "\"GameInfo\"\r\n{\r\n\tgame \"Dota 2\"\r\n}\r\n";
        assert!(add_our_lines(no_fs, GAMEINFO_TEXT, "mod_enjoyer").is_err());

        let own_paths = BRANCH_TEXT.replace("\t\tSteamAppId", "\t\tSearchPaths\r\n\t\t{\r\n\t\t}\r\n\t\tSteamAppId");
        assert!(add_our_lines(&own_paths, GAMEINFO_TEXT, "mod_enjoyer").is_err());

        let brace_not_alone = BRANCH_TEXT.replace("373300\t// Report crashes under beta DLC, not the S1 game.\r\n\t}", "373300 }");
        assert!(add_our_lines(&brace_not_alone, GAMEINFO_TEXT, "mod_enjoyer").is_err());

        let no_dota = GAMEINFO_TEXT.replace("Game\t\t\t\tdota\r\n", "");
        assert!(add_our_lines(BRANCH_TEXT, &no_dota, "mod_enjoyer").is_err());

        assert!(add_our_lines("\"GameInfo\" {", GAMEINFO_TEXT, "mod_enjoyer").is_err());
    }

    #[test]
    fn half_removed_markers_are_an_error() {
        let edited = add_our_lines(BRANCH_TEXT, GAMEINFO_TEXT, "mod_enjoyer").unwrap();
        let without_end = edited.replace(END_MARK, "");
        assert!(remove_our_lines(&without_end).is_err());
        let without_begin = edited.replace(BEGIN_MARK, "");
        assert!(remove_our_lines(&without_begin).is_err());
    }

    #[test]
    fn words_are_quoted_only_when_needed() {
        assert_eq!(word("dota_*LANGUAGE*"), "dota_*LANGUAGE*");
        assert_eq!(word("two words"), "\"two words\"");
        assert_eq!(word(""), "\"\"");
    }
}
