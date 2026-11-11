mod draw;
pub(crate) mod icons;
pub(crate) mod layout;
mod pages;
pub(crate) mod prelude;
mod spotlight_view;
pub(crate) mod widgets;

use crate::platform::{self, HOTKEY_PRESSED, Tray};
use crate::prelude::*;
use crate::settings::{self, Settings};
use crate::{actions, apps, clipboard, focus, media, mixer, monitor, notes, radios, reminders, spotlight, system, update, weather};
use chrono::Local;
use global_hotkey::GlobalHotKeyManager;
use global_hotkey::hotkey::HotKey;
use pages::clock::month_rows;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tray_icon::menu::MenuEvent;

fn tab_icon(t: Tab) -> fn(&egui::Painter, egui::Pos2, Color32) {
    match t {
        Tab::Media => ui::icon_music,
        Tab::Controls => ui::icon_sliders,
        Tab::Tasks => ui::icon_tasks,
        Tab::Mixer => ui::icon_speaker,
        Tab::Clock => ui::icon_clock,
        Tab::Timer => ui::icon_timer,
        Tab::Shelf => ui::icon_tray,
        Tab::Clipboard => ui::icon_clipboard,
        Tab::Apps => ui::icon_grid,
        Tab::Settings => ui::icon_gear,
    }
}

/// The short name shown beside the icon of the chosen tab.
fn tab_name(t: Tab) -> &'static str {
    match t {
        Tab::Media => "Media",
        Tab::Controls => "Controls",
        Tab::Tasks => "Tasks",
        Tab::Mixer => "Mixer",
        Tab::Clock => "Clock",
        Tab::Timer => "Timer",
        Tab::Shelf => "Shelf",
        Tab::Clipboard => "Clipboard",
        Tab::Apps => "Apps",
        Tab::Settings => "Settings",
    }
}

