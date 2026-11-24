use std::collections::HashSet;

use crate::builds::{Build, Builds, GameTexts};
use crate::draft::{Rank, Role};
use crate::items;
use crate::vpk::NewFile;

const EARLY_BEFORE_MIN: u32 = 15;
const LATE_AFTER_MIN: u32 = 26;
const MOST_PROMOTED_LATE: usize = 3;
const MOST_OTHER_ITEMS: usize = 12;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShopBuilds {
    pub tag: String,
    pub files: Vec<NewFile>,
}

pub fn generate(builds: &Builds, texts: &GameTexts, rank: Rank, preferred_role: Option<Role>) -> ShopBuilds {
    let mut hero_ids: Vec<&str> = builds.all().map(|(hero, _)| hero).collect();
    hero_ids.sort_unstable();

    let mut files = Vec::with_capacity(hero_ids.len());
    for hero in hero_ids {
        let roles = builds.roles(hero, rank);
        let role = preferred_role.filter(|wanted| roles.contains(wanted)).or_else(|| roles.first().copied());
        let Some(role) = role else { continue };
        let Some(build) = builds.of(hero, rank, role) else { continue };
        let short = items::hero_short_name(hero).to_ascii_lowercase();
        if short.is_empty() || !short.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            continue;
        }
        let body = format_hero_file(builds, texts, hero, build, rank);
        files.push((format!("itembuilds/default_{short}.txt"), body.into_bytes()));
    }

    let tag = if files.is_empty() {
        String::new()
    } else {
        format!("v1:{}:{}:{}:{}", builds.made, builds.patch, rank.place(), preferred_role.map_or(9, Role::place))
    };
    ShopBuilds { tag, files }
}

fn format_hero_file(builds: &Builds, texts: &GameTexts, hero: &str, build: &Build, rank: Rank) -> String {
    let mut starting: Vec<&str> = Vec::new();
    let start_bought = build.starting(texts);
    for bought in &start_bought {
        for _ in 0..bought.copies.max(1) {
            starting.push(bought.item.as_str());
        }
    }

    let mut core_entries: Vec<(&str, u32)> = build.core(texts).into_iter().map(|item| (item.item.as_str(), item.minute)).collect();
    core_entries.extend(builds.pro_core(hero, build, texts).into_iter().map(|pro| (pro.item.as_str(), pro.minute)));
    core_entries.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0)));

    let mut seen_core: HashSet<String> = HashSet::new();
    let (mut early, mut mid, mut late) = (Vec::new(), Vec::new(), Vec::new());
    for (item, minute) in core_entries {
        if !seen_core.insert(item.to_ascii_lowercase()) {
            continue;
        }
        if minute < EARLY_BEFORE_MIN {
            early.push(item);
        } else if minute <= LATE_AFTER_MIN {
            mid.push(item);
        } else {
            late.push(item);
        }
    }

    let others = builds.others(hero, build, texts, &[]);
    let mut other_items: Vec<&str> = Vec::new();
    for other in &others {
        let item = other.item.item.as_str();
        if !seen_core.insert(item.to_ascii_lowercase()) {
            continue;
        }
        if late.len() < MOST_PROMOTED_LATE && other.item.minute > LATE_AFTER_MIN && late_needs_promotion(&late, other.item.minute) {
            late.push(item);
        } else if other_items.len() < MOST_OTHER_ITEMS {
            other_items.push(item);
        }
    }

    let title = format!("{} - {} (Dota 2 Mod Enjoyer)", build.role.name(), rank.name());
    let mut out = String::with_capacity(1024);
    out.push_str("\"itembuilds\"\r\n{\r\n");
    out.push_str("\t\"author\"\t\t\"Dota 2 Mod Enjoyer\"\r\n");
    out.push_str(&format!("\t\"hero\"\t\t\t\"{}\"\r\n", escape(hero)));
    out.push_str(&format!("\t\"Title\"\t\t\t\"{}\"\r\n", escape(&title)));
    out.push_str("\r\n\t\"Items\"\r\n\t{\r\n");
    write_section(&mut out, "#DOTA_Item_Build_Starting_Items", &starting);
    write_section(&mut out, "#DOTA_Item_Build_Early_Game", &early);
    write_section(&mut out, "#DOTA_Item_Build_Mid_Items", &mid);
    write_section(&mut out, "#DOTA_Item_Build_Late_Items", &late);
    write_section(&mut out, "#DOTA_Item_Build_Other_Items", &other_items);
    out.push_str("\t}\r\n}\r\n");
    out
}

fn late_needs_promotion(late: &[&str], minute: u32) -> bool {
    late.len() < MOST_PROMOTED_LATE && minute > LATE_AFTER_MIN
}

