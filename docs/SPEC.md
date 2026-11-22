# How Dota 2 Mod Enjoyer works

This is the specification the code is written from. Everything in it was worked out from the files
of an ordinary Dota 2 install (game build 6973, November 2026) and can be re-checked against any
install with the `dme dev` commands. Facts marked **(confirmed in game)** were also verified by
launching Dota; facts marked **(to confirm in game)** have not been yet.

Paths below are relative to the Dota folder, normally
`C:\Program Files (x86)\Steam\steamapps\common\dota 2 beta`.

## 1. Files

| File | Used for | Access |
|---|---|---|
| `game\bin\win64\dota2.exe` | Proof that a folder is a Dota install | exists? |
| `game\dota\steam.inf` | Game version | read |
| `game\dota\pak01_dir.vpk`, `pak01_NNN.vpk` | The game's own content archive | read |
| `game\dota\gameinfo.gi` | The game's list of content folders | read |
| `game\dota\gameinfo_branchspecific.gi` | Where our content folder is added | read, **edit** |
| `game\bin\win64\dota.signatures` | Where the edited file is declared | read, **edit** |
| `game\mod_enjoyer\pak01_dir.vpk` | Our mod package | **create, delete** |

Read from inside the game's archive:

| File in the archive | Used for |
|---|---|
| `scripts/items/items_game.txt` | The item database (2.6) |
| `panorama/images/..._png.vtex_c` | Pictures of heroes, items, sets and abilities (2.7) |
| `resource/localization/items_english.txt`, `dota_english.txt`, `abilities_english.txt` | Names (2.8) |
| `scripts/npc/heroes/npc_dota_hero_*.txt` | Each hero's slots, their names, the hero's short names (2.10) and its slots for ability effects (2.9) |
| `materials/models/props_tree/*.vmat_c` | The looks of trees, to copy into the package (3.17) |
| `materials/dev/deferred_post_process*.vmat_c` | What the map is drawn through, to copy into the package (3.20) |
| `scripts/npc/items.txt`, `scripts/npc/npc_abilities.txt` | What an item is built from, and what a talent changes, for the Builds page (6.2) |
| `panorama/images/items/*_png.vtex_c`, `panorama/images/spellicons/*_png.vtex_c` | Pictures of the items and skills of a build (6.2) |
| `panorama/images/rank_tier_icons/handicap/*icon_psd.vtex_c` | The game's drawings for the five roles, shown on the Draft page (5.3) |
| `panorama/images/rank_tier_icons/rank1_psd.vtex_c` to `rank8_psd.vtex_c` | The game's medals for the eight ranks, shown on the Draft page (5.3) |

Nothing else under the Dota folder is written. Our own files live in
`%LOCALAPPDATA%\Dota2ModEnjoyer`:

| File | Holds |
|---|---|
| `picks.json` | What the player picked, and whether the picks are put away (3.12) |
| `state.json` | What was last put in the game, for which game build (3.12) |
| `settings.json` | What the app remembers about itself: that the notice was read, a Dota folder pointed out by hand, the mode for a low-end computer (3.18), the version whose news was last shown (4.2), that the window could not be made see-through on this computer (3.13), the rank and role chosen on the Draft page (5.3) |
| `presets.json` | The picks a player kept under a name in the app (3.13) |
| `crash.txt` | What made the app close, when something went wrong inside it; read and taken away at the next start (3.13) |
| `draft.json` | The numbers of the draft helper as they were last fetched (5.4) |
| `builds.json` | The builds of the build helper as they were last fetched (6.4) |
| `backup\<game build>\` | The two game files as Valve shipped them |

One more file is written, and only when the player asks for it: the app's own program file,
when the app updates itself (4).

## 2. Valve's formats

### 2.1 KeyValues text

Most of Valve's text files use one syntax:

```
"key"    "value"
"key"
{
    "nested key"    "value"
}
```

- A token is either a quoted string or a bare word. A bare word ends at whitespace, `{`, `}` or `"`.
- `//` starts a comment that runs to the end of the line.
- Inside a quoted string a backslash escapes the next character (`\\`, `\"`).
- Keys are compared without regard to upper and lower case. A key may appear more than once.
- Files may start with a UTF-8 byte order mark. Line endings are CRLF in the files Valve ships.

### 2.2 VPK archive

A VPK is a directory file (`pak01_dir.vpk`) plus optional numbered data files (`pak01_000.vpk` ...).
All numbers are little-endian.

Header, version 2 (28 bytes):

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | Magic `0x55AA1234` |
| 4 | 4 | Version (`2`; version `1` has only the first 12 bytes) |
| 8 | 4 | Tree size in bytes |
| 12 | 4 | Size of file data stored inside the directory file |
| 16 | 4 | Size of the archive checksum section |
| 20 | 4 | Size of the "other" checksum section (`48`) |
| 24 | 4 | Size of the signature section |

The tree follows the header. It is three nested lists of NUL-terminated strings:

```
extension
    folder
        file name, then an 18-byte entry, then that entry's preload bytes
        ...
        ""            (end of this folder)
    ...
    ""                (end of this extension)
...
""                    (end of tree)
```

A single space stands for "no extension" or "root folder". The full path of a file is
`folder/name.extension`.

Entry (18 bytes):

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | CRC32 of the file |
| 4 | 2 | Number of preload bytes that follow the entry |
| 6 | 2 | Archive index; `0x7FFF` means "inside the directory file" |
| 8 | 4 | Offset of the data |
| 12 | 4 | Length of the data (not counting preload bytes) |
| 16 | 2 | Terminator `0xFFFF` |

File contents are the preload bytes followed by the data. With archive index `0x7FFF` the data
starts at `header size + tree size + offset` in the directory file; otherwise it starts at `offset`
in `pak01_<index, three digits>.vpk`.

After the tree and the embedded file data, a version 2 file has:

1. The archive checksum section: 28-byte records for the numbered data files. Empty when all data is
   embedded.
2. The "other" checksum section, three MD5 values of 16 bytes each:
   the tree; the archive checksum section; and everything in the file before this third value.
3. The signature section. May be empty.

The package we write holds its files embedded: the item database and, for some picks, a few
small files beside it (3.17, 3.20). It has correct CRC32 and MD5 values and an empty signature
section. Dota mounts it. **(confirmed in game)**

### 2.3 `dota.signatures`

A text file with CRLF line endings and no byte order mark. Each line declares one game file:

```
...\..\..\dota\gameinfo_branchspecific.gi~SHA1:224B6F879D00D419B6A10FE07B4DDB3D827FD31F;CRC:7B89F784
```

- `SHA1` is the SHA-1 of the file, as 40 upper-case hex digits.
- `CRC` is the CRC32 of the file with its four bytes printed in little-endian order. A CRC32 of
  `0x84F7897B` is written `7B89F784`.

The last line Valve writes is `DIGEST:` followed by 1,024 hex digits and CRLF. It covers the lines
above it, so those cannot be changed.

A line added after the `DIGEST` line that declares `gameinfo_branchspecific.gi` again, with the
hash of an edited copy, makes the game accept the edited copy. **(confirmed in game)**

### 2.4 `gameinfo.gi` and `gameinfo_branchspecific.gi`

Both are KeyValues files with a `GameInfo` root. `gameinfo.gi` holds the list of content folders:

```
FileSystem
{
    SearchPaths
    {
        Game_AudioLanguage      dota_*LANGUAGE*
        Game_LowViolence        dota_lv
        Game                    dota
        Game                    core
        Mod                     dota
        Write                   dota
        ...
    }
}
```

Folders are relative to `game\`. Earlier entries win over later ones. In each folder the game mounts
`pak01_dir.vpk` if there is one.

`gameinfo_branchspecific.gi` is merged over `gameinfo.gi`. As shipped it has a `FileSystem` block
with only app ids. When it also has a `SearchPaths` block, that block takes the place of the one in
`gameinfo.gi`, so it must repeat every entry. **(confirmed in game)**

### 2.5 `steam.inf`

Lines of `Name=Value`. `ClientVersion` is the game build number and `VersionDate` its date.

### 2.6 `scripts/items/items_game.txt`

The item database, a 50 MB KeyValues file inside the game archive. Root key `items_game`. The
sections used here:

- `prefabs` holds named sets of default values. An item names its prefab with `"prefab"`; any key
  the item does not set takes the prefab's value. `default_item` and `wearable` both default
  `item_slot` to `weapon`; `courier`, `ward`, `terrain` and the like default it to their own name.
- `player_loadout_slots` is a numbered list of the slots that belong to the player rather than to
  a hero: `courier`, `ward`, `terrain`, `hud_skin`, `music`, `announcer` ... (32 on build 6973).
  This document calls them world slots.
- `items` has one block per item, keyed by its numeric id. Keys used here:

  | Key | Meaning |
  |---|---|
  | `name` | Unique internal name. Sets refer to items by this. |
  | `prefab` | `default_item` for a hero's built-in look, `wearable` for a cosmetic, `courier`, `ward` ... for world items. |
  | `used_by_heroes` | Block whose keys are hero ids such as `npc_dota_hero_axe`. World items have none. |
  | `item_slot` | Where the item goes: `weapon`, `head`, `belt` ... or a world slot. |
  | `baseitem` | `1` on items the game hands to everybody. |
  | `model_player` | The model file, such as `models/items/axe/.../x.vmdl`. |
  | `visuals` | Everything else the item changes: models of couriers and wards, particles, sounds, icons, maps, styles. |
  | `item_name` | Token of the display name; the text is in `resource/localization/items_english.txt`. |
  | `item_rarity` | How rare the item is: the id of one of the `rarities`. |

- `item_sets` has one block per set, keyed by a short name:

  ```
  "armor_of_the_shattered_vanguard"
  {
      "name"            "#DOTA_Set_Armor_of_the_Shattered_Vanguard"
      "items"
      {
          "Axe of the Shattered Vanguard"     "1"
          ...
      }
      "store_bundle"    "Armor of the Shattered Vanguard"
      "portrait_image"  "econ/sets/v2/armor_of_the_shattered_vanguard"
  }
  ```

  `store_bundle` is the `name` of the item the store sells the whole set as.

- `rarities` has one block per rarity, keyed by its id. `value` puts them in order, the commonest
  lowest; `loc_key` is the token of the name; `color` is the key of an entry of `colors`.
- `colors` has one block per colour, with `hex_color` written `#rrggbb`.

  On build 6973:

  | Rarity | `value` | Colour |
  |---|---|---|
  | `common` | 1 | `#b0c3d9` |
  | `uncommon` | 2 | `#5e98d9` |
  | `rare` | 3 | `#4b69ff` |
  | `mythical` | 4 | `#8847ff` |
  | `legendary` | 5 | `#d32ce6` |
  | `ancient` | 6 | `#eb4b4b` |
  | `immortal` | 7 | `#e4ae39` |
  | `arcana` | 8 | `#ade55c` |
  | `seasonal` | 9 | `#fff34f` |

#### Slots and their default items

When a player has put nothing in a slot, the game shows the slot's default item. That is the one
thing this tool builds on.

