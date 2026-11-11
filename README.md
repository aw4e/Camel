<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo-horizontal-dark.svg">
    <img src="assets/brand/logo-horizontal.svg" height="96" alt="Camel">
  </picture>
</p>

<p align="center"><b>A Mac-style notch bar for Windows.</b></p>

<p align="center">
  <a href="https://github.com/aw4e/Camel/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/aw4e/Camel?color=0a84ff"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/github/license/aw4e/Camel?color=30d158"></a>
  <img alt="Windows 10 and 11" src="https://img.shields.io/badge/Windows-10%20%7C%2011-555">
  <img alt="Rust, edition 2024" src="https://img.shields.io/badge/Rust-2024-ff9f0a">
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#features">Features</a> ·
  <a href="#spotlight-commands">Spotlight</a> ·
  <a href="#privacy-and-security">Privacy</a> ·
  <a href="#build-from-source">Build</a> ·
  <a href="#contributing">Contributing</a>
</p>

Camel sits at the top center of your screen as a small black pill, hugging the edge with the flared corners of a real
notch, like the MacBook notch and Dynamic Island. Hover it and it springs open into a compact control center: now playing
with synced lyrics, per-app volume, clock, focus timer, file shelf, clipboard history, an app manager and a
Spotlight-style launcher.

- **Small and quiet.** One executable of about 7 MB, written in Rust with [egui](https://github.com/emilk/egui) (glow
  backend). It trims its own memory when idle and hides while a fullscreen game or presentation is running.
