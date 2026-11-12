use crate::apply::Pick;
use crate::error::{Error, Result};
use crate::vpk::{Archive, NewFile};

pub const SLOT: &str = "trees";

const FOLDER: &str = "materials/models/props_tree/";

#[derive(Debug, PartialEq, Eq)]
pub struct Kind {
    pub id: &'static str,
    pub name: &'static str,
    pub picture: &'static str,
    swaps: &'static [(&'static str, &'static str)],
}

pub const KINDS: &[Kind] = &[
    Kind {
        id: "autumn",
        name: "Autumn trees",
        picture: "econ/terrain/autumn_terrain",
        swaps: &[
            ("tree_oak_leaves_00", "tree_oak_autumn_leaves_00"),
            ("tree_oak_leaves_04", "tree_oak_autumn_leaves_04"),
            ("tree_oak_leaves_05", "tree_oak_autumn_leaves_04"),
            ("tree_oak_leaves_08", "tree_oak_autumn_leaves_08"),
            ("tree_pine_frond_00", "tree_pine_frond_autumn_00"),
            ("dire_tree007c", "dire_tree007c_winter"),
            ("dire_tree007_moss", "dire_tree007b_winter"),
        ],
    },
    Kind {
        id: "winter",
        name: "Winter trees",
        picture: "econ/terrain/winter_terrain",
        swaps: &[
            ("tree_oak_leaves_00", "tree_oak_leaves_00b_snow"),
            ("tree_oak_leaves_01", "tree_oak_leaves_01b_snow"),
            ("tree_oak_leaves_04", "tree_oak_leaves_04b_snow"),
            ("tree_oak_leaves_05", "tree_oak_leaves_05b_snow"),
            ("tree_blossoms_white_00", "tree_oak_leaves_04b_snow"),
            ("tree_redblossom_00", "tree_plumblossom_snow_00"),
            ("tree_plumblossom_00", "tree_plumblossom_snow_00"),
            ("tree_magical_white_00", "tree_magical_white_snow_00b"),
            ("tree_pine_frond_00", "tree_pine_frond_snow_00b"),
            ("tree_bamboo_leaves_01", "tree_bamboo_leaves_01b_snow"),
            ("dire_tree007c", "dire_tree007c_winter"),
            ("dire_tree007_moss", "dire_tree007_winter"),
        ],
    },
    Kind {
        id: "spring",
        name: "Spring trees",
        picture: "econ/terrain/spring_terrain",
        swaps: &[
            ("tree_oak_leaves_00", "tree_plumblossom_00"),
            ("tree_oak_leaves_01", "tree_plumblossom_00"),
            ("tree_oak_leaves_04", "tree_blossoms_white_00"),
            ("tree_oak_leaves_05", "tree_plumblossom_00"),
            ("tree_redblossom_00", "tree_plumblossom_00"),
            ("dire_tree007_moss", "dire_tree007_moss_spring"),
        ],
    },
];

pub fn kind(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|kind| kind.id.eq_ignore_ascii_case(id.trim()))
}

pub fn is_pick(pick: &Pick) -> bool {
    pick.hero.is_none() && pick.slot.eq_ignore_ascii_case(SLOT)
}

impl Kind {
    fn path(material: &str) -> String {
        format!("{FOLDER}{material}.vmat_c")
    }

    pub fn is_there(&self, archive: &Archive) -> bool {
        self.swaps.iter().all(|(own, other)| archive.contains(&Kind::path(own)) && archive.contains(&Kind::path(other)))
    }