fn tab_label(t: Tab) -> &'static str {
    match t {
        Tab::Media => "Now playing",
        Tab::Controls => "Controls",
        Tab::Tasks => "Tasks",
        Tab::Mixer => "Volume mixer",
        Tab::Clock => "Clock",
        Tab::Timer => "Focus timer",
        Tab::Shelf => "File shelf",
        Tab::Clipboard => "Clipboard",
        Tab::Apps => "Apps",
        Tab::Settings => "Settings",
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tab {
    Media,
    Shelf,
    Clipboard,
    Tasks,
    Controls,
    Clock,
    Timer,
    Mixer,
    Apps,
    Settings,
}

pub(crate) const MAIN_TABS: [Tab; 9] = [Tab::Media, Tab::Mixer, Tab::Clock, Tab::Timer, Tab::Tasks, Tab::Shelf, Tab::Clipboard, Tab::Controls, Tab::Apps];

/// The pages in four groups. In the tab bar a group is one unit: closed it is one icon, chosen it opens to show its
/// pages, and the page you are on shows its name.
pub(crate) const GROUPS: [(&str, &[Tab]); 4] = [
    ("Media", &[Tab::Media, Tab::Mixer]),
    ("Time", &[Tab::Clock, Tab::Timer, Tab::Tasks]),
    ("Shelf", &[Tab::Shelf, Tab::Clipboard]),
    ("System", &[Tab::Controls, Tab::Apps]),
];

pub(crate) fn group_of(t: Tab) -> Option<usize> {
    GROUPS.iter().position(|(_, pages)| pages.contains(&t))
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Compact,
    Toast,
    Open,
    Spot,
}

#[derive(Clone, Copy)]
enum ToastIcon {
    Clipboard,
    Awake,
    Volume,
    Timer,
    Apps,
    Battery,
    BatteryLow,
    Keyboard,
    Online,
    Offline,
}

struct Toast {
    icon: ToastIcon,
    title: String,
    body: String,
    until: Instant,
    /// A 0..1 bar under the title: the volume HUD.
    level: Option<f32>,
}

/// Critically damped spring (response 0.42 s).
struct Spring {
    x: f32,
    v: f32,
}

impl Spring {
    fn step(&mut self, target: f32, dt: f32) {
        const K: f32 = 224.0;
        const C: f32 = 30.0;
        let mut left = dt.min(0.05);
        while left > 0.0 {
            let h = left.min(1.0 / 240.0);
            self.v += (K * (target - self.x) - C * self.v) * h;
            self.x += self.v * h;
            left -= h;
        }
        if (self.x - target).abs() < 0.3 && self.v.abs() < 2.0 {
            self.x = target;
            self.v = 0.0;
        }
    }

    fn snap(&mut self, target: f32) {
        (self.x, self.v) = (target, 0.0);
    }

    fn settled(&self, target: f32) -> bool {
        self.x == target && self.v == 0.0
    }
}

#[derive(Default)]
struct Spot {
    open: bool,
    opened_at: Option<Instant>,
    query: String,
    sel: usize,
    hits: Vec<spotlight::Hit>,
    /// Row that asked for a second Enter (restart, shut down, sign out, quit a program).
    armed: Option<usize>,
}

pub(crate) struct Notch {
    media: media::Handle,
    monitor: monitor::Monitor,
    settings: Settings,
    tab: Tab,
    prev_tab: Tab,
    tab_at: Instant,
    /// The page last open in each group, which a closed group returns to when it is clicked.
    group_last: [Tab; 4],
    w: Spring,
    h: Spring,
    /// OS window size; only changes when a transition starts or ends, never per frame.
    win: Vec2,
    last_top: f32,
    /// Animated body rect from the last paint, in window coordinates.
    shape: Rect,
    trim_at: Instant,
    mode: Mode,
    mode_at: Instant,
    last_hover: Instant,
    pinned: bool,
    hidden: bool,
    topmost_at: Instant,
    art_tex: Option<(Arc<media::Art>, egui::TextureHandle, Color32)>,
    mixer: Option<mixer::Mixer>,
    mixer_at: Instant,
    mixer_scroll: usize,
    shelf: Vec<PathBuf>,
    shelf_scroll: usize,
    history: clipboard::History,
    /// Thumbnail textures of the pictures in the history, by picture id.
    clip_thumbs: std::collections::HashMap<u64, egui::TextureHandle>,
    clip_scroll: usize,
    focus: focus::Focus,
    /// Text being typed into the timer, and whether the field was just opened (it takes focus once).
    timer_edit: Option<(String, bool)>,
    /// Apps tab data, loaded when the tab opens and dropped when it closes.
    apps: Option<Vec<apps::App>>,
    startups: Option<Vec<apps::Startup>>,
    apps_startup: bool,
    can_admin: bool,
    apps_query: String,
    apps_scroll: usize,
    apps_n: usize,
    /// An uninstaller we started, watched until its registry entry disappears.
    pending: Option<(apps::App, Instant)>,
    leftovers: Option<Vec<PathBuf>>,
    leftover_name: String,
    toast: Option<Toast>,
    clip_seq: u32,
    prev_charging: Option<bool>,
    low_warned: bool,
    spot: Spot,
    index: spotlight::Index,
    hotkey: Option<(GlobalHotKeyManager, HotKey)>,
    tray: Tray,
    /// 24-hour clock and the Windows animation setting, read from Windows every few seconds.
    h24: bool,
    reduce_motion: bool,
    sys_at: Instant,
    caps: Option<bool>,
    lang: Option<String>,
    net: Option<system::Network>,
    online: Option<bool>,
    output: Option<String>,
    /// Keep awake: no screen timeout, no sleep.
    awake: bool,
    master: Option<mixer::Master>,
    master_at: Instant,
    last_vol: Option<(f32, bool)>,
    force_open_until: Instant,
    last_frame: Instant,
    swipe: f32,
    tab_scroll: f32,
    /// Media page shows the scrolling lyrics instead of the player controls.
    lyrics_view: bool,
    /// Fractional index of the lyric line the view is scrolled to.
    lyric_pos: f32,
    /// Seconds the lyrics are ahead of the player for the current track, and which track that is.
    lyric_offset: f64,
    lyric_track: String,
    updater: update::Updater,
    weather: weather::Service,
    /// Wi-Fi and Bluetooth, and the mute state of the microphone and speakers as the controls page last read them.
    radios: radios::Radios,
    controls_at: Instant,
    mic_muted: Option<bool>,
    speakers_muted: Option<bool>,
    /// What is being typed into the Tasks page, and how far its list is scrolled.
    task_input: String,
    tasks_scroll: usize,
    reminders: reminders::Reminders,
    /// The reminder that fired last, for Snooze.
    last_fired: Option<reminders::Reminder>,
    /// The player shown last, to say so when it changes.
    source: String,
    launched: Instant,
    update_at: Option<Instant>,
    /// A check the user asked for, so its outcome is reported even when nothing is new.
    update_asked: bool,
    update_announced: Option<String>,
}

impl Notch {
    pub(crate) fn new(ctx: &egui::Context) -> Self {
        let settings = Settings::load();
        let weather = weather::Service::spawn(settings.weather, settings.city.clone());
        let record = apps::load_record();
        let tray = platform::make_tray(settings.minimal);
        let hotkey = platform::register_hotkey(ctx, settings.spotlight);
        Self {
            media: media::spawn(settings.lyrics),
            monitor: monitor::Monitor::new(),
            settings,
            tab: Tab::Media,
            prev_tab: Tab::Media,
            tab_at: Instant::now() - Duration::from_secs(1),
            group_last: [Tab::Media, Tab::Clock, Tab::Shelf, Tab::Controls],
            w: Spring { x: PILL_W, v: 0.0 },
            h: Spring { x: PILL_H, v: 0.0 },
            win: Vec2::ZERO,
            last_top: 0.0,
            shape: Rect::NOTHING,
            // first trim ~2 s after launch, then every 30 s of idle
            trim_at: Instant::now() - Duration::from_secs(28),
            mode: Mode::Compact,
            mode_at: Instant::now(),
            last_hover: Instant::now() - Duration::from_secs(10),
            pinned: false,
            hidden: false,
            topmost_at: Instant::now(),
            art_tex: None,
            mixer: None,
            mixer_at: Instant::now(),
            mixer_scroll: 0,
            shelf: Vec::new(),
            shelf_scroll: 0,
            history: clipboard::History::default(),
            clip_thumbs: Default::default(),
            clip_scroll: 0,
            focus: focus::Focus::new(25),
            timer_edit: None,
            apps: None,
            startups: None,
            apps_startup: false,
            can_admin: false,
            apps_query: String::new(),
            apps_scroll: 0,
            apps_n: 0,
            pending: record.pending.map(|a| (a, Instant::now())),
            leftovers: Some(record.leftovers).filter(|l| !l.is_empty()),
            leftover_name: record.leftover_name,
            toast: None,
            clip_seq: monitor::clipboard_seq(),
            prev_charging: None,
            low_warned: false,
            spot: Spot::default(),
            index: spotlight::build_index(),
            hotkey,
            tray,
            h24: system::use_24h(),
            reduce_motion: system::reduced_motion(),
            sys_at: Instant::now(),
            caps: None,
            lang: None,
            net: None,
            online: None,
            output: None,
            awake: false,
            master: None,
            master_at: Instant::now(),
            last_vol: None,
            force_open_until: Instant::now(),
            last_frame: Instant::now(),
            swipe: 0.0,
            tab_scroll: 0.0,
            lyrics_view: false,
            lyric_pos: 0.0,
            lyric_offset: 0.0,
            lyric_track: String::new(),
            updater: update::Updater::new(),
            weather,
            radios: radios::Radios::spawn(),
            controls_at: Instant::now() - Duration::from_secs(5),
            mic_muted: None,
            speakers_muted: None,
            task_input: String::new(),
            tasks_scroll: 0,
            reminders: reminders::Reminders::load(),
            last_fired: None,
            source: String::new(),
            launched: Instant::now(),
            update_at: None,
            update_asked: false,
            update_announced: None,
        }
    }

    fn save_record(&self) {
        apps::save_record(&apps::Record {
            pending: self.pending.as_ref().map(|(a, _)| a.clone()),
            leftover_name: self.leftover_name.clone(),
            leftovers: self.leftovers.clone().unwrap_or_default(),
        });
    }

    fn show_toast(&mut self, icon: ToastIcon, title: &str, body: String) {
        self.toast = Some(Toast { icon, title: title.to_owned(), body, until: Instant::now() + Duration::from_millis(2800), level: None });
    }

    /// The volume HUD: a short toast with a bar.
    fn show_volume(&mut self, (level, muted): (f32, bool)) {
        let title = if muted { "Muted" } else { "Volume" };
        self.toast = Some(Toast {
            icon: ToastIcon::Volume,
            title: title.to_owned(),
            body: format!("{:.0}%", level * 100.0),
            until: Instant::now() + Duration::from_millis(1600),
            level: Some(if muted { 0.0 } else { level }),
        });
    }

    fn set_weather(&mut self, on: bool) {
        self.settings.weather = on;
        self.settings.save();
        self.weather.set_enabled(on);
        self.show_toast(
            ToastIcon::Online,
            if on { "Weather on" } else { "Weather off" },
            if on { "Shown on the Clock page".to_owned() } else { "No requests are made".to_owned() },
        );
    }

    fn set_minimal(&mut self, on: bool) {
        self.settings.minimal = on;
        self.settings.save();
        if let Some(item) = &self.tray.minimal {
            item.set_checked(on);
        }
    }

    fn set_awake(&mut self, on: bool) {
        self.awake = on;
        monitor::set_awake(on);
        if let Some(item) = &self.tray.awake {
            item.set_checked(on);
        }
        self.show_toast(ToastIcon::Awake, if on { "Keeping this PC awake" } else { "Sleep is allowed again" }, "Screen and sleep timers".to_owned());
    }

    /// Runs the row picked in Spotlight.
    fn run_hit(&mut self, h: &spotlight::Hit) {
        match &h.action {
            Some(a) => self.run_action(a.clone()),
            None if h.kind == spotlight::Kind::Calc => {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(h.title.clone());
                }
            }
            None => spotlight::launch(&h.path),
        }
    }

    fn run_action(&mut self, a: actions::Action) {
        use actions::Action::*;
        match a {
            Lock => actions::lock(),
            Sleep => actions::sleep(),
            Restart => actions::power("/r"),
            Shutdown => actions::power("/s"),
            SignOut => actions::power("/l"),
            KeepAwake => self.set_awake(!self.awake),
            // the speakers' mute shows up in the volume HUD by itself
            Mute(false) => {
                mixer::toggle_mute(false);
            }
            Mute(true) => match mixer::toggle_mute(true) {
                Some(muted) => self.show_toast(ToastIcon::Volume, if muted { "Microphone muted" } else { "Microphone on" }, "Default input device".to_owned()),
                None => self.show_toast(ToastIcon::Volume, "No microphone found", "Nothing to mute".to_owned()),
            },
            Note(text) => {
                if notes::add(&text) {
                    self.show_toast(ToastIcon::Clipboard, "Note saved", shorten(&text, 40));
                } else {
                    self.show_toast(ToastIcon::Clipboard, "Could not save the note", notes::path().display().to_string());
                }
            }
            Update => self.update_action(),
            Weather => self.set_weather(!self.settings.weather),
            City(name) => {
                self.settings.city.clone_from(&name);
                self.settings.save();
                self.weather.set_city(&name);
                if !self.settings.weather {
                    self.set_weather(true);
                }
                self.show_toast(ToastIcon::Online, "Weather city", shorten(&name, 36));
            }
            Remind(spec) => {
                let when = reminders::describe(spec.due, spec.every, Local::now());
                let text = spec.text.clone();
                self.reminders.add(spec);
                self.show_toast(ToastIcon::Timer, &format!("Reminder set, {when}"), shorten(&text, 40));
            }
            Done(id) => {
                self.reminders.remove(id);
                self.show_toast(ToastIcon::Timer, "Ticked off", "Reminder removed".to_owned());
            }
            Snooze => match self.last_fired.clone() {
                Some(r) => {
                    let spec = reminders::Spec { text: r.text.clone(), due: Some(Local::now().timestamp() + 600), every: None };
                    self.reminders.add(spec);
                    self.show_toast(ToastIcon::Timer, "Snoozed for 10 minutes", shorten(&r.text, 40));
                }
                None => self.show_toast(ToastIcon::Timer, "Nothing to snooze", "No reminder has fired yet".to_owned()),
            },
            OpenNotes => {
                let file = notes::path();
                if !file.exists() {
                    let _ = std::fs::create_dir_all(file.parent().unwrap_or(&file));
                    let _ = std::fs::write(&file, "");
                }
                spotlight::launch(&file);
            }
            Timer(d) => {
                self.focus.start(d, Instant::now());
                self.show_toast(ToastIcon::Timer, "Timer started", actions::format_duration(d));
            }
            Kill(name) => {
                let n = actions::kill(&name);
                self.show_toast(
                    ToastIcon::Apps,
                    &format!("Quit {}", shorten(&name, 18)),
                    format!("{n} {} closed", if n == 1 { "program" } else { "programs" }),
                );
            }
        }
    }

    /// Enter or a click on row `i`. Risky actions arm the row first and only run on the second one.
    fn activate(&mut self, i: usize) {
        let Some(h) = self.spot.hits.get(i).cloned() else { return };
        if h.action.as_ref().is_some_and(|a| a.needs_confirm()) && self.spot.armed != Some(i) {
            (self.spot.armed, self.spot.sel) = (Some(i), i);
            return;
        }
        self.run_hit(&h);
        self.spot.open = false;
    }

    fn sync_art(&mut self, ctx: &egui::Context, art: &Option<Arc<media::Art>>) {
        match (art, &self.art_tex) {
            (Some(a), Some((cur, ..))) if Arc::ptr_eq(a, cur) => {}
            (Some(a), _) => {
                let img = egui::ColorImage::from_rgba_unmultiplied(a.size, &a.rgba);
                let tex = ctx.load_texture("art", img, egui::TextureOptions::LINEAR);
                self.art_tex = Some((a.clone(), tex, accent(a)));
            }
            (None, _) => self.art_tex = None,
        }
    }

    fn apply_settings(&mut self, ctx: &egui::Context, before: &Settings) {
        if self.settings.autostart != before.autostart && !settings::set_autostart(self.settings.autostart) {
            self.settings.autostart = before.autostart;
            self.show_toast(ToastIcon::Apps, "Could not change autostart", "Run Camel as administrator".to_owned());
        }
        let s = &self.settings;
        if s.always_on_top != before.always_on_top {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if s.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal }));
        }
        if s.lyrics != before.lyrics {
            self.media.lyrics.enabled.store(s.lyrics, Ordering::Relaxed);
        }
        if s.spotlight != before.spotlight {
            if let Some((mgr, hk)) = &self.hotkey {
                let _ = if s.spotlight { mgr.register(*hk) } else { mgr.unregister(*hk) };
            } else if s.spotlight {
                self.hotkey = platform::register_hotkey(ctx, true);
            }
        }
        s.save();
    }

    fn open_spotlight(&mut self, ctx: &egui::Context) {
        self.spot = Spot { open: true, opened_at: Some(Instant::now()), ..Spot::default() };
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn poll_window(&mut self, ctx: &egui::Context) {
        let fullscreen = self.settings.hide_fullscreen && monitor::fullscreen_app() && !self.spot.open;
        if self.settings.always_on_top && !fullscreen && self.topmost_at.elapsed() > Duration::from_secs(1) {
            self.topmost_at = Instant::now();
            monitor::keep_on_top();
        }
        if fullscreen != self.hidden {
            self.hidden = fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(!fullscreen));
        }
    }

    fn poll_hotkey(&mut self, ctx: &egui::Context) {
        if !HOTKEY_PRESSED.swap(false, Ordering::Relaxed) {
            return;
        }
        if self.spot.open {
            self.spot.open = false;
        } else {
            self.open_spotlight(ctx);
        }
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if Some(&ev.id) == self.tray.quit.as_ref() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if self.tray.awake.as_ref().is_some_and(|i| i.id() == &ev.id) {
                let on = self.tray.awake.as_ref().is_some_and(|i| i.is_checked());
                self.set_awake(on);
            } else if let Some(item) = self.tray.minimal.as_ref().filter(|i| i.id() == &ev.id) {
                let on = item.is_checked();
                self.set_minimal(on);
            } else if Some(&ev.id) == self.tray.settings.as_ref() {
                self.tab = Tab::Settings;
                self.force_open_until = Instant::now() + Duration::from_secs(6);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
    }

    /// Volume HUD: watches master volume and mute. The Mixer tab has its own sliders, so it stays quiet there.
    fn poll_volume(&mut self) {
        if !self.settings.volume_hud {
            self.last_vol = None;
            return;
        }
        if self.master.is_none() || self.master_at.elapsed() > Duration::from_secs(5) {
            self.master = mixer::Master::new();
            self.master_at = Instant::now();
        }
        let Some(v) = self.master.as_ref().and_then(|m| m.read()) else {
            (self.master, self.last_vol) = (None, None);
            return;
        };
        let quiet = self.tab == Tab::Mixer && self.mode == Mode::Open;
        if let Some(prev) = self.last_vol
            && ((v.0 - prev.0).abs() > 0.004 || v.1 != prev.1)
            && !quiet
        {
            self.show_volume(v);
        }
        self.last_vol = Some(v);
    }

    fn poll_clipboard(&mut self) {
        let seq = monitor::clipboard_seq();
        if seq == self.clip_seq {
            return;
        }
        self.clip_seq = seq;
        if !self.settings.clipboard_toast {
            return;
        }
        let body = match monitor::read_clipboard() {
            monitor::Clip::Text(t) => {
                self.history.push_text(&t);
                clipboard::one_line(&t, 200)
            }
            monitor::Clip::Image { w, h, rgba } => match clipboard::make_pic(w, h, rgba) {
                Some(pic) => {
                    self.history.push_image(pic);
                    format!("Image {w} × {h}")
                }
                None => "Image too large to keep".to_owned(),
            },
            monitor::Clip::Hidden => "Hidden".to_owned(),
            monitor::Clip::Other => "Files or other data".to_owned(),
        };
        self.show_toast(ToastIcon::Clipboard, "Copied", body);
    }

    /// Watches an uninstaller we started until its registry entry is gone, then looks for leftovers.
    fn poll_uninstall(&mut self) {
        let Some((app, started)) = &self.pending else { return };
        if apps::installed(app) {
            if started.elapsed() > Duration::from_secs(900) {
                self.pending = None;
                self.save_record();
            }
            return;
        }
        let Some((app, _)) = self.pending.take() else { return };
        let found = apps::leftovers(&app);
        let body = if found.is_empty() { "No leftovers found".to_owned() } else { format!("{} leftover folder(s) found", found.len()) };
        self.leftovers = Some(found).filter(|f| !f.is_empty());
        self.leftover_name = app.name.clone();
        self.save_record();
        self.apps = None;
        self.show_toast(ToastIcon::Apps, &format!("{} removed", shorten(&app.name, 22)), body);
    }

    /// Windows settings every few seconds; the keyboard every tick, because a Caps Lock toast has to feel immediate.
    fn poll_system(&mut self) {
        if self.sys_at.elapsed() > Duration::from_secs(5) {
            self.sys_at = Instant::now();
            self.h24 = system::use_24h();
            self.reduce_motion = system::reduced_motion();
            if self.settings.device_toasts {
                self.poll_devices();
            }
        }
        if self.settings.key_toasts {
            self.poll_keys();
        }
    }

    fn poll_keys(&mut self) {
        let caps = system::caps_lock();
        if self.caps.is_some_and(|c| c != caps) {
            self.show_toast(ToastIcon::Keyboard, if caps { "Caps Lock on" } else { "Caps Lock off" }, "Keyboard".to_owned());
        }
        self.caps = Some(caps);
        if let Some(lang) = system::keyboard_language() {
            if self.lang.as_ref().is_some_and(|prev| *prev != lang) {
                self.show_toast(ToastIcon::Keyboard, "Keyboard language", lang.clone());
            }
            self.lang = Some(lang);
        }
    }

    fn poll_devices(&mut self) {
        if self.net.is_none() {
            self.net = system::Network::new();
        }
        if let Some(online) = self.net.as_ref().and_then(|n| n.online()) {
            if self.online.is_some_and(|prev| prev != online) {
                let (icon, title) = if online { (ToastIcon::Online, "Back online") } else { (ToastIcon::Offline, "Connection lost") };
                self.show_toast(icon, title, "Internet".to_owned());
            }
            self.online = Some(online);
        }
        if let Some((id, name)) = system::audio_output() {
            if self.output.as_ref().is_some_and(|prev| *prev != id) {
                self.show_toast(ToastIcon::Volume, "Audio output", shorten(&name, 36));
            }
            self.output = Some(id);
        }
    }

    fn poll_reminders(&mut self) {
        for r in self.reminders.take_due(Local::now().timestamp()) {
            self.show_toast(ToastIcon::Timer, &r.text.chars().take(40).collect::<String>(), "Reminder. Type snooze for 10 more minutes".to_owned());
            if let Some(t) = &mut self.toast {
                t.until = Instant::now() + Duration::from_secs(10);
            }
            monitor::beep();
            self.last_fired = Some(r);
        }
    }

    fn poll_focus(&mut self) {
        let Some(ended) = self.focus.tick(Instant::now()) else { return };
        let (title, body) = match (ended, self.focus.rounds()) {
            (focus::Phase::Focus, true) => ("Focus complete", format!("Break time, {} min", self.focus.total().as_secs() / 60)),
            (focus::Phase::Focus, false) => ("Focus complete", "Time for a short break".to_owned()),
            _ => ("Break over", "Back to focus".to_owned()),
        };
        self.show_toast(ToastIcon::Timer, title, body);
        if let Some(t) = &mut self.toast {
            t.until = Instant::now() + Duration::from_secs(8);
        }
        monitor::beep();
    }

    fn poll_battery(&mut self) {
        if !self.monitor.refresh(self.settings.privacy_dots) {
            return;
        }
        let (battery, charging) = (self.monitor.stats.battery, self.monitor.stats.charging);
        let Some(b) = battery else { return };
        if self.prev_charging.is_some_and(|p| p != charging) {
            self.show_toast(ToastIcon::Battery, if charging { "Charging" } else { "On battery" }, format!("{b}%"));
        }
        if b <= 15 && !charging && !self.low_warned {
            self.low_warned = true;
            self.show_toast(ToastIcon::BatteryLow, "Battery low", format!("{b}% remaining"));
        } else if b > 20 || charging {
            self.low_warned = false;
        }
        self.prev_charging = Some(charging);
    }

    /// Scroll and middle-click while the notch is open.
    fn gestures(&mut self, hover: Option<egui::Pos2>, scroll: Vec2, middle: bool) {
        if middle && self.tab == Tab::Media {
            self.media.send(media::Cmd::Toggle);
        }
        // scroll over the tab row switches pages; elsewhere it scrolls the page content
        let in_top = hover.is_some_and(|p| p.y < 34.0);
        if in_top {
            self.tab_scroll += scroll.y;
            if self.tab_scroll.abs() > 40.0 {
                let i = MAIN_TABS.iter().position(|t| *t == self.tab).unwrap_or(0) as i32;
                let ni = (i + if self.tab_scroll < 0.0 { 1 } else { -1 }).clamp(0, MAIN_TABS.len() as i32 - 1);
                self.tab = MAIN_TABS[ni as usize];
                self.tab_scroll = 0.0;
            }
        } else if self.tab == Tab::Media {
            self.swipe += scroll.x;
            if self.swipe.abs() > 90.0 {
                self.media.send(if self.swipe < 0.0 { media::Cmd::Next } else { media::Cmd::Prev });
                self.swipe = 0.0;
            }
        } else {
            let step = if scroll.y.abs() > 8.0 { scroll.y.signum() as i32 } else { 0 };
            let bump = |v: &mut usize, max: usize| *v = (*v as i32 - step).clamp(0, max as i32) as usize;
            if step != 0 && self.tab == Tab::Shelf {
                bump(&mut self.shelf_scroll, self.shelf.len().saturating_sub(5));
            }
            if step != 0 && self.tab == Tab::Mixer {
                let n = self.mixer.as_ref().map_or(0, |m| m.apps.len());
                bump(&mut self.mixer_scroll, n.saturating_sub(3));
            }
            if step != 0 && self.tab == Tab::Apps {
                bump(&mut self.apps_scroll, self.apps_n.saturating_sub(3));
            }
            if step != 0 && self.tab == Tab::Timer {
                self.focus.adjust(step as i64 * 60);
            }
            if step != 0 && self.tab == Tab::Tasks {
                bump(&mut self.tasks_scroll, self.reminders.list().len().saturating_sub(3));
            }
            if step != 0 && self.tab == Tab::Clipboard {
                bump(&mut self.clip_scroll, self.history.items().len().saturating_sub(3));
            }
        }
    }

    /// Springs the body towards its size for `mode` and keeps the OS window just big enough around it.
    /// Resizing the window every frame tears, so it only changes when a transition starts (grow to the destination
    /// at once) or ends (shrink after the shape settled). Returns the target size and whether the shape still moves.
    fn fit_window(&mut self, ctx: &egui::Context, mode: Mode, dt: f32) -> (Vec2, bool) {
        let target = match mode {
            Mode::Spot => vec2(SPOT_W, 52.0 + if self.spot.hits.is_empty() { 0.0 } else { 8.0 + 40.0 * self.spot.hits.len() as f32 }),
            Mode::Open => vec2(
                OPEN_W,
                if self.tab == Tab::Settings {
                    SETTINGS_H
                } else if self.tab == Tab::Clock && month_rows(Local::now().date_naive()) > 5 {
                    OPEN_H + 14.0
                } else {
                    OPEN_H
                },
            ),
            Mode::Toast => TOAST,
            Mode::Compact if self.settings.minimal => MINIMAL,
            Mode::Compact => vec2(PILL_W + if self.dots_active() { 30.0 } else { 0.0 }, PILL_H),
        };
        self.w.step(target.x, dt);
        self.h.step(target.y, dt);
        if self.reduce_motion {
            self.w.snap(target.x);
            self.h.snap(target.y);
        }
        let shape_moving = !self.w.settled(target.x) || !self.h.settled(target.y);
        let win_h = if mode == Mode::Compact && self.settings.minimal { MINIMAL_WIN_H } else { target.y };
        let want = vec2((target.x + 2.0 * EAR).round(), win_h.round());
        let win = if shape_moving { self.win.max(want) } else { want };
        // Spotlight floats at 22% of the screen height like macOS; everything else hangs from the top edge.
        let monitor = ctx.input(|i| i.viewport().monitor_size);
        let top = if mode == Mode::Spot { monitor.map_or(0.0, |m| (m.y * 0.22).round()) } else { 0.0 };
        if win != self.win || top != self.last_top {
            self.win = win;
            self.last_top = top;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(win));
            if let Some(mon) = monitor {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos2(((mon.x - win.x) / 2.0).round(), top)));
            }
        }
        (target, shape_moving)
    }

    /// Checks shortly after launch and then daily, if allowed; tells the user once about a new version.
    fn poll_update(&mut self, ctx: &egui::Context) {
        if self.updater.restart_due() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let due = match self.update_at {
            None => self.launched.elapsed() > Duration::from_secs(20),
            Some(t) => t.elapsed() > Duration::from_secs(24 * 3600),
        };
        if self.settings.auto_update && due {
            self.update_at = Some(Instant::now());
            self.updater.check();
        }
        match self.updater.status() {
            update::Status::Available(r) if self.update_announced.as_deref() != Some(&r.version) => {
                self.show_toast(ToastIcon::Apps, &format!("Camel {} is available", r.version), "Settings or the update command installs it".to_owned());
                self.update_announced = Some(r.version);
                self.update_asked = false;
            }
            update::Status::UpToDate if self.update_asked => {
                self.update_asked = false;
                self.show_toast(ToastIcon::Apps, "Camel is up to date", format!("Version {}", update::VERSION));
            }
            update::Status::Failed(e) if self.update_asked => {
                self.update_asked = false;
                self.show_toast(ToastIcon::Apps, "Update failed", e);
            }
            _ => {}
        }
    }

    /// The Update button and the `update` command: install what was found, otherwise look again.
    pub(crate) fn update_action(&mut self) {
        match self.updater.status() {
            update::Status::Available(r) => {
                self.show_toast(ToastIcon::Apps, &format!("Installing Camel {}", r.version), "Camel restarts when it is done".to_owned());
                self.updater.install();
            }
            update::Status::Checking | update::Status::Installing => {}
            _ => {
                self.update_asked = true;
                self.updater.check();
                self.show_toast(ToastIcon::Apps, "Checking for updates", format!("Version {}", update::VERSION));
            }
        }
    }

    /// Says which app the media page now follows when it changes, for example after a click on the album art.
    fn note_player(&mut self, source: &str) {
        if source != self.source {
            if !self.source.is_empty() && !source.is_empty() {
                self.show_toast(ToastIcon::Volume, "Now playing from", source.to_owned());
            }
            self.source = source.to_owned();
        }
    }

    fn mode_alpha(&self) -> f32 {
        ((self.mode_at.elapsed().as_secs_f32() - 0.06) / 0.15).clamp(0.0, 1.0)
    }
}

