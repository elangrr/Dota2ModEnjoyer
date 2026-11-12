use std::collections::HashMap;

use crate::kv::{self, Block};

#[derive(Default)]
pub struct Names {
    tokens: HashMap<String, String>,
}

impl Names {
    pub fn parse(bytes: &[u8]) -> Names {
        let tokens = Block::document(bytes)
            .pairs()
            .find_map(|pair| pair.value.as_block())
            .and_then(|lang| lang.block("Tokens"))
            .map(|tokens| {
                tokens
                    .pairs()
                    .filter_map(|pair| {
                        let key = std::str::from_utf8(pair.key).ok()?.to_ascii_lowercase();
                        Some((key, kv::unescape(pair.value.as_str()?)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Names { tokens }
    }

    pub fn extend(&mut self, other: Names) {
        for (token, text) in other.tokens {
            self.tokens.entry(token).or_insert(text);
        }
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn get(&self, token: &str) -> Option<&str> {
        let key = token.strip_prefix('#').unwrap_or(token).to_ascii_lowercase();
        self.tokens.get(&key).map(String::as_str)
    }

    pub fn search(&self, part: &str) -> Vec<(&str, &str)> {
        let part = part.to_ascii_lowercase();
        let mut found: Vec<(&str, &str)> = self
            .tokens
            .iter()
            .filter(|(token, _)| token.contains(&part))
            .map(|(token, text)| (token.as_str(), text.as_str()))
            .collect();
        found.sort_unstable();
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_names_up_by_token() {
        let file = "\u{feff}\"lang\"\r\n{\r\n\t\"Language\"\t\"English\"\r\n\t\"Tokens\"\r\n\t{\r\n\t\t\"DOTA_Set_Armor_of_the_Vanguard\"\t\"Armor of the Vanguard\"\r\n\t\t\"DOTA_Item_Quoted\"\t\"The \\\"Best\\\" Axe\"\r\n\t}\r\n}\r\n";
        let names = Names::parse(file.as_bytes());
        assert_eq!(names.len(), 2);
        assert_eq!(names.get("#DOTA_Set_Armor_of_the_Vanguard"), Some("Armor of the Vanguard"));
        assert_eq!(names.get("dota_set_armor_of_the_vanguard"), Some("Armor of the Vanguard"));
        assert_eq!(names.get("#DOTA_Item_Quoted"), Some("The \"Best\" Axe"));
        assert_eq!(names.get("#Missing"), None);
        assert_eq!(names.search("ITEM_q"), [("dota_item_quoted", "The \"Best\" Axe")]);
        assert_eq!(names.search("dota_").len(), 2);
        assert!(names.search("nothing").is_empty());
    }

    #[test]
    fn names_of_several_files_can_be_put_together() {
        let file = |tokens: &str| format!("\"lang\" {{ \"Tokens\" {{ {tokens} }} }}");
        let mut names = Names::parse(file("\"Item_A\" \"A\" \"Shared\" \"first\"").as_bytes());
        names.extend(Names::parse(file("\"Style_B\" \"B\" \"SHARED\" \"second\"").as_bytes()));
        assert_eq!(names.len(), 3);
        assert_eq!(names.get("#Style_B"), Some("B"));
        assert_eq!(names.get("#Shared"), Some("first"), "the name already there is kept");
    }

    #[test]
    fn unreadable_files_give_no_names() {
        assert!(Names::parse(b"").is_empty());
        assert!(Names::parse(b"\"lang\" { \"Language\" \"English\" }").is_empty());
        assert!(Names::parse(&[0xFF, 0x00, 0x7B]).is_empty());
    }
}
