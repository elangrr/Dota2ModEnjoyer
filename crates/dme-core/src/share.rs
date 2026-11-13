use crate::apply::Pick;
use crate::items;

pub const PREFIX: &str = "DME1-";
const MOST: usize = 1 << 20;
pub const LONGEST_NAME: usize = 40;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shared {
    pub name: Option<String>,
    pub picks: Vec<Pick>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NotACode {
    #[error("There is no share code in this. A share code starts with {PREFIX}")]
    Missing,
    #[error("This share code is not whole: a part of it is missing or was changed. Copy all of it again.")]
    Broken,
    #[error("This share code was made by a newer version of the app. Update the app to use it.")]
    Newer,
}

pub fn code(name: Option<&str>, picks: &[Pick]) -> String {
    let mut text = String::new();
    if let Some(name) = name.map(tidy_name).filter(|name| !name.is_empty()) {
        text.push_str(&format!("@{name}\n"));
    }
    for pick in picks {
        text.push_str(&line(pick));
        text.push('\n');
    }
    let mut packed = miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 10);
    packed.extend_from_slice(&crc32fast::hash(text.as_bytes()).to_le_bytes());
    format!("{PREFIX}{}", encode(&packed))
}

pub fn read(text: &str) -> Result<Shared, NotACode> {
    let Some(start) = text.find(PREFIX) else { return Err(if is_of_a_later_version(text) { NotACode::Newer } else { NotACode::Missing }) };
    let letters: String = text[start + PREFIX.len()..].chars().take_while(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '-' | '_')).collect();
    let packed = decode(&letters).ok_or(NotACode::Broken)?;
    let Some(split) = packed.len().checked_sub(4) else { return Err(NotACode::Broken) };
    let (packed, check) = packed.split_at(split);
    let unpacked = miniz_oxide::inflate::decompress_to_vec_with_limit(packed, MOST).map_err(|_| NotACode::Broken)?;
    if crc32fast::hash(&unpacked).to_le_bytes() != check {
        return Err(NotACode::Broken);
    }
    let text = String::from_utf8(unpacked).map_err(|_| NotACode::Broken)?;
    let mut shared = Shared::default();
    for line in text.lines().filter(|line| !line.is_empty()) {
        match line.strip_prefix('@') {
            Some(name) if shared.name.is_none() && shared.picks.is_empty() => shared.name = Some(tidy_name(name)).filter(|name| !name.is_empty()),
            Some(_) => return Err(NotACode::Newer),
            None => shared.picks.push(pick_of(line).ok_or(NotACode::Newer)?),
        }
    }
    Ok(shared)
}

fn is_of_a_later_version(text: &str) -> bool {
    text.match_indices("DME").any(|(at, _)| {
        let rest = &text[at + 3..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        rest[digits.len()..].starts_with('-') && digits.parse::<u32>().is_ok_and(|version| version > 1)
    })
}

fn tidy_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(LONGEST_NAME).collect::<String>().trim().to_string()
}

fn line(pick: &Pick) -> String {
    let owner = pick.hero.as_deref().map_or("", items::hero_short_name);
    let mut line = format!("{owner}.{}={}", pick.slot, pick.item);
    if pick.style > 0 {
        line.push_str(&format!(":{}", pick.style));
    }
    if let Some(effect) = pick.effect {
        line.push_str(&format!("+{effect}"));
    }
    line
}

fn pick_of(line: &str) -> Option<Pick> {
    let (place, what) = line.split_once('=')?;
    let (owner, slot) = place.split_once('.')?;
    let word = |text: &str| !text.is_empty() && text.chars().all(|letter| letter.is_ascii_alphanumeric() || letter == '_');
    let number = |text: &str| !text.is_empty() && text.chars().all(|letter| letter.is_ascii_digit());
    if !(owner.is_empty() || word(owner)) || !word(slot) {
        return None;
    }
    let (what, effect) = match what.split_once('+') {
        Some((what, effect)) if number(effect) => (what, Some(effect.parse().ok()?)),
        Some(_) => return None,
        None => (what, None),
    };
    let (item, style) = match what.split_once(':') {
        Some((item, style)) if number(style) => (item, style.parse().ok()?),
        Some(_) => return None,
        None => (what, 0),
    };
    if !number(item) {
        return None;
    }
    let pick = match owner {
        "" => Pick::world(slot, item),
        hero => Pick::hero(&items::hero_id(hero), slot, item),
    };
    Some(pick.styled(style).with_effect(effect))
}

const LETTERS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let three = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let whole = u32::from(three[0]) << 16 | u32::from(three[1]) << 8 | u32::from(three[2]);
        for place in 0..=chunk.len() {
            text.push(char::from(LETTERS[(whole >> (18 - 6 * place) & 63) as usize]));
        }
    }
    text
}

