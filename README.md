# Jumpman

**A Rust and Bevy engine that plays ports of four Nintendo 64 classics, plus a set of original game modes, on macOS, iOS and Android.**
You bring your own legally obtained ROMs; the app extracts what it needs on your own device. This repository contains **no ROM
and no game data**.

| | |
|---|---|
| **Super Mario 64** | Mario's controller and physics ported from the [n64decomp/sm64](https://github.com/n64decomp/sm64) decompilation, with the original sound engine, many levels, enemies and objects |
| **Mario Kart 64** | Karts, CPU opponents, items, Grand Prix / Time Trial / VS / Battle, surfaces, original music and sound effects |
| **Ocarina of Time** | Link, Epona and 27 scenes from [zeldaret/oot](https://github.com/zeldaret/oot), with the ROM's own music, day and night |
| **Banjo-Kazooie** | Banjo, 14 worlds and their sub-areas, enemies, collectibles, swimming, climbing and music ([n64decomp/banjo-kazooie](https://github.com/n64decomp/banjo-kazooie)) |
| **Original modes** | Gem Clash, Sunstone Shindig, Star Deck, Relic Descent, Hold the Gate, A Hundred Marios, Last Star Standing and more, built on the same engine |

It is **not an emulator**: it re-implements the games' logic natively (ported function by function from the decompilations) and
reads only the assets extracted from your ROMs. It is a non-commercial fan project, not affiliated with Nintendo or Rare.
How complete each game is: [DECOMP_TALLY.md](DECOMP_TALLY.md).

## Download

| Platform | Where | Notes |
|---|---|---|
| **Android** (arm64, e.g. Retroid Pocket) | [Releases, "android-latest"](https://github.com/rnaud/jumpman/releases/tag/android-latest) | Rebuilt automatically on every push to `main`. Sideload the `.apk`. Builds are signed with the project key, so each one installs over the previous (uninstall once if you have an older debug-signed build). Setup: [native/ANDROID.md](native/ANDROID.md) |
| **iOS / iPadOS** | TestFlight ("Jumpman Medley") | Invite-only for now. Open an issue to ask for access. Or build it yourself: [native/ios](native/ios), [native/BYOR.md](native/BYOR.md) |
| **macOS** (Apple silicon) | Build from source | `cd native && ./package-mac.sh` or `cargo run --release`. No prebuilt download yet |
| **Windows / Linux** | Not tested | Bevy supports them; nobody has tried it yet |

## First run: bring your own ROMs

The app ships without game data. On first launch it shows a setup screen: choose your ROM files and it extracts the assets into its own
storage (seconds to a couple of minutes; nothing is uploaded anywhere). Supported dumps, any byte order (`.z64`, `.v64`, `.n64`):

- Super Mario 64 (US)
- Mario Kart 64 (US, v1.0)
- The Legend of Zelda: Ocarina of Time (US, v1.2)
- Banjo-Kazooie (US, v1.0)

You can import any subset; each game's modes unlock when its data exists. Importing is done by the pure-Rust importer in
[`native/byor-rs`](native/byor-rs) on every platform. On the desktop you can also run it by hand:

```sh
cd native
cargo run --release -p byor-rs -- --source-root .. --data-dir ~/Library/Application\ Support/Jumpman --sm64 /path/to/sm64.z64
```

(`--mk64`, `--oot` and `--bk` work the same way. Details and the licensing reasoning: [native/BYOR.md](native/BYOR.md),
[native/ANDROID_IMPORT.md](native/ANDROID_IMPORT.md).)

## Build from source

Requirements: Rust stable (rustup); Xcode for iOS; JDK 21, Android SDK/NDK 27 and `cargo-ndk` for Android.

```sh
git clone --recurse-submodules https://github.com/rnaud/jumpman   # the decomp sources are submodules
cd jumpman/native
cargo run --release                  # title screen; reads imported data from your user data folder
cargo run --release -- --list        # list course ids
cargo test --workspace --locked      # simulation goldens and headless gameplay tests
```

Some tests need locally imported data or the ROM environment variables documented in the importer tests, and are skipped otherwise. Packaging
(macOS app, iOS/TestFlight, Android APK) and every debug hook (`JUMPMAN_COURSE`, `JUMPMAN_SCREENSHOT`, `JUMPMAN_TOUCH`, ...) are documented in
the [developer guide](docs/DEVELOPER_GUIDE.md) and [native/README.md](native/README.md).

## Playing

Keyboard (rebindable), gamepad and touch are supported. Defaults: `WASD` move, `Space` jump, `Shift` attack, `Ctrl` crouch / ground pound, `Q`/`E` camera,
`Esc` pause. The title screen is a file-select list; the hub room lets you walk to a door for each game or mode. On touch, only the essential
buttons stay on screen and the rest are in the pause menu. Full controls, every mode and the settings: [developer guide](docs/DEVELOPER_GUIDE.md).

## Repository layout

```
native/            Rust workspace: sim/ (engine-free simulation), src/ (the Bevy app), byor-rs/ (the ROM importer),
                   ios/ and android/ (platform wrappers), byor/ (import packaging and the Python reference importer)
reference/         decompilation sources as git submodules (sm64, mk64, oot, banjo-kazooie)
scripts/           the original Python exporters, now the frozen reference the Rust importer is tested against
assets/ui, assets/audio   the only tracked assets: original UI art, fonts and small data files
docs/              guides, audits and design briefs (some historical)
DECOMP_TALLY.md    what is and is not ported, per game
```

## Status and contributing

Active development; things move daily. The honest accounting of what is missing lives in [DECOMP_TALLY.md](DECOMP_TALLY.md) and
`docs/`. Issues and pull requests are welcome, especially for platforms I cannot test (Windows, Linux, other Android devices).
Please never commit ROMs, extracted assets or other Nintendo/Rare material.

## Credits and legal

- Logic is ported from the decompilation projects [n64decomp/sm64](https://github.com/n64decomp/sm64) (and the sm64-port sound engine),
  [n64decomp/mk64](https://github.com/n64decomp/mk64), [zeldaret/oot](https://github.com/zeldaret/oot) and
  [n64decomp/banjo-kazooie](https://github.com/n64decomp/banjo-kazooie). All credit for those decompilations goes to their contributors.
- [Quaternius' Universal Animation Library](https://quaternius.com/packs/universalanimationlibrary.html) is CC0 1.0.
- Fonts: Lilita One and Nunito (SIL OFL). Gem Clash, Sunstone Shindig and the other non-Nintendo modes, the menu art and the fox hero are original.
- Super Mario 64, Mario Kart 64, Ocarina of Time and Banjo-Kazooie and their characters are trademarks of their owners.
- Jumpman's own code is MIT licensed ([LICENSE](LICENSE)). That license does not cover Nintendo- or Rare-derived game data, which this
  repository does not contain.
