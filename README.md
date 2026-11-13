<p align="center"><img src="assets/logo.png" width="112" alt="The logo of Dota 2 Mod Enjoyer"></p>

<h1 align="center">Dota 2 Mod Enjoyer</h1>

<p align="center">
  <a href="https://github.com/elangrr/Dota2ModEnjoyer/releases/latest/download/Dota2ModEnjoyer.exe"><img src="assets/buttons/download.svg" alt="Download, for Windows 10 and 11, 64-bit (x64)" width="340"></a>
</p>

<p align="center">
  <!-- virustotal --><a href="https://www.virustotal.com/gui/file/29081e9b765b43bb631bda9018d4adaf7fa006e70beb6b76d788bdb020237a4d"><img src="https://img.shields.io/badge/VirusTotal-3%20of%2071%20scanners%20flag%20it-dfb317?style=for-the-badge&logo=virustotal&logoColor=white" alt="VirusTotal: 3 of 71 scanners flag it" height="40"></a><!-- /virustotal -->
  &nbsp;
  <a href="#support-us"><img src="assets/buttons/donate.svg" alt="Support us" height="40"></a>
</p>

<p align="center"><img src="assets/demo.webp" alt="The app being used: finding a hero, picking a set and a courier, and the list of picks" width="820"></p>

> [!CAUTION]
> Use at your own risk. This app edits two of Dota 2's own files, which may break Valve's terms
> of service. It is not made or approved by Valve.

## How to use

1. Download `Dota2ModEnjoyer.exe` and start it. It is one file, with nothing to install.
2. Close Dota 2 if it is open. The app finds the game by itself.
3. Choose a hero and click what it should wear.
4. Press **Play**. Your picks are put in the game and Dota starts.

The first time, Windows may say "Windows protected your PC", because the file is not signed with
a paid certificate. Click **More info**, then **Run anyway**.

Only you see your picks, and you do not need to own the items.

## Screenshots