fn decode(text: &str) -> Option<Vec<u8>> {
    let values: Option<Vec<u32>> = text.bytes().map(|letter| LETTERS.iter().position(|known| *known == letter).map(|value| value as u32)).collect();
    let values = values?;
    if values.len() % 4 == 1 {
        return None;
    }
    let mut bytes = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        let whole = chunk.iter().enumerate().fold(0u32, |whole, (place, value)| whole | value << (18 - 6 * place));
        bytes.extend(whole.to_be_bytes()[1..chunk.len()].iter());
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AXE: &str = "npc_dota_hero_axe";
    const WILLOW: &str = "npc_dota_hero_dark_willow";

    fn picks() -> Vec<Pick> {
        vec![
            Pick::hero(AXE, "weapon", "9040"),
            Pick::hero(AXE, "head", "8910").styled(2),
            Pick::hero(WILLOW, "ability_effects_1", "23145"),
            Pick::world("courier", "10003").styled(1).with_effect(Some(15)),
            Pick::world("fx_teleport", "17482").styled(2),
            Pick::world("emblem", "12996"),
        ]
    }

    #[test]
    fn picks_come_back_from_their_code_as_they_were_with_their_name() {
        let code = code(Some("  My   ranked look "), &picks());
        assert!(code.starts_with(PREFIX));
        assert!(code[PREFIX.len()..].chars().all(|letter| letter.is_ascii_alphanumeric() || letter == '-' || letter == '_'), "{code}");
        let shared = read(&code).unwrap();
        assert_eq!(shared.name.as_deref(), Some("My ranked look"));
        assert_eq!(shared.picks, picks());
        assert_eq!(read(&super::code(None, &picks())).unwrap(), Shared { name: None, picks: picks() });
        assert_eq!(read(&super::code(Some("  "), &[])).unwrap(), Shared::default());
    }

    #[test]
    fn a_code_is_found_among_the_words_it_was_pasted_with() {
        let code = code(None, &picks());
        let message = format!("look at this one:\n{code}\nnice, right?");
        assert_eq!(read(&message).unwrap().picks, picks());
        assert_eq!(read(&format!("({code})")).unwrap().picks, picks());
    }

    #[test]
    fn a_code_that_lost_a_part_or_is_none_at_all_is_said_to_be_so() {
        let code = code(None, &picks());
        assert_eq!(read("hello"), Err(NotACode::Missing));
        assert_eq!(read(""), Err(NotACode::Missing));
        assert_eq!(read(&code[..code.len() - 6]), Err(NotACode::Broken), "cut short");
        assert_eq!(read(PREFIX), Err(NotACode::Broken));
        let mut changed: Vec<char> = code.chars().collect();
        let middle = PREFIX.len() + (changed.len() - PREFIX.len()) / 2;
        changed[middle] = if changed[middle] == 'A' { 'B' } else { 'A' };
        assert_eq!(read(&changed.into_iter().collect::<String>()), Err(NotACode::Broken));
        assert_eq!(read("DME2-AAAA"), Err(NotACode::Newer));
        assert_eq!(read("see DME12-x"), Err(NotACode::Newer));
        assert_eq!(read("DME0-AAAA DMEx-"), Err(NotACode::Missing));
    }

    #[test]
    fn a_line_this_version_cannot_read_is_taken_for_a_later_versions() {
        let packed = |text: &str| {
            let mut packed = miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 10);
            packed.extend_from_slice(&crc32fast::hash(text.as_bytes()).to_le_bytes());
            format!("{PREFIX}{}", encode(&packed))
        };
        assert_eq!(read(&packed("axe.weapon=9040\n")).unwrap().picks, [Pick::hero(AXE, "weapon", "9040")]);
        for strange in ["axe.weapon=9040~3\n", "axe.weapon=abc\n", "axe:weapon=9040\n", "axe.weapon=9040+\n", "axe.=1\n", "axe.weapon=1\n@late name\n", "a b.weapon=1\n"] {
            assert_eq!(read(&packed(strange)), Err(NotACode::Newer), "{strange:?}");
        }
    }

    #[test]
    fn a_code_that_unpacks_to_too_much_is_not_read() {
        let text = format!("axe.weapon=1\n{}", "x".repeat(MOST + 10));
        let mut packed = miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 10);
        packed.extend_from_slice(&crc32fast::hash(text.as_bytes()).to_le_bytes());
        assert_eq!(read(&format!("{PREFIX}{}", encode(&packed))), Err(NotACode::Broken));
    }

    #[test]
    fn base64_for_addresses_goes_both_ways_for_any_length() {
        for length in 0..40 {
            let bytes: Vec<u8> = (0..length).map(|number| (number * 37 + 11) as u8).collect();
            let text = encode(&bytes);
            assert_eq!(decode(&text).as_deref(), Some(bytes.as_slice()), "{length}");
        }
        assert_eq!(encode(b"Man"), "TWFu");
        assert_eq!(encode(&[0xfb, 0xff]), "-_8");
        assert_eq!(decode("TWF"), Some(b"Ma".to_vec()));
        assert_eq!(decode("T"), None, "a letter left over");
        assert_eq!(decode("TW=="), None, "no = at the end");
    }

    #[test]
    fn a_whole_look_of_a_hero_makes_a_code_that_fits_in_a_message() {
        let slots = ["weapon", "offhand_weapon", "head", "shoulder", "armor", "belt", "arms", "back", "neck", "legs"];
        let look: Vec<Pick> = slots.iter().enumerate().map(|(number, slot)| Pick::hero(AXE, slot, &(9040 + number * 7).to_string())).collect();
        let code = code(Some("Axe"), &look);
        assert!(code.len() < 160, "{} letters: {code}", code.len());
    }
}
