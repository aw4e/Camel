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

| Feature | What it does |
|---|---|
| **Now playing** | Title, artist, album art, seek bar and transport controls for any app that uses Windows media controls (Spotify, browsers, players), with synced lyrics from [lrclib.net](https://lrclib.net). |
| **Shelf** | Drag files onto the notch to park them, click to open, right-click to remove. |
| **Clipboard history** | The last eight things you copied, text and pictures. Click to copy again, right-click to remove, hover to pin. Kept in memory only. |
| **Clock** | Time, date and the whole month, with today marked. The notch grows by 14 px in months that need six rows. |
| **Focus timer** | A countdown with a progress ring and a length you set yourself, from 1 second to 3 hours. The time left shows in the closed pill. |
| **Mixer** | Master volume plus a slider for each app that is playing audio. |
| **Spotlight** | `Alt+Space` opens a launcher: calculator, unit conversion (`5 km in miles`, `100 f to c`), Start Menu apps, files from Desktop, Documents and Downloads, system actions and a last row that opens a link or searches the web (DuckDuckGo). See below. |
| **Volume HUD** | When the master volume or mute changes (volume keys, another app), a bar with the level shows in the notch for a moment. It stays quiet while the Mixer page is open. |
| **Keep awake** | Stops the screen and sleep from turning off. Switch it on from the tray menu or by typing `awake` in Spotlight. A small sun shows in the closed pill while it is on. Off again every time Camel starts. |
| **Toasts** | A short notice when you copy something, plug in or unplug the charger, or the battery is low. |
| **Privacy dots** | Green and orange dots while a microphone or camera is in use. |
| **Apps** | Installed programs with a search box: uninstall them, clean up leftovers, and switch startup items on or off. |
| **Updates** | Checks GitHub for a newer release and installs it when you ask. |
| **Tray icon** | Keep awake, Settings and Quit. |

Camel hides itself while a fullscreen game or presentation is running.

### Details

**Lyrics.** Names that YouTube reports (`Artist - Song (Official Video)`, `Artist - Topic`, `ArtistVEVO`) are cleaned and
searched several ways, and a result is only used when its title and length match. Players that report no timeline (some
browser tabs) show no lyrics, because there is no position to follow.

**Clipboard history.** Hover a row to pin it (up to four): a pinned item stays on top and survives new copies and Clear.
A picture shows a thumbnail, its size and how much memory it takes. Pictures are kept as compressed PNG, 24 MB at most in
total, and the oldest go first. Nothing is saved to disk, and anything a password manager marks as sensitive is skipped.

**Focus timer.** Click the time and type `30` or `25:30`, use the − and + buttons (5 minutes, or 1 with Shift), scroll over
the timer (1 minute per notch), or pick 15, 25 or 45. It is locked while it runs, and you get a beep and a notice when the
session ends.

**Apps.** Programs are read from the registry. Hover a row to uninstall it (its own uninstaller runs, with the usual admin
prompt) or open its install folder. When the uninstaller finishes, Camel looks for leftover folders and offers to move them
to the Recycle Bin. The **Startup** view lists what Task Manager lists (Run keys, 32-bit Run keys and both Startup folders)
and lets you switch each item off and on. Machine-wide items need Camel to run as administrator and show "Admin" instead of
a switch when it does not.

**Updates.** Camel checks GitHub shortly after it starts and then once a day, and tells you when a newer release exists. It
never installs on its own: press **Update** in Settings (next to Quit) or type `update` in Spotlight. The download must match
the SHA-256 published with the release, the old copy is kept aside until the new one has started, and Camel restarts into
it. The version number sits next to Quit; click it to check now. Turn **Check for updates** off in Settings and Camel makes
no update request unless you ask.


## Spotlight commands

Type a few letters and press Enter. Actions that can lose work show a red row and need a second Enter.

| Type | Does |
|---|---|
| `lock`, `sleep` | Locks the PC, or puts it to sleep. |
| `restart`, `shutdown`, `sign out` | Restarts, shuts down or signs out, after a second Enter. |
| `awake`, `caffeine` | Turns keep awake on or off. |
| `timer 10m`, `focus 1h30m`, `timer 25` | Starts the focus timer at that length. Accepts `10m`, `90s`, `1h30m`, `25`, `25:30`. |
| `update`, `version` | Checks for a newer version, or installs the one that was found. |
| `mute`, `mic` | Mutes or unmutes the speakers (the volume HUD shows it) or the default microphone. |
| `note buy milk` | Saves a dated line to `%APPDATA%\Camel\notes.txt`. `notes` opens that file. |
| `kill chrome` | Ends every running program whose name starts with it, after a second Enter. System programs (Explorer, `svchost`, …) and Camel itself are never offered. |
| `5 km in miles`, `100 f to c`, `2 kg to lb` | Converts length, weight, volume, time, speed and temperature. Enter copies the result. |

## Controls

- **Hover** the pill to open it. Move away to close it.
- **Click** the tab icons, or **scroll** over the tab row, to switch pages.
- **Scroll sideways** on the media page to skip tracks. **Middle-click** toggles play and pause.
- **Gear icon** opens settings. Quit lives there and in the tray menu.

Settings (all on by default except autostart): hover to expand, always on top, hide on fullscreen, check for updates,
synced lyrics, clipboard history, privacy indicators, volume HUD, Spotlight, start with Windows. They are saved to
`%APPDATA%\Camel\settings.json`.

## Install

Download `camel.exe` from the [latest release](https://github.com/aw4e/Camel/releases/latest), put it anywhere and run
it. The file is not code-signed, so Windows SmartScreen may warn on the first run; check the SHA-256 in
`camel.exe.sha256` against the file if you want to be sure it is the one that was released.

## Build

Requires Windows 10 or 11 and a recent Rust toolchain (edition 2024).

```powershell
cargo run --release
```

The release build is `target\release\camel.exe`. It is a single file; copy it anywhere and run it.

### Administrator rights

The release build asks for administrator rights when it starts: it reopens itself through the UAC prompt and the
unelevated copy exits. That is what lets the Apps tab move leftover folders out of `Program Files` and switch off
machine-wide startup entries. `cargo run --release` works the same way. If you decline the prompt, Camel keeps running
without admin rights, and the actions that need them say so. Start it with `--no-elevate` to skip the prompt.
Debug builds (`cargo run`) never ask.

Running elevated has side effects:

- **Start with Windows** registers a scheduled task with the highest run level instead of a `Run` entry, because Windows
  silently skips elevated programs listed under `Run`. Switch the setting off and on once after updating, so the task is created.
- Windows blocks drag and drop from a normal Explorer window into an elevated window, so dragging files onto the shelf
  does not work in the release build.
- Programs and files you open from Spotlight, the shelf or the Apps tab start with administrator rights too.

### Fonts

Camel uses the embedded Open Sans (OFL) by default. If a `fonts` folder with `SF-Pro-Display-Regular.otf` and
`SF-Pro-Display-Semibold.otf` sits next to the exe (or in the project root while developing), it uses those
instead. SF Pro is Apple's font and is not included in this repository, so `fonts/` is git-ignored. Obtain it from
Apple under its license if you want that look.

## Design rules

Layout numbers are not picked by eye. They are named constants at the top of `src/layout.rs`, and its
tests fail if a screen drifts from them:

- **Margins:** every screen has a 16 px left and right margin, on a 4 px grid. List rows start at the margin and inset
  their text by 8.
- **Vertical rhythm:** content starts 5 px under the tab bar (y = 35). Every screen keeps at least 5 px free above the
  bottom edge, including a six-row calendar and the apps list with its leftovers banner.
- **Spacing scale:** gaps come from the Fibonacci numbers 3, 5, 8, 13, 21, 34, whose neighbours keep the golden ratio, so
  nested paddings compose without looking arbitrary. Small gaps are 5, standard 8, large 13.
- **Golden ratio:** the content width (428 px) is cut 1.618 : 1 with a 13 px gutter. The Clock puts the time and date in
  the wide column (256 px) and the month in the narrow one (159 px). The Timer puts its controls in the wide column
  and the progress ring in the narrow one. A tab pill is as wide as it is tall times 1.618.
- **Icons in buttons:** icons are drawn at 16 px and scaled to fit, leaving one Fibonacci step (5 px) above and below, never
  smaller than half size. A 24 px tab pill gets a 14 px icon.
- **Grouped buttons:** related buttons share one rounded track with segments, like the tab bar. The timer's presets are
  one group and its − and + another, with Reset standing alone. The tab bar itself is grouped in pairs (audio, time,
  storage, apps).
- **Today in the calendar:** the disc behind today is wider than the digits (17 px for two digits) and still clears the
  rows above and below.
- **Icons:** one drawing language, a 1.5 px stroke with round caps and joins, and rounded corners.

The Settings screen is the densest, so its two groups are the one place where rows are tighter (21 px).

## Brand

Logo, app icon and favicon live in `assets/brand/`. See [assets/brand/BRAND.md](assets/brand/BRAND.md) for colors, sizes and usage.

## Project layout

```
src/main.rs         entry point
src/notch.rs        app state, input, animation, window sizing
src/notch/draw.rs   closed pill, toast and the tab bar
src/notch/tabs/     one file per page: media, mixer, clock, timer, shelf, clipboard, apps, settings
src/notch/spotlight_view.rs  Spotlight panel
src/layout.rs       golden-ratio and Fibonacci layout tokens, with tests
src/platform.rs     tray, hotkey, fonts, window frame, elevation
src/widgets.rs      small drawing and input helpers
src/ui.rs           icons, sliders, switches, notch shape
src/media.rs        Windows media session worker
src/lyrics.rs       lrclib lookup, name cleaning, LRC parsing
src/mixer.rs        WASAPI master and per-app volume, mute
src/monitor.rs      battery, CPU/RAM, mic/camera, clipboard, memory trim
src/spotlight.rs    launcher index, search and calculator
src/settings.rs     settings file and autostart
src/apps.rs         installed programs, uninstall, leftovers, startup entries
src/actions.rs      Spotlight actions: power, timers, quitting programs, unit conversion
src/clipboard.rs    clipboard history and pins
src/notes.rs        quick notes
src/update.rs       release check, verified download, in-place swap
src/focus.rs        focus timer
```

## Privacy

Camel makes two kinds of network request. With synced lyrics on, it sends the current artist, track title and
duration to lrclib.net to fetch lyrics. With **Check for updates** on, it asks api.github.com for the latest release of
this repository, which reveals your IP address and nothing else of yours. Turn both off and it makes none. Settings
stay on your machine.
The web row in Spotlight only opens your browser when you pick it.

The Apps tab reads the Windows registry and file system on your machine and sends nothing anywhere. Leftover
cleanup only offers folders named exactly like the program (or its install folder) under the usual data locations,
never registry keys, and only moves them to the Recycle Bin when you click **Clean up**. Camel remembers a finished
uninstall in `%APPDATA%\Camel\uninstall.json` until you clean up or dismiss it, so closing Camel does not lose the
list and a failed clean up can be retried. Files that Windows will not release (a shell extension Explorer still has
loaded, for example) are queued for deletion at the next restart.

Clipboard history lives in memory and disappears when Camel quits. Turn **Clipboard history** off in settings and
Camel stops reading the clipboard altogether.

## License

Camel is released under the [MIT License](LICENSE): use it, change it, share it, even sell it, as long as the
copyright notice stays. Contributions are welcome through issues and pull requests, and they are accepted under the
same license.

## Credits

- Font: Open Sans, embedded in the exe, under the SIL Open Font License ([assets/OFL.txt](assets/OFL.txt)).
- The Rust crates Camel is built from keep their own licenses (MIT, Apache-2.0 and similar).

## Notes

- Windows only. It relies on Windows media controls, WASAPI, the registry and DWM.
- Spotlight indexes up to 60,000 files, and the index lives in memory only.
- The Apps tab covers classic desktop programs. Microsoft Store apps, Windows updates and hidden system components are
  not listed, and switching off a machine-wide startup entry needs Camel to run as administrator.
- Mic/camera dots read the Windows privacy consent store, so they follow what Windows itself reports.