- For a hero slot, the default is the item with prefab `default_item` that is used by that hero
  and has that slot. Besides `weapon`, `head` and so on there are special slots:
  `hero_base` (arcanas that replace the hero's model), `persona_selector` (personas), the slots a
  persona wears (`weapon_persona_1` ...), `voice`, `costume`, `summon`. Build 6973 has 838 hero
  slots over 127 heroes. One default item is shared: "All Heroes' Default Pet" (8366) is the
  default of `summon` for ten heroes. The item database also keeps default items for slots a
  hero had once and has no more; which slots a hero has today is in the hero's own file (2.10).
- An item goes in a world slot if its own `item_slot` is a world slot, or else if its prefab's is.
  (The default cursor pack and the default Roshan carry a stray
  `item_slot "weapon"` of their own.) The default of a world slot is the item with `baseitem "1"`
  and no heroes that goes in it; where there are several, the one with the lowest id. Build 6973
  has a default for 20 world slots: `announcer`, `mega_kills`, `courier`, `music`, `ward`,
  `hud_skin`, `loading_screen`, `weather`, `cursor_pack`, `terrain`, `radiant_creeps`,
  `dire_creeps`, `radiant_towers`, `dire_towers`, `versus_screen`, `streak_effect`, `kill_effect`,
  `radiant_siege_creeps`, `dire_siege_creeps`, `roshan`.
- Slots without a default cannot be changed by rewriting a default item: `heroic_statue`,
  `multikill_banner`, `teleport_effect`, `blink_effect`, `emblem`, `death_effect`, `head_effect`,
  `map_effect`, `courier_effect`, `tormentor`, `ancient`, `pet_effigy`. What the items of four
  of them do, this tool does another way: `teleport_effect` and `blink_effect` (3.16), `emblem`
  (3.19) and `map_effect` (3.20).

An item fits a hero slot if that hero uses it, it has that slot and it is not a default item. It
fits a world slot if no hero uses it, it goes in that slot and it is not a base item.

#### Styles

Some items come in several styles. They are described inside `visuals`:

```
"visuals"
{
    "styles"
    {
        "0" { "name" "#DOTA_Style_X_0"  "skin" "0"  "alternate_icon" "0" }
        "1" { "name" "#DOTA_Style_X_1"  "skin" "1"  "alternate_icon" "1"  "model_player" "models/.../y.vmdl" }
    }
    "alternate_icons"
    {
        "1" { "icon_path" "econ/items/.../x_style1" }
    }
    "asset_modifier" { "type" "particle_create"  "modifier" "particles/.../gold.vpcf"  "style" "1" }
}
```

A style may name a `skin` of the model, a `model_player` of its own and an `alternate_icon`, which
is a key of `alternate_icons`. A block in `visuals` with `"style" "N"` applies only to style N; one
without applies to every style. An item that nobody chose a style for shows style 0. Style names
are in `resource/localization/dota_english.txt`.

On build 6973, 560 items have styles. In 556 of them the style blocks are named `0`, `1`, `2` ...
in order; the other four are treated as having no styles.

Some styles are not for a player to choose. A style block can carry an `auto_style_rule`, as in
`"auto_style_rule" "modifier_sven_gods_strength > 0"`. The game then shows that style by itself
while the rule holds. 21 items on build 6973 have such styles; the Magus Cypher, Rubick's arcana,
has 41 of them, one for each number of spells stolen. They are states of the item, and this tool
offers none of them as a choice (3.4).

#### Files that items name

A model `x.vmdl` is in the game archive as `x.vmdl_c`, an effect `x.vpcf` as `x.vpcf_c`. A map is
not in the archive: `maps/scenes/x.vmap` is a package of its own, `game\dota\maps\scenes\x.vpk`.

Build 6973 has 26,661 items and 1,922 sets. 13,426 items fit a slot that has a default.
Every set resolves to its items by name.

### 2.7 Pictures

The pictures the game's own menus show are in the archive as compiled textures. The picture an
item calls `econ/items/axe/x` (its `image_inventory`) is
`panorama/images/econ/items/axe/x_png.vtex_c`. A hero's portrait is
`panorama/images/heroes/<hero id>_png.vtex_c`. A set's picture is its `portrait_image`; most sets
also have a smaller one, with `_small` added to the name. An ability's picture is
`panorama/images/spellicons/<ability>_png.vtex_c`, and a Dota item's, such as the Blink
Dagger's, `panorama/images/items/<item, without item_>_png.vtex_c`. The logo of a professional
team is `panorama/images/teams/<the team's number>_png.vtex_c`, and some teams and players
have a picture as a brand, `panorama/images/econ/brands/brand_<name>_png.vtex_c`.

A compiled texture file is a head followed by pixel data. Numbers are little-endian. An offset
counts from the position of the number that states it.

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | Length of the head. The pixel data starts here. |
| 8 | 4 | Offset of the list of blocks |
| 12 | 4 | Number of blocks |

An entry in the list of blocks is 12 bytes: a name of four letters, the offset of the block, its
length. The block named `DATA` is the texture header:

| Offset | Size | Field |
|---|---|---|
| 20 | 2 | Width |
| 22 | 2 | Height |
| 26 | 1 | Format of the pixel data |
| 27 | 1 | In how many sizes the picture is stored |
| 32 | 4 | Offset of the list of extra entries |
| 36 | 4 | Number of extra entries |

An extra entry is 12 bytes: a kind, the offset of its data, the length of its data. Kind 3 is there
when the picture was padded to make a texture whose sides are powers of two. Its data has the
picture's own width at byte 2 and height at byte 4, two bytes each; the picture is the top left
part of the texture.

The pixel data, by format, with the number of pictures on build 6973 among the 17,208 the app
can show:

| Format | Pixel data | Pictures |
|---|---|---|
| 16, 18 | A PNG file | 11,139 and 3,753 |
| 2 | DXT5 blocks | 2,296 |
| 28 | Four bytes a pixel: blue, green, red, opacity | 20 |

Where a picture is stored in several sizes, the full size is last.

DXT5 stores each 4 by 4 pixels in 16 bytes. Eight are for opacity: two levels, then sixteen
3-bit numbers that each choose one of the two or a level between them (six in between if the
first level is the larger; otherwise four, plus none and full). Eight are for colour: two colours
in 5-6-5 bits, then sixteen 2-bit numbers that each choose one of the two or one of the two
colours a third of the way between them.

The head of a texture file holds, as text, notes on the steps the game's compiler took. If the
notes have the word `YCoCg` in them, the four bytes of a pixel of a DXT5 picture hold brightness
and colour differences in place of red, green, blue and opacity: `Co`, `Cg`, a scale and
brightness `Y`. With `s = (scale >> 3) + 1`, `co = (Co - 128) / s` and `cg = (Cg - 128) / s`:

```
red = Y + co - cg        green = Y + cg        blue = Y - co - cg
```

Such a picture has no see-through parts. Pictures stored as PNG are red, green and blue whatever
the notes say.

All 17,208 pictures decode. Heroes' portraits, items, sets, abilities and loading screens of
every format were looked at and are right.

### 2.8 Names

The game's texts are in `resource/localization/<file>_english.txt` in the archive. These are
KeyValues files with a `lang` root and a `Tokens` block of `"token" "text"` pairs. Tokens are
compared without regard to upper and lower case. Where the item database refers to a token it
writes `#` in front.

| Name of | File | Token |
|---|---|---|
| An item | `items_english` | The item's `item_name` |
| A set | `items_english` | The set's `name` |
| A style | `dota_english` | The style's `name` |
| A slot | `dota_english` | `LoadoutSlot_<slot>`, as in `LoadoutSlot_offhand_weapon` |
| A rarity | `dota_english` | The rarity's `loc_key`, as in `Rarity_Mythical` |
| A hero | `abilities_english` | `<hero id>:n`, as in `npc_dota_hero_axe:n` |
| An ability | `abilities_english` | `DOTA_Tooltip_ability_<ability>`, as in `DOTA_Tooltip_ability_juggernaut_blade_fury` |

Some texts start with a note on grammar for translators, as in `#|f|#Crystal Maiden`; the name is
what follows the note.

A persona has slots of its own, named after the hero's: `weapon_persona_1` next to `weapon`. The
game gives both the same text ("Weapon"), so where both are listed the persona's is shown as
"Persona Weapon".

### 2.9 Ability effects

An item that changes how an ability looks or sounds does so in its `visuals`, like everything
else it changes (2.6). A block of type `particle` puts one effect in the place of another,
`ability_icon` changes the ability's picture, `sound` one of its sounds:

```
"asset_modifier"
{
    "type"        "particle"
    "asset"       "particles/units/heroes/hero_juggernaut/juggernaut_blade_fury.vpcf"
    "modifier"    "particles/econ/items/juggernaut/jugg_ti8_sword/juggernaut_blade_fury_abyssal.vpcf"
    "apply_when_equipped_in_ability_effects_slot"    "1"
}
```

Dota lets a player use such effects without wearing the item they come with. Three things in
the game's files describe that:

- Each hero has a file of its own in the archive, `scripts/npc/heroes/<hero id>.txt`:
  KeyValues, root `DOTAHeroes`, then the hero's id. Its `ItemSlots` block lists the hero's
  slots, each with a `SlotName`. Besides the slots that things are worn in, there are slots
  named `ability_effects_<N>`, each for one ability; the slot's `SlotText` is the internal name
  of that ability, such as `juggernaut_blade_fury`. A persona's are named
  `ability_effects_<N>_persona_1`.
- An item that can go in such a slot says so in its `static_attributes`, with an attribute
  called `can_equip_as_ability_effects` whose value is the number N. The file writes the
  attribute in two ways, `"can_equip_as_ability_effects" "4"` and

  ```
  "can_equip_as_ability_effects"
  {
      "attribute_class"    "can_equip_as_ability_effects"
      "value"              "4"
  }
  ```

  and an item that can go in several slots has it once for each.
- The blocks of the item's `visuals` that are its effects for slot N are marked with
  `"apply_when_equipped_in_ability_effects_slot" "N"`, as above. Worn as a whole, the item
  brings all its blocks; in the slot for ability effects, those.

The item database has a default item for each of these slots, "All Heroes' Default Ability
Effects 1" and so on (ids 790 to 798, and 914 to 922 for a persona's). Each is used by `all`
heroes, and none has `visuals`.

Measured on build 6973 with `dme dev effects`:

- 120 of the 129 hero files list such slots, 355 in all, numbered 1 to 9. Eight are a persona's
  (Anti-Mage and Pudge).
- 507 items have the attribute. All are `wearable`, each is one hero's, and each is worn in a
  slot its hero has a default item for. 469 can go in one slot and 38 in several, up to five.
  436 are immortal.
- 2,115 blocks are marked: 1,193 `particle`, 587 `ability_icon`, 216 `sound`, 60
  `particle_combined`, 28 `entity_model` and 31 of eight other types.
- 24 items have blocks marked for a style other than the first, and one of them, Invoker's
  Dark Artistry Hair, has its effects in its second style only.
- Loose ends in the data: 22 times an item has the attribute for a number it marks nothing
  for; a few items mark blocks without having the attribute; and two items have the attribute
  for a slot their hero's file does not list.

### 2.10 A hero's own file

`scripts/npc/heroes/<hero id>.txt` (2.9) says three more things this tool goes by.

- `ItemSlots` lists the slots the hero has, in the order of Dota's own loadout. The item
  database is not a guide to that: on build 6973 it has default items for nine slots that four
  heroes have no more (Tiny's `head`, `armor`, `arms` and `offhand_weapon`, three of Troll
  Warlord's, one each of Keeper of the Light and Luna). An item picked for such a slot would
  show nowhere.
- A slot's `SlotText` is the token of its name, such as `#LoadoutSlot_Tiny_Body_0` ("Tiny") for
  Tiny's `shoulder`, the slot that holds what he looks like while he is small. For a slot for
  ability effects the text is the ability instead (2.9). `DisplayInLoadout "0"` marks a slot
  Dota does not show.
- `Model` is the model of the hero's body, `models/heroes/axe/axe.vmdl`. Tiny, who grows, has
  one for each size: `Model`, `Model1`, `Model2` and `Model3`. The item database names those
  sizes as heroes of their own, `npc_dota_hero_tiny_variant_0` to `_3`.
- `NameAliases` holds other names for the hero, the short ones Dota's own search finds the hero
  by, separated by `;` or `,`: `am` for Anti-Mage, `wr` for Windranger. 116 heroes have some.

### 2.11 Effects that are not part of an item

The section `attribute_controlled_attached_particles` of the item database lists 828 effects
that can be attached to something, each under a number:

```
"15"
{
    "system"          "particles/econ/courier/courier_trail_01/courier_trail_01.vpcf"
    "attach_type"     "absorigin_follow"
    "attach_entity"   "parent"
}
```

Most are parts of items. An item's `visuals` names the effects it comes with in blocks of type
`particle_create`, and 1,401 of those blocks name a system that is in this list, which says how
to attach it. 43 of the effects have a name of their own, under the token `Attrib_Particle<N>`
in `dota_english.txt`: Ethereal Flame (15), Frostbloom (826) and so on. Those are the effects
that a gem can add to an item in Dota. 35 of the 43 are made for couriers (their system is in a
folder named `courier`); the other eight are seasonal effects that anything can have.

An effect can have a `default_color` block with `r`, `g` and `b`, the colour it shows in when it
is given none. 31 effects have one. That is not the list of effects that take a colour: of the
named effects for couriers, 25 have a `default_color` and ten have none, Ethereal Flame among
them, which Dota players know in many colours. The files do not say which effects take one.

The `colors` section has, besides the colours of rarities (2.6), 42 colours for effects. Their
ids start with `unusual_`; each has a `hex_color` and a `color_name` token, as in
`unusual_ship`, `#191970`, "Ships in the Night". In Dota a colour comes as a gem too, the
Prismatic gem (2.12). This tool once offered them and no longer does (3.15).

An effect is told things through its control points, and the item database sets them with
blocks of type `particle_control_point`, 128 of them on build 6973. What a point means is the
effect's own business. Of the effects of single items, some read a colour from point 15, some
from 16, some from 1. One effect of the game's own shows the pattern this tool goes by: point 15
holds a colour as `red green blue`, and point 16 says with `1 0 0` that a colour is given. A
terrain colours the frame that is dragged around units like this:

```
"asset_modifier"
{
    "type"                    "particle_control_point"
    "asset"                   "particles/ui_mouseactions/bounding_area_view.vpcf"
    "control_point_number"    "15"
    "cp_position"             "0 255 0"
}
```

followed by the same for point 16 with `1 0 0`. The effects that gems colour in Dota are taken
to read the two points the same way; the files do not say so. They do show that such a block
reaches an effect the item itself comes with: the 10th Anniversary Relic Sword (24609) sets
point 16 of the effect it names in a `particle_create` block.

An event, such as a Battle Pass, changes common effects for whoever took part: a teleport, the
use of a Blink Dagger, a hero's levelling up. The item database has one item for each event's
effects, such as "The International 2019 Effects" (17482). Such an item has prefab `tool`,
`item_class` `dota_item_wearable`, no heroes and no slot. Its `visuals` are blocks that put the
event's effect in the place of the game's:

```
"asset_modifier"
{
    "type"        "particle"
    "asset"       "particles/items2_fx/teleport_start.vpcf"
    "modifier"    "particles/econ/events/ti9/teleport_start_ti9_lvl2.vpcf"
    "criteria"    "isbetween( effects_mask, 2, 3, 3, 5 )"
}
```

`criteria` says which players get the block: those who reached a certain level in the event.
An event with three levels of an effect has three such blocks for it, and the blocks of one
level say the same in their `criteria`; the file is not consistent about the spaces. Items
that show a team's effect have no `criteria`. A few blocks are of type `particle_control_point`
and change a setting of the game's effect rather than replace it.

Build 6973 has 56 such items. Between them they replace 46 different effects of the game's.
Six more items are made for two world slots the game has for such effects, `teleport_effect`
and `blink_effect` (2.6). They hold the same kind of blocks, without `criteria`.

An effect is a file that names the files it is made of in turn, which are other effects and the
pictures they are drawn with (2.13). Many of those parts are not in the game's own archive but
in that of `game\core`, such as `materials/particles/powerups/healsign.vtex`. Looked for in both,
every effect that the events' items bring is whole on build 6973. (An earlier count looked in
the game's archive only, and took 54 of them for incomplete.)

### 2.12 Sockets and gems

In Dota an item can have sockets, and a gem in a socket adds something to the item: an effect,
a colour, a counter, another look for an ability. What sits in a socket is an attribute of the
item. The `attributes` section of the item database defines `socket 1`, `socket 2` ... for the
gems a player puts in, which are kept with the player's own copy of the item and not in the
game's files, and `static socket 1` to `static socket 5` for the gems an item comes with.
Those are written in the item's `static_attributes`:

```
"static_attributes"
{
    "static socket 1"
    {
        "attribute_class"    "socket"
        "value"              "socket { required_hero: 'npc_dota_hero_necrolyte' gem_def_index: 3005 } asset_modifier: 91"
    }
}
```

The value names the gem (`gem_def_index`, the id of the gem's own item) and then says what the
gem holds. Gems are the items with prefab `socket_gem`. A static attribute `gem type` says
what sort of gem each is, and `required socket type` which sort of socket it sits in:

| `gem type` | Gems | What the socket says it holds |
|---|---|---|
| 1 | Ethereal Gem (3000), Unusual Effect Gem (3045) | `effect: N`, a number of the list of 2.11 |
| 2 | Prismatic Gem (3001) | a colour: `red: R green: G blue: B` |
| 3 | Inscribed Gem and 60 others | a counter |
| 5 | Kinetic Gem (3005) | `asset_modifier: N` |
| 6, 7 | Autograph runes, Spectator gems | a signature, a team |

On build 6973 the file writes 120 sockets out: 108 with an asset modifier and one of a
Spectator gem, in items, and 11 with an effect, in the section `items_unusual_lists`. None has
a colour. How one is written follows from the game's own description of what a socket holds,
whose names are in its program files: a socket with a colour has a `socket`, and `red`,
`green` and `blue`.

The number after `asset_modifier:` in the socket of a Kinetic gem is a key of the section
`asset_modifiers` of the item database, which has 163 entries of this shape:

```
"91"
{
    "name"       "necronub_base_attack"
    "loc_key"    "#DOTA_Asset_Mod_Necronub_Base_Attack"
    "asset_modifier0"
    {
        "type"        "ability_icon"
        "asset"       "necrolyte_ghost_shroud"
        "modifier"    "necrolyte/necronub/necrolyte_sadist"
    }
    "asset_modifier1"
    {
        "type"        "particle"
        "asset"       "particles/units/heroes/hero_necrolyte/necrolyte_base_attack.vpcf"
        "modifier"    "particles/econ/items/necrolyte/necronub_base_attack/necrolyte_base_attack_ka.vpcf"
    }
}
```

Its blocks are of the kinds an item's `visuals` have (2.9): 151 change an ability's icon, 45
an animation and 41 an effect. So a part of what such an item changes is in the gem the item
comes with and not in its `visuals`. 106 wearables and one bundle come with gems, 108 between
them, at most two in one item.

The game's program files name what the game on a player's computer is told about each unit by
the server it plays on. Among what every unit has are `m_bHasColorGem` and `m_colorGemColor`;
Terrorblade has an `m_nArcanaColor` besides; and the messages that create a projectile carry a
`colorgemcolor`. So the colour of a gem is worked out by the server, from the item the player
really has, and sent to everybody. The game on the player's own computer does not work it out
from its item database. All the item database has on the matter is the key
`affected_by_color_gems`, on one block each of two of Terrorblade's items.

An arcana's level is decided in the same way: every unit has an `m_nArcanaLevel` that the
server tells the game. Some blocks of an item's `visuals` name a level, as in
`"required_arcana_level" "1"`. They show only for a player whose arcana has reached it. Build
6973 has 38 such blocks; seven items that can be picked have some for a level above 0
(Earthshaker's Arms of Deep Magma, Faceless Void's Bracers of Aeons and a few more). A block for
level 0 shows for everybody; one for a higher level cannot show through a default item.

### 2.13 What a compiled file names

The game's files are compiled: `x.vpcf` is in the archive as `x.vpcf_c`. Such a file starts
the way a compiled texture does (2.7): its length, two version numbers, then where its list of
blocks is and how many there are. Each entry of the list is a name of four letters, where the
block is (counted from that number) and how long it is.

The block `RERL` lists the other files the file names. It starts with where its entries are
(counted from that number) and how many there are. Each entry is 16 bytes: a number the game
knows the file by, eight bytes; where its path is, counted from those four bytes; four bytes
more. A path ends with a zero byte and has no `_c`.

A file named that way is looked for in the game's archive, then in that of `game\core`, which
holds what Valve's games share (`game\core\pak01_dir.vpk`). A picture of the game's interface,
such as a loading screen's, is named from inside `panorama/images/`: a material names
`loadingscreens/x/loadingscreen.vtex`, and the archive has
`panorama/images/loadingscreens/x/loadingscreen.vtex_c`. Shaders (`.vfx`) are not files of the
archives and are not looked for.

A few files are not where they are named, and are one folder name away. Tyrian Latticean
Hierarchy (34620) names `particles/econ/items/nyx_assassin/nyx_ti9_immortal/nyx_ti9_tyrian_carapace_hit.vpcf`,
and the archive has that file under `particles/_2econ/...`. Eminence of Ristul (12930) names
two models in the folder `tpl_shoulder_lvl1` that are in `tpl_shoulder_lvl2` and
`tpl_shoulder_lvl3`. Where the game's own archive has exactly one file of the same name whose
path differs from the named one in one folder name, and in nothing else, that file stands in
for the named one: it counts as there, what it names is looked for in turn, and Play puts a
copy of it in the package under the name it is asked for by (3.2). With two such files, or
with one further away, nothing stands in. On build 6973 three files stand in.

### 2.14 Sound events

A sound in the game is an event with a name, such as `Hero_Axe.Attack`, which says which
recordings (`.vsnd`) to play and how: volume, pitch, how far it carries, and other events to
play with it (`soundevent_layer2` and so on). The events are in compiled files,
`soundevents/**/*.vsndevts_c`, 946 on build 6975. Each hero has one of its own in
`soundevents/game_sounds_heroes/`, which holds the hero's sounds and those of the hero's items:
Axe's has `Hero_Axe.Attack` and Fists of Axe Unleashed's `Hero_Axe.Attack.Jungle` side by side.
The 127 files of the heroes hold 2,943 events. Names are matched without regard to case: an item
of Razor's names an event that the file writes in other letters.

An item changes a sound with a block of type `sound` in its `visuals` (2.6), from `asset` to
`modifier`. On build 6975, 213 items of heroes do, 331 times in all.

A compiled file of events is a list of blocks, as in 2.13. The block `DATA` holds the events, as
KeyValues 3 in binary, of the fifth version (it starts with `\x05` and `3VK`):

- 16 bytes that name the format, then 25 four-byte numbers. The first says how the two buffers
  that follow are compressed (1 is LZ4, a single block each); the others say how big each part
  below is, and how many objects and arrays there are. Numbers 13 to 16 are the sizes of the two
  buffers, unpacked and packed, and 7 and 8 their sums.
- The first buffer: every string, each ending in a zero byte, in the order they are first used;
  then their number; then the numbers of arrays that keep them here (positions, below).
- The second buffer, in streams that are read side by side: the number of members of each
  object; one byte for the length of each short array; four-byte numbers, which are the index of
  a string for every key and every string value; eight-byte numbers, most of them doubles; one
  byte for each value saying what kind it is; and `00 DD EE FF` at the end.

The kinds the files use: 5 a double, 6 a string, 9 an object, 11 a four-byte number, 13 and 14
true and false, 15 and 16 the numbers 0 and 1, 17 and 18 the doubles 0 and 1, which carry nothing
more, 24 an array of up to 255 items of one kind, its kind written once after its length, and 25
the same with doubles from the first buffer (the `position` of a sound in the world). The other
blocks of the file, such as `RED2`, are about how it was made.

`dme dev sounds` reads every such file and writes it again: 940 come out the same, byte for byte
once unpacked, and 6 are compressed another way and are not read (music, fan voices and one of
the tools'; none is a hero's).

### 2.15 Particle files, and what holds an effect

An effect is a particle system, a compiled file `x.vpcf_c` (2.13), 83,719 of them on build
6986. Its block `DATA` holds a `CParticleSystemDefinition` as KeyValues 3 in binary: of the
fifth version (2.14) in 9,756 files that were compiled lately, and of the second, third or
fourth in the other 73,963, the files of every emblem among them.

Those earlier versions start with their number and `3VK`, 16 bytes that name the format, and a
header of four-byte numbers: how the values are compressed (0 not at all, 1 as one block of
LZ4), a dictionary and a frame size (two bytes each), how many single bytes, four-byte numbers
and eight-byte numbers there are, the size of the strings and kinds together, how many objects
and how many arrays there are (two bytes each), the size of the buffer unpacked and packed, and
how many blocks of bytes follow (none in particle files). The fourth version has two numbers
more, the first of them the count of two-byte numbers (none either). Then comes one buffer, in
streams that are read side by side:

- single bytes: each boolean, and the length of each short array (kind 24, fourth version);
- padding to a multiple of four, then the four-byte numbers: first how many strings there are,
  then the index of a string for every key and every string value, whole numbers, and the
  length of every object, array (kind 8) and long typed array (kind 10);
- padding to a multiple of eight, then the eight-byte numbers, most of them doubles;
- the strings, each ending in a zero byte;
- one byte for each value saying what kind it is, the kinds of 2.14 and 10; a kind with its
  top bit set is followed by one byte more, a mark: `86 01` is a string that names another
  file of the game's, as `m_ChildRef` does for a part of the effect;
- `00 DD EE FF`.

The header counts objects and arrays of every kind, those inside typed arrays too. `dme dev
particles` reads every such file and writes it again: all 73,963 come out the same once
unpacked.

**What holds an effect.** A particle system is placed by its control points; the first, number
0, is where the effect is. The file says in itself what holds each point, in
`m_controlPointConfigurations`: a list of configurations (nearly always one, named `preview`),
each with `m_drivers`, one driver for a point:

```
m_iControlPoint   1                       (left out for point 0)
m_iAttachType     "PATTACH_POINT_FOLLOW"  (left out for the usual way: at the origin, following)
m_attachmentName  "attach_blade"
m_entityName      "self"                  ("parent", "self_with_wearables")
```

For an effect that an item creates (a block `particle_create`, 2.11), `self` is the item and
`parent` is the hero who wears it. That is what the game goes by, and not the name
`preview`: of the 2,742 systems that items create, 2,016 are not in the list of 2.11 at all,
and their files hold the attachment points their effects sit on (`attach_eye_l` on
`self_with_wearables`, `attach_blade` on `self`). Where a system is in the list too, the two
agree: an entry `absorigin_follow` on `parent` is a driver that names `parent` and no way of
holding, 77 times. Axe's Bloodchaser, an axe, creates its sparks on the ground that way.

An entry added to the list for a system whose file says otherwise changed nothing in a match
(2026-11-20): the file is what counts.

## 3. What the tool does

### 3.1 Find Dota

1. Collect Steam folders from the registry: `HKCU\Software\Valve\Steam` (`SteamPath`) and
   `HKLM\SOFTWARE\WOW6432Node\Valve\Steam` (`InstallPath`).
2. In each, read `steamapps\libraryfolders.vdf` (KeyValues) and collect every `path` value.
3. The first library that contains `steamapps\common\dota 2 beta\game\bin\win64\dota2.exe` wins.

A folder given by the user is accepted if it contains `game\bin\win64\dota2.exe`.

### 3.2 Build the package

The input is a list of picks. A pick says to show this item, in this style, in this slot. The slot
is a hero and one of its slots, or a world slot. Steps 2 and 3 here are about such picks. Other
picks say something else and are worked in as their own sections describe: a pick for one of a
hero's slots for ability effects (3.14), for the effect an event had for something (3.16), for an
emblem (3.19), for other trees (3.17), for another look of the map (3.20) and for a persona
(3.22). A pick for an item can also name an effect to put on the item (3.15).

1. Read `scripts/items/items_game.txt` from the game archive.
2. For each pick, find the slot's default item (section 2.6). Stop if the slot has none, if the
   item does not fit the slot, if the item has no such style, or if a slot is picked twice.
3. Rewrite the default item's block so that it keeps its own identity and takes the picked item's
   looks. The rule is in section 3.3; a style other than the first is worked in as section 3.4
   describes.
4. Everything outside the rewritten blocks is copied byte for byte.
5. Check the result: it parses, it has the same number of items and sets, and each rewritten item
   is still the default of its slot, with its own name, prefab and heroes, names the picked
   item's model, and holds as many gems as it was given (3.3). Step 7 then has a default item
   of a hero name its own model again, and checks that too.
6. Look for every model, effect and map the rewritten items name. If the game lacks an item's
   `model_player`, stop, because the item could not be shown. Other missing files are reported
   and do not stop anything; Valve's own data names a few files the game never shipped. A
   file that another stands in for (2.13) is not missing.
7. Put the picked models where the default items' own are (3.24): their files go in the
   package under the defaults' names. Put the bodies the picked items give heroes where the
   heroes' own are (3.26). Make the heroes' own sounds like those the picked items play in
   their place (3.25).
8. Write the result to `game\mod_enjoyer\pak01_dir.vpk`, with those models and sounds, and with other
   trees or another look for the map where they are picked (3.17, 3.20), and with a copy of a
   file that the picks name and that is one folder away from where it is named (2.13), under
   the name it is asked for by.

Two heroes cannot both have a pick for a slot whose default item they share (2.6), because the one
item can show one thing. Such picks are refused with a sentence that names both heroes.

A set is shorthand for one pick per piece. A piece is left out, and reported, if the hero has no
default item in its slot (taunts, bundles) or if an earlier piece of the set already fills the
slot. Asking for style N of a set gives style N to the pieces that have one and the first style to
the rest.

### 3.3 The block rewrite rule

The rewritten block of a default item holds, in this order:

1. The default item's own pairs, except its looks. The looks are `model_player`,
   `model_player1` to `model_player3`, `visuals`, `portraits`, `particle_folder`,
   `particle_snapshot`, `styles`, `match_cycle_to_parent`, `override_attack_attachments`,
   `disable_hero_portrait_override`, `set_parent_skin`, `courier_slots`, `image_inventory`,
   `item_name`, `item_description` and `item_type_name`.
2. The picked item's pairs, except what it is and how it is traded. These are not copied: `name`,
   `prefab`, `used_by_heroes`, `item_slot`, `baseitem`, `item_class`, `min_ilevel`, `max_ilevel`,
   `player_loadout`, `is_dedicated_showcase_background`, `item_quality`, `item_rarity`,
   `forced_item_quality`, `static_attributes`, `capabilities`, `price_info`, `tool`, `bundle`,
   `tags`, `associated_item`, `equip_all_items_in_set`, `has_store_custom_item_details_panel`,
   `hide_in_store`, `hide_tradecraftdelete`, `hide_in_purchase_popup`, `shards_purchase_price`,
   `treasure_display_override`, `plus_auto_grant_yearly_pattern_start`,
   `plus_auto_grant_yearly_pattern_end`, `creation_date`, `expiration_date`, `event_id`,
   `branding` and `developer`.

A key that is in neither list is kept from both; if both items have it, the picked item's wins.
Pairs are copied exactly as the game's file writes them, nested blocks included. On build 6973
every key of every item that can be picked is on one of the two lists; `dme dev copied-keys` shows
any that are not, and is worth running after a Dota update.

So the default item is still the default item of its slot, with the same id, name and prefab, but
it now names the picked item's model, effects, icon and display name. If the picked item has no
value for a look (for example no `visuals`), the default item ends up without one too, exactly as
when a player equips that item. (Where a default item of a hero has a model of its own, 3.24 then
has it name that model again, with the picked model's file put under its name.)

One thing among an item's `static_attributes` is a look all the same: the gems the item comes
with (2.12), which sit in its sockets and give abilities another icon, another animation or
another effect for as long as the item is worn. So the rewritten default item gets the picked
item's `static socket` attributes in its own `static_attributes`, as written and in their order.
Sockets the default item had of its own go, as its other looks do. Whatever else its
`static_attributes` say stays as written. A default item without `static_attributes` is given
some if there is a gem to put there. 106 of the items that can be picked come with gems on build
6973. Without this, an item such as Necrophos's Moldering Mask of Ka (6291) shows without the
icon and the attack effect its gem brings.

The game takes an item's slot from the item's own `item_slot` before its prefab's. Two world
defaults on build 6973 name a slot they are not the default of: the default cursor pack and the
default Roshan both say `weapon`. When such an item is rewritten, its `item_slot` pair is
rewritten too, to name the slot the item is the default of. Without this, a cursor pack picked for
the default cursor pack did not show; with it, it does, and Roshan keeps working.
**(confirmed in game)** A default item that names the right slot, or none, is left as written.

A default item rewritten this way shows the picked item. **(confirmed in game)** on build 6973
for: a hero's set; a set with one piece swapped for a piece of another set; an arcana in
`hero_base` (Juggernaut); a persona in `persona_selector` with an item in a persona slot (Pudge);
and the world slots `courier`, `ward`, `terrain`, `weather`, `hud_skin`, `music`, `announcer`,
`mega_kills`, `cursor_pack`, `loading_screen`, `versus_screen`, `streak_effect`, `roshan`, and the
creeps, siege creeps and towers of both sides. That is every world slot that has items to pick;
`kill_effect` has none.

### 3.4 Styles

The game shows a default item in style 0, since nobody owns it and could choose another. To show
style N of the picked item, the copy of its `visuals` is changed so that N is what an item without
styles would show:

- the `styles` block is left out;
- blocks marked for another style are left out;
- blocks marked `"style" "N"` are kept, without the mark, so that they always apply;
- the style's `skin` takes the place of the item's `skin`, or is added;
- the style's `model_player` takes the place of the item's, or is added;
- the icon that the style's `alternate_icon` points to becomes the item's `image_inventory`.

Style 0 needs none of this, and the item is copied as written, unless the style names a model
of its own: then it is worked in the same way, so that its model can be put where the default's
own is (3.24). That the game would otherwise load a first style's model by its name is a guess,
not seen. A style can also carry
`body_groups`, `asset` or `entity_scale_flying` (20 styles in all on build 6973); those are not
worked in.

An item whose styles the game switches by itself (2.6) is always copied as written, whatever
style a pick names, with all its styles, their rules and every block still marked for its
style. The default item then goes through the same states the item does. **(confirmed in
game)** with Profane Union in Lifestealer's back slot, which takes its rage look during Rage.

**(confirmed in game)** with a style that brings a model, skin and effects of its own (the second
style of Invoker's Dark Artistry Hair).

### 3.5 Results on build 6973

`dme dev try-all` builds, in memory, every item in every slot in every style, and every set:

- 14,217 single picks (13,434 first styles, 783 others) over 838 hero slots and 20 world slots: all
  build and pass the checks of section 3.2.
- 1,915 sets of 127 heroes, 191 of them with styled pieces: all build. 16 pieces are left out
  (10 taunts, 1 bundle, 5 second items for a slot already filled).
- 6 picks are refused because the game lacks the model: the second style of five Huskar items and
  one piece for Pudge's persona.
- 48 other files are named and not there: 29 effects, 18 models and 1 map.
- Ability effects (3.14): 581 picks over 327 slots of 120 heroes, 36 of them for a style other
  than the first: all build. So do 5,321 pairs of such a pick and an item worn in the slot the
  effects ride on, which between them have every item that can be worn there.
- Effects on items (3.15): 5,418 picks, which put every effect the app offers on an item
  of the courier and on an item of each hero, each effect on another of the hero's slots in
  turn: all build.
- Effects of events (3.16): 352 picks, every event for every kind of effect at every level: all
  build, and the game has every file they name.
- Ability effects with the effects of events (tried since build 6974): 83 rounds, each with
  the ability effects of one round above and one event for every kind of effect, so that every
  pick of ability effects meets every kind: all build. Before the check of 3.14 counted what is
  left, Tinker's Mecha Boots of Travel Mk III with any of the 85 teleports did not.
- Gems (3.3): each of the 106 items that come with gems, 108 between them: all build.
- Personas (3.22): 140 picks for 12 heroes, each persona in each of its styles and
  with each item for one of its slots: all build.
- Emblems (3.19): the 11 emblems, each on one default item of each of the 127 heroes: all
  build, and the game has every file they name.
- Trees (3.17): the three kinds come to 7, 12 and 6 files, 42 to 96 KB.
- Looks for the map (3.20): four items, one file of 1 to 3 KB each.

A round of `try-all` holds one pick for every slot at once. Picks that are fine one at a time
and fail together are reported as a failure of their own. If any pick fails, `try-all` ends with
an error, and a release stops there.

### 3.6 Enable

1. Take the vanilla text of `gameinfo_branchspecific.gi`. If the file already carries our lines,
   remove them first. The result must have the SHA-1 that Valve's own line in `dota.signatures`
   declares; otherwise stop and change nothing.
2. Insert, just before the closing brace of its `FileSystem` block, a `SearchPaths` block that
   repeats the list from `gameinfo.gi` with `Game mod_enjoyer` placed before `Game dota` and
   `Mod mod_enjoyer` before `Mod dota`. The inserted lines are wrapped in two marker comments so
   they can be removed exactly.
3. In `dota.signatures`, drop anything after the `DIGEST` line and add one line declaring the
   edited file: the same path text as Valve's line, with the new SHA-1 and CRC.
4. Write `dota.signatures` first, then `gameinfo_branchspecific.gi`. Each is written to a temporary
   file and renamed into place.

Before the first edit, copies of both original files are saved under
`%LOCALAPPDATA%\Dota2ModEnjoyer\backup\<game build>\`.

### 3.7 Restore

1. Remove our lines from `gameinfo_branchspecific.gi` and check the SHA-1 against Valve's line.
2. Cut `dota.signatures` back to the end of the `DIGEST` line.
3. Delete `game\mod_enjoyer`.

Steam's "Verify integrity of game files" has the same effect on the two game files. Restoring
gives back normal Dota. **(confirmed in game)**

### 3.8 Status

The tool decides what state the game is in from the files alone:

| State | Meaning |
|---|---|
| Off | No lines of ours in either file. |
| On | Our lines are in `gameinfo_branchspecific.gi`, `dota.signatures` declares exactly that file, and our package exists. |
| Needs repair | Anything in between, for example after a Dota update put `dota.signatures` back. |
| Changed by another program | `gameinfo_branchspecific.gi`, with our lines taken out, is not the file Valve's line declares. Nothing is written until Steam has verified the game's files. |

### 3.9 Safety rules

- Refuse to write while `dota2.exe` is running. This is looked at twice: before the work starts,
  and again right before the first write, since working things out takes a moment in which the
  game may have been started.
- Work everything out in memory and check it before the first write.
- Never write a game file in place: write a temporary file next to it, then rename. The
  temporary file's name has the number of the process that writes it
  (`dota.signatures.1234.dme-tmp`), so that two programs writing one file at once never
  share one.
- Where Windows refuses a write, a rename or a deletion in a way that another program holding
  the file causes (access denied, the file in use, a part of it locked: errors 5, 32 and 33),
  try again for up to 5 seconds. An antivirus looks at a file just written, and holds it
  meanwhile. What still fails is told in plain words, the error as it is going in the details
  only: "Windows did not let the app change one of Dota's files. Another program may be using
  it, such as an antivirus or Steam. Wait a moment, then try again." A full disk is told so too.
- Never overwrite a backup of an original with an edited file.

### 3.10 What follows from the method

- A pick shows wherever the game falls back on the default item, and only there. A slot in which
  the player has an item of their own equipped shows that item. Ability effects ride on the
  default item of the slot their item is worn in (3.14), and show where that default item does.
- Other players who use the default look show the pick too, on this computer only.
- Slots without a default item cannot be changed (section 2.6). What an item for four of them
  does can be had another way: `teleport_effect` and `blink_effect` (3.16), `emblem` (3.19) and
  `map_effect` (3.20).
- The colour of a gem and the level of an arcana go by what the game's server says (2.12), so
  a part of an item that needs either does not show through a default item. The catalogue
  marks such items (3.11).
- What is for every hero, the effects of events and an emblem, rides on default items, and so
  shows on whoever plays a hero that has them, not on the player's own hero alone. The game's
  files cannot tell the player's hero from the others (the `criteria` of a block go by a
  player's event level, compendium level and an item's style only). A player can narrow it
  down to some heroes (3.21).
- A default item that several heroes share shows one pick on all of them. There is one such
  item, the default pet of ten heroes (2.6).
- While the package is in use, `mod_enjoyer` is the first `Mod` folder in the game's list, and
  the game keeps a few files of its own there instead of in `game\dota`. Seen on build 6973: the
  hero guides it downloads (`workshop\steampublic`), a dated folder for crash reports (`rpt`) and
  an empty `save`. A replay downloaded meanwhile went to `game\dota\replays` as always.
  Restoring removes `game\mod_enjoyer` with whatever is in it; the game downloads guides again
  when it wants them.

### 3.11 What can be picked

The app and the listings of the command-line tool show a catalogue made from the item database,
the names and the list of files in the archive. Only what the tool can really show is in it:

- A hero's slot or a world slot is listed if it has a default item and at least one item to
  pick. Of a hero's slots, only those its own file lists (2.10) are listed, because what is
  picked for a slot the hero has no more would show nowhere. A hero's own slots come first, in
  the order of its file, which is that of Dota's loadout; then its slots for ability effects
  (below), then the choice of a persona (`persona_selector`) and the persona's slots. A hero
  whose file cannot be read has the slots the item database gives it, in the database's order.
- A hero's slot is called what its file calls it (2.10), or else what such a slot is called on
  any hero (2.8). Where a file gives two of the hero's slots one name, as it gives both of
  Alchemist's heads, the one that has another name on any hero goes by that ("Body - Head"). A
  persona's slot is called "Persona ..." unless its own name already differs from that of the
  hero's slot.
- The items listed for a slot are those that fit it (2.6), by name. An item whose
  `model_player` the game lacks is left out. Its name is the text of its `item_name`, or else
  its internal `name`; its picture is its `image_inventory`.
- Styles are listed for an item that has more than one. A style above the first that names a
  model of its own which the game lacks is left out. A style's name is the text of its `name`, or
  else "Style N"; its picture is its `alternate_icon`, or else the item's. Styles that the game
  switches by itself (2.6) are not listed.
- A set lists the pieces it stands for (3.2), less those whose model the game lacks and those
  for a slot the hero has no more. A set with no piece left is not listed. Its picture is the
  small version of its `portrait_image`, or else the large one, or else the picture of its first
  piece.
- Every hero with a slot or a set to pick from is listed, by name, with its portrait. A hero
  also goes by the short names in its file (2.10) and by the name in its id where that differs,
  such as `furion` for Nature's Prophet.
- An item is as rare as its `item_rarity` says, or else its prefab's. A set is as rare as the
  item its `store_bundle` names, or else as its rarest piece. A slot's default has no rarity.
  The rarities themselves, with their order, names and colours, are the game's (2.6).
- After the slots a hero wears things in come its slots for ability effects: the ones its own
  file lists (2.9), in the file's order, that have an item to pick. Such a slot is called after
  its ability, as in "Blade Fury effects", and where nothing is picked it shows the ability's
  picture. Its items are those that fit it (3.14), each with its own name, picture and rarity.
  They need no model, since nothing of them is worn. An item's styles are listed only if its
  effects differ by them, and only the styles that have effects. A persona's slots for ability
  effects come after what the persona wears.
- A slot lists the effects that can be put on what is picked there (3.15): for the courier, the
  43 effects the game has a name for, by name; for what a hero wears, the same 43, first the
  eight not made for couriers and then the 35 that are, which on a hero are an experiment; for
  other slots, none. An effect is left out if the game lacks its file.
- An item, an ability's effects from an item, or an event's effect says what of it may not show
  in Dota: a file it names, or one such a file is made of, that the game has in neither archive
  (2.13); or blocks of its `visuals` for an arcana's level above 0 (2.12); or, for one map
  effect, fog that shows checkered (3.20). On build 6973 that is 25 choices for missing files,
  none of them effects of events, 7 for an arcana's level and 1 for checkered fog.
  `dme dev catalog` lists the files that are gone.
- After the game's own world slots comes the slot for an emblem (3.19), with the emblems that
  have something to show on a hero and whose effect the game still has the file of. Where
  nothing is picked it shows no emblem.
- Then comes one slot for each kind of effect that an event's effects can take the place of
  (3.16), with the events that have one for it. An event whose effect the game has no file for
  is left out; the levels of an event are listed the way styles are. Where nothing is picked,
  such a slot shows the Dota item whose effect it is.
- Few of the events' items have a picture of their own. The others show the first of these that
  the game has:
  1. the logo of the team whose effects they are, where the files of the effects name a team
     by its number, as `teleport_start_ti9_team39.vpcf` does;
  2. the picture of the event's pass: the item with the same `event_id` whose `tool` is of the
     type `compendium`, the one with the shortest name if there are several;
  3. the picture of something else the event had: an emblem, before a treasure, before a key,
     before a bundle, the one with the lowest id;
  4. for effects that belong to no event, the picture of a brand (2.7) called what the files
     of the effects end in after their last word with a number in it, as
     `teleport_start_ti6_lvl3_escape_gaming.vpcf` does: with its words apart, then together,
     then with fewer of them, and never a name of one or two letters;
  5. the picture of the Dota item whose effect it is.

  The levels of an event have no pictures of their own.
- Last come the two experiments: the slot for other trees (3.17), with the kinds the game has
  every material of, and the slot for another look of the map (3.20), with the map effects the
  game has the material of.
- A slot says which other heroes share its default item (2.6).

On build 6973 that is 125 heroes with 731 slots, 10,693 items and 1,915 sets, and 19 world slots
with 2,728 items; 526 items have styles to choose from. After those come the slot for emblems,
with 11; 19 slots for the effects of events, with 246 choices; the slot for trees, with three;
and the slot for the map, with four. Every item, emblem and map effect and every set has a
rarity:

| Rarity | Items | Sets |
|---|---|---|
| Common | 2,034 | 98 |
| Uncommon | 2,739 | 72 |
| Rare | 3,606 | 876 |
| Mythical | 4,053 | 746 |
| Legendary | 235 | 19 |
| Ancient | 1 | 0 |
| Immortal | 600 | 56 |
| Arcana | 27 | 27 |
| Seasonal | 141 | 21 |

For ability effects there are 325 slots of 120 heroes with 547 choices between them: 462
immortal, 46 mythical, 27 legendary and 12 others. 32 of the choices come in styles.

`dme dev catalog` counts all this and reads every picture.

### 3.12 Picks, Play and Dota updates

Two files keep apart what the player wants and what is in the game.

- `picks.json` holds the player's picks, at most one for a slot. Putting a set on a hero is one
  pick per piece; a set the hero was wearing completely is taken off first, so that going from set
  to set leaves no stray pieces. A persona and what its hero wears of its own take each other's
  place (3.22). The file also says whether the picks are put away (`"off": true`), and which
  heroes get the effects of events and an emblem where that is not every hero (3.21).
- `state.json` is written when a package is. It holds the picks in the package, the game build it
  was made from, the number of the rewrite rules it was made by, and which heroes got the effects
  of events. The number goes up whenever sections 3.3, 3.4 or 3.14 to 3.22 change what is written
  for the same picks; it is 11 now. The file is deleted before the first of the game's files is
  written and written again after the last, so that a write that fails on the way leaves no
  picks that pass for the ones in the game, and the next Play writes them all. Going back to
  normal Dota deletes it too.

A pick in `picks.json` is a slot, an item and, where they are not the plain ones, a style and
an effect:

```
{ "slot": "courier", "item": "10091", "effect": 15 }
{ "hero": "npc_dota_hero_axe", "slot": "head", "item": "9500", "style": 1 }
{ "slot": "fx_teleport", "item": "17482", "style": 2 }
{ "slot": "emblem", "item": "12996" }
{ "slot": "trees", "item": "autumn" }
{ "slot": "map_effect", "item": "18380" }
```

A colour that an older version saved with a pick is read and left out.

Which heroes get the effects of events and an emblem is written beside the picks:
`"effects_for": "picked"`, or `"effects_for": { "these": ["npc_dota_hero_axe"] }`; nothing for
every hero.

When the catalogue is read, each pick is settled against it: one whose item is gone is dropped,
a style that is gone gives way to the item's first, and an effect that is gone, or that the
slot cannot take, is left out.

If there is no `picks.json`, the picks in `state.json` count as the player's.

Going back to normal Dota restores the game's files (3.7) and puts the picks away. Picks that
are put away are kept and nothing more: nothing is picked, in the game or in the app, and Play
starts normal Dota. They come back, exactly as they were, with one click in the app ("Bring
back my picks") or with `dme play` on the command line. A new pick made while picks are put away
starts over instead: the ones put away are dropped, which the app lets the player undo. Without
picks there is nothing to put away.

Play makes the game show the picks in use, writing only if it has to, and then starts the game:

| The game is | Play does |
|---|---|
| Nothing to show, state Off | Nothing |
| Nothing to show, anything of ours in the game | Restore (3.7) |
| State On, with these picks and these heroes for the effects of events, made from this game build by these rules | Nothing |
| Anything else: other picks, a Dota update, older rules, a state that needs repair | Build the package (3.2) and enable it (3.6) |

A Dota update shows as a new game build in `steam.inf`, or as `dota.signatures` put back to
Valve's. Either way the next Play makes the package again from the new game files, so new and
changed items are picked up and nothing of the old build is carried over.

An update undoes half of what Play did. Steam writes Valve's new `dota.signatures` and leaves
`gameinfo_branchspecific.gi` alone where Valve did not change it, so the edited file is no
longer declared; or the other way round, leaving a line of ours for a file that is Valve's
again (`ModState::is_half_undone`, `engine::UNDECLARED` and `engine::LEFTOVER`). Steam does
it by itself, and a player who starts Dota from Steam then plays with files that Valve's own
do not vouch for. As a match was looked for so, a player was stopped by "Valve Anti-Cheat was
unable to verify that your machine is secure", until Steam verified the files. So the app,
whenever it finds the game so, Dota is not running and Steam is done with Dota's files
(`StateFlags` 4 in Steam's `appmanifest_570.acf` beside the game; a copy of the game without
that file has no Steam to wait for), puts the picks back in by itself (3.27). Where there are
no picks in use, or they are put away, or putting them back did not work, it puts the game back
to normal instead (3.7), keeps the picks, and says so: Play puts them back in. It does that
once, until the game is found otherwise. While the app is open it looks at the files again
every 9 seconds, for an update that comes meanwhile. While Dota runs so, nothing is written,
and the bar at the bottom says to close it if it cannot find a match. A window that only has
its picture taken does either only to a copy of the game it is pointed to (`--dota`).

An update also changes the game's archive under an app that is open. The app holds the
archive's list of files in memory and reads the files themselves from disk as they are asked
for (2.2), so after an update the list names files by where they were: with the list of build
6983 and the archives of build 6986, a hero's page had a picture come out empty and names
fall back to the files' own words ("Berserkers Call"), and items the update brought or took
away were not known of until the app was started again. So the app remembers the build its
files were read from, and when the look every 9 seconds finds another build and Steam is done
with Dota's files, it reads them again (`App::read_again_after_an_update`): the catalog, the
archive for the pictures, every picture anew, and the game's words for the Builds page when
that is next shown. The picks are gone through as at a start, and one that names something the
update took away is taken off and said. The page in view stays. Where reading does not work it
is tried again at the next looks, three times for one build, and what was read before stays
in use.

The game is started by running `steam.exe -applaunch 570` from the Steam folder (3.1). Steam does
the starting, with the launch options set in Steam; the tool never runs or touches the game
itself. **(confirmed in game)** If Dota is already running, nothing is written and nothing is
started. In the mode for a low-end computer a few settings follow the game's number (3.18).

The app stays open after Play, and nothing in it is locked while Dota runs: Play and "Back to
normal Dota" refuse to write then (3.9), and everything else only changes `picks.json`.

### 3.13 The app

The app is one window. Down its left side are the logo and the names of its pages, each under
a small drawing of what it is: Heroes, World and My picks at the top; Support, Settings and Help
at the bottom, which open in front of the page. A bar along the bottom has a badge and one
sentence saying where things stand, "Back to normal Dota", and Play. The side and the bar are
panels of frosted glass: sheets that let through what is behind them, a little lighter at the
top, with a rim that catches the light. On Windows 11 what is behind the whole window is the
desktop, blurred by Windows itself (the "acrylic" it draws for its own panels, asked for with
`DWMWA_SYSTEMBACKDROP_TYPE`), darkened by night and lightened by day, under a faint veil of
the look's colour that keeps text readable. Elsewhere it is the look's own plain colour. Either
way a faint glow of the app's colour sits in one corner. A picture the app takes of itself has
a blurred desktop of the app's own making behind it, since Windows' is not in the picture.
A computer whose graphics cannot make a window see-through makes the window library give up
as the window is being made. The app then notes that in `settings.json` (`"no_glass": true`)
and starts itself again, without: the plain backdrop from then on.
Nothing in the window scrolls under a panel. Play and the buttons like it are a calm shade of
the app's colour; the page in view has a quiet sheet and a short mark of that colour beside its
name. The app has two looks, Night and Day, and a switch with a sun and a moon at the
bottom of the side bar chooses one (`theme` in `settings.json`). A settings file that says `system`, which 0.2.0 and
0.2.1 offered, has the app follow Windows until the switch is clicked. Pictures of the game's stay on dark in
both. Text is set in Inter (`assets/fonts`), titles in its heavier cut. The badge
says in two or three words whether the game has the picks: "Mods on", "Not in the game yet",
"Normal Dota", "Old picks in the game" or "Needs attention".

- Heroes is a grid of heroes, each a card that is all picture with the name on it. Chips above it show only the heroes of one main attribute, which
  a hero's own file names with `AttributePrimary`, or only the heroes with picks. A hero's tile
  has a dot in the colour of its attribute. A hero's page has the list of its slots on the left, with a row
  for whole sets at the top, and on the right what can go in the row that is open. The first tile
  is always the default. Styles of the picked item are chosen above the tiles. "Share", beside
  "Reset hero", copies a share code of the hero's picks (3.23).
- World is the same page for the world slots. Under the game's own slots, a few words say
  where the effects begin that are for every hero: the emblem (3.19) and the effects of events
  (3.16). Then come the experiments (3.17, 3.20). The page of each of these has a small "?"
  beside its title, which says what a pick there does and when it shows while the pointer rests
  on it. Where the effects of events and an emblem would go on no hero (3.21), the page of each
  says so in the open, with a button that gives them to every hero, and the bar at the bottom
  says "No hero gets your event effects or emblem yet" in the place of "ready" or "in the game".
- My picks lists everything that is picked, the world's picks first and then each hero's. A
  click on a pick opens its page, and its cross takes it off. Above the list are the presets:
  picks kept under a name in `presets.json`, beside the app's settings. A click on a preset
  picks what it has in the place of what is picked now, which can be undone. Items a Dota
  update took away are left out and counted. The same name again takes the place of what was
  kept under it. "Share" copies a share code of what is picked (3.23), and a right-click on a
  preset shares it or forgets it. "Use a code" opens a box to paste a code in: it says what the
  code has, and picks it, which can be undone, or keeps it as a preset.
- Every page has one search box, and what is typed goes there without the box being clicked
  first, whatever was clicked last. Escape empties it. On the grid of heroes the best fit comes
  first: a hero whose short name was typed in full or whose name starts with what was typed,
  then one with a word that starts with it, then one that has it anywhere (2.10). From the
  third letter on, the grid also lists the items and sets that have what was typed in their
  names, wherever they are: the world's first, then each hero's, 120 at most. A click on one
  opens its page with its name typed in. Enter opens the first hero, or the first of those.
- A tile of an item or a set shows a larger look while the pointer rests on it: the picture
  twice the size, the name in full, the rarity, the styles side by side or the pieces of the
  set, and what may not show.
- Once an item is picked in a slot that has effects to put on it, a list above the tiles
  chooses one, or none (3.15). On a hero, an effect made for couriers is marked "(courier)",
  and a line under it says it is an experiment. Picking another item in the slot keeps the
  effect.
- A tile of an item that has a part that may not show (3.11) has a small "!" in the corner of
  its picture, and says why when the pointer rests on it. Once it is picked, the same sentence
  shows above the tiles. A set has the mark where one of its pieces does, and names the piece.
  The mark is also on what typing finds, on the rows of a hero's page and on My picks.
- The page of a slot whose default item several heroes share says so and names them. A pick
  there takes the place of what another of those heroes had picked.
- Low-end PC mode is a checkbox in the settings (3.18). It is kept with the app's own settings
  and is no pick.
- In a hero's list, under what the hero wears, a few words say where the slots for ability
  effects begin (3.11). The "?" of such a slot's page says what a pick there does and, once an
  item is picked, which slot to look at in Dota if the effects do not show: the one the item is
  worn in (3.14).
- A picture of an item or a set has a band in the colour of its rarity along the bottom, as in
  Dota. A row of buttons above the tiles, one for each rarity that occurs in the list, shows
  only the rarities switched on; with none switched on, everything is shown. The choice holds
  from slot to slot and from hero to hero until the app is closed.
- A pick is saved the moment it is made. Nothing is written to the game until Play.
- After "Back to normal Dota" the picks are put away (3.12): the app shows nothing as picked,
  and the button beside Play reads "Bring back my picks".
- The first start shows a notice about the risks, once, and then four sentences on how the
  app is used, once. If Dota is not found, the app asks for its folder.
- Something that goes wrong is said in one sentence in the corner above the bar, with "Copy
  details" and "Report". "Report" opens a new issue on the project's page on GitHub, with the
  details written in, for the player to read over and send; the app sends nothing. In what is
  copied and reported, the path of the player's own folders, which names their Windows account,
  is written `%USERPROFILE%`, however its letters are cased and whichever way its slashes
  lean. The text is gone through letter by letter: a name with a letter that is two letters in
  lower case, as the Turkish İ is, was passed over by counting letters, and went into a report
  as it was (`report::starts_with`). What "Copy details" copies on the Draft and the Builds
  page is written so too. Help has "Report a problem" at its end, for a report with only the
  version written in. Taking several picks off at once can be undone from the same place. A
  message that asks nothing of the player goes away by itself after 7 seconds, one with an
  Undo after 12; one about something that went wrong, or about an update, stays until closed.
- Help says what a player cannot see for themselves (3.10), as a list of headings that each
  open on a click, and which version the app is.
- In a build that looks for newer versions of itself, an Update button shows on the left side for
  as long as one is out, and Help says how updating goes (4).
- The page of the persona slot says that a persona takes the place of what the hero wears
  (3.22). Picking it takes the hero's own picks for those slots off; picking one of those
  takes the persona off.
- "Support" on the left side opens a page with a button that opens the project's page on GitHub,
  to give it a star, and, in a build that names them (`crates/dme-app/src/support.rs`), where
  the maker takes donations: a page for PayPal, and wallets whose addresses are copied with a
  click. The app opens `https` addresses only, and asks for nothing by itself.
- "Draft" on the left side opens the draft helper (5). `--open draft` shows it in a picture.
- "Builds" on the left side opens the build helper (6). `--open builds:axe:offlane` shows
  the build of a hero in a role in a picture, and `--open builds` the page with no hero.
- "Settings" on the left side opens the settings page, with: the language of the window (3.28);
  which heroes get the effects of events
  and an emblem (3.21), as every hero, the heroes picked for, or heroes chosen from a list;
  Low-end PC mode (3.18); "Update automatically" (4.2), off by default; where Dota is and where
  the app keeps the picks, its settings and the backups; and the version and license.
  `--open settings` shows it in a picture.

The app reads the game's files on a thread of its own and pictures on three more. Only that first
thread ever writes to the game's files, and the window is not closed in the middle of a write.

Something going wrong inside the app itself (a panic, a mistake in its code) does not leave the
window frozen or gone without a word (`crash.rs`):

- On a thread that does the work (the game's files, updates, pictures), the work is given up and
  the player is told: "Something went wrong inside the app, and it stopped what it was doing.
  Try again. If it happens again, please report it." The details say where in the code. The app
  goes on, and a thread for the game's files that is gone is started again.
- On the window's own thread, the app has to close. What happened is written to `crash.txt` in
  the data folder, and once the window is gone a box says so and that the picks are kept. The
  next start says that the app closed last time, with "Report" and "Copy details", once.

One copy of the app runs at a time for one data folder (`one_copy.rs`). Each copy reads the
picks once and saves all of them at every change, so two copies would each take away what was
picked in the other. A copy holds a mutex of Windows' named after its data folder
(`Local\Dota2ModEnjoyer-` and a hash of the folder's path). A second copy waits 2 seconds for
one that is closing, then brings the first one's window forward and closes. A copy that an
update started waits up to 15 seconds, while the version it replaces closes. Windows lets the
mutex go when a copy ends, however it ends.

For testing, the app can work itself and take a picture of its own window, which stays off screen
meanwhile. Nobody's screen is captured.

```
Dota2ModEnjoyer.exe --screenshot out.png [--size 1180x760] [--open hero:axe:weapon | world:courier | picks | draft | builds:axe | help | settings | support | share]
                    [--click X,Y] [--right-click X,Y] [--rest X,Y] [--type TEXT] [--scroll X,Y,DOWN] [--wait SECONDS] ...
                    [--dota FOLDER] [--data-dir FOLDER] [--no-start] [--ignore-running]
                    [--update-from MANIFEST] [--pretend-version VERSION] [--no-restart]
                    [--numbers-from FILE] [--game-language LANGUAGE]
```

Steps are taken in the order given, each once the window has come to rest. `--rest` leaves the
pointer at a place, for what shows while it rests there. `--wait` lets time
go by, for what the app comes to do by itself meanwhile; the picture is taken after two minutes
at the latest, whatever is still to do. A new version of the app that is coming does not hold
the steps up, so that a picture can show how far it has come; the start of it does. `--no-start` does everything Play does except start
Dota. `--ignore-running` writes even while Dota runs and only works together with `--dota`: it
is for a copy of the game. `--update-from`, `--pretend-version` and `--no-restart` are for
trying the app's own updates (4.5). `--numbers-from` says where the Draft page fetches its
numbers (5.4), and the Builds page its builds from beside them (6.4). `--game-language` names
the game's language to take the names of heroes and items from, in the place of the window's
(3.28).

A window that only has its picture taken leaves the clipboard and the browser alone: what it
would copy is written to `copied.txt` in its data folder, and an address it would open to
`opened.txt`. Two settings of the environment try out what cannot be tried otherwise:
`DME_TRY_NO_GLASS` makes a window that is to be see-through fail as it is being made, and has
one whose picture is to be taken be see-through; `DME_TRY_ONE_COPY` has a window whose picture
is to be taken hold its data folder as a player's window does, and a second one close without
bringing the first forward.

One more option is the app's own and not for testing: `--updated-from VERSION` is what the app
tells the version it starts in its place after an update (4.2).

### 3.14 Ability effects

A pick can name one of a hero's slots for ability effects (2.9) instead of a slot that things
are worn in, for example Juggernaut, `ability_effects_1`, item 9984. Such a pick gives the
ability the effects the item has for it, whatever the hero wears.

Such a slot has no default item of the hero's to rewrite. The game's default item for it is
every hero's, so effects put there would show on every hero. The effects ride on another default
item instead: the hero's default item of the slot the picked item is worn in. An immortal sword's
effects ride on the hero's default sword. For that default item:

1. Its contents are worked out as always. If nothing is picked for its slot they are its own, as
   written; otherwise it is rewritten to look like the item picked for its slot (3.3, 3.4).
2. Then, for each pick of ability effects that rides on it, in the order of the slots' numbers:
   - The picked item's effects are the blocks of its `visuals` that are marked with the slot's
     number. Of an item picked in a style, they are the blocks marked for no style and those
     marked for that style, the latter with the mark of the style taken out (3.4).
   - Two kinds of blocks leave the default item's `visuals` to make room. The first is blocks
     marked with the same number, which are the effects the item worn has for that ability. The
     second is blocks that have the `type` and the `asset` of one of the new blocks, because one
     item cannot replace a thing twice. Upper and lower case make no difference here.
   - The new blocks are added at the end, as written. A default item without `visuals` is given
     one.

An item whose styles the game switches by itself (2.6) has states rather than styles, and no
style is picked for its effects either: they are those of all its states. On build 6974, 11
items that give ability effects are such items, among them Profane Union, whose effects for
Rage are all marked for the state the game puts it in while Lifestealer rages.

- A block that every state has alike is added once, without a mark.
- A block that puts something in the place of a thing that no state changes otherwise is
  added without its mark, to apply always. Profane Union's effects for Rage take the place of
  Rage's own, so they show while the hero rages, as they do on the item.
- Where the states change one thing in different ways, or one state makes something the others
  do not, the blocks keep their marks, and the default item gets the item's states, each with
  its rule and nothing else, so that the game takes it through them as it does the item. Troll
  Warlord's Bitter Lineage changes an icon with his stance, Scale of Bitter Spoil the colour of
  an effect, and Sven's Vigil Triumph has three effects of Great Cleave outside God's Strength
  only. The states bring nothing of the item's look: no name, skin or model.
- A default item that has states of its own already, from the look picked for its slot, cannot
  go through other ones as well. There the blocks of the item's first state are added, without
  their marks. Where it has these very states, the item itself worn there, the blocks keep
  their marks. On build 6974 the first happens with Vigil Triumph's effects and a sword of
  Sven's with styles of its own, worn in its first style.

Nothing else of the picked item is taken, neither its model nor what else it shows when worn.

An item fits a hero's slot for ability effects if the hero uses it, it is not a default item, it
has the attribute with the slot's number, it has a block marked with that number, it is worn in
a persona's slot exactly if the slot is the persona's, and the hero has a default item where it
is worn. A pick is refused if the item does not fit, if it has no such style, or if it has no
effects in the style picked. An item whose effects are all in a later style is picked in the
first style that has some.

After the rewrite each default item is checked as in 3.2 and, on top of that, has to hold as
many blocks marked with each slot's number as are left of those added for it once everything
else is done to it. The effects of events come last, and a block of theirs takes the place of
an ability's effect for the same thing (3.16), as with Tinker's Mecha Boots of Travel Mk III
and a teleport.

What follows from this:

- To the game, the default item is a worn item that happens to have these effects, the same as
  when the item itself is worn. So they show where any pick shows (3.10): where the player's own
  Dota loadout has the default item in that slot, which here is the slot the picked item is
  worn in.
- Where items in other slots replace the same thing, the game decides between them the way it
  does between two worn items.

**(confirmed in game)**

### 3.15 An effect on an item

A pick for an item can carry one more thing, which in Dota comes from a gem: an effect to put
on the item on top of its own, by its number in the list of 2.11. It applies only to a pick that
shows an item in the place of a default item, so not to one for ability effects, for an event's
effect, for an emblem, for trees or for the map.

The effect is worked into the `visuals` of the default item after it has been rewritten to look
like the picked item (3.3, 3.4) and after any ability effects (3.14). It is added as a block the
way an item names an effect it comes with:

```
"asset_modifier"
{
    "type"        "particle_create"
    "modifier"    "particles/econ/courier/courier_trail_01/courier_trail_01.vpcf"
}
```

The list of 2.11 tells the game how to attach it, as it does for the effects items come with.
An item that already comes with that very effect is given nothing. A default item without
`visuals` is given some; what is new is laid out like the file's own. **(confirmed in game)** on
a courier.

The effects made for couriers attach to the item's owner the way they attach to a courier, at
its feet and to the whole of it. On what a hero wears they are offered as an experiment: they
may sit lower or look smaller on a hero.

A pick is refused if the game's list has no effect with that number.

Version 0.1 also put a colour on an item's effects, as a Prismatic gem (2.12) in a socket of the
default item and as settings of its effects (points 15 and 16, 2.11). In a match on Valve's
servers the colour of a gem is what the server says, from what the player owns, so it never
showed there; the first user saw it fail on Terrorblade's arcana. The colour was taken out. A
colour saved with an older pick is left out (3.12), and `dme apply` refuses `color=` with a
sentence that says why.

### 3.16 The effect of an event

The game has no default item for the slots `teleport_effect` and `blink_effect`, and none at
all for the effects of events (2.11). What such an item does is done another way: its blocks
are put on the default items of every hero.

A pick names a kind of effect, an event's item, and a level:

- The kind is a world slot of this tool's own, `fx_teleport`, `fx_blink` and so on, 19 of
  them. Each stands for the effects of the game's that make up one thing, such as
  `teleport_start.vpcf`, `teleport_end.vpcf` and `teleport_origin.vpcf` for a teleport. That
  list is this tool's: the game's files only say which effect an item replaces by which.
  `fx_attack` is what an event put on a hero's attacks. The game has an empty effect for it,
  `particles/econ/attack/attack_modifier_empty.vpcf`, and seven events' items put theirs in its
  place (The International 2019, 10, 2023 and 2024, Spring and Aghanim's 2021, the Battle
  Pass 2022), with five different looks between them.
- The item is one that holds such blocks (2.11): the item of an event's effects, or one made
  for the slots `teleport_effect` and `blink_effect`.
- The level, written where other picks have a style, counts from 0. The blocks of an item
  for one kind are grouped by what their `criteria` say, spaces and case aside, and the groups
  are put in the order of the lowest level they are for. Blocks without `criteria` are one
  group.

The blocks of that group are added to the `visuals` of every default item of every hero, 829 of
them on build 6973, or of the heroes chosen for it (3.21), each block without its `criteria`,
since it is for everybody now. A block that replaces the very thing one of the new blocks
replaces gives way, as in 3.14. Several picks of this kind are added in the order of their
slots' names, after everything else that is done to a default item, so the same picks always
give the same file.

Every default item gets them, rather than one for each hero, because any of a hero's slots may
hold an item of the player's own in Dota. The effect shows for a hero as long as one of its
slots is on its default item. To the game, a hero then wears several items that replace the
same effect by the same one.

World slots get nothing, since a courier does not teleport.

A pick is refused if the item holds no such blocks for the kind, if it has no such level, or if
a kind is picked twice.

An effect that is not a replacement of one effect by another cannot be had this way: it is
created on the hero once for every item that names it, and so would show several times over.
An emblem is of that kind, and is given one item to ride on for each hero (3.19). The effects
for the slots `death_effect`, `head_effect` and `courier_effect` are left out.

What makes an effect look thinner than it did when its event ran is, on build 6973, not lost
files (2.11) but most likely the lightest video settings, which leave parts of effects out.

**(confirmed in game)** for the effects of Dota items, a Shiva's Guard among them.

### 3.17 Other trees (an experiment)

A tree of a map is a model, and a model lists the looks it can have. A look is a material: a
file such as `materials/models/props_tree/tree_oak_leaves_00.vmat_c`, which says which
pictures cover the tree. Valve's seasonal terrains are maps of their own that have the trees
of the normal map in the same places, in other looks: `tree_oak_autumn_leaves_00` where the
normal map shows `tree_oak_leaves_00`.

A pick for the slot `trees`, which is this tool's own, names one of three kinds. For each
material of the normal map's trees that the kind has a stand-in for, the package (3.2) gets a
copy of the stand-in's file under the name of the map's own material. The game looks in the
package first, so it covers the map's trees with the season's leaves. A material names its
pictures by their own paths, so the copy works under another name.

| Kind | Materials that get a stand-in | For example |
|---|---|---|
| `autumn` | 7: four of the oaks' leaves, the pines' fronds, two of the Dire's trees | `tree_oak_leaves_00` shows `tree_oak_autumn_leaves_00` |
| `winter` | 12: the oaks' leaves and blossoms, the pines' fronds, the bamboo's leaves, two of the Dire's trees | `tree_pine_frond_00` shows `tree_pine_frond_snow_00b` |
| `spring` | 6: four of the oaks' leaves, their red blossoms, one of the Dire's trees | `tree_oak_leaves_00` shows `tree_plumblossom_00` |

The game's files do not say which material stands in for which. The list is this tool's, and
follows Valve's own seasonal maps: each stand-in is a look that Valve's map of that season
shows on that sort of tree. Leaves only stand in for leaves of the same sort of tree.

The trees themselves are not touched. They keep the same shapes in the same places and are as
large as they were. Nothing is made smaller, thinner or easier to see through.

The item database is not touched by this pick. A package can hold other trees and nothing else.
On other maps, a terrain whose trees show the same materials shows the same swap, and one that
has looks of its own does not.

The first way this was done put copies of whole tree models in the package, under the names
of the normal map's models. That did not show in the game, and was dropped.

**(to confirm in game)**

### 3.18 Starting Dota for a low-end computer (an experiment)

With this mode on, Play starts the game with a list of settings after the game's number:

```
steam.exe -applaunch 570 +cl_globallight_shadow_mode 0 +dota_cheap_water 1 ...
```

Steam hands what follows the number on to the game, where `+name value` gives a setting a
value for that run. The settings are 17 of the options of Dota's own video settings at their
lightest: no shadows, simple water and lighting, no fog, no smoothing of edges, no grass, no
wind in the trees, and so on. Each name was checked to be in the game's own program files on
build 6973, and two of them are used by Valve's own `cfg/perftest.cfg`. Nothing is on the list
that takes away what a player has to see: the effects of spells and the size of the picture are
left as they are.

Nothing is written anywhere for this, neither a file of the game's nor one of the player's, such
as an `autoexec.cfg`. The mode is a setting of the app (`"low_end": true` in `settings.json`),
not a pick, and `dme play --low-end` does the same from the command line.

It is not known yet whether the game takes these values over the ones saved in its video
settings, and whether it then saves them as the player's own. If it does, turning the mode off
does not bring the old settings back; they are set again in Dota's video options.

**(to confirm in game)**

### 3.19 An emblem

An emblem is an item for the world slot `emblem`, which has no default item (2.6). It is the glow
an event put around the heroes of the players who earned it. Its `visuals` hold blocks of several
kinds:

```
"asset_modifier"
{
    "type"        "particle_create"
    "modifier"    "particles/econ/events/ti8/ti8_hero_effect.vpcf"
    "spawn_in_loadout_only"    "2"
}
"asset_modifier"
{
    "type"        "particle_create"
    "asset"       "npc_dota_loadout_generic"
    "modifier"    "particles/econ/events/ti8/emblem_loadout_ambient_ti8.vpcf"
}
```

What shows on a hero during a game are the blocks of type `particle_create` that name no
`asset` and are not marked `"spawn_in_loadout_only" "1"`. The others are for the stand that
Dota shows a hero on in its loadout, which they name as their `asset` or are marked to show on
only, or they are of types that only the event itself made use of.

A pick for the slot `emblem` names such an item. Its blocks that show on a hero are added, as
written, to the `visuals` of one default item of each hero.

The effect of an event goes on all of them (3.16). An emblem goes on one, because it is an
effect that an item creates, so a hero that wears two items with it would show it twice. The
item that carries it is the one the hero is surest to wear. Only a slot that the hero's own file
lists (2.10) can carry it, and only one that is a part of how the hero itself looks. That leaves
out a slot for an ability, for the units the hero summons, for its voice, a taunt or another
shape, the slot that stands for the hero as a whole, and everything of a persona's. Among the
slots that remain, the carrier is the default item of:

1. a slot that nothing can be picked for, which the hero wears whatever the player owns (the
   slot made for effects, `ambient_effects`, if it is one of them, or else the first);
2. or else a slot that these picks have something for, since where that pick shows, the emblem
   does (`ambient_effects` before the others here too);
3. or else the slot with the fewest items to pick, which a player is the least likely to own
   something for (of several with as few, the first).

On build 6973 each of the 127 heroes has such an item. 65 of them wear it whatever the player
owns, 55 of those in `ambient_effects`. The other 62 show the emblem only while one slot with
items to pick is on its default item in Dota.

Only the heroes chosen for it get one (3.21). The emblem's blocks are added after everything
else that is done to a default item (3.3, 3.14 to 3.16). Nothing else of the emblem's item is
used. A pick is refused if the item is no emblem or has nothing that shows on a hero, or if the
slot is picked twice.

The effects of emblems are not in the list of effects that says how to attach one (2.11).
Neither are 1,859 of the 2,343 effects that the things heroes wear create, and those show.
What holds an emblem's effect is said in its own file (2.15), and the emblems differ:

| Emblem | The driver of control point 0 |
|---|---|
| Sunken (638), Crystal Echelon (663), Overgrown (12996) | `parent`, the usual way |
| Divinity (13815), Nemestice 2021 (18787), Aghanim's 2021 (22263), Battle Pass 2022 (23238), the four of Diretide (13453, 18379, 18393, 18394) | `self`, `PATTACH_WORLDORIGIN` |

In Dota the emblem's item has no model, so `self` is where the hero stands, and both kinds stay
at the hero's feet. Here the emblem rides on a piece the hero wears, so an emblem of the second
kind is held by that piece and goes where the piece goes: the user saw the Nemestice emblem
follow the hero's attacks (2026-11-20), where an emblem of the first kind had stayed put.

So Play puts each emblem's effect file in the package, at the game's own path, with the driver
of point 0 of every configuration made like the first kind's: `m_entityName` is `parent`, and
`m_iAttachType` is left out. Nothing else in the file changes: not the drivers of its other
points, which give those points their numbers, not the parts it names, and not the blocks
before `DATA`. The values are written without compressing them, as the game has 192 of its own
particle files. A file that says `parent` already is left to the game. So is one that this tool
does not understand to the last byte: read and written again untouched it has to come out the
same, or it is not changed, and the emblem shows as the game's file has it. `dme dev try-all`
counts such a file as FAILED, and `dme dev particles` says what each emblem's file says.

An entry for the emblem's system in the list of 2.11 (`absorigin_follow`, `parent`) was tried
first and changed nothing in a match; it was taken out again.

**(to confirm in game)**

### 3.20 Another look for the map (an experiment)

A map effect is an item for the world slot `map_effect`, which has no default item either. An
event gave it out to have the whole map drawn another way, as a drawing or in the colours of
a season. Its `visuals` name a material for that:

```
"asset_modifier"
{
    "type"        "map_effect"
    "asset"       "map_shader"
    "modifier"    "materials/dev/deferred_post_process_graphic.vmat"
}
```

With no map effect worn, the game draws the map through
`materials/dev/deferred_post_process.vmat`. A pick for the slot `map_effect` names a map
effect, and the package (3.2) gets a copy of the file of that item's material under the name
of the game's own. The game looks in the package first, and draws the map through the copy.

Build 6973 has four map effects:

| Item | Event | Draws the map through |
|---|---|---|
| Diretide Shader Effect (18380) | Diretide 2020 | `deferred_post_process_graphic` |
| Aghanim's Labyrinth Shader Effect (18713) | Aghanim's Labyrinth, 2021 | `deferred_post_process_aghanim_bp` |
| Diretide Shader Effect 2022 (24892) | Battle Pass 2022 | `deferred_post_process_winter` |
| 10th Anniversary Shader Effect 2023 (29301) | The 10th anniversary | `deferred_post_process_outlines` |

The second is the one whose picture the game keeps under `fog_of_war`. The fog over what a
player cannot see is a part of what this material draws.

Only the block whose `asset` is `map_shader` is used. Two of the items name one more material
each: the first a `portrait_shader` for the picture of the hero, the last a
`post_effect_shader` that lays the look of an old screen over everything. Those cannot be had
this way and are left out.

The item database is not touched by this pick. A pick is refused if the item is no map
effect, if the game lacks either material, or if the slot is picked twice.

In a match, Aghanim's Labyrinth's was seen to draw the fog as a checkered pattern until the
video settings change. Its fog needs two pictures (`fow_nebula`, `fow_starfield`) that the map does
not load at its start, since it loads what the game's own material needs. The catalogue marks
that effect (`Checkered`, 3.11) and the app says so on its tile. The others are not known to
have such a fault. Whether the game's own material and an event's are alike enough for one to
stand in for the other is still **(to confirm in game)**.

### 3.21 Which heroes get what is for every hero

The effects of events (3.16) and an emblem (3.19) can go on default items of every hero. A
player chooses which heroes get them. Left alone, it is the heroes picked for:

| Choice | Heroes | In `picks.json` |
|---|---|---|
| The heroes picked for | each hero that has a pick of its own: an item, a set, ability effects | nothing |
| Every hero | all of them | `"effects_for": "every"` |
| These heroes | the ones named | `"effects_for": { "these": [ ... ] }` |

The others get none of those blocks, neither the events' effects nor an emblem. So a default
item that several heroes share (2.6), the default pet of ten heroes, gets them only if every one
of those heroes is to have them, since it shows on all of them. Any hero the game has default
items for can be chosen, also one with nothing to pick, such as Kez and Largo on build 6974. The
choice is kept with the picks and outlives them. Putting picks away or forgetting them leaves it as it
is. Where the effects of events or an emblem are picked and the choice reaches no hero (the
heroes picked for, with nothing picked for a hero, or an empty list), the app says so, and so
does `dme apply` or `dme play` (3.13). It is compared, like the picks, to decide whether Play has to write (3.12). `dme
effects-for` says or changes it; the app has it in its settings.

It cannot be narrowed down to the player's own hero in a match. The files are read when Dota
starts, before a hero is picked, and nothing in a block of `visuals` can say "the player's own
hero" (3.10). What is chosen shows on whoever plays one of those heroes, on the player's own
screen.

### 3.22 A persona on the hero's own items (an experiment)

A persona is chosen in the slot `persona_selector`. Its item's `visuals` hold a block of type
`persona`, `"persona" "1"`, which sends the game over to the persona's own slots
(`head_persona_1` ...), and blocks that change the hero itself: its model (`entity_model`), its
sounds, its icons, its size. Rewriting the default item of `persona_selector` (3.3) shows the
persona only to a player who owns one. The first users found that it did not show otherwise.

So a persona is put on the hero's own default items instead, the way an arcana that changes the
hero's model is (Juggernaut's, in `hero_base`, 3.3):

1. The default item that stands for the hero as a whole, the one of `hero_base`, takes the
   persona's look (3.3, 3.4), without its block of type `persona`. A hero without a default for
   `hero_base` (Anti-Mage and Crystal Maiden on build 6973) has it on the default of
   `persona_selector`.
2. Each of the persona's slots that stands in for a slot the hero wears things in (3.19 says
   which those are) gives its part: what is picked for the persona's slot, or else that slot's
   default item. The part goes on the default item of the hero's slot it stands in for, as
   `head_persona_1` does for `head`, looking like it (3.3). A part whose slot the hero has not
   (Pudge's `armor_persona_1`) goes in the first slot of the hero's, in the game's order, that the
   persona has no part for.
3. Every other slot the hero wears things in looks like nothing: its default item keeps its
   own pairs except its looks (3.3), and has no model.
4. A pick for the effects of an ability of the persona's (3.14) rides on the default item of the
   hero's slot that wears the part it would ride on.

A persona takes the place of what its hero wears, so a pick for one of those slots, or for
`hero_base`, cannot go with it and is refused; `picks.json` keeps only the later of the two
(3.12). The persona's own default items are left as they are.

It is not known whether a model put on a default item this way shows in a match on Valve's
servers. Juggernaut's arcana showed so; a persona is a larger change.

**(to confirm in game)**

### 3.23 Share codes

Picks can be passed on as a share code: one piece of text to copy and paste, for a friend with
the app or for another computer (`crates/dme-core/src/share.rs`). It holds the game's ids and a
name, nothing else, so it reads the same whatever language the game is in. Nothing is sent
anywhere: the player copies it and pastes it.

A code is `DME1-` and then, in the letters of base64 for web addresses (`A-Z a-z 0-9 - _`, no
`=`), lines of text packed with deflate (raw, as RFC 1951 has it), and after them the CRC-32 of
the lines in 4 bytes, least significant first. A line is a pick: the hero's short name, a dot,
the slot, `=`, the item, then `:` and the style where it is not 0, and `+` and the effect where
there is one. A world slot has no name before the dot.

```
@Axe
axe.weapon=9040
axe.head=8910:2
.courier=10003:1+15
```

A first line that starts with `@` names the picks. Three picks of one hero with a name make a
code of 84 letters, ten of them about 150. A code is found among other words that were pasted
with it. One that lost a part, or has a letter changed, fails its check and is said to be not
whole. One that starts with `DME` and a higher number, or has a line this version cannot read,
is said to be from a newer version. What a code unpacks to may be 1 MB at most.

The app checks a code's picks against the player's own game as it checks a preset's (3.12):
items the game does not have are left out and counted, and a style that is gone goes back to
the item's first. Picking what a code has puts each pick in its slot, taking the place of what
was picked there and of what heroes that share its default item had picked; the other picks
stay. That can be undone.

### 3.24 The picked model where the default's own is

A player reported heroes with pieces missing in matches: one without hair, Dragon Knight without
his sword, Venomancer with nothing on, and crashes. In an online match the game loads the models
of what each hero wears as Valve's server has it do, and for a slot on its default that is the
default item's own model. A default item rewritten by 3.3 names the picked item's model instead,
which the game then has to load by itself, and that is the likely way to lose a piece or to see
"ERROR" after a change of the video settings. So, after 3.3, the picked model goes where the
game looks for the default's own:

1. Each rewritten default item of a hero that names a model of its own (`model_player`, and
   `model_player1` to `3` where it has them) names that model again, in the place of the
   picked one.
2. The picked model's compiled file goes in the package under the default model's name:
   `models/heroes/dragon_knight/weapon.vmdl_c` holds the bytes of the picked sword's
   `.vmdl_c`. A compiled model names its materials and the rest by their full paths (2.13), so
   it finds them where it is put.
3. Everything else of the rewrite stays: the picked item's effects, name, icon, portrait and
   the skin of a style (3.4). The model of a style is the one that is put in place: 3.4 works
   in the first style too where it names a model of its own (146 items on build 6975, 11 of
   them another model than the item's), since the game would otherwise load it by its name.

The result is read back: it has as many items and sets, and each default item names its own
model where one was put.

Left as 3.3 has it:

- a default item without a model of its own: the picked model stays named. On build 6975 that
  is 57 hero slots (Lion's head, Earthshaker's head, Morphling's...) whose look is part of the
  hero's body; what is picked for them is not known to show online;
- a model that more than one default item names, or one item under two keys (Troll Warlord's
  four default weapons share one), since a file put there would show on each of them;
- a model that a default item names other than as its own, such as the `asset` of a swap of
  models: Faceless Void's arcana head swaps his own bracers, belt and shoulders for ones that
  go with it, and a picked bracer put where his own is would be swapped too. 30 such swaps
  are in items that can be picked, on build 6975;
- a picked model that a default item names that way: the same arcana refits certain bracers
  and shoulders by their own names, and would not find one put under the default's name. 450
  such swaps are in items that can be picked;
- a model the game does not have, which stays named and does not show, as before;
- world slots, and the models a style switches to by itself (2.6).

Other items that use a default's model with another skin, such as the 108 Diretide Shimmer
items, show the picked model on whoever wears one, on this computer only.

`dme dev try-all` puts the models in place in every round as Play does; on build 6975 that is
10,062 of the 14,205 single picks. Three sets of 16 pieces made the package 0.7 MB bigger
than the item database alone. Idea taken from ArdysaModsTools (its public GPL code), which ships its sets
this way; the code is this project's own. **(to confirm in a match)**

### 3.25 The picked item's sounds in the place of the hero's own

A rewritten default item carries the picked item's swaps of sounds (2.14) like the rest of its
`visuals`, and in a bot match on this computer the hero plays them. In a match on Valve's
servers it does not: there the server says which sound to play, it goes by the default item as
Valve wrote it, and that changes no sound. Axe with Fists of Axe Unleashed picked attacked with
the sound of his own axe (a player's report, 0.3.2).

So the game is given the sound the server asks for, made like the item's. After the rewrite:

1. The swaps are those that the default items of heroes have in the rewritten item database and
   did not have in Valve's, each once, in the order of the items. Of a block marked for a style,
   only those for the first count, the style the game shows a default item in (3.4). Blocks for
   ability effects count wherever they ride (3.14).
2. A swap is made if the event it swaps from is in a hero's own file of events and the one it
   swaps to is in one of them too. Events elsewhere are shared by every hero, such as Blink
   Dagger's and the teleport's: making one of them like an item's would give every hero that
   sound, and they are left alone (8 swaps of 4 items on build 6975, among them Zeus's and
   Earthshaker's arcanas for Blink Dagger and Tinker's Mecha Boots for the teleport). So are
   two of Disruptor's, whose event no file has.
3. The event swapped from becomes a copy of the event swapped to, as the game has it. Where two
   swaps are of the same event, the first is made. A swap whose event plays the hero's own event
   along with it, through another event or not, is left alone, as the copy would play itself;
   none does on build 6975 (Lifestealer's Dark Maw Inhibitor names its event only to count how
   often it plays, and that is not playing it).
4. Each hero's file with a swap in it goes in the package under its own path, with the same
   blocks before `DATA` and its events written again (2.14). The buffers are written without
   compressing them, as a single LZ4 block of literals, which every reader of LZ4 unpacks.

Whoever on Valve's servers asks for the hero's sound then plays the item's, on this computer,
for every hero that the server gives the default sound: the same as models (3.10). A player whose
own item changes that sound is asked for the item's event by the server, which is not changed.

`dme dev try-all` makes the heroes' files for every round, and `dme dev sounds` for every item
of a hero on its own: 321 of the 331 swaps are made. **(to confirm in a match)**

### 3.26 The body a picked item gives the hero, where the hero's own is

Some items swap a hero's whole body for another model, and the server decides a hero's model
the way it decides its sounds (3.25), so online such an item showed on the hero's own body. Two
kinds of blocks in `visuals` do it:

- `entity_model` whose `asset` is a hero, as `npc_dota_hero_nevermore`: the hero's body. Arcanas
  (Demon Eater, Manifold Paradox, Axe Unleashed's bare body), personas (3.22) and Tiny's sets,
  which name his sizes (2.10).
- `hero_model_change`: the model of a form the hero takes, named in `asset`, as
  `models/heroes/lycan/lycan_wolf.vmdl` for Lycan's Shapeshift, and also Terrorblade's
  Metamorphosis, Dragon Knight's dragon, Lone Druid's and Muerta's true forms and Undying's
  golem.

After the rewrite, the swaps are those that the default items of heroes have and did not have
in Valve's file, of the first style, each once (as 3.25 takes sounds). Each puts the item's
compiled model in the package where the game looks for the hero's body (its own file says,
2.10) or for the form's, as 3.24 does with pieces. It is left out where the place is not in
`models/heroes/` (something other heroes could have too), where the model is swapped for
itself, where the game lacks the item's model, and where another swap or a piece (3.24) put a
model there first. On build 6975, 145 items of heroes swap a body; `dme dev bodies` puts 150 of
their 151 swaps in place, the one left being a model swapped for itself.

Left out on purpose: the models of a hex (`hex_model`), as the item database names most of them
only as `hex` and the game's files do not say which model each hero's hex is, nor whether
Scythe of Vyse uses it too.

An arcana's body comes with its own animations. The game picks one by what the server says the
hero does, and an arcana's body may have two that fit where the hero's own has one, so a loop
such as standing still may start again on the other now and then. ArdysaModsTools' notes say
so, and that it rebuilds such bodies with Valve's tools to avoid it; that is not done here.
**(to confirm in a match)**

### 3.27 The picks put back in after a Dota update

Steam updates Dota by itself, and an update undoes what Play did: half of it (3.12), or, where
Valve changed `gameinfo_branchspecific.gi` too, all of it but the package, which nothing loads
then. Either way the picks no longer show. `Status::undone_by_an_update` says so of a game that
is half undone, or that is not set up to load the package while `state.json` has picks put in
for another build than the one in `steam.inf`. A game whose files Steam verified is of the
build the picks were put in for, and is not taken for one: the player wanted normal Dota.

Found so, the app puts the picks back in by itself (`Engine::mend`, `Job::Mend`), as soon as
all of this holds:

- Dota is not running, and Steam is done with Dota's files (3.12). Both are looked at again by
  `Engine::mend` itself, right before it writes, and Play looks at the first again before the
  first write (3.9).
- There are picks in use: not none, and not put away.
- What can be picked was read from the new build (3.12, `read_again_after_an_update`), and the
  picks were settled against it, so that one the update took away is taken off and said first.
- It was tried fewer than two times for this build.

It does what Play does to the files: the package made anew from the new build's archive, both
game files edited, and the new build's own files backed up first under the new build, so that
going back to normal gives back the bytes of the new build's files. Dota is not started. The bar
says "Dota was updated. Your picks are back in."

Where it does not work, the game is put back to normal (3.7), so that it is never left half
undone, and the player is told in one sentence, with "Copy details": the app could not put the
picks back in, so it put Dota back to normal; Play tries again. Where going back to normal does
not work either, it is tried once more, then the game is put back to normal as in 3.12, once.
Where the new build's files cannot be read, the game is put back to normal as in 3.12.

Until then the bar says what is going on: while Steam is busy, "Steam is updating Dota. Your
picks go back in once it is done.", the badge "Dota updating", and Play waits; once it is done,
that the picks go back in in a moment. A player who started Dota from Steam before the app could
put them back plays without them, half undone or not: nothing is written while Dota runs, the
bar says to close it if it cannot find a match, and the app puts the picks back in once it is
closed, so that the next start, from Steam or with Play, has them. The app has to be open for
any of this; one that is opened later does it as it starts. `dme status` says when a game is
undone by an update, and when Steam is busy with its files.

### 3.28 Languages

The window speaks English and Russian; a language is one file more (below). At the first start
it speaks the first language Windows is set to show its own texts in (`GetUserPreferredUILanguages`,
as `ru-RU`) that the app has and can show, and English where there is none. The choice is kept in
`settings.json` (`"language": "ru"`), and a settings file without it is a first start. Settings
has a chooser at its top, each language written in itself ("English", "Русский"). A choice
there changes the window at once, without starting again: every text is looked up as it is drawn.

**Where the texts are.** Every sentence the window shows is in `crates/dme-app/src/lang/`. `en.rs`
gives each text a key and its English, which is the source and the fallback; `ru.rs` and the
like give the same keys in their language. A text a language does not have is said in English.
A text can have places in braces: `{hero}` takes a value as it is, and `{count|pick|picks}` the
form of a word that goes with the number `count`, in the order of the language's forms: English
has two (one, more), Russian three (1, 21; 2 to 4, 22 to 24; the rest), Chinese one. A sentence
that is put together from parts is one text with places, so that each language orders its words
and forms its plurals as it does: "Good against {heroes}.", "{count} {count|day|days} old".
Numbers with a fraction are written with the language's mark before the fraction ("52.3%",
"52,3%"). A test names every text a language lacks or has twice, every place a translation
lost or made up, and every place with another number of forms than the language has.

The engine knows nothing of the window. It says what happened in kinds, which
`lang/words.rs` turns into words: an error as `Error::plain_kind` (a file another program
holds, a full disk, Dota running, Dota not found, Steam not found, a game file changed by
something else, missing models; anything else as the engine says it, in English), what going
back to normal left as it was (`engine::Note`), why a text is not a share code, roles, ranks,
attributes, and the words the catalog puts around the game's names (`catalog::Words`: "Persona
Weapon", "Blade Fury effects", "Level 2", "No emblem").

**What stays English, on purpose:** the command-line tool `dme`; what "Copy details" copies and
what "Report" writes into a public issue on GitHub, title and all (the maintainer reads them);
`CHANGELOG.md` and the entries of what is new (the sheet's title, line and button are in the
player's language); and the details of errors and logs. The update's own notes, which the
maintainer writes, are said as they are.

**The game's names.** Heroes, items, sets, styles, slots, rarities, abilities, talents and gems
are named as the game names them in the window's language: from `resource/localization/
<file>_<language>.txt` (`items`, `dota`, `abilities`), with the game's name of the language
(`english`, `russian`, `schinese`, `spanish`, `latam`, `brazilian`), and from the English
files name by name where those lack one (2.8). The Russian game keeps most names of heroes,
items and abilities in English; its slots, talents and some items have Russian ones. The game's
names are read again when the language changes. Typing finds a hero, an item or a set by its name
in the window's language and by its English name (`Catalog::english`; a hero has its English
name among its other names). An event's effect of a Dota item is named by the game's name of
that item, in any language but English.

**Fonts.** Inter (`assets/fonts`) has every letter of English, Russian, Spanish and Portuguese.
Chinese characters it has not, and the app brings no font for them: where a language needs them
(`Letters::Han`), a font Windows itself has is put behind Inter, read from the Windows fonts
folder, which is found through the environment (`SystemRoot`): Microsoft YaHei (`msyh.ttc`), or
else SimSun or DengXian. It is read only when it is needed. A language whose letters no font on
the computer can show is not offered, nor started in. `--game-language schinese` shows the game's
names of another language than the window's, with the font that language needs: that is how the
Chinese names were seen before the app had a word of Chinese.

**Adding a language** (also in `docs/DEVELOPING.md`): copy `lang/ru.rs` to `lang/<code>.rs`,
give it its code, its name in itself, the names Windows gives it, the game's name of it, its
forms and its mark before a fraction, and put it on its own line in `languages!` in
`lang/mod.rs`. Translate every text; `cargo test -p dme-app lang` names what is still missing.

## 4. The app updating itself

The app can put a newer version of itself in the place of its own program file. Nothing of
the game's has a part in this. It is one of the three things the app ever fetches from the
network. The others are the two files of numbers of the draft helper (5.4) and the build
helper (6.4).

A build is told two things, both in `crates/dme-core/src/update.rs`:

| Constant | Holds | Left empty |
|---|---|---|
| `MANIFEST_URL` | The `https` address of the manifest of the latest release | The build asks nobody, and has no Update button |
| `PUBLIC_KEY` | The public half of the key that releases are signed with, as 64 hex digits | The build goes by the checksum alone |

Both are set. A build looks at the releases of this project on GitHub and takes only what was
signed with this project's key. Whoever hands out builds of their own sets both to their own
(4.4), since with these values a build updates itself to this project's releases.

### 4.1 The manifest

A small JSON file that is published where `MANIFEST_URL` points:

```
{
  "version": "0.2.0",
  "url": "https://example.org/releases/0.2.0/Dota2ModEnjoyer.exe",
  "sha256": "c1eb4a36a6b501f1d23318996d34e33762c9710fa3b8a6563c34b7d413977cf7",
  "size": 15042584,
  "signature": "...",
  "notes": "Emblems, and a fix for the trees."
}
```

- `version`: numbers with dots between them, with or without a `v` in front. It is newer than
  the app's own version if its numbers are larger, compared one by one from the left; a number
  that is missing counts as 0.
- `url`: where the app's file of that version is, an `https` address as well.
- `sha256` and `size`: the SHA-256 of that file, as 64 hex digits, and its length in bytes.
- `signature`, for a build that has a `PUBLIC_KEY`: an Ed25519 signature, as 128 hex digits, of
  four lines of text with a line feed between each two: `Dota2ModEnjoyer`, the version, the
  checksum in lower case, the size. The address and the notes are not signed, because the file is
  known by its checksum wherever it is put, and a file that was signed as one version cannot be
  passed off as a later one.
- `notes`, if there are any: a sentence or two for the player to read.

Keys the app does not know are passed over. A file of more than 64 KB, or one that does not
have the first four, is no manifest.

### 4.2 What the app does

1. At its start the app clears away what an earlier update left beside its file (step 5),
   and asks for the manifest, on a thread of its own. If nobody answers, or the answer is no
   manifest, nothing is said and the app works as it is. Left open, it asks again every 5
   minutes, so a release that comes out while the app is open beside Dota is noticed without
   the app being started again.
2. If the manifest offers a newer version, and is signed where the build demands that, an
   Update button shows on the left side for as long as the app runs. One sentence in the corner
   above the footer says so once for each version, with the notes, and with "Update" and
   "Later". It takes the place of nothing else that is being said there, only of the offer of a
   version before it. While the app asks for the Dota folder, which has neither side bar nor
   footer, the sentence is in the corner of that page, since the new version may be the one
   that finds Dota.
3. Update fetches the file, at most 256 MB of it, and takes it only if its length and its
   SHA-256 are the ones the manifest names. A version that comes out during that is left for
   the version being fetched to find. Meanwhile the ring of the Update button fills up as far
   as the file has come, from the top round to the right, by the size the manifest names, and
   the button says the share ("45%"). Where nothing else is said in the corner, a card there
   says the same with a larger ring: "Downloading version 0.2.4…", and that the app starts
   again by itself. The page that asks for the Dota folder has no side bar, and has the card.
   How far it has come is told at most a hundred times, once for each whole hundredth.
4. The new file takes the place of the running one. Windows does not let the file of a running
   program be written to or deleted, but it lets it be renamed. So the new version is written,
   whole, to `Dota2ModEnjoyer.exe.new` beside the app; the running file is renamed to
   `Dota2ModEnjoyer.exe.old`; and the new one is renamed into its place. If that last step
   fails, the old file is put back. At no time is half a file under the app's name.
5. The app starts its file again, which is the new version now, and closes, though not in the
   middle of writing to the game's files. It tells the new version which version it was, with
   `--updated-from`. The new version shows, once, what is new since that version, in plain
   words: "Updated to version 0.2.3", then the changes, with "Got it" under them (where
   versions were skipped, each with its number). Those words are `CHANGELOG.md`, which the app
   carries within itself (4.4). A version the changelog says nothing of says instead, above the
   footer: "The app was updated. This is version 0.2.0 now." The new version clears the `.old`
   file away. It tries for a few seconds, since the old version may still be closing.

What is new is also shown to a player who put a newer version in place by hand. The app keeps
in `settings.json` the version whose news was shown last (`seen_version`), or that was started
first, and shows what came after it, once, after the notice and the tour. Versions up to 0.2.2
did not keep it: a player who has read the notice and whose settings name no version is shown
what is new in this version. The first start shows nothing of the kind.

A window of an older version that was left open does not stand in the way. Its file, the
`.old` of an earlier update, cannot be deleted while it runs. The next update renames it
further aside, to `.old.1` and so on up to `.old.9`, and goes on. Every start of the app clears
away the ones that have stopped running.

The player's picks and settings are not touched. They are in the app's own folder (1), not
beside the program.

No new version is fetched or put in place without the click on Update, unless the player asked
for that. Settings has "Update automatically", which is off until it is
switched on. With it on, a newer version is fetched and started as soon as the app hears of
it, and nothing is offered first. Switched on while a version is on offer, it fetches that one
at once. An update that fails is said as always, and is not tried again by itself. The Update
button stays for that. The choice is kept in `settings.json` as `auto_update`.

Whatever goes wrong after the click is said in one sentence with "Copy details", and leaves
the running version as it was.

### 4.3 Fetching

The app fetches with the part of Windows that is made for it, WinHTTP, and so brings no
network code of its own. That part checks who it is talking to against the certificates the
computer trusts, goes by the computer's own settings for reaching the internet, and follows a
server that sends it elsewhere, though never from `https` to plain `http`. An answer other
than `200` counts as a failure, and no wait is longer than 20 seconds.

A build for players fetches from `https` addresses and from nothing else, the manifest and the
file alike.

### 4.4 Making a release

```
dme dev update-key release.key
dme dev release Dota2ModEnjoyer.exe --url https://... [--key release.key] [--notes "..."] [--version 0.2.0] [--out update.json] [--trial]
```

`update-key` makes a key. It writes the secret half to a file, which stays with whoever makes
releases and out of the repository, and prints the public half, which is what `PUBLIC_KEY` is
to be. It does not write over a file that is there.

`release` writes the manifest for a file of the app: it works out the checksum and the size,
signs if it is given a key, and reads the result back the way the app will. The manifest is
written whole or not at all, so an app that asks at that moment never reads half of one. Without
`--version` the release has the version of the tool itself, which is built together with the
app and is `version` in the `Cargo.toml` of the workspace. It refuses an address that is not
`https`, a release without a signature where the build demands one, and a key other than the
build's. With `--trial` the address may be a plain `http` one or the path of a file. That is a
release for trying updates out on this computer (4.5), and never one for players.

Neither command needs a Dota install.

To have builds of your own update themselves, make a key, put its public half in `PUBLIC_KEY`
and the address the manifest will have in `MANIFEST_URL`, then build and hand that build out.
From then on, for each release, raise `version`, build, run `dme dev release`, and put the app's
file and the manifest where their addresses say.

Every release says what is new in it, in `CHANGELOG.md`: a heading with the version's number,
`## 0.2.3`, and a list of the changes under it, the newest version first, in words a player
understands. The app shows them after an update (4.2), and the release's page on GitHub lists
them. A test fails while the version in `Cargo.toml` is newer than the newest one the file
tells of, and `publish\release.ps1` stops on a version the file says nothing of.

### 4.5 Trying it

```
Dota2ModEnjoyer.exe --update-from MANIFEST [--pretend-version 0.0.1] [--no-restart]
```

`--update-from` names the manifest to ask for in the place of the build's, and makes this a
trial. The manifest and the file may then be at a plain `http` address, or be files on this
computer. A manifest that cannot be fetched or read the first time is said, not passed over.
The app asks again every 10 seconds, not every 5 minutes. No signature is demanded in a
trial, since its manifest is one's own, on one's own computer. `--pretend-version` is the version the app is to take itself for.
`--no-restart` puts the new version in place and does not start it. A new version that is a file
on this computer comes at once; how far it has come is told all the same, in 20 steps over 5
seconds, so that the ring can be seen filling up as over the internet.

A build made while the environment variable `DME_UPDATE_TRIAL` holds
the address of a manifest has that address built in, and is a trial whenever it is started
with nothing after its name, as a player starts it. Help then says, under the version, where
this copy looks for updates. The address may be a file on the computer the build was made on,
so such a build is for that computer and for nobody else. A window that is only there to have
its picture taken leaves the built-in address alone, and asks only where `--update-from` says.

```
set DME_UPDATE_TRIAL=C:\somewhere\update.json        (then build the app)
dme dev release C:\somewhere\Dota2ModEnjoyer.exe --url C:\somewhere\Dota2ModEnjoyer.exe --trial --out C:\somewhere\update.json
```

To see an update happen, start the trial build. Then raise `version`, build again the same
way, put the new file where the manifest's `url` says, and write the manifest with `dme dev
release --trial`. The app that is open offers the new version within 10 seconds.

A window that is having its picture taken and updates itself
starts the new version as a window of the same kind: off screen, with the same folders, and
with a picture of its own, `NAME.restarted.png` beside `NAME.png`. The window that updated
closes without a picture. So the restart is tried without a window on anybody's screen.

Tried with the 0.1.0 build: an update from files on the computer; one over `http` from the
same computer, also with a server that sends the app elsewhere for the manifest and for the
file; a file that is not the one its manifest describes; a manifest that is not there; and
`https` addresses, one of which sends the app elsewhere. The whole of it was also tried to its
end, off screen: the app started while nothing newer was out, a release was put out while it was
open, the app offered it within its 10 seconds, the click on Update put the new file in place,
the new version started, said that the app was updated, and cleared the `.old` file away.

## 5. The draft helper

The Draft page says which heroes to pick in a match. The player puts the heroes of both teams
in by hand as they are picked, and the page lists the heroes that win more often against those
enemies and beside that team. Nothing of this reads the game. The app does not look at Dota's
window, its memory or its files to learn what was picked, and the page works the same with Dota
closed.

### 5.1 The numbers

The page works from one file of numbers, `draft.json`. It is made once a week by
`publish/make_numbers.py` from the statistics of STRATZ (stratz.com), which are added up from
public matches. The app never asks STRATZ anything. It fetches the file from this project's
repository on GitHub, at the address `NUMBERS_URL` in `crates/dme-core/src/draft.rs`.

```json
{
  "format": 1,
  "made": 1794489536,
  "source": "STRATZ",
  "patch": "7.41f",
  "weeks": 4,
  "heroes": ["npc_dota_hero_antimage", "npc_dota_hero_axe"],
  "ranks": [
    { "played": [[9000, 0], [800, 0], [0, 7000], [0, 300], [0, 0]],
      "won": [[4600, 0], [390, 0], [0, 3500], [0, 140], [0, 0]] }
  ],
  "pairs": [
    { "against": [[0, -51], [51, 0]],
      "with": [[0, 12], [12, 0]] }
  ]
}
```

| Key | Holds |
|---|---|
| `format` | 1. The app refuses a file of another format and goes on with the numbers it has |
| `made` | When the file was made, in seconds since 1970 |
| `source`, `patch`, `weeks` | Whose numbers these are, the patch of Dota the matches were played on, and how many weeks of matches are added up |
| `heroes` | The heroes, by the names the game's files know them by. Everything below names a hero by its place in this list |
| `ranks` | The eight ranks, from Herald to Immortal |
| `played`, `won` | In a rank, for each of the five roles (carry, mid, offlane, soft support, hard support), each hero's ranked matches in that role, and how many of those it won |
| `pairs` | Four pairs of ranks, in this order. Herald with Guardian, Crusader with Archon, Legend with Ancient, Divine with Immortal |
| `against` | In a pair of ranks, for each hero, a number for each other hero. It says by how much the hero wins more often against the other than expected, in tenths of a percent |
| `with` | The same for the two heroes on one team |
| `lanes` | How lanes go, for each of the four pairs of ranks. Files made before 0.3.1 have none, and versions before it leave it alone |
| `played`, `won`, `drawn` | In `lanes`, for each of the five roles, how often each hero met a hero of the other team in its lane in that role, and how many of those lanes it won and how many ended even |
| `against`, `with` (in `lanes`) | For each of the five roles, for each hero, a number for each other hero: by how much more often the hero came out of its lane ahead against that hero, or beside it on its own team, than expected, in tenths of a percent |
| `weekly` | The newest whole week of the ones added up and the week before it, each by itself (5.7): `weeks`, when the two began, the newest first, and `ranks`, for each of the eight ranks the two weeks' `played` and `won`, as in `ranks`. Files made before 0.5.0 have none, versions before it leave it alone, and a version that finds it of another shape leaves it out and reads the rest |

A lane is counted once for every hero of the other team met in it, so a carry who met an
offlaner and a soft support counts it twice, once against each. A lane is won, lost or even
as STRATZ judges it, and a lane STRATZ calls a stomp is a win or a loss like any other. Coming
out ahead counts a lane that ended even as half won.

"Expected", for a lane, is the hero's own share of lanes won in that role, plus what the other
hero does to the lanes of that role as a rule. That is the share by which the heroes that met
it did better or worse than in all their lanes, each counted as often as it met them. A hero
that wins its lane against every carry is then no reason for one carry over another, and the
numbers say what a pair does beyond that, as the match's numbers do. What a pair shows beyond
it is believed by `n / (n + 400)`, so that a pair that met seldom is taken to go as the rule
has it, and cut off at 30 percent, as above. It is not mirrored: a carry against an offlaner
and that offlaner against the carry are two numbers, one from each side of the lane.

"Expected" is what the two heroes' own shares of wins give if the heroes have nothing to do
with each other. For a hero that wins `a` of its matches against one that wins `b` that is
`a(1-b) / (a(1-b) + b(1-a))`, and for the two on one team it is `ab / (ab + (1-a)(1-b))`. What a
pair shows beyond that is multiplied by `n / (n + 400)`, where `n` is how many matches the pair
has, so that luck in a few matches is not taken for a counter. It is cut off at 30 percent
either way. STRATZ counts a pair once from each of its two heroes, and the table has the
middle of the two counts.

How often a hero wins is counted for each rank by itself. What two heroes do to each other is
counted for two ranks together, because STRATZ has it no finer, so the two ranks of a pair
share `against` and `with`. Every rank has enough matches for its own counts. Herald, which
has the fewest, had 2.3 million ranked matches of a hero in four weeks.

The maker adds up the whole weeks since the newest patch came out, four at most and two at
least. Where fewer than two whole weeks have passed since the patch, the two newest are added
up, which are of the patch before it, and `patch` names the patches the weeks are of rather
than the newest: "7.41f", or "7.41f and 7.42" for a week a patch came out in. The file of
builds names the newest patch, which STRATZ counts builds over. It asks STRATZ about sixty questions, each for all heroes at once, with a token that
it reads from the environment variable `STRATZ_TOKEN`. The token is never written anywhere.
`python publish/make_numbers.py --check` tries the sums on made-up numbers. The maker writes
no file larger than the 4 MB the app takes (5.4). With the lanes and the weeks apart the file is about 2.2 MB.

STRATZ's lanes come from its `laneOutcome`, asked for each role of the first hero by
`positionIds`. Its answer names every row's position and rank as the first ones there are,
whatever was asked, so the maker goes by what it asked. An answer of 16000 rows may have been
cut short, and the maker then asks for that role again, a hero at a time.

### 5.2 What is suggested

The page asks for a rank, a role or none, the heroes of one team and the heroes of
the other. Every other hero gets points, which are percent of matches.

1. The hero's own share of wins less 50, in the role that was asked for or in all roles
   together. It is multiplied by `n / (n + 400)`, where `n` is the hero's matches there.
2. Plus the hero's number in `against` for each enemy, in the pair the rank is in.
3. Plus the hero's number in `with` for each hero of the team, in that pair.

The heroes are listed by their points, the most first. With a role, only heroes that have at
least a tenth of their matches in that role are listed. A hero that is picked is not listed. A
picked hero the file does not know, such as one that came out this week, changes nothing, and
a hero that the game on this computer does not have is not shown.

A whole draft is weighed the same way. For each team, the points of its five heroes are added
up, which are each hero's own share of wins less 50 in its role, the number in `against` for
every hero against every enemy, and the number in `with` for every two heroes of a team, each
pair once. A hero with fewer than 100 matches in its role counts by all its matches together.
The player's team's sum less the enemy's is how far ahead the team is, `d`. Its chance to win
is `100 / (1 + e^(-d/25))` percent, which is 50 for two drafts that are level, close to `50 + d`
for small differences, and never 0 or 100.

That is "Team composition". "Lane winning" weighs the heroes of a role by their lanes alone,
from `lanes` in the pair the rank is in:

1. How much more often the hero comes out of its lanes ahead in that role than the role does
   as a whole, multiplied by `n / (n + 400)`, where `n` is how often it met a hero there. The
   whole role is the heroes' lanes added up: safe lanes are won more often than off lanes, so
   a carry is weighed against carries.
2. Plus the hero's number in the lanes' `against` for each hero of the other team in its lane.
   The carry and the hard support share the safe lane and meet the offlaner and the soft
   support, and the other way round; the mids meet each other. Which heroes those are is told
   by the roles of the places (5.3). For a carry or an offlaner the enemy's support there
   counts half and the enemy's core in full, because a core's lane is mostly against the
   other core. For a mid or a support every enemy of the lane counts in full.
3. Plus half of its number in the lanes' `with` for the hero of its own team it shares the
   lane with: the carry and the hard support, the offlaner and the soft support. The mid has
   none. The lane is mostly the hero's own doing.

Only heroes that are played in the role, as above, and that have lanes in it are listed. A
whole draft is weighed by the whole match in both.

### 5.3 The page

The page has two lists to choose from, "My rank" and "My role", which are kept in
`settings.json`. In the line of its name is a switch of two halves, "Team composition" and
"Lane winning" ("Team" and "Lane" in a narrow window), which says what the list weighs heroes
by (5.2). It is kept in `settings.json` too, and Team composition is where it starts. Each is behind a box that shows what is chosen. A rank has the game's medal for it beside
its name, and a role has the game's drawing for it.
Under them are five places for each team. The places are in the order of a ranked draft, in which each team picks
two heroes, then two more, then its last, and they stand in those three groups.

A click on a free place makes it the one to fill, and the enemy's first place is that when the
page opens. Typing shows the heroes whose names fit. A click on one, or Enter for the first,
puts it in the place and goes on to the next free one. That is the other place of the same
phase and team, then the other team's places of that phase, then the next phase, where the
enemy's places come first. A click on a hero that is put in takes it off.

Every place has a role, shown as the game's own drawing for that role in a corner of the
place. A click on the drawing opens the five roles, each with its drawing and its name, to
choose from. The first two places of a
team start as soft support and hard support, the next two as carry and offlane, and the last as mid, which is how
ranked teams mostly pick. Choosing another role for a place gives the place of that team that
had it the role this one had, so a team has each role once. The place of the player's team
with the role the player chose is the player's own. Its drawing is on the app's colour, and
the place says "You" where there is room.

Under the teams is the list. It is for the place that is being filled, whichever team's,
with one exception: while no place has a hero and the player has not clicked a place, it is
for the player's own place, so that the best heroes this week (5.7) are those of "My role" and
change with it at once. Typing still fills the enemy's first place first. The list's title
says whose place it is for: "Best heroes this week for you, in mid", "Heroes for the enemy's
carry". The list has the heroes of that place's role (5.2), with that place's team as the team and the other
as the enemy, so for a place of the enemy it shows what does best for them. Each line has the
hero, its points, and what speaks for and against it in words, with the picked heroes that
add or take the most. What a hero adds is green and what it takes is red, in the words and
in the points, and so is the share of its matches a hero wins, green above half and red
below. A click on a line puts the hero in the place the list is for. Where that is the place
being filled, it does what typing the hero's name does and goes on to the next free one;
before the first pick, when it is the player's own place, the place being filled stays the
next. The cross at its end marks the hero banned (5.6). The three best heroes of a list for a
place of the player's team have "Suggest to my team" (5.8). A hero the player does not play is
left out by the hero pool (5.5).

Once all ten places have a hero, the page says which draft is better (5.2), small, at the
right end of the line of the two lists, over the teams. A bar is shared by the two teams by
their chances, with each team's percent on its part, and a few words stand before it where
there is room. Two drafts within one percent of each other are called even. Under the teams
is a large button, "Go to build", which names the player's own hero and opens its build
(6.3). With no role chosen no hero is the player's own: the button cannot be pressed, and
the words beside it say to choose "My role". The list says whose
numbers it shows, of which patch and how old they are, and those words open stratz.com.

### 5.4 Fetching and keeping

Nothing is fetched before the Draft page is opened. The app then fetches the file the way it
fetches updates (4.3), from an `https` address, and takes no more than 4 MB. It keeps the file
as `draft.json` beside its settings and reads it from there at the next start. A kept file
that is less than a day old is not fetched again, and the app looks once in a run.

A file that does not come, or that is not of the shape above, changes nothing. The numbers
that are kept stay in use. Where there are none, the page says that they could not be fetched
and has "Try again".

Numbers made before 0.3.1 have no lanes. With Lane winning chosen and such numbers kept, the
app fetches the file again once in a run, however new the kept one is. Where the file still
has none, the list makes way for a few words that say so, and "Fetch again".

For testing, `--numbers-from` says where to fetch the file from, which may be a file on the
computer. A window that is only there to have its picture taken fetches only then. A build
made while the environment variable `DME_NUMBERS_TRIAL` holds such a place fetches from
there in the place of the built-in address, with no option, the way `DME_UPDATE_TRIAL` does
it for updates (4.5). That is for a build somebody is to try before the files are put out.
`publish/release.ps1` builds with neither of the two set.

### 5.5 My Hero Pool

The hero pool is the heroes the player plays, chosen in Settings under "My Hero Pool" and kept
in `settings.json` as the names the game's files know them by, in the order they were chosen,
each once (`draft_heroes`; the build of 2026-11-22 wrote it as a set, which is read as it is).
The part has three places to begin with, and a box "More" adds one place at a time, up to ten.
A saved pool of any size shows whole, with as many places as it has heroes. A click on a place
opens a box to type in under the places, with the heroes whose names fit, by their name in the
window's language or in English, each with its picture; a click on one, or Enter for the first,
puts it in the place, in the place of the hero there if there is one. A hero already in the
pool is not offered. A cross on a hero takes it out and leaves its place empty, and a cross on
an empty place beyond the third takes the place away. Empty places are not kept. Escape and
"Cancel" end the search.

The pool has to do with the player's own place, and with no role chosen with every place of the
player's team. There the list has the pool alone, those of its heroes that are played in the
place's role, and a switch of two halves beside its title, "My heroes" and "All heroes",
whose choice is kept in `settings.json` too (`draft_all_heroes`). Where none of them is
played in the role, the list has every hero and says so. The enemy's places and the places
of the player's team mates have every hero, as before. A hero the player does not play is
so for good, and a banned one for one draft (5.6): the two are kept apart. Where the pool is
empty, quiet words beside the title of such a list, or under it in a narrow window, say
"Add the heroes you play in Settings: My Hero Pool", and a click on them opens Settings at
that part.

### 5.6 Bans

Ranked matches ban heroes before any is picked, so there is no enemy to go by. While no
place has a hero, a line under the teams names three heroes to nominate for a ban. Each gets
points:

1. Its own share of wins less 50 in the player's rank, in all roles together, believed by
   `n / (n + 400)` as in 5.2.
2. Plus the middle of its numbers in `against` for the heroes it is to be kept from. Those are
   the heroes of the pool that are played in the player's role, or all of them where none are.
   Without a pool, they are the heroes played in the player's role, each counted as often as it
   is played there. Without either, nothing is added.

Neither the heroes of the pool nor the three best of the player's own list (5.8) are
named to ban, nor a hero that is banned already. A click on one marks it banned. So does the
cross on a hero of the list, "Ban" beside the heroes that fit what is typed, which bans the
first of them, and "Ban" in the menu of a right click on one of them. A banned hero is in no
list and leaves any place it had. It stands in the same line, crossed out, and a click on it
takes the ban off; so does putting it in a place. "New draft" takes every ban off.

### 5.7 Rising and falling

With no place filled, the list for Team composition is of the heroes that win most in the
role and rank, and is called "Best heroes this week" with whose place it is for and its role:
the player's own, with the role chosen in "My role" (5.3). The file is made once a
week from the weeks since the patch (5.1), so that is this week's list.

Where the file has `weekly`, a hero of the list is rising or falling where its share of wins
in the place's role went up or down from the week before to the newest week by at least 1
percent of matches and by at least twice what luck alone moves it by in so many matches,
`100 * sqrt(p(1-p)(1/n1 + 1/n0))`, where `p` is its share over both weeks and `n1` and `n0` are
its matches in each. A hero played fewer than 100 times in one of the two is neither. Such a
hero has "Rising" in green or "Falling" in red beside its name, with a small triangle, in
Team composition however much is picked, and its tooltip says its share in each week. The
maker takes the two weeks from the same answers it adds the weeks up from, and asks STRATZ
nothing more for them. They add about 90 KB to the file.

### 5.8 Suggest to my team

The three best heroes of a list for a place of the player's team, as it is shown (with the
pool and the bans), have a button "Suggest to my team" on their line, before the points. It
copies one line for Dota's chat with the hero's name as the app shows it: "Outworld Destroyer
is a good pick against the enemy" once the enemy has a hero, and before that, when "against
the enemy" would not be true, the role of the list: "Outworld Destroyer is a good pick in
mid". A line of the list narrower than 720 points has a small speech bubble in the place of
the words, which say what it does while the pointer rests on it. A click on the button does
not put the hero in a place. Lists for the enemy's places have no such button. The line is
copied the way "Copy details" is, so that a window that is only there to have its picture
taken writes it to `copied.txt` in its data folder and leaves the clipboard alone, and a
message says what was copied and that it is pasted in Dota's chat with Ctrl+V.

## 6. The build helper

The Builds page says what the players of a hero buy and skill in a role, in the player's rank.
Like the Draft page it reads nothing of the game that is running. It shows what is added up
from finished matches.

### 6.1 The builds

The page works from a second file, `builds.json`, which `publish/make_numbers.py` makes in the
same run as `draft.json` (5.1) and from STRATZ as well. It is beside that file, where it is
fetched from and where it is kept.

```json
{
  "format": 2,
  "made": 1794499945,
  "source": "STRATZ",
  "patch": "7.41f",
  "sampled": 160000,
  "items": ["item_tango", "item_power_treads", "item_bfury", "item_monkey_king_bar"],
  "skills": ["antimage_mana_break", "antimage_blink", "special_bonus_unique_antimage_3"],
  "heroes": {
    "npc_dota_hero_antimage": [
      { "role": 0, "pair": 1, "matches": 95312,
        "start": [[0, 1, 87700, 43900]],
        "items": [[1, 8, 94300, 47200], [2, 15, 93400, 46800]],
        "talents": [[2, 57200, 31700]] }
    ]
  },
  "orders": { "npc_dota_hero_antimage": [[0, 600, [1, 0, 0, 1, 0]]] },
  "against": { "npc_dota_hero_phantom_assassin": [[3, 240]] },
  "pros": { "source": "OpenDota", "matches": 1227,
            "heroes": { "npc_dota_hero_storm_spirit": [[1, 109, [[4, 109, 1]]]] } }
}
```

| Key | Holds |
|---|---|
| `format` | 2. The app refuses a file of another format and goes on with the builds it has |
| `made`, `source`, `patch` | As in `draft.json` |
| `sampled` | How many single matches the orders of skill points and what is bought against whom are from |
| `items`, `skills` | The items and the skills the file names, by the names the game's files know them by. Everything else names one by its place in these lists. Talents are among the skills |
| `heroes` | For each hero, by the name the game's files know it by, its builds |
| `role` | The role the build is for, from 0 for carry to 4 for hard support |
| `pair` | The pair of ranks, from 0 for Herald with Guardian to 3 for Divine with Immortal. STRATZ counts builds for two ranks together |
| `matches` | How many matches the build is added up from |
| `start` | What is bought before the match begins. Each is the item, how many of it, the matches it was bought in and how many of those were won |
| `items` | What is bought after the start, boots among it, in the order it is bought. Each is the item, the minute it is bought in on average, matches and wins |
| `talents` | For each talent that is taken, matches and wins |
| `orders` | For each hero, for the roles it has builds for: the role, how many single matches the order is from, and the skill or talent each skill point is put in, the first point first |
| `against` | For each enemy hero, the items that are bought more often against it, each with how many times as often, in hundredths: 240 is 2.4 times |
| `pros` | What pros buy: whose numbers they are, how many pro matches they are from, and for each hero, for the roles enough pros played it in, the role, how many pros played it so and the items they bought, each with how many of them bought it and the minute they first bought it in on average. A file without it is read, and has nothing of the pros |

A hero has a build for each role that has at least a tenth of its ranked matches in the pair
of ranks, for three roles at most, and only where the build is of at least 150 matches. An
item is in a build where enough of the hero's players buy it, which is 15 of 100 at the
start, 5 of 100 for boots and 3 of 100 after that, thirty items at most. STRATZ counts what
is bought after the start by the minute, and leaves out every minute an item was bought in
by fewer than about 250 matches, unless the question's `matchLimit` says otherwise. The
maker sets it to 0. Without it an item that is bought at many different minutes, as most
that are bought for the match at hand are, was not there at all: Puck's Aghanim's Scepter,
bought in 24 of 100 Divine and Immortal matches, and Linken's Sphere, in 27. A second, third or
fourth copy of a starting item counts where at least half of those who start with the item
have that many. STRATZ calls each copy after the first that is bought in the same moment
"given" (`wasGiven`), as the second, third and fourth Iron Branch. The purchase logs of pro
matches have each of those bought, so they count whether they are called bought or given. A
first copy that is given is not one the player bought, and is left out.
The builds are of the newest whole week of matches, and the maker asks one question for each
hero and pair of ranks, about five hundred.

STRATZ loses purchases of some items, so its numbers of them are not used. In patch 7.41f a
Bottle was bought in 6 of 100 matches of mid heroes across all ranks by STRATZ's numbers,
and in about 5 to 10 of 100 even on Storm Spirit, Puck, Queen of Pain or Lina. Of 123 mid
players in ranked matches that OpenDota had parsed lately, mostly Immortal, 35 of the 38 on
those heroes and the like bought one. Of five such matches, in which the mid player bought a
Bottle in the second minute by OpenDota's records and in four of them still had it at the
end, STRATZ's records of the same matches had no Bottle. So the maker leaves the Bottle out
of what STRATZ says (`LOST_BY_STRATZ`), and what pros buy says whether to have it.

What pros buy is asked of OpenDota (opendota.com), whose database of pro matches takes a
question in SQL, and whose records of purchases are whole. It is one question, without a
token, for the pro matches since the patch began, and for those of the last 28 days where
the patch is newer than that (`PRO_LEAST_DAYS`): the first two days of 7.42 had 26 pro
matches, where the nine weeks of 7.41f had 1,350, and with the patch's own alone every
build would have lost what the pros buy, the mid's Bottle with it, for some weeks after each
patch. So the page says "recent pro matches", not "of the patch". A pro match says which lane each player
was in (`lane_role`: 1 the safe lane, 2 the mid, 3 the off lane), not their role. Of the two
in the safe lane, the one with more last hits at minute 10 is taken for the carry and the
other for the hard support; of the two in the off lane, the one with more for the offlaner
and the other for the soft support; and in mid the one with the most. In 7.41f carries had
64 last hits at minute 10 on average, mids 59, offlaners 55, and the supports 8. A third
player in a lane is left out. For each hero and role the question gives how many pros played
it so, and what they bought, each item once for each pro, with the first minute they bought
it in. A hero and role are in it where at least 8 pros played them, and an item where at
least 30 of 100 of them bought it. Of those, only items that STRATZ counts among what is
bought after the start are kept, which leaves out what is used up, such as a Town Portal
Scroll, and what is bought before the match begins on average. Where OpenDota cannot be
reached, the file is written without them.

STRATZ adds up no order of skill points, and nothing by the heroes of the other team. Both
are made from single matches, which STRATZ hands out as guides: up to 600 for each hero and
role, all of them won, by Divine and Immortal players, of the last days. One question is
asked for each hero and role, about three hundred. What is made from them is the same for
every rank.

The order of skill points is made point by point. Each point is what most of those matches
do that did the same up to there, while there are at least ten of them, and what most of all
the matches do with that point where there are fewer. A point is said while a quarter of the
matches got as far, and nineteen points at most. A hero has a point to put somewhere at
every level up to 16 and at the levels 18, 20 and 25, so the nineteenth is that of level 25.

What is bought against whom is counted over all heroes together, since the matches of one
hero against one other are too few to say anything. For an item and an enemy hero, the maker
adds up how often the item was bought in the matches against that hero, and how often it
would have been bought there if the players had bought as they do in all of their matches.
The item is said to be bought more often against the hero where it was expected at least 8
times, was bought more often than expected by 3 times the spread that luck gives, and at
least 1.25 times as often. Before that, 15 purchases are added to both sides, which pulls
what few purchases show towards "as usual", and what is bought more of everything against
that hero is taken out, since matches against some heroes are longer. No more than 5 times
is said, no more than 14 items for a hero, and only items that some build has.

### 6.2 Names and pictures

The file names things as the game's files do, and has no words or pictures of its own.
Everything shown is read from the game on the player's computer, when the page is first opened.

| What | Where it is read from |
|---|---|
| The name of an item or a skill | `resource/localization/abilities_english.txt`, under `DOTA_Tooltip_ability_` and its name |
| The picture of an item | `panorama/images/items/NAME_png.vtex_c`, without the `item_` the name begins with |
| The picture of a skill | `panorama/images/spellicons/NAME_png.vtex_c` |
| A hero's talents | The hero's own file (2.10), `Ability10` to `Ability17`. Two for each of the levels 10, 15, 20 and 25, and of each two the one the game shows on the right comes first |
| What an item is built from | `scripts/npc/items.txt`, where a recipe names its `ItemResult` and the parts in `ItemRequirements` |

A talent's text has its amounts left open, as in `+{s:bonus_attack_factor}% Stifling Dagger
Damage`. The amount is written where the skill's value is listed in the hero's own file,
under the talent's name, as in `"attack_factor" { "value" "30" "special_bonus_unique_x" "+15" }`.
A name in the text that begins with `bonus_` stands for the value without it. For `{s:value}`
the amount is in the talent's own description, in the hero's file or in
`scripts/npc/npc_abilities.txt`, or it is the number the talent's name ends in. The sign is
taken off the amount, since the text has its own. An amount that is found nowhere shows as "?".

An item is left out of a build as a part of another item of the build where at least 60 of
100 of those who buy it go on to the other. A Perseverance that nearly everybody turns into a
Battle Fury is not shown, and a Skull Basher that most players keep is.

### 6.3 The page

"Builds" is under "Draft" on the left side. The page shows one hero's build in one role, for
the rank chosen as "My rank". The role and the rank are two lists beside the hero's name. The
role's list has all five roles. A role too few play the hero in for a build of its own, as
Crystal Maiden as a carry, shows the build of the role the hero is played in most, and beside
the lists: "Too few play Crystal Maiden as Carry in this rank for a build of its own. This is
the build of Hard support, the role Crystal Maiden is played in most."

Every hero is on the page and on the Draft page, also one with nothing to pick, which the
Heroes page leaves out: Kez and Largo on build 6975 (`Catalog::plain_heroes`). STRATZ counts
no talents of those two and has no single matches of them, so their builds have no order of
skill points, and the talents line says "the numbers do not count them" and shows no shares
in the place of 0 of 100.

Which hero it is comes about in three ways. "Go to build" on the Draft page, which shows once
all ten places have a hero, opens the build of the player's own hero in the role the player
plays. That hero is the one in the place of the player's team with the player's role (5.3),
so the button waits for a role to be chosen. Opening the page while no hero is chosen shows
that same hero, if there is one. And typing a hero's name on the page finds any hero.

The build is on one screen, in five lines, each with its name at the left edge.

| Line | What it shows |
|---|---|
| Start | What is bought before the match begins, as one player could buy it with the 600 gold every hero starts with (the game's files do not say it; it has been 600 since 7.09). The file has each item that at least 15 of 100 start with, which together cost more: a support's Tango, Blood Grenade, Magic Stick, Wind Lace and Boots came to 1380. The page takes them the most bought first, each while the gold left pays for it, and as many of its copies as it pays for; what costs more than is left is passed over, and something after it that costs less is still taken. The prices are the game's own, `ItemCost` in `scripts/npc/items.txt`; an item without one costs nothing, and without that file everything is shown. Under the line's name, what it comes to, "595 of 600 gold". An item that is bought three times is there three times, up to four. Where the line has no room for that, or there are more, it is there once with "×3" in its corner |
| Core items | The items that at least 35 of 100 of the hero's players buy, boots among them, and those that at least 50 of 100 pros who played the hero in the same role buy (6.1), together in the order they are bought, each with its minute in a corner |
| Other items | The rest, the most bought first |

What the shop does not sell is left out of every line, though STRATZ counts it as bought: the
Healing Lotus, Great Healing Lotus and Greater Healing Lotus, picked up from the pools on the
map. The game's file of items says so of each, `"ItemPurchasable" "0"`. So is what a patch
took out of the game, which that file keeps with `"IsObsolete" "1"`: the Javelin, the Helm of
the Overlord and Angel's Demise in 7.42, though the numbers of the weeks before it have them.

| Line | What it shows |
|---|---|
| Skills | A square for each skill point, with the hero's level over it. In it is the skill's picture, or for a talent the level it is of |
| Talents | As the game shows them, level 25 at the top, each with how many take it. Of each two, the one more players take stands out. A talent the numbers count none of, where they count talents the hero no longer has, came with a patch after them: it says "new" in the place of how many take it, and neither of its level stands out (`Build::is_new_talent`) |

Under each item is how many of 100 of the hero's players buy it, and under an item of the
core that is there for the pros, how many of 100 of the pros do. The two are not told apart
on the line: the user asked for one build, without a mark for the pros. Resting the pointer
on an item, a skill point or a talent says more: the minute, how often its buyers win, what a
talent does, and for an item of the pros how many pros buy it and, where the rank's numbers
have the item, how many players of the rank do. Where they do not, as for the Bottle that
STRATZ loses, nothing is said of the rank.

Every item of the core is shown. Where they are more than the line has room for, as they
can be with the pros' items, the pictures of that line are smaller, down to 30 points wide.

An item of the pros is one the core of the build does not have. One that is a part of an
item of the core is left out, as Boots of Speed beside Power Treads, and so is one that is a
part of another item of the pros that at least 60 of 100 of the pros who buy it go on to.
Such an item is not among the other items too.

Resting the pointer on one of the other items also names the heroes it is bought more often
against (6.1), the three it is the most so for, with a draft or without one.

With a draft on the Draft page, the other items go by the heroes the hero has against it.
Those are the other team's, and the enemy's for a hero that is in neither team. An item that
is bought 2 times as often against one of them (6.1) has twice the odds of being bought in
this match. An item that 10 of 100 buy, which is odds of 1 to 9, comes to 2 to 9, which is 18
of 100. The odds are doubled and not the share, since the times are counted in matches of
the highest ranks, where the item may be bought less often to begin with. Two such heroes
count one on top of the other. Such an item moves up in its line, its number is green, and the hero it is bought
against most shows in its corner.

The page grows with the window, up to one and three quarter times its smallest size. In a window that is too small
for the smallest of them the page can be scrolled. Where the hero has no build for the
player's role, the role it is played in most is shown.

### 6.4 Fetching and keeping

`builds.json` is fetched and kept as `draft.json` is (5.4), when the Builds page is first
opened, and takes no more than 8 MB. `--numbers-from` names where `draft.json` is fetched
from, and `builds.json` is fetched from beside it.

### 6.5 Neutral items

What pros take of neutral items is in `builds.json` too, under a key of its own, `neutrals`.
The file stays of format 2: the apps of 0.4.3 and before pass over a key they do not know, as
the shape of the file that they read (`File` in `builds.rs`) does not refuse one. The app of
this version reads the part apart from the rest, and where it is not of the shape below, reads
the file without it.

```json
"neutrals": {
  "source": "OpenDota", "patch": "7.42", "matches": 50,
  "names": ["item_pogo_stick", "item_enhancement_alert", "item_brick"],
  "heroes": {
    "npc_dota_hero_slark": [
      [0, 9, [[1, 9, 0, [[0, 4]], [[1, 6]]], [2, 9, 0, [[2, 2]], [[1, 7]]]]],
      [5, 13, [[1, 13, 0, [[0, 4]], [[1, 8]]]]]
    ]
  }
}
```

| Key | Holds |
|---|---|
| `source`, `patch`, `matches` | Whose numbers, the patch whose pro matches they are of, and how many |
| `names` | The trinkets and enhancements, by the names the game's files know them by. The rest names them by their place here |
| `heroes` | For each hero, the role (0 to 4, or 5 for every role together), how many pros played it so, and its tiers. A tier is its number, how many of those pros made something of it, how many of them kept the trinket they had and took only a new enhancement, the trinkets and the enhancements taken, each with how many took it, the most taken first |

**From the newest patch alone.** A patch changes the trinkets of each tier: 7.42 brought the
Red Ledger, the Vile Phial, the Brick and the Balmhook, and made Revenant's Brooch a trinket of
tier 3. So, unlike what pros buy (6.1), what they take of neutral items is counted in the pro
matches of the newest patch alone, though they are few in its first weeks (50 on the fourth
day of 7.42, about 20 more each day). The maker asks OpenDota once, in SQL. The newest patch
is that of OpenDota's newest pro match, in its table `match_patch`, which names each pro
match's patch by when it was played ("7.42", without the letters of the small patches after
it): STRATZ named 7.42 only days after it came out. STRATZ is not asked: its numbers of
neutral items were empty, and its single matches had neither the new trinkets nor any
enhancement.

A pro match has, for each player, what was made at each tier (`neutral_item_history`), in
order: the first of tier 1, the second of tier 2, and so on, each with the trinket and the
enhancement. One without a trinket kept the one before. What comes after the fourth is made
again at tier 4 and is not counted. OpenDota calls the Defiant Shell `guardian_shell`, and the
maker gives it the game's name. The roles are told apart as for what pros buy (6.1).

A hero in a role is said where at least 8 pros played it so, as for what pros buy. For the
other roles, the hero in every role together is said, where at least 8 pros played it at all.
A tier is said where at least 5 of them made something of it, and a trinket or an enhancement
where at least 15 of 100 of those, and two at the least, took it: three trinkets and two
enhancements of each tier at the most. On the fourth day of 7.42, 12 heroes in a role had
their own and 24 heroes one of every role. The part had 6,169 bytes then. With every hero in
every role it would come to some hundreds of thousands, far inside the 8 MB the file may have.

What the app shows of it goes by the game's own file of neutral items,
`scripts/npc/neutral_items.txt`: its tiers (`neutral_tiers`), the minute each opens
(`start_time`), its trinkets (under `item_pools`, in pools of three) and its enhancements
(under `enhancements`, by the hero's attribute and for every hero). Of what pros took, only
what the game has in that tier is shown: what a patch took out of a tier or moved to another
is left out, and a tier with nothing left is not shown. Where the game's file cannot be read,
nothing is. The names and pictures are those of items (6.2). The game has the names of some
hundreds of items, the neutral items among them, under the name with `:n` after it, as
`DOTA_Tooltip_Ability_item_pogo_stick:n`, "Tumbler's Toy": the app looks there too.

On the page, the line "Neutral items" is between the other items and the skills. Under its
name, how many pros and the patch, and "in any role" where the pros are of every role. For each
tier, "Tier 2" and the minute it opens, then the trinkets most taken and the enhancement most
taken, each with how many of 100 of those who got as far took it. Resting the pointer on them
says how many pros, in which role and patch. Where the tiers do not fit, the pictures are
smaller (6.3). A hero that too few pros played has no such line, and the page is as before.
The page's foot names the pro matches they are from. `dme dev builds` says what of them the
game has no name or picture for, and what is not in the tier the game has it in.
