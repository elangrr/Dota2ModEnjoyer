# Developing Dota 2 Mod Enjoyer

For people who build the app themselves or work on its code. How to use it is in the
[README](../README.md); how it works, in detail, is in [SPEC.md](SPEC.md).

## Building and checking

The project is Rust on Windows, edition 2024.

```
cargo clippy --workspace --all-targets          has to stay free of warnings
cargo test --workspace
cargo build --release --workspace
target\release\dme.exe dev try-all              builds every pick in memory; 0 FAILED; only reads
```

`cargo build --release` builds both programs: `target\release\Dota2ModEnjoyer.exe` and
`target\release\dme.exe`. Each is one file that needs nothing else beside it.

The release profile leaves the names of the functions in the programs (`strip = false`). With
them taken out the app is 9 MB smaller, and Microsoft's virus scanner took it for malware.

Both files say their name, maker and version to Windows, and the app's file wears the logo
(`assets/icon.ico`). That is compiled in by `windres`, which comes with the MinGW tools. If
`windres` is not in the `PATH`, the build warns and makes the programs without those details.
`assets/logo.svg` is the one drawing of the logo. The app draws its window icon and the mark in
its side bar from the same numbers (`crates/dme-app/src/logo.rs`), and a test compares the two.

The app's font is Inter, in `assets/fonts` with its license (SIL Open Font License 1.1). It has
every letter of English, Russian, Spanish and Portuguese; Chinese characters come from a font of
Windows' own (see "Adding a language"). The colours of its two looks are in `crates/dme-app/src/theme.rs`, and nowhere else: a test there
holds them to a readable contrast.

## The code

The code is three crates: `dme-core` (everything that reads and writes files), `dme-cli` (`dme`)
and `dme-app` (the window, made with `egui`).

`cargo test` runs the tests; they use small made-up files, not Valve's. The `dme dev` commands
check the format descriptions in the spec against a real install and only read. The last two
are for releases of the app (see below) and have nothing to do with the game:

```
dme dev check-vpk               recompute the checksums in the game's archive
dme dev check-signatures        recompute the hash of every file dota.signatures declares
dme dev try-all                 build every item, style, set, effect, gem and emblem, in memory; fails if a pick does
dme dev copied-keys             which keys a default item takes from a picked item; run after a Dota update
dme dev catalog                 count what the app offers, read every picture it would show, list the files that are gone
dme dev effects                 how the game describes ability effects worn apart from their item
dme dev sounds                  write every file of sound events again and compare; make the heroes' sounds for every item
dme dev bodies                  where Play puts the bodies that items give heroes, and the models of their forms
dme dev particles               write every particle file of an earlier version again and compare; what holds each emblem's effect
dme dev values <path>           print the values of a compiled file (a particle file, a file of sound events); --member for one of them
dme dev compare                 list exactly what the installed mod package changes
dme dev item 595                print one item as the game stores it (--from <vpk> for the mod package)
dme dev picture <image> <png>   write one of the game's pictures as a PNG file
dme dev name <text>             look a text up in one of the game's name files
dme dev find <text>             list the files in the game's archive whose path contains a text
dme dev update-key <file>       make a key to sign releases of the app with
dme dev release <exe> --url ... write update.json, the description of a release (--trial: one to try on this computer)
```

The app can click through itself and save a picture of its own window, which stays off screen
meanwhile, so nobody's screen is captured. This opens Axe's page, clicks a tile and saves the
result, keeping its picks in a folder of its own, apart from yours:

```
Dota2ModEnjoyer.exe --data-dir scratch --screenshot out.png --size 1180x760 --open hero:axe --click 808,280
```

To try Play and "Back to normal Dota" without touching the real game, point both programs at a
copy of the game's small files with `--dota <folder> --data-dir <folder>`, and give the app
`--no-start`. [SPEC.md](SPEC.md), section 3.13, lists the options.

Such a window leaves your clipboard and your browser alone. What it would copy, such as a share
code or the details of a problem, goes to `copied.txt` in its data folder, and an address it
would open, such as a report, goes to `opened.txt`. Two settings of the environment try out what
a test window cannot meet by itself. `DME_TRY_NO_GLASS=1` makes the window fail as it is made
see-through, as on a computer that cannot; the app should start again, plain. `DME_TRY_ONE_COPY=1`
has a test window hold its data folder like a player's, so that a second one started meanwhile
closes. The app has no `--help`: an option it does not know is left alone, and without
`--screenshot` the window opens on your screen with your own picks.

## Adding a language

Every text of the window is in `crates/dme-app/src/lang/` ([SPEC.md](SPEC.md), section 3.28).
`en.rs` has each text's key and English words; each other language is one file with the same
keys. To add one:

1. Copy `ru.rs` to `<code>.rs`, as `zh.rs`, `es.rs` or `pt.rs`.
2. At its top, set `code` (what the settings keep), `name` (the language in itself, as the
   chooser shows it), `windows` (the names Windows gives it: `es` stands for every `es-...`,
   `zh-CN` for itself alone), `game` (the end of the game's file names: `schinese`, `spanish`,
   `latam`, `brazilian`), `forms` and `plural` (`one_and_more`, `slavic` or `one_form`), `decimal`
   and `letters` (`Letters::Han` for Chinese characters).
