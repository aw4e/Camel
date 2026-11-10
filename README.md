<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo-horizontal-dark.svg">
    <img src="assets/brand/logo-horizontal.svg" height="96" alt="Camel">
  </picture>
</p>

<p align="center"><b>A Mac-style notch bar for Windows.</b></p>

A notch bar for Windows, in the style of the MacBook notch and Dynamic Island. Camel sits at the top center of your
screen as a small black pill, hugging the screen edge with the flared corners of a real notch. Hover it and it
springs open into a compact control center: now playing, a file shelf, clock, per-app volume and a
Spotlight-style launcher.

Written in Rust with [egui](https://github.com/emilk/egui) (glow backend). About 7 MB, roughly 20 MB of RAM when idle.

## Features

| | |
|---|---|
| **Now playing** | Title, artist, album art, seek bar and transport controls for any app that talks to Windows media controls (Spotify, browsers, players). Synced lyrics from [lrclib.net](https://lrclib.net). |
| **Shelf** | Drag files onto the notch to park them, click to open, right-click to remove. |
| **Clock** | Time, date and the current week. |
| **Mixer** | Master volume plus a slider for each app that is playing audio. |
| **Spotlight** | `Alt+Space` opens a launcher: calculator, Start Menu apps and files from Desktop, Documents and Downloads. |
| **Toasts** | A short notice when you copy something, plug in or unplug the charger, or the battery is low. |
| **Privacy dots** | Green and orange dots while a microphone or camera is in use. |
| **Tray icon** | Settings and Quit. |

Camel hides itself while a fullscreen game or presentation is running.

## Controls

- **Hover** the pill to open it. Move away to close it.
- **Click** the tab icons, or **scroll** over the tab row, to switch pages.
- **Scroll sideways** on the media page to skip tracks. **Middle-click** toggles play and pause.
- **Gear icon** opens settings. Quit lives there and in the tray menu.

Settings (all on by default except autostart): hover to expand, always on top, hide on fullscreen, synced lyrics,
clipboard toast, privacy indicators, Spotlight, start with Windows. They are saved to
`%APPDATA%\Camel\settings.json`.

## Build

Requires Windows 10 or 11 and a recent Rust toolchain (edition 2024).

```powershell
cargo run --release
```

The release build is `target\release\camel.exe`. It is a single file; copy it anywhere and run it.

### Fonts

Camel uses the embedded Open Sans (OFL) by default. If a `fonts` folder with `SF-Pro-Display-Regular.otf` and
`SF-Pro-Display-Semibold.otf` sits next to the exe (or in the project root while developing), it uses those
instead. SF Pro is Apple's font and is not included in this repository, so `fonts/` is git-ignored. Obtain it from
Apple under its license if you want that look.

## Brand

Logo, app icon and favicon live in `assets/brand/`. See [assets/brand/BRAND.md](assets/brand/BRAND.md) for colors, sizes and usage.

## Project layout

```
src/main.rs       window, animation, every screen
src/ui.rs         icons, sliders, switches, notch shape
src/media.rs      Windows media session worker and lyrics
src/mixer.rs      WASAPI master and per-app volume
src/monitor.rs    battery, CPU/RAM, mic/camera, clipboard, memory trim
src/spotlight.rs  launcher index, search and calculator
src/settings.rs   settings file and autostart
```

## Privacy

Camel makes one kind of network request: with synced lyrics on, it sends the current artist, track title and
duration to lrclib.net to fetch lyrics. Turn **Synced lyrics** off and it makes none. Settings stay on your machine.

## Notes

- Windows only. It relies on Windows media controls, WASAPI, the registry and DWM.
- Spotlight indexes up to 60,000 files, and the index lives in memory only.
- Mic/camera dots read the Windows privacy consent store, so they follow what Windows itself reports.
