# vholumin

**English** · [Português](README.pt-BR.md)

Toggle your game's volume in the Windows Volume Mixer with a hotkey.

Press **`Ctrl+Alt+M`** and the game's volume drops to the minimum. Press it again and it
goes back to the maximum. Only the game changes: Discord, music and your browser stay where
they are.

vholumin lives in the system tray, uses no CPU while it waits, and never touches the game.
It changes the volume the same way the Windows Volume Mixer does.

> The interface (tray menu, settings window and installer) is in Brazilian Portuguese.

## How it works

- **The rule:** if the game is above the minimum volume, the hotkey takes it to the
  minimum. If it is already at the minimum, the hotkey takes it to the maximum. The
  defaults are **5%** and **100%**.
- **Always the real volume:** vholumin reads the current volume from Windows on every
  press, so changing it by hand in the Mixer doesn't confuse it. For example: game at 60%
  → hotkey → 5% → hotkey → 100%.
- **Which program it controls**, in two modes:
  - **Focused app** (*App em foco*, the default): the program in front when you press the
    hotkey, along with processes that have the same name. Browsers and Discord, for
    example, play sound from a "sibling" process.
  - **Game list** (*Lista de jogos*): the `.exe` files you add, even when the game is in
    the background. For example, you're in Discord with the game open behind it. If more
    than one game from the list is running, they all toggle together.
- It works on every audio output at once (headphones, speakers, HDMI…).

## Tray icon

The icon is the only visual feedback: no pop-up window, no sound.

| Icon | When |
|---|---|
| Blue, two waves | Game at maximum (and on startup) |
| Gray, one wave | Game at minimum |
| Red, with an X | Flashes briefly when the hotkey found no audio from the target |
| Gray, crossed out | Hotkey paused, settings window open, or hotkey already taken by another program |

Hover over the icon for details, for example `vholumin — cs2.exe em 5%`.

- **Left click:** opens the settings.
- **Right click:** menu with Settings, Mode, Pause hotkey, Start with Windows, Reload
  config.toml, Open vholumin folder and Exit.

A game only shows up in the Mixer after it plays some sound. Until then, the hotkey shows
the red icon.

## Installation

Requires **Windows 10 or 11, 64-bit**.

1. Download `vholumin-setup-<version>.exe` from the
   [latest release](https://github.com/matheusfaoro/vholumin/releases/latest) and run it.
2. It installs for your user only, **without asking for administrator rights**, to
   `%LOCALAPPDATA%\Programs\vholumin`, with a Start menu shortcut.
3. Leave **"Iniciar o vholumin junto com o Windows"** (start vholumin with Windows) checked
   if you want it to open automatically.

The installer isn't digitally signed, so Windows may show "Windows protected your PC". Click
**More info → Run anyway**.

**Updating:** run the new version's installer over the old one. It closes vholumin by
itself and keeps your settings.

**Uninstalling:** Windows Settings → Apps → vholumin → Uninstall. It asks whether you also
want to delete your settings and log.

## Settings

Click the tray icon to open the settings window:

- **Hotkey:** click the field and press the combination. It turns green if it's free and
  red if another program already uses it.
- **Minimum and maximum volume:** two sliders. The minimum always stays below the maximum.
- **Mode:** "App em foco" (focused app) or "Lista de jogos" (game list). In list mode you
  can add whatever is playing right now, browse for an `.exe`, or remove a game.
- **Start with Windows.**

While the window is open, the hotkey is paused.

Settings are stored in `%APPDATA%\vholumin\config.toml`. You don't need to edit it by hand,
but the file is simple (the keys are in Portuguese):

```toml
atalho = "Ctrl+Alt+M"    # hotkey
volume_minimo = 5        # minimum volume (%)
volume_maximo = 100      # maximum volume (%)
modo = "foco"            # "foco" (focused app) or "lista" (game list)
jogos = ["VALORANT-Win64-Shipping.exe", "cs2.exe"]   # game list
iniciar_com_windows = true                           # start with Windows
```

A wrong value in the file never stops vholumin from starting. The broken field falls back
to its default, and the problem shows up at the top of the tray menu and in the log. **F12**
isn't accepted as a hotkey, because Windows reserves it and it never fires.

## Anti-cheat

vholumin is designed to stay out of anti-cheat's way. It **never**:

- opens the game's process (not even to read its name);
- reads or writes another program's memory, injects DLLs, or installs keyboard hooks;
- simulates keystrokes or clicks;
- installs drivers, services or overlays, or asks for administrator rights.

It only uses the Windows hotkey registration, the focused window, the process list and the
Windows audio API, which is the same one the Volume Mixer uses.

Even so, no one can guarantee 100% for every anti-cheat. The safety comes from the
approach: vholumin does the same kind of thing the Windows Volume Mixer does.

## Log and diagnostics

- **Log:** `%APPDATA%\vholumin\log.txt`, capped at about 1 MB. It records what each hotkey
  press found. The tray menu's **Open vholumin folder** item takes you straight to it.
- **Commands** (run from a terminal):
  - `vholumin.exe --listar`: lists every audio session, as the Mixer sees them.
  - `vholumin.exe --alternar <PID>`: does what the hotkey would do if process `<PID>` were
    focused.

## Building from source

Requirements:

- [Rust](https://rustup.rs) 1.85 or newer, with the MSVC toolchain
  (`stable-x86_64-pc-windows-msvc`) and the Visual Studio Build Tools with the Windows SDK.
- [Inno Setup](https://jrsoftware.org/isinfo.php) 6.7 or newer, only to build the installer.

```bat
cargo build --release
ISCC installer\vholumin.iss
```

- The program ends up in `target\release\vholumin.exe`.
- The installer ends up in `target\instalador\vholumin-setup-<version>.exe`.
- The version lives only in `Cargo.toml`. The `.exe` and the installer take it from there.

Other commands:

- `cargo test`: runs the automated tests.
- `cargo test exportar_imagens_do_app -- --ignored`: regenerates the app icon and the
  installer images in `res/`.

To test a development build without touching the installed copy's settings and log, set
the `VHOLUMIN_DATA_DIR` environment variable to another folder before running it.

### Layout

```
src/          program code (tray, audio, hotkey, settings, window)
res/          manifest and icon embedded in the .exe, installer images
installer/    installer script (Inno Setup)
build.rs      embeds the manifest, icon and version into the .exe
```

## License

Distributed under the [MIT License](LICENSE). © 2026 Matheus Faoro