3. Put its code on a line of its own in `languages! { ... }` in `lang/mod.rs`.
4. Translate every text after its `=>`. Keep every `{place}`; a `{count|...|...}` has as many
   forms as `forms` says. Look up the game's own words in its files of that language
   (`dme dev find localization/`, then `dme dev extract`), as for ranks and slots.
5. `cargo test -p dme-app lang` names every text that is missing or there twice, and every place
   that was lost or made up. `cargo clippy --workspace --all-targets` and `cargo test --workspace`
   have to pass.
6. Look at every page in it: give a scratch data folder a `settings.json` with
   `"language":"<code>"` (and `"notice_accepted":true,"tour_seen":true,"seen_version":"<version>"`),
   and take pictures with `--data-dir <that folder> --open <page> --screenshot out.png`, at
   `--size 1280x820` and at the smallest, `--size 760x560`, by night and with `"theme":"day"`.

## The draft helper's numbers

The Draft page works from one file, `draft.json`, and the Builds page from another,
`builds.json`. [SPEC.md](SPEC.md), sections 5 and 6, describes them. This makes both in a
folder, from the statistics of [STRATZ](https://stratz.com):

```
set STRATZ_TOKEN=<your token>
python publish/make_numbers.py numbers
```

A token is free. STRATZ gives one to whoever logs in with Steam at stratz.com/api, and asks
for a link back to it wherever its numbers are shown. The script asks about eight hundred
questions, which takes about twenty minutes, and needs nothing but Python. `python publish/make_numbers.py --check` tries its sums.

What pros buy in each role is one question to [OpenDota](https://www.opendota.com)'s
database of pro matches, which needs no token. Where it cannot be reached, the script says
so and writes `builds.json` without it.

To see the pages with files of your own, without publishing anything:

```
Dota2ModEnjoyer.exe --data-dir scratch --numbers-from numbers/draft.json --open draft
Dota2ModEnjoyer.exe --data-dir scratch --numbers-from numbers/draft.json --open builds:axe:offlane
dme dev builds numbers/builds.json --hero axe
```

A build made while the environment variable `DME_NUMBERS_TRIAL` names such a `draft.json`
fetches both files from there with no option at all. That is handy for a build somebody else
is to try by starting it the usual way. A released build is made without it.

The last one reads the builds against your own game and says what the game has no name, no
picture or no talent for.

The released app's two files are made anew once a week and put on the branch `data` of this
repository, which holds those two files alone.

A build of the app fetches `draft.json` from the address `NUMBERS_URL` in
`crates/dme-core/src/draft.rs`, and `builds.json` from beside it. If you hand out builds of
your own, point that at files you publish yourself.

## Releases of the app

The app updates itself (see Updates in the [README](../README.md)). A build looks at this
project's releases on GitHub and takes only what was signed with this project's key, which
only its maintainer has. If you hand out builds of your own, have them look at your releases:

1. Make a key to sign releases with: `dme dev update-key <file>`. Keep the key file to yourself,
   outside the project folder. The command prints the public half of the key.
2. In `crates/dme-core/src/update.rs`, set `PUBLIC_KEY` to that public half, and `MANIFEST_URL`
   to the `https` address that the description of the latest release will have. With GitHub
   Releases that can be `https://github.com/<you>/<repository>/releases/latest/download/update.json`.
3. Build, and hand that build out. It is the first one that looks for updates.

Then, for every release:

1. Raise `version` in `Cargo.toml`, write what is new in it at the top of
   [CHANGELOG.md](../CHANGELOG.md) in words a player understands, and build. The app shows
   those words once after the update, and a test fails until the new version has them.
2. Describe the release. This writes `update.json`:

   ```
   dme dev release target\release\Dota2ModEnjoyer.exe --url <address> --key release.key --notes "What is new"
   ```

   The address is the one the exe will have. With GitHub Releases it can be
   `https://github.com/<you>/<repository>/releases/latest/download/Dota2ModEnjoyer.exe`.
3. Put the exe at that address and `update.json` at `MANIFEST_URL`. With GitHub Releases, they
   are the two files of a new release.

The app takes a new version only over `https`, only if the file is byte for byte the one
`update.json` describes, and only if the release was signed with your key.
[SPEC.md](SPEC.md), section 4, has the details.

### Trying an update without publishing anything

A build made while the environment variable `DME_UPDATE_TRIAL` names a file looks for updates
in that file, on the computer it was built on, every 10 seconds. Nothing is signed and nothing
leaves the computer. Help says, under the version, that the copy is such a build.

```
set DME_UPDATE_TRIAL=C:\trial\update.json
cargo build --release                           the old version; start it and leave it open
```

Then raise `version` in `Cargo.toml`, build again with the variable set, and put the new
version out for the old one to find:

```
copy target\release\Dota2ModEnjoyer.exe C:\trial\Dota2ModEnjoyer.exe
dme dev release C:\trial\Dota2ModEnjoyer.exe --url C:\trial\Dota2ModEnjoyer.exe --trial --notes "What is new" --out C:\trial\update.json
```

Start the old version from a copy of its file, or build the second time into another folder
(`CARGO_TARGET_DIR`), because Windows does not let a build write over a program that is
running. The Update button shows in the open app within 10 seconds.

## License

The code is under the GNU General Public License, version 3 or any later version
([LICENSE](../LICENSE)). What you contribute is under the same license.