    pub fn files(&self, archive: &Archive) -> Result<Vec<NewFile>> {
        let gone = |material: &str| Error::NotFound(format!("this copy of Dota does not have the material {material}, which {} need", self.name.to_lowercase()));
        let mut files = Vec::new();
        for (own, other) in self.swaps {
            if !archive.contains(&Kind::path(own)) {
                return Err(gone(own));
            }
            let material = archive.read(&Kind::path(other))?.ok_or_else(|| gone(other))?;
            files.push((Kind::path(own), material));
        }
        Ok(files)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::vpk::{self, PackFile};

    pub(crate) fn archive_with(paths: &[String]) -> Archive {
        let files: Vec<PackFile> = paths.iter().map(|path| PackFile { path, data: path.as_bytes() }).collect();
        Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&files).unwrap()).unwrap()
    }

    pub(crate) fn trees_of(kind: &Kind) -> Vec<String> {
        let mut paths: Vec<String> = kind.swaps.iter().flat_map(|(own, other)| [Kind::path(own), Kind::path(other)]).collect();
        paths.sort();
        paths.dedup();
        paths
    }

    #[test]
    fn the_kinds_are_told_apart_and_swap_each_material_once() {
        for (place, kind) in KINDS.iter().enumerate() {
            assert!(KINDS[..place].iter().all(|other| other.id != kind.id && other.name != kind.name), "{}", kind.id);
            assert!(!kind.swaps.is_empty(), "{}", kind.id);
            for (at, (own, other)) in kind.swaps.iter().enumerate() {
                assert!(kind.swaps[..at].iter().all(|(earlier, _)| earlier != own), "{own} is swapped twice");
                assert_ne!(own, other);
                assert!(kind.swaps.iter().all(|(swapped, _)| swapped != other), "{other} is swapped out and in");
                let sort = |material: &str| ["pine", "bamboo", "dire"].into_iter().find(|sort| material.contains(sort)).unwrap_or("oak");
                assert_eq!(sort(own), sort(other), "{other} is no stand-in for {own}");
            }
            assert_eq!(super::kind(&kind.id.to_uppercase()), Some(kind));
        }
        assert_eq!(kind("palms"), None);
    }

    #[test]
    fn a_pick_for_the_trees_is_told_by_its_slot() {
        assert!(is_pick(&Pick::world("trees", "autumn")));
        assert!(is_pick(&Pick::world("Trees", "anything")));
        assert!(!is_pick(&Pick::world("courier", "autumn")));
        assert!(!is_pick(&Pick::hero("npc_dota_hero_treant", "trees", "autumn")), "no hero's slot");
    }

    #[test]
    fn a_seasons_material_goes_in_the_package_under_the_name_of_the_one_it_stands_in_for() {
        let spring = kind("spring").unwrap();
        let files = spring.files(&archive_with(&trees_of(spring))).unwrap();
        let told: Vec<(&str, &str)> = files.iter().map(|(path, data)| (path.as_str(), std::str::from_utf8(data).unwrap())).collect();
        assert_eq!(
            told,
            [
                ("materials/models/props_tree/tree_oak_leaves_00.vmat_c", "materials/models/props_tree/tree_plumblossom_00.vmat_c"),
                ("materials/models/props_tree/tree_oak_leaves_01.vmat_c", "materials/models/props_tree/tree_plumblossom_00.vmat_c"),
                ("materials/models/props_tree/tree_oak_leaves_04.vmat_c", "materials/models/props_tree/tree_blossoms_white_00.vmat_c"),
                ("materials/models/props_tree/tree_oak_leaves_05.vmat_c", "materials/models/props_tree/tree_plumblossom_00.vmat_c"),
                ("materials/models/props_tree/tree_redblossom_00.vmat_c", "materials/models/props_tree/tree_plumblossom_00.vmat_c"),
                ("materials/models/props_tree/dire_tree007_moss.vmat_c", "materials/models/props_tree/dire_tree007_moss_spring.vmat_c"),
            ]
        );
        assert_eq!(kind("winter").unwrap().files(&archive_with(&trees_of(kind("winter").unwrap()))).unwrap().len(), 12);
        assert_eq!(kind("autumn").unwrap().files(&archive_with(&trees_of(kind("autumn").unwrap()))).unwrap().len(), 7);
    }

    #[test]
    fn trees_the_game_does_not_have_are_not_offered() {
        let autumn = kind("autumn").unwrap();
        let all = trees_of(autumn);
        assert!(autumn.is_there(&archive_with(&all)));
        for missing in ["tree_oak_autumn_leaves_08", "tree_pine_frond_00"] {
            let rest: Vec<String> = all.iter().filter(|path| **path != Kind::path(missing)).cloned().collect();
            let archive = archive_with(&rest);
            assert!(!autumn.is_there(&archive), "{missing}");
            assert!(matches!(autumn.files(&archive), Err(Error::NotFound(why)) if why.contains(missing)), "{missing}");
        }
        assert!(!kind("winter").unwrap().is_there(&archive_with(&all)));
    }
}