impl eframe::App for Notch {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    /// Runs while the window is hidden too, unlike `ui`.
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        self.poll_window(ctx);
        self.poll_hotkey(ctx);
        self.poll_tray(ctx);
        self.poll_volume();
        self.poll_clipboard();
        self.poll_uninstall();
        self.poll_focus();
        self.poll_battery();
        self.poll_system();
        self.poll_reminders();
        self.poll_update(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        let m = self.media.state();
        self.sync_art(&ctx, &m.art);
        self.note_player(&m.source);
        if self.toast.as_ref().is_some_and(|t| t.until < now) {
            self.toast = None;
        }

        let (hover, hovered_files, dropped, scroll, middle, focused) = ctx.input(|i| {
            (
                i.pointer.hover_pos(),
                i.raw.hovered_files.len(),
                i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect::<Vec<_>>(),
                i.smooth_scroll_delta,
                i.pointer.button_clicked(egui::PointerButton::Middle),
                i.viewport().focused,
            )
        });
        // the window can be larger than the shape mid-transition; transparent margins do not count as hover
        let slack = if self.settings.minimal && self.mode == Mode::Compact { MINIMAL_WIN_H } else { 2.0 };
        let hover = hover.filter(|p| self.shape.expand2(vec2(EAR, slack)).contains(*p));
        if hover.is_some() {
            self.last_hover = now;
        }
        let near = now.duration_since(self.last_hover) < Duration::from_millis(250);
        if hover.is_none() && !near {
            self.pinned = false;
        }
        if !self.settings.hover_expand && self.mode == Mode::Compact && ctx.input(|i| i.pointer.primary_clicked()) {
            self.pinned = true;
        }
        if hovered_files > 0 {
            self.tab = Tab::Shelf;
            self.last_hover = now;
        }
        if !dropped.is_empty() {
            self.tab = Tab::Shelf;
            self.last_hover = now;
            for p in dropped {
                if !self.shelf.contains(&p) {
                    self.shelf.push(p);
                }
            }
        }

        let want_open = (near && (self.settings.hover_expand || self.pinned)) || hovered_files > 0 || now < self.force_open_until;
        let mode = if self.spot.open {
            Mode::Spot
        } else if want_open {
            Mode::Open
        } else if self.toast.is_some() {
            Mode::Toast
        } else {
            Mode::Compact
        };
        if mode != self.mode {
            self.mode = mode;
            self.mode_at = now;
        }

        if mode == Mode::Open {
            self.gestures(hover, scroll, middle);
        }
        if mode == Mode::Spot && focused == Some(false) && self.spot.opened_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400)) {
            self.spot.open = false;
        }

        let (target, shape_moving) = self.fit_window(&ctx, mode, dt);

        if self.tab != self.prev_tab {
            self.prev_tab = self.tab;
            if let Some(g) = group_of(self.tab) {
                self.group_last[g] = self.tab;
            }
            self.tab_at = now;
            (self.apps, self.startups, self.apps_scroll, self.timer_edit) = (None, None, 0, None);
            if self.tab == Tab::Settings {
                self.settings.autostart = settings::autostart_enabled();
            }
            if self.tab != Tab::Controls {
                self.radios.unwatch();
            }
        }
        let fading = self.tab_at.elapsed() < Duration::from_millis(200);
        let moving = shape_moving || fading;
        // 30 fps is plenty for the equalizer and progress bar; full rate only while something springs
        let wake = if moving {
            0 // next vsync: paced by the display, not by a timer
        } else if m.playing {
            33
        } else if self.focus.running() || (mode == Mode::Open && matches!(self.tab, Tab::Clock | Tab::Timer)) {
            250
        } else {
            1000
        };
        if wake == 0 {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(wake));
        }

        if mode == Mode::Compact && !moving && self.trim_at.elapsed() > Duration::from_secs(30) {
            self.trim_at = now;
            monitor::trim_memory();
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            // the shape springs around the window centre; content keeps its final layout and is clipped to the shape
            let win_rect = ui.max_rect();
            let (w, h) = (self.w.x.max(40.0), self.h.x.max(20.0));
            let body = Rect::from_min_size(pos2(win_rect.center().x - w / 2.0, win_rect.top()), vec2(w, h));
            self.shape = body;
            let rect = Rect::from_min_size(pos2((win_rect.center().x - target.x / 2.0).round(), win_rect.top()), target);
            if mode == Mode::Spot {
                let r = body.shrink(0.5);
                ui.painter().rect_filled(r, 20.0, Color32::from_rgb(28, 28, 30));
                ui.painter().rect_stroke(r, 20.0, Stroke::new(1.0, fill(30)), StrokeKind::Inside);
            } else {
                // the corner radii grow with the body: top 6 -> 19, bottom 14 -> 24
                let k = ((h - PILL_H) / 120.0).clamp(0.0, 1.0);
                ui::notch_shape(ui.painter(), body, (6.0 + 13.0 * k).min(h), (14.0 + 10.0 * k).min(h));
            }
            ui.set_clip_rect(body.intersect(ui.clip_rect()));

            let a = self.mode_alpha();
            if a == 0.0 {
                return;
            }
            match mode {
                Mode::Compact => self.draw_compact(ui, rect, &m, a),
                Mode::Toast => self.draw_toast(ui, rect, a),
                Mode::Open => self.draw_open(ui, rect, &m, a),
                Mode::Spot => self.draw_spotlight(ui, rect, a),
            }
        });
    }
}