fn write_section(out: &mut String, heading: &str, items: &[&str]) {
    out.push_str(&format!("\t\t\"{heading}\"\r\n\t\t{{\r\n"));
    for item in items {
        out.push_str(&format!("\t\t\t\"item\"\t\t\"{}\"\r\n", escape(item)));
    }
    out.push_str("\t\t}\r\n");
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::kv::{self, Block};
    use crate::names::Names;

    fn sample_builds() -> (Builds, GameTexts) {
        let file = json!({
            "format": 2, "made": 1_700_000_000, "source": "STRATZ", "patch": "7.42", "sampled": 500,
            "items": [
                "item_tango", "item_branches", "item_power_treads", "item_bfury",
                "item_bkb", "item_abyssal_blade", "item_monkey_king_bar", "item_bottle"
            ],
            "skills": [],
            "heroes": {
                "npc_dota_hero_phantom_assassin": [
                    {
                        "role": 0, "pair": 1, "matches": 1000,
                        "start": [[0, 1, 900, 450], [1, 3, 800, 400]],
                        "items": [
                            [2, 9, 950, 500],
                            [3, 16, 850, 440],
                            [4, 24, 600, 320],
                            [5, 34, 250, 140],
                            [6, 31, 180, 100]
                        ],
                        "talents": []
                    },
                    {
                        "role": 1, "pair": 1, "matches": 300,
                        "start": [[0, 1, 250, 120]],
                        "items": [[2, 10, 260, 130], [4, 22, 180, 95]],
                        "talents": []
                    }
                ]
            },
            "pros": {
                "source": "OpenDota", "matches": 100,
                "heroes": {
                    "npc_dota_hero_phantom_assassin": [[1, 20, [[7, 18, 2]]]]
                }
            }
        });
        let items_txt = br#""DOTAAbilities" {
            "item_tango" { "ItemCost" "90" }
            "item_branches" { "ItemCost" "55" }
        }"#;
        let texts = GameTexts::from_files(Names::default(), &[], items_txt, b"");
        (Builds::parse(file.to_string().as_bytes()).unwrap(), texts)
    }

    #[test]
    fn generates_valid_valve_itembuilds_files_with_expected_sections() {
        let (builds, texts) = sample_builds();
        let generated = generate(&builds, &texts, Rank::Archon, None);
        assert!(!generated.tag.is_empty());
        assert_eq!(generated.files.len(), 1);
        let (path, bytes) = &generated.files[0];
        assert_eq!(path, "itembuilds/default_phantom_assassin.txt");
        kv::validate(bytes, path).unwrap();

        let root = Block::document(bytes).block("itembuilds").unwrap();
        assert_eq!(root.str("hero").unwrap(), b"npc_dota_hero_phantom_assassin");
        assert_eq!(root.str("author").unwrap(), b"Dota 2 Mod Enjoyer");
        let items = root.block("Items").unwrap();
        let list = |section: &str| -> Vec<String> {
            items
                .block(section)
                .unwrap()
                .pairs()
                .filter_map(|p| p.value.as_str().map(|v| String::from_utf8_lossy(v).into_owned()))
                .collect()
        };
        assert_eq!(list("#DOTA_Item_Build_Starting_Items"), ["item_tango", "item_branches", "item_branches", "item_branches"]);
        assert_eq!(list("#DOTA_Item_Build_Early_Game"), ["item_power_treads"]);
        assert_eq!(list("#DOTA_Item_Build_Mid_Items"), ["item_bfury", "item_bkb"]);
        assert_eq!(list("#DOTA_Item_Build_Late_Items"), ["item_abyssal_blade", "item_monkey_king_bar"]);
        assert!(list("#DOTA_Item_Build_Other_Items").is_empty());
    }

    #[test]
    fn preferred_role_selects_that_build_and_includes_pro_core_items() {
        let (builds, texts) = sample_builds();
        let mid = generate(&builds, &texts, Rank::Archon, Some(Role::Mid));
        let carry = generate(&builds, &texts, Rank::Archon, Some(Role::Offlane));
        assert_ne!(mid.tag, carry.tag);
        let mid_text = std::str::from_utf8(&mid.files[0].1).unwrap();
        assert!(mid_text.contains("Mid - Archon"), "{mid_text}");
        assert!(mid_text.contains("\"item_bottle\""), "pro core item included in early game: {mid_text}");
        let fallback_text = std::str::from_utf8(&carry.files[0].1).unwrap();
        assert!(fallback_text.contains("Carry - Archon"), "falls back to most played role when offlane has no build");
    }
}