| | |
|---|---|
| ![The hero grid](assets/screens/heroes.jpg) | ![A hero's page with a set picked](assets/screens/hero.jpg) |
| Every hero, with search and filters | A whole set in one click, or piece by piece |

![A hero's page by day](assets/screens/day.jpg)

## What you can change

- Every hero's items, as whole sets or piece by piece, with their styles.
- Ability effects, such as an immortal's Blade Fury on a plain sword.
- Personas, without owning them (an experiment).
- The courier, ward, terrain, weather, music, announcer, HUD, loading screen and more.
- Effects of past events, such as teleports, Blink Dagger and emblems. They go on the heroes you
  picked something for, and Settings can make that every hero.
- Other trees, an event's look for the whole map, and a mode for low-end PCs (experiments).

Just type to search, for heroes, items and sets. **My picks** lists everything you picked.
There you can save your picks as a preset, and **Share** copies a short code that a friend
pastes under **Use a code** to get the same look.

## Updates

The app updates itself. When a new version is out, an **Update** button shows on the left. One
click downloads it and starts it again, and your picks stay. [CHANGELOG.md](CHANGELOG.md) lists
what each version changed.

After a Dota update, start Dota with **Play** in the app. Steam updates Dota by itself, and that
undoes half of the app's changes until Play puts them back.

## Troubleshooting

Most problems are fixed by checking the game's files in Steam. Right-click Dota 2, then
**Properties**, **Installed Files**, **Verify integrity of game files**. Then press Play in the
app again.

| Problem | What to do |
|---|---|
| A pick does not show | Unequip your own item in that slot in your Dota loadout. |
| An item has a small "!" | Dota no longer has some of its files, so a part of it may not show. |
| Dota says Valve Anti-Cheat could not verify your machine | Close Dota, press **Back to normal Dota**, and look for a match again. |
| Picked heroes show as "ERROR" | Do not change video settings during a match. It is fine again next game. |
| Nothing changes in Dota | Dota was running when you pressed Play. Close it and press Play again. |
| Something else | Press **Report** beside the message, or **Report a problem** in Help. |

**Report** opens a new report on this GitHub page with the details filled in, and the name of
your Windows account left out. It needs a free GitHub account. Without one, **Copy details**
copies the same text.

## Questions

**Does it collect any data?** No. Its one request to the internet asks GitHub whether a new
version is out.

**Can I get banned?** Nobody can promise that you cannot. The app never touches the running game
and gives no advantage in a match.

**How do I undo everything?** Press **Back to normal Dota**. It puts the game's files back
exactly as they were.

**What does it change?** It adds a folder, `game\mod_enjoyer`, and a few lines to
`game\dota\gameinfo_branchspecific.gi` and `game\bin\win64\dota.signatures`. Copies of the
originals are kept in `%LOCALAPPDATA%\Dota2ModEnjoyer\backup`.

## Command line

`dme.exe` does the same from a terminal. `dme status`, `dme apply axe.weapon=5675`, `dme play`
and `dme restore` are the main commands, and `dme --help` lists the rest.

## Building

You need Rust. Run `cargo build --release`. [docs/DEVELOPING.md](docs/DEVELOPING.md) has the
rest, and [docs/SPEC.md](docs/SPEC.md) explains how it works.

## Virus scans

Every release is scanned on [VirusTotal](https://www.virustotal.com) before it goes out. Each
link opens the full result for that file. A release that Microsoft's scanner flags is not put out.

| Version | The app | The command-line tool |
|---|---|---|
| 0.2.3 | [3 of 71 scanners flag it](https://www.virustotal.com/gui/file/29081e9b765b43bb631bda9018d4adaf7fa006e70beb6b76d788bdb020237a4d) | [1 of 71 scanners flag it](https://www.virustotal.com/gui/file/7702421d7dd1590ba45f96078a82f42a0f1c6f1148d93eafe47db78686ad12c0) |
| 0.2.2 | [0 of 66 scanners flag it](https://www.virustotal.com/gui/file/4bcc6789a106aad87dae596f72e6c4e83ebb725feaa6477076e9bd48c0513ed4) | [1 of 71 scanners flag it](https://www.virustotal.com/gui/file/c0a1879c6fb6d66e477678ab6fc1a535a2e9e046263088e59266be36290108c7) |
| 0.2.1 | [1 of 71 scanners flag it](https://www.virustotal.com/gui/file/49f0b1372cf47ea129d2c8eb793f7e4cdc34a390af9954e91d82f4cd00232cde) | [1 of 71 scanners flag it](https://www.virustotal.com/gui/file/09fda3379fdef4d66bdf03399b351a86160124a48a672bf62cd4a5176dadfea6) |
| 0.2.0 | [3 of 71 scanners flag it](https://www.virustotal.com/gui/file/ee7fac6d829cc825357d50f0246eed970065cca191ba637a2d976f251b45e6ed) | [1 of 71 scanners flag it](https://www.virustotal.com/gui/file/d769c42b799c6b232b7bd9bc2cc566a3c3ac53bb100817071e4714755390cae8) |

## Support us

The app is free and its code is open. Donations are welcome and never asked for.

| With | Where |
|---|---|
| PayPal | [paypal.me/elangrr](https://paypal.me/elangrr) |
| Solana (SOL), on the Solana network | `122Y7kRw7ELd4SpVG9LPCfVMtQgHiWDiSKakQom966RG` |
| Ethereum (ETH), on the Ethereum network | `0xccf73af707f8bd2ee81dec77467b473705ef901e` |

Send only the currency named, on the network named. Anything else sent to an address is lost.

## License

Copyright (C) 2026 Fluxen

Dota 2 Mod Enjoyer is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either version 3 of
the License, or (at your option) any later version. It is distributed in the hope that it will
be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
FITNESS FOR A PARTICULAR PURPOSE. See [LICENSE](LICENSE) for the full text.

The app's font is Inter, under the SIL Open Font License
([assets/fonts/Inter-LICENSE.txt](assets/fonts/Inter-LICENSE.txt)).

Dota 2 is a trademark of Valve Corporation. This project is not affiliated with or endorsed by
Valve.