- **Private by default.** Clipboard history stays in memory. The only network requests are lyrics and update checks, and
  both can be switched off. See [Privacy and security](#privacy-and-security).
- **Open source.** MIT licensed, with tests for the logic and for the layout rules.

## Install

1. Download `camel.exe` from the [latest release](https://github.com/aw4e/Camel/releases/latest) and put it anywhere.
2. Run it. Windows asks for administrator rights once (see [Administrator rights](#administrator-rights)).

Windows 10 or 11 is required. The executable is not code-signed, so SmartScreen may warn on the first run. To check that
the file is the one that was released, compare its hash with the `camel.exe.sha256` file from the same release:

```powershell
(Get-FileHash .\camel.exe -Algorithm SHA256).Hash.ToLower()
```

Camel updates itself when you ask it to (see [Updates](#details)). To uninstall, quit Camel from the tray menu and delete
`camel.exe` and the `%APPDATA%\Camel` folder. If you turned on **Start with Windows**, switch it off first, or remove the
task with `schtasks /Delete /TN Camel /F`.

## Using Camel

- **Hover** the pill to open it. Move away to close it. With **Hover to expand** off, click it instead.
- **Click** the tab icons, or **scroll** over the tab row, to switch pages.
- **Scroll sideways** on the media page to skip tracks. **Middle-click** toggles play and pause.
- **Alt+Space** opens Spotlight.
- **Gear icon** opens settings. Quit lives there and in the tray menu.

Settings are saved to `%APPDATA%\Camel\settings.json`. Everything is on by default except **Start with Windows**:

| Setting | Effect |
|---|---|
| Start with Windows | Starts Camel when you sign in. |
| Hover to expand | Opens the notch on hover instead of on click. |
| Always on top | Keeps the notch above other windows. |
| Hide on fullscreen | Hides the notch while a fullscreen app is running. |
| Check for updates | Looks for a newer release shortly after start and then daily. |
| Synced lyrics | Fetches lyrics for the current track. |
| Clipboard history | Records what you copy. Off means Camel never reads the clipboard. |
| Privacy indicators | Shows dots while a microphone or camera is in use. |
| Volume HUD | Shows a bar when the master volume or mute changes. |
| Caps Lock and language | Shows a notice when Caps Lock or the keyboard language changes. |
| Network and audio alerts | Shows a notice when the internet drops or returns and when the default audio output changes (headphones, Bluetooth speakers). |
| Spotlight (Alt+Space) | Enables the launcher and its hotkey. |

## Features

| Feature | What it does |
|---|---|
| **Now playing** | Title, artist, album art, seek bar and transport controls for any app that uses Windows media controls (Spotify, browsers, players), with synced lyrics from [lrclib.net](https://lrclib.net) in a scrolling, Spotify-style view. |
| **Shelf** | Drag files onto the notch to park them, click to open, right-click to remove. |
| **Clipboard history** | The last eight things you copied, text and pictures. Click to copy again, right-click to remove, hover to pin. Kept in memory only. |
| **Clock** | Time, date and the whole month, with today marked. It follows the Windows 12 or 24-hour setting. The notch grows by 14 px in months that need six rows. |
| **Focus timer** | A countdown with a progress ring and a length you set yourself, from 1 second to 3 hours. The time left shows in the closed pill, with a ring beside it. Optional rounds run focus and breaks on their own. |
| **Mixer** | Master volume plus a slider for each app that is playing audio. |
| **Spotlight** | A launcher for apps, files, a calculator, unit conversion, system actions and the web. See [Spotlight commands](#spotlight-commands). |
| **Volume HUD** | A bar with the level shows in the notch when the master volume or mute changes. It stays quiet while the Mixer page is open. |
| **Keep awake** | Stops the screen and sleep from turning off. Switch it on from the tray menu or by typing `awake` in Spotlight. A small sun shows in the closed pill while it is on. Off again every time Camel starts. |
| **Toasts** | A short notice when you copy something, plug in or unplug the charger, or the battery is low. |
| **Privacy dots** | Green and orange dots while a microphone or camera is in use. In the open notch they replace the battery or CPU readout while they show. |
| **Apps** | Installed programs with a search box: uninstall them, clean up leftovers, and switch startup items on or off. |
| **Updates** | Checks GitHub for a newer release and installs it when you ask. |
| **Tasks** | A page with a field to add a reminder or to-do and the list of what is pending; click the circle to tick one off. Also from Spotlight with `remind` and `todo`. A toast and a beep when one is due. |
| **Alerts** | Notices for Caps Lock, keyboard language, losing or regaining internet and a new audio output. |
| **Minimal notch** | A tray option that shrinks the closed notch to a thin line, for screens with thick top bezels. |
| **Tray icon** | Keep awake, Minimal notch, Settings and Quit. |

### Details

**Lyrics.** When lyrics are found, a lyrics button appears at the right of the media page. It swaps the player for a
scrolling view: the current line is large and white, the lines around it fade out, and clicking a line jumps there. If the lyrics run early or late (a music video with an intro, for example), **Shift+click** the line you hear to line
them up. Names
that YouTube reports (`Artist - Song (Official Video)`, `Artist - Topic`, `ArtistVEVO`) are cleaned and
searched several ways, and a result is only used when its title and length match. A player that reports no timeline (some
browsers) gets an estimated position, counted from the start of the track: click the line you hear to sync it.

**Clipboard history.** Hover a row to pin it (up to four): a pinned item stays on top and survives new copies and Clear.
A picture shows a thumbnail, its size and how much memory it takes. Pictures are kept as compressed PNG, 24 MB at most in
total, and the oldest go first. Nothing is saved to disk, and anything a password manager marks as sensitive is skipped.

**Tasks.** On the Tasks page type into the field and press Enter, or type `remind` or `todo` in Spotlight, then an optional time and the text: `remind 15m stretch`,
`remind 17:30 call mom` (or `5pm`), `remind every 45m drink water` (repeats, at most once a minute) or just `todo buy milk`
for an item without a time. When one is due it shows as a toast for ten seconds with a beep, and `snooze` asks again in ten
minutes. `todo` in Spotlight lists what is pending, and Enter on a row ticks it off; on the page, click the circle. They are saved in `%APPDATA%\Camel\reminders.json`,
so they survive a restart. A time like `10:30` means the clock; write `10m30s` for a duration.

**Focus timer.** Click the time and type `30` or `25:30`, use the − and + buttons (5 minutes, or 1 with Shift), scroll over
the timer (1 minute per notch), or pick 15, 25 or 45. It is locked while it runs, and you get a beep and a notice when the
session ends. The **Rounds** chip next to the time turns on focus and break rounds: when a focus session ends a 5 minute
break starts by itself, then focus again, and after the fourth focus session a 15 minute break. The ring turns green during
breaks and the label shows where you are in the cycle.

**Now playing.** When several players are open, hover the album art to see which one is shown and click it to switch.

**Alerts.** The Caps Lock and language notice is checked every quarter second. The language is the keyboard layout of the
window you are typing in, so switching to an app with another layout shows it too. The connection and audio output checks
run every five seconds. Each group has its own switch in Settings.

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

Type a few letters and press Enter. Actions that can lose work show a red row and need a second Enter. Anything else you
type searches Start Menu apps and files in Desktop, Documents and Downloads, and the last row opens a link or searches the
web with DuckDuckGo.

| Type | Does |
|---|---|
| `lock`, `sleep` | Locks the PC, or puts it to sleep. |
| `restart`, `shutdown`, `sign out` | Restarts, shuts down or signs out, after a second Enter. |
| `awake`, `caffeine` | Turns keep awake on or off. |
| `timer 10m`, `focus 1h30m`, `timer 25` | Starts the focus timer at that length. Accepts `10m`, `90s`, `1h30m`, `25`, `25:30`. |
| `remind 15m stretch`, `remind every 45m drink water`, `todo buy milk` | Adds a reminder or a to-do. `todo` lists them and Enter ticks one off, `snooze` repeats the last one in 10 minutes. |
| `time tokyo`, `time in new york` | The time in another city, with the gap from here. About 40 cities; daylight saving comes from Windows. Enter copies it. |
| `until 25 dec`, `days until 2026-12-25` | The days left to a date. Enter copies it. |
| `update`, `version` | Checks for a newer version, or installs the one that was found. |
| `mute`, `mic` | Mutes or unmutes the speakers (the volume HUD shows it) or the default microphone. |
| `note buy milk` | Saves a dated line to `%APPDATA%\Camel\notes.txt`. `notes` opens that file. |
| `kill chrome` | Ends every running program whose name starts with it, after a second Enter. System programs (Explorer, `svchost`, …) and Camel itself are never offered. |
| `5 km in miles`, `100 f to c`, `2 kg to lb` | Converts length, weight, volume, time, speed and temperature. Enter copies the result. |
| `2*(3+4)` | Calculates. Enter copies the result. |

## Privacy and security

**Network.** Camel makes two kinds of request, and both can be turned off:

| Request | When | What it sends |
|---|---|---|
| lrclib.net | **Synced lyrics** is on and a track plays | The artist, track title and duration. |
| api.github.com and github.com | **Check for updates** is on, or you ask for an update | A request for the latest release of this repository, which reveals your IP address and nothing else of yours. |

With both off, Camel makes no network requests. The web row in Spotlight only opens your browser when you pick it.

**Local data.** Clipboard history lives in memory and disappears when Camel quits. Turn **Clipboard history** off and
Camel stops reading the clipboard altogether. Settings, notes and the uninstall record live in `%APPDATA%\Camel`.

**Apps.** The Apps tab reads the registry and the file system on your machine and sends nothing anywhere. Leftover cleanup
only offers folders named exactly like the program (or its install folder) under the usual data locations, never registry
keys, and only moves them to the Recycle Bin when you click **Clean up**. Camel remembers a finished uninstall in
`%APPDATA%\Camel\uninstall.json` until you clean up or dismiss it, so closing Camel does not lose the list and a failed
clean up can be retried. Files that Windows will not release are queued for deletion at the next restart.

**Updates.** A download is accepted only from this repository's release downloads and only when its SHA-256 matches the
one published with the release. The executable is not code-signed, so trust rests on the GitHub account that publishes the
releases.

### Administrator rights

The release build asks for administrator rights when it starts: it reopens itself through the UAC prompt and the
unelevated copy exits. That is what lets the Apps tab move leftover folders out of `Program Files` and switch off
machine-wide startup entries. If you decline the prompt, Camel keeps running without admin rights, and the actions that
need them say so. Start it with `--no-elevate` to skip the prompt. Debug builds (`cargo run`) never ask.

Running elevated has side effects:

- **Start with Windows** registers a scheduled task with the highest run level instead of a `Run` entry, because Windows
  silently skips elevated programs listed under `Run`.
- Windows blocks drag and drop from a normal Explorer window into an elevated window, so dragging files onto the shelf
  does not work in the release build.
- Programs and files you open from Spotlight, the shelf or the Apps tab start with administrator rights too.

## Build from source

Requires Windows 10 or 11 and a recent Rust toolchain (edition 2024).

```powershell
git clone https://github.com/aw4e/Camel.git
cd Camel
cargo run --release
```

The release build is `target\release\camel.exe`, a single file you can copy anywhere.

Before sending a change, run:

```powershell
cargo fmt --check
cargo clippy --all-targets
cargo test
```

`rustfmt.toml` sets the line width to 160. A few tests talk to the real system and are skipped by default: run
`cargo test --bin camel clipboard_round_trip -- --ignored` (writes to the clipboard) or `finds_lyrics_online` (needs the
network) by hand.

### Releases

The version lives in `Cargo.toml`. Pushing a tag that matches it (`v0.2.2` for `version = "0.2.2"`) starts the
[release workflow](.github/workflows/release.yml), which tests, builds on Windows and publishes `camel.exe` together with
`camel.exe.sha256`. Camel's updater reads that release.

### Fonts

Camel uses the embedded Open Sans by default. If a `fonts` folder with `SF-Pro-Display-Regular.otf` and
`SF-Pro-Display-Semibold.otf` sits next to the exe (or in the project root while developing), it uses those instead. SF Pro
is Apple's font and is not included in this repository, so `fonts/` is git-ignored. Obtain it from Apple under its license
if you want that look.

### Project layout

```
src/main.rs                  entry point
src/notch.rs                 app state, input, animation, window sizing
src/notch/draw.rs            closed pill, toast and the tab bar
src/notch/tabs/              one file per page: media, mixer, clock, timer, tasks, shelf, clipboard, apps, settings
src/notch/spotlight_view.rs  Spotlight panel
src/layout.rs                golden-ratio and Fibonacci layout tokens, with tests
src/platform.rs              tray, hotkey, fonts, window frame, elevation
src/widgets.rs               small drawing and input helpers
src/ui.rs                    icons, sliders, switches, notch shape
src/media.rs                 Windows media session worker
src/lyrics.rs                lrclib lookup, name cleaning, LRC parsing
src/mixer.rs                 WASAPI master and per-app volume, mute
src/monitor.rs               battery, CPU/RAM, mic/camera, clipboard, memory trim
src/spotlight.rs             launcher index, search and calculator
src/settings.rs              settings file and autostart
src/apps.rs                  installed programs, uninstall, leftovers, startup entries
src/actions.rs               Spotlight actions: power, timers, quitting programs, unit conversion
src/clipboard.rs             clipboard history and pins
src/notes.rs                 quick notes
src/update.rs                release check, verified download, in-place swap
src/reminders.rs             reminders and to-dos, parsing and storage
src/worldtime.rs            time in other cities and days to a date
src/system.rs               Windows queries: clock format, animation setting, keyboard, network, audio output
src/focus.rs                 focus timer
```

## Design rules

Layout numbers are not picked by eye. They are named constants in `src/layout.rs`, and its tests fail if a screen drifts
from them:

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
  tasks and apps, storage).
- **Today in the calendar:** the disc behind today is wider than the digits (17 px for two digits) and still clears the
  rows above and below.
- **Icons:** one drawing language, a 1.5 px stroke with round caps and joins, and rounded corners.

The Settings screen is the densest, so its two groups are the one place where rows are tighter (21 px).

## Known limitations

- Windows only. Camel relies on Windows media controls, WASAPI, the registry and DWM.
- The Apps tab covers classic desktop programs. Microsoft Store apps, Windows updates and hidden system components are not
  listed.
- Spotlight indexes up to 60,000 files, and the index lives in memory only.
- Mic and camera dots read the Windows privacy consent store, so they follow what Windows itself reports.
- Lyrics follow the position the player reports. Some browsers report none at all; Camel then counts the time itself from
  the start of the track, and you click the line you hear to correct it (the seek bar is hidden in that case). Lyrics that
  are out of sync are tracked in [#2](https://github.com/aw4e/Camel/issues/2).

## Contributing

Bug reports, ideas and pull requests are welcome.

- **Found a bug or want a feature?** Open an [issue](https://github.com/aw4e/Camel/issues). For a bug, say which Windows
  version and which player or app was involved, and what you expected.
- **Sending code?** Keep the checks above green, add a test for logic that can be tested without a window, and keep layout
  numbers in `src/layout.rs` rather than in the drawing code. One change per pull request keeps review quick.
- **Security problem?** Please do not post details in a public issue. Use **Report a vulnerability** on the repository's
  Security tab, or open an issue that only says you found one and ask for a private channel.

By contributing you agree that your work is released under the same [MIT License](LICENSE).

## License and credits

Camel is released under the [MIT License](LICENSE): use it, change it, share it, even sell it, as long as the copyright
notice stays.

- Font: Open Sans, embedded in the exe, under the SIL Open Font License ([assets/OFL.txt](assets/OFL.txt)).
- Lyrics: [lrclib.net](https://lrclib.net).
- The Rust crates Camel is built from keep their own licenses (MIT, Apache-2.0 and similar).
- Logo, app icon and favicon live in `assets/brand/`. See [assets/brand/BRAND.md](assets/brand/BRAND.md) for colors, sizes
  and usage.
