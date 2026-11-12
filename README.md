# Dota 2 Mod Enjoyer

Pick what your heroes and your game look like, press Play, and Dota starts that way.

> **Use at your own risk.** This tool edits two of Dota 2's own files. That may break Valve's terms
> of service. It is not affiliated with Valve. What you pick is only visible on your own computer.

The tool is in development. There is an app with a window, `Dota2ModEnjoyer.exe`, and a
command-line tool, `dme`. Both can show any item the game has in any slot that has a default: sets
and single pieces on heroes, styles, arcanas, personas, and world items such as couriers, wards and
terrain. They can also give an ability the effects of an item the hero does not wear, put an effect
on an item the way a gem does in Dota, use the effect a past event had for a teleport, a Blink
Dagger, attacks and more, and put an event's emblem around your heroes.

The newest parts have been checked against the game's files and not yet seen in the game:
personas without owning them, courier effects on heroes, emblems, event attacks, and the three
experiments (other trees, another look for the map, and a mode for low-end computers).

## Getting it

Download `Dota2ModEnjoyer.exe` from the
[latest release](https://github.com/elangrr/Dota2ModEnjoyer/releases/latest). It is one file
and needs no installing. Windows may warn about a program it has not seen before, because the
file is not signed with a paid certificate.

## Using the app

Start `Dota2ModEnjoyer.exe`. It finds Dota 2 by itself and reads the heroes, items, names and
pictures from your own game.

1. Under **Heroes**, choose a hero, then a whole set, or go slot by slot and mix pieces. Under
   **World** are the courier, ward, terrain, announcers, music and the rest.
2. Press **Play**. Your picks are put in the game and Dota 2 starts.

Your picks are remembered. After a Dota 2 update, press Play as usual and they are put back.

To search, just type. There is no need to click the search box first. Heroes are found by the
short names Dota itself knows them by, such as `am`, `wr`, `qop`. Enter opens the first hero found,
Escape clears what you typed.

**Back to normal Dota** takes every change out of the game's files and puts your picks away, so
nothing is picked any more and Play starts normal Dota. **Bring back my picks** picks everything
again as it was. If you pick something new instead, you start over from there.

The line under a picture has the colour of the item's rarity, as in Dota: purple for mythical,
gold for immortal and so on. The buttons above the pictures show only the rarities you choose.

Under what a hero wears, its page lists the abilities that items have effects for, such as "Blade
Fury effects". Pick an item there and the ability gets that item's effects, icon and sounds,
whatever the hero wears, so a plain sword can have an immortal sword's Blade Fury. Only effects
that Dota itself lets players use apart from their item are listed, which is most immortals' and a
few others'.

Once you have picked a courier, or something a hero wears, the **Effect** list above the pictures
puts one more effect on it, the way a gem does in Dota: Ethereal Flame and 42 others. On a hero,
the eight seasonal ones (Frostbloom, Snowdrift ...) are made for it; the 35 marked "(courier)" are
made for couriers and are an experiment there.

Colours (the Prismatic gems) are gone. In a match, Dota's servers decide what colour a gem has
from what you really own, so a colour never showed there.

A persona you pick is put on the hero's own items, so it shows without your owning it in Dota. It
takes the place of what the hero wears: picks for those slots are taken off, and you can still
pick for the persona's own slots. Personas are an experiment.

Emblems, teleports and other effects are on the World page, under "Effects". They are what events
gave the players who took part.

- Emblem puts the glow of an event's emblem around every hero. It rides on one slot of each
  hero. For about half of the heroes that is a slot nothing else can go in, so they always
  show it; the others show it while that slot is on its default item in your Dota loadout.
- For Teleport, Blink Dagger, Attack, Force Staff, Eul's Scepter, Bottle, level up, the fountain
  and more, pick an event and, where it had several, a level. The effect is for every hero, and a
  hero shows it as long as one of its slots in your Dota loadout is on its default item.

These go on every hero, which means on everyone in a match who plays one. In **Settings** you
can have them only on the heroes you picked something for, or on heroes you choose. Dota's files
cannot tell your own hero from the others, so that is as close as it gets.

A small ! on an item means a part of it may not show. Either Dota no longer has some of its
files, or some of its effects go by the level of an arcana, which Dota's servers give only to its
owner. The lightest video settings leave parts of effects out as well.

Three things are new enough to be marked as experiments. If one misbehaves, turn it off and
everything is as before.

- Trees (World page, at the bottom) puts the leaves of Valve's autumn, winter or spring terrain
  on the trees of the normal map. The trees stay where they are and as large as they are, so
  nothing is made smaller or easier to see through.
- Map Effect (World page, at the bottom) draws the whole map the way one of Valve's events had
  it, such as Diretide's. Aghanim's Labyrinth's is marked "!" because its fog of war can look
  like a checkerboard.
- Low-end PC mode (the checkbox beside Play) makes Play start Dota with its lightest video
  settings: no shadows, simple water and lighting, no fog, grass or other extras. No file is
  written for this. Dota may keep the settings afterwards; you can change them back in Dota's
  video options.

Good to know:

- A pick takes the place of a slot's default item. Where you wear an item of your own, Dota shows
  that instead, so unequip it in your Dota loadout for the pick to show.
- Ability effects come along with the default item of the slot their item belongs to. An immortal
  sword's show while your Dota loadout has the default weapon. The app names the slot.
- Other players who use the default look show your pick too, on your screen only.
- Things the game has no default item for cannot be changed: taunts, kill banners, effigies,
  the Tormentor and the Ancient.
- Ten heroes have one default pet between them. A pet picked for one of them shows on all ten,
  so only one of them can have a pick there. The app says so on the pet's page.
- Nothing is written while Dota 2 is running. Close the game, then press Play. The app can stay
  open while you play.
- Change Dota's video settings before a match, not during one. Changed in the middle of a match,
  Dota has shown picked heroes as "ERROR" until the next game.
- **Settings** (top right) has which heroes get the effects of events, Low-end PC mode, updates,
  and where Dota and your picks are.

## How it works

Dota 2 already ships its cosmetic items in its own files, because your game has to draw other
players' items. Every slot has a built-in "default item" that the game shows when you have put
nothing there: one per slot of each hero, and one each for the courier, the ward, the terrain and
so on. The tool makes a copy of the game's item list in which the default items of the slots you
picked look like the items you picked, puts that copy in its own folder, and tells the game to look
in that folder first.

Nothing of the game's is downloaded, and none of Valve's files are copied anywhere else.
[docs/SPEC.md](docs/SPEC.md) has the details.

## What it changes

| File | Change |
|---|---|
| `game\mod_enjoyer\pak01_dir.vpk` | Created. Holds the edited item list and, if you picked other trees or a map effect, a few small copies of Valve's own material files under other names. |
| `game\dota\gameinfo_branchspecific.gi` | A marked block of lines is added. |
| `game\bin\win64\dota.signatures` | One line is added at the end. |

"Back to normal Dota" (`dme restore`) undoes all three exactly. So does Steam's "Verify integrity
of game files". Copies of the two original files are kept in
`%LOCALAPPDATA%\Dota2ModEnjoyer\backup`, next to your picks and the app's settings.

While your picks are on, Dota keeps a few small files of its own in `game\mod_enjoyer` too: hero
guides it has downloaded and, after a crash, its crash report. Going back to normal removes the
folder with them, and Dota downloads guides again when it needs them. Downloaded replays are
kept where they always are.

The tool refuses to write while Dota 2 is running, and it never touches the running game. To
start the game it asks Steam to.

## Updates

The app can update itself. When a newer version is out, an **Update** button shows at the top.
One click downloads the new version, puts it in the place of the old one and starts it, and the
new version says that the app was updated. Picks and settings stay as they are. The app looks
when it starts and, left open, every half hour. **Settings** has "Update automatically", which is
off by default. Switched on, the app updates and starts again by itself, without the click.
Versions before 0.1.2 do not look for updates, so get 0.1.2 or later by hand once.

## Building

`cargo build --release` builds both programs: `target\release\Dota2ModEnjoyer.exe` and
`target\release\dme.exe`. Each is one file that needs nothing else beside it.
[docs/DEVELOPING.md](docs/DEVELOPING.md) is for working on the code and making releases.

## Using the command-line tool

```
dme status                  where Dota is, whether mods are on, and what is picked
dme heroes                  the heroes, with how many sets and items each has
dme sets axe                the sets of one hero
dme slots axe               the slots of one hero and what it has in each normally
dme items axe weapon        the items that can go in one slot of a hero
dme world                   the slots that are not a hero's: courier, ward, terrain ...
dme items courier           the items that can go in a world slot
dme effects                 the effects that can be put on an item
dme apply <picks>           show the picks in the game, and remember them
dme play                    put the remembered picks in the game if they are not there, start Dota
dme play --low-end          the same, starting Dota with its lightest video settings (an experiment)
dme restore                 back to normal Dota; the picks are put away until dme play
dme restore --forget        the same, and forget the picks
dme effects-for picked      the effects of events and an emblem only on heroes you picked for
dme effects-for axe,lina    ... only on these heroes; `every` for all of them again
```

The listings of sets and items say how rare each one is.

The app and `dme` share the picks: what you pick in one, the other plays.

A pick is written in one of three ways:

```
dme apply axe=armor_of_the_shattered_vanguard     a whole set on a hero
dme apply axe.weapon=5675                         one item in one slot of a hero
dme apply courier=10096                           a world item
dme apply juggernaut.ability_effects_1=9984       an item's effects for one ability, without the item
dme apply "courier=10096,effect=ethereal flame"    an item with an effect on it
dme apply pudge.persona_selector=13786           a persona, put on the hero's own items
dme apply fx_teleport=17482:2                     the effect an event had for a teleport, in its third level
dme apply emblem=12996                            the glow of an event's emblem around every hero
dme apply trees=winter                            winter leaves on the normal map's trees (an experiment)
dme apply map_effect=18380                        the map drawn the way an event had it (an experiment)
```

- Sets and items go by the id or name the listings show, or a part of the name that fits only one.
- `dme slots <hero>` lists a hero's slots for ability effects after the slots things are worn in,
  and `dme items <hero> ability_effects_1` the items that have effects for one.
- Add `:N` for a style: `invoker.head=7986:1` is that item's second style.
- An effect goes by its number or its name, or by a part of the name that fits only one;
  `dme effects` lists them. Put a pick with spaces in it in quotes.
- `dme world` lists the `fx_` slots after the game's own, and `dme items fx_teleport` the events
  that have an effect for one. `:N` after an event is its level, counted from 0.
- `dme items emblem`, `dme items trees` and `dme items map_effect` list what can go there.
- Give as many picks as you like. A later pick for a slot replaces an earlier one, so
  `dme apply axe=armor_of_the_shattered_vanguard axe.weapon=5675` is the set with another weapon.
- `dme apply` replaces whatever was applied before. After a Dota 2 update, run `dme play`.

## License

Copyright (C) 2026 Fluxen

Dota 2 Mod Enjoyer is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either version 3 of
the License, or (at your option) any later version. It is distributed in the hope that it will
be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE. See [LICENSE](LICENSE) for the full text.

Dota 2 is a trademark of Valve Corporation. This project is not affiliated with or endorsed by
Valve.
