#![windows_subsystem = "windows"]

mod media;
mod mixer;
mod monitor;
mod settings;
mod spotlight;
mod ui;

use chrono::{Datelike, Local, Timelike};
use eframe::egui::{self, Align2, Color32, Key, Modifiers, Rect, Response, Sense, Stroke, StrokeKind, Vec2, pos2, vec2};
use global_hotkey::hotkey::{Code, HotKey, Modifiers as HkMods};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use settings::Settings;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};
use ui::{BLUE, DIM, GRAY, GREEN, ORANGE, RED, bold, fade, fill, font};

/// Set by the hotkey callback. With an event handler installed the crate's channel stays empty.
static HOTKEY_PRESSED: AtomicBool = AtomicBool::new(false);

const PILL_W: f32 = 150.0;
const PILL_H: f32 = 32.0;
const OPEN_W: f32 = 460.0;
const OPEN_H: f32 = 136.0;
/// Transparent margin each side of the body, room for the flared top corners.
const EAR: f32 = 19.0;
const TOAST: Vec2 = vec2(300.0, 44.0);
const TAB_W: f32 = 36.0;
const SPOT_W: f32 = 480.0;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Media,
    Shelf,
    Clock,
    Mixer,
    Settings,
}

const MAIN_TABS: [Tab; 4] = [Tab::Media, Tab::Shelf, Tab::Clock, Tab::Mixer];

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
    Battery,
    BatteryLow,
}

struct Toast {
    icon: ToastIcon,
    title: String,
    body: String,
    until: Instant,
}

/// Damped spring; critically damped like Atoll's spring(response 0.42, dampingFraction 1.0).
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
}

struct Notch {
    media: media::Handle,
    monitor: monitor::Monitor,
    settings: Settings,
    tab: Tab,
    prev_tab: Tab,
    tab_at: Instant,
    pill: Spring,
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
    art_tex: Option<(Arc<media::Art>, egui::TextureHandle, Color32)>,
    mixer: Option<mixer::Mixer>,
    mixer_at: Instant,
    mixer_scroll: usize,
    shelf: Vec<PathBuf>,
    shelf_scroll: usize,
    toast: Option<Toast>,
    clip_seq: u32,
    prev_charging: Option<bool>,
    low_warned: bool,
    spot: Spot,
    index: spotlight::Index,
    hotkey: Option<(GlobalHotKeyManager, HotKey)>,
    _tray: Option<TrayIcon>,
    tray_settings: Option<MenuId>,
    tray_quit: Option<MenuId>,
    force_open_until: Instant,
    last_frame: Instant,
    swipe: f32,
    tab_scroll: f32,
}

impl Notch {
    fn new(ctx: &egui::Context) -> Self {
        let settings = Settings::load();
        let (tray, tray_settings, tray_quit) = make_tray();
        let hotkey = register_hotkey(ctx, settings.spotlight);
        Self {
            media: media::spawn(settings.lyrics),
            monitor: monitor::Monitor::new(),
            settings,
            tab: Tab::Media,
            prev_tab: Tab::Media,
            tab_at: Instant::now() - Duration::from_secs(1),
            pill: Spring { x: 0.0, v: 0.0 },
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
            art_tex: None,
            mixer: None,
            mixer_at: Instant::now(),
            mixer_scroll: 0,
            shelf: Vec::new(),
            shelf_scroll: 0,
            toast: None,
            clip_seq: monitor::clipboard_seq(),
            prev_charging: None,
            low_warned: false,
            spot: Spot::default(),
            index: spotlight::build_index(),
            hotkey,
            _tray: tray,
            tray_settings,
            tray_quit,
            force_open_until: Instant::now(),
            last_frame: Instant::now(),
            swipe: 0.0,
            tab_scroll: 0.0,
        }
    }

    fn show_toast(&mut self, icon: ToastIcon, title: &str, body: String) {
        self.toast = Some(Toast { icon, title: title.to_owned(), body, until: Instant::now() + Duration::from_millis(2800) });
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
        let s = &self.settings;
        if s.always_on_top != before.always_on_top {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if s.always_on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
        }
        if s.autostart != before.autostart {
            settings::set_autostart(s.autostart);
        }
        if s.lyrics != before.lyrics {
            self.media.lyrics.enabled.store(s.lyrics, Ordering::Relaxed);
        }
        if s.spotlight != before.spotlight {
            if let Some((mgr, hk)) = &self.hotkey {
                let _ = if s.spotlight { mgr.register(*hk) } else { mgr.unregister(*hk) };
            } else if s.spotlight {
                self.hotkey = register_hotkey(ctx, true);
            }
        }
        s.save();
    }

    fn open_spotlight(&mut self, ctx: &egui::Context) {
        self.spot = Spot { open: true, opened_at: Some(Instant::now()), ..Spot::default() };
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn mode_alpha(&self) -> f32 {
        ((self.mode_at.elapsed().as_secs_f32() - 0.06) / 0.15).clamp(0.0, 1.0)
    }
}

impl eframe::App for Notch {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    // logic() still runs while the window is hidden, ui() does not.
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(500));

        let fs = self.settings.hide_fullscreen && monitor::fullscreen_app() && !self.spot.open;
        if fs != self.hidden {
            self.hidden = fs;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(!fs));
        }

        if HOTKEY_PRESSED.swap(false, Ordering::Relaxed) {
            if self.spot.open {
                self.spot.open = false;
            } else {
                self.open_spotlight(ctx);
            }
        }
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if Some(&ev.id) == self.tray_quit.as_ref() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if Some(&ev.id) == self.tray_settings.as_ref() {
                self.tab = Tab::Settings;
                self.force_open_until = Instant::now() + Duration::from_secs(6);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }

        let seq = monitor::clipboard_seq();
        if seq != self.clip_seq {
            self.clip_seq = seq;
            if self.settings.clipboard_toast {
                self.show_toast(ToastIcon::Clipboard, "Copied", monitor::clipboard_preview());
            }
        }

        if self.monitor.refresh(self.settings.privacy_dots) {
            let s = &self.monitor.stats;
            let (bat, charging) = (s.battery, s.charging);
            if let Some(b) = bat {
                if self.prev_charging.is_some_and(|p| p != charging) {
                    let t = if charging { "Charging" } else { "On battery" };
                    self.show_toast(ToastIcon::Battery, t, format!("{b}%"));
                }
                if b <= 15 && !charging && !self.low_warned {
                    self.low_warned = true;
                    self.show_toast(ToastIcon::BatteryLow, "Battery low", format!("{b}% remaining"));
                } else if b > 20 || charging {
                    self.low_warned = false;
                }
                self.prev_charging = Some(charging);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        let m = self.media.state();
        self.sync_art(&ctx, &m.art);
        if self.toast.as_ref().is_some_and(|t| t.until < now) {
            self.toast = None;
        }

        // ---- input that decides the mode
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
        let hover = hover.filter(|p| self.shape.expand2(vec2(EAR, 2.0)).contains(*p));
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

        // ---- gestures while open
        if mode == Mode::Open {
            if middle && self.tab == Tab::Media {
                self.media.send(media::Cmd::Toggle);
            }
            // scroll over the tab row switches pages; elsewhere it scrolls the page content
            let in_top = hover.is_some_and(|p| p.y < 34.0);
            if in_top {
                self.tab_scroll += scroll.y;
                if self.tab_scroll.abs() > 40.0 {
                    let i = MAIN_TABS.iter().position(|t| *t == self.tab).unwrap_or(0) as i32;
                    let ni = (i + if self.tab_scroll < 0.0 { 1 } else { -1 }).clamp(0, 3);
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
            }
        }
        if mode == Mode::Spot && focused == Some(false) && self.spot.opened_at.is_some_and(|t| t.elapsed() > Duration::from_millis(400)) {
            self.spot.open = false;
        }

        // ---- window size
        let lyric_line = self.settings.lyrics && self.tab == Tab::Media && m.lyrics.is_some();
        let target = match mode {
            Mode::Spot => vec2(SPOT_W, 52.0 + if self.spot.hits.is_empty() { 0.0 } else { 8.0 + 40.0 * self.spot.hits.len() as f32 }),
            Mode::Open => vec2(OPEN_W, if lyric_line { OPEN_H + 24.0 } else { OPEN_H }),
            Mode::Toast => TOAST,
            Mode::Compact => vec2(PILL_W + if self.dots_active() { 30.0 } else { 0.0 }, PILL_H),
        };
        self.w.step(target.x, dt);
        self.h.step(target.y, dt);
        let shape_moving = !self.w.settled(target.x) || !self.h.settled(target.y);
        // Resizing the OS window every frame tears. Resize it once when a transition starts (grow to the
        // destination at once) or ends (shrink after the shape settled) and animate the shape inside it.
        let want = vec2((target.x + 2.0 * EAR).round(), target.y.round());
        let win = if shape_moving { self.win.max(want) } else { want };
        // Spotlight floats at 22% of the screen height like macOS; everything else hangs from the top edge.
        let top = if mode == Mode::Spot { ctx.input(|i| i.viewport().monitor_size).map_or(0.0, |m| (m.y * 0.22).round()) } else { 0.0 };
        if win != self.win || top != self.last_top {
            self.win = win;
            self.last_top = top;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(win));
            if let Some(mon) = ctx.input(|i| i.viewport().monitor_size) {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos2(((mon.x - win.x) / 2.0).round(), top)));
            }
        }

        if self.tab != self.prev_tab {
            self.prev_tab = self.tab;
            self.tab_at = now;
        }
        let tab_px = MAIN_TABS.iter().position(|t| *t == self.tab).unwrap_or(0) as f32 * TAB_W;
        self.pill.step(tab_px, dt);
        let fading = self.tab_at.elapsed() < Duration::from_millis(200);
        let moving = shape_moving || !self.pill.settled(tab_px) || fading;
        // 30 fps is plenty for the equalizer and progress bar; full rate only while something springs
        let wake = if moving {
            0 // next vsync: paced by the display, not by a timer
        } else if m.playing {
            33
        } else if mode == Mode::Open && self.tab == Tab::Clock {
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

        // ---- paint
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
                // Atoll: top radius 6 -> 19, bottom radius 14 -> 24
                let k = ((h - PILL_H) / 120.0).clamp(0.0, 1.0);
                ui::notch_shape(ui.painter(), body, 6.0 + 13.0 * k, 14.0 + 10.0 * k);
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

// ---------------------------------------------------------------- drawing

impl Notch {
    fn accent(&self) -> Color32 {
        self.art_tex.as_ref().map_or(Color32::from_gray(200), |a| a.2)
    }

    fn dots_active(&self) -> bool {
        let s = &self.monitor.stats;
        self.settings.privacy_dots && (s.mic || s.cam)
    }

    fn draw_compact(&self, ui: &mut egui::Ui, rect: Rect, m: &media::State, a: f32) {
        let p = ui.painter().clone();
        let half = (PILL_W + if self.dots_active() { 30.0 } else { 0.0 }) / 2.0;
        let c = rect.center();
        let art = Rect::from_center_size(pos2(c.x - half + 18.0, c.y), Vec2::splat(20.0));
        match &self.art_tex {
            Some((_, tex, _)) => egui::Image::new(tex).corner_radius(5).tint(fade(Color32::WHITE, a)).paint_at(ui, art),
            None => {
                p.rect_filled(art, 5.0, fade(Color32::from_gray(35), a));
            }
        }
        let time = ui.input(|i| i.time) as f32;
        ui::equalizer(&p, pos2(c.x + half - 20.0, c.y), 4, m.playing, time, fade(self.accent(), a), 14.0);

        let s = &self.monitor.stats;
        if self.dots_active() {
            let mut x = c.x - if s.mic && s.cam { 7.0 } else { 0.0 };
            for (on, col) in [(s.mic, GREEN), (s.cam, ORANGE)] {
                if on {
                    p.circle_filled(pos2(x, c.y), 3.5, fade(col, a));
                    x += 14.0;
                }
            }
        }
    }

    fn draw_toast(&self, ui: &mut egui::Ui, rect: Rect, a: f32) {
        let Some(t) = &self.toast else { return };
        let p = ui.painter().clone();
        let o = rect.min;
        let icon = Rect::from_center_size(pos2(o.x + 26.0, o.y + TOAST.y / 2.0), Vec2::splat(28.0));
        let (bg, glyph): (Color32, fn(&egui::Painter, egui::Pos2, Color32)) = match t.icon {
            ToastIcon::Clipboard => (BLUE, ui::icon_clipboard),
            ToastIcon::Battery => (GREEN, ui::icon_battery),
            ToastIcon::BatteryLow => (RED, ui::icon_battery),
        };
        ui::tile(&p, icon, fade(bg, a));
        glyph(&p, icon.center(), fade(Color32::WHITE, a));
        let clip = Rect::from_x_y_ranges(o.x + 48.0..=o.x + TOAST.x - 16.0, o.y..=o.y + TOAST.y);
        let q = p.with_clip_rect(clip);
        q.text(pos2(o.x + 48.0, o.y + 7.0), Align2::LEFT_TOP, &t.title, bold(12.0), fade(Color32::WHITE, a));
        q.text(pos2(o.x + 48.0, o.y + 23.0), Align2::LEFT_TOP, &t.body, font(11.0), fade(GRAY, a));
    }

    fn draw_open(&mut self, ui: &mut egui::Ui, rect: Rect, m: &media::State, a: f32) {
        let p = ui.painter().clone();
        let o = rect.min;
        let time = ui.input(|i| i.time) as f32;

        // segmented control: one track, the selected pill springs between segments
        let track = Rect::from_min_size(o + vec2(12.0, 4.0), vec2(TAB_W * 4.0 + 4.0, 24.0));
        p.rect_filled(track, 12.0, fade(fill(16), a));
        let in_set = ui.ctx().animate_bool_with_time(ui.scope_id().with("pill"), MAIN_TABS.contains(&self.tab), 0.15);
        let pill = Rect::from_min_size(track.min + vec2(2.0 + self.pill.x, 2.0), vec2(TAB_W, 20.0));
        p.rect_filled(pill, 10.0, fade(fill(46), a * in_set));
        for (i, tab) in MAIN_TABS.iter().enumerate() {
            let r = Rect::from_min_size(track.min + vec2(2.0 + i as f32 * TAB_W, 2.0), vec2(TAB_W, 20.0));
            let resp = hit(ui, &format!("tab{i}"), r);
            let col = if self.tab == *tab || resp.hovered() { Color32::WHITE } else { GRAY };
            let icon: fn(&egui::Painter, egui::Pos2, Color32) = match tab {
                Tab::Media => ui::icon_home,
                Tab::Shelf => ui::icon_tray,
                Tab::Clock => ui::icon_clock,
                _ => ui::icon_speaker,
            };
            icon(&p, r.center(), fade(col, a));
            if resp.clicked() {
                self.tab = *tab;
            }
        }
        // page content fades in after a tab switch
        let ease = (self.tab_at.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0);
        let content = a * ease * (2.0 - ease);

        // right side: privacy, battery / stats, settings
        let right = o.x + OPEN_W - 14.0;
        let gear = Rect::from_center_size(pos2(right - 2.0, o.y + 16.0), vec2(24.0, 22.0));
        let g = hit(ui, "gear", gear);
        let on = self.tab == Tab::Settings;
        if on || g.hovered() {
            p.rect_filled(gear, 12.0, fade(fill(if on { 46 } else { 24 }), a));
        }
        ui::icon_gear(&p, gear.center(), fade(if on || g.hovered() { Color32::WHITE } else { GRAY }, a));
        if g.clicked() {
            self.tab = if on { Tab::Media } else { Tab::Settings };
        }

        if on {
            let q = Rect::from_center_size(pos2(right - 52.0, o.y + 16.0), vec2(44.0, 22.0));
            let r = hit(ui, "quit", q);
            let red = if r.hovered() { Color32::from_rgb(255, 99, 88) } else { RED };
            p.text(q.center(), Align2::CENTER_CENTER, "Quit", bold(12.0), fade(red, a));
            if r.clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else {
            let s = &self.monitor.stats;
            let mut x = right - 28.0;
            if let Some(b) = s.battery {
                ui::battery_gauge(&p, pos2(x - 24.0, o.y + 16.0), b, s.charging, a);
                p.text(pos2(x - 31.0, o.y + 16.0), Align2::RIGHT_CENTER, format!("{b}%"), bold(12.0), fade(Color32::WHITE, a));
                x -= 76.0;
            } else {
                p.text(pos2(x, o.y + 16.0), Align2::RIGHT_CENTER, format!("CPU {:.0}%  RAM {:.0}%", s.cpu, s.ram), font(11.0), fade(GRAY, a));
                x -= 126.0;
            }
            if self.settings.privacy_dots {
                for (on, col, is_mic) in [(s.mic, GREEN, true), (s.cam, ORANGE, false)] {
                    if on {
                        if is_mic {
                            ui::icon_mic(&p, pos2(x, o.y + 16.0), fade(col, a));
                        } else {
                            p.circle_filled(pos2(x, o.y + 16.0), 4.0, fade(col, a));
                        }
                        x -= 20.0;
                    }
                }
            }
        }

        match self.tab {
            Tab::Media => self.tab_media(ui, o, m, content, time),
            Tab::Shelf => self.tab_shelf(ui, o, content),
            Tab::Clock => tab_clock(&p, o, content),
            Tab::Mixer => self.tab_mixer(ui, o, content),
            Tab::Settings => self.tab_settings(ui, o, content),
        }
    }

    fn tab_media(&mut self, ui: &mut egui::Ui, o: egui::Pos2, m: &media::State, a: f32, time: f32) {
        let p = ui.painter().clone();
        let accent = self.accent();
        // art shrinks to 85% while paused and its glow fades, like Atoll
        let play = ui.ctx().animate_bool_with_time(ui.scope_id().with("play"), m.playing, 0.3);
        let side = 72.0 * (0.85 + 0.15 * play);
        let art = Rect::from_center_size(o + vec2(16.0 + 36.0, 40.0 + 36.0), Vec2::splat(side));
        if self.art_tex.is_some() {
            ui::glow(&p, art.center(), 58.0, fade(accent, 0.42 * play * a));
        }
        match &self.art_tex {
            Some((_, tex, _)) => egui::Image::new(tex).corner_radius(11).tint(fade(Color32::WHITE, a)).paint_at(ui, art),
            None => {
                p.rect_filled(art, 11.0, fade(fill(14), a));
                ui::icon_note(&p, art.center(), fade(DIM, a), 1.8);
            }
        }
        p.rect_stroke(art, 11.0, Stroke::new(1.0, fade(fill(28), a)), StrokeKind::Inside);

        let x0 = o.x + 100.0;
        let x1 = o.x + OPEN_W - 16.0;
        let (title, artist) = if m.title.is_empty() { ("Not playing", "") } else { (m.title.as_str(), m.artist.as_str()) };
        ui::marquee(&p, pos2(x0, o.y + 40.0), x1 - x0, title, bold(14.0), fade(Color32::WHITE, a), time);
        ui::marquee(&p, pos2(x0, o.y + 58.0), x1 - x0, artist, font(12.0), fade(GRAY, a), time);

        if m.dur > 0.0 {
            let cy = o.y + 88.0;
            let bar = Rect::from_x_y_ranges(x0 + 36.0..=x1 - 40.0, cy - 6.0..=cy + 6.0);
            if let Some(f) = seek_bar(ui, bar, m.pos / m.dur, a) {
                self.media.send(media::Cmd::Seek(f * m.dur));
            }
            p.text(pos2(x0, cy), Align2::LEFT_CENTER, fmt_time(m.pos), font(11.0), fade(GRAY, a));
            p.text(pos2(x1, cy), Align2::RIGHT_CENTER, format!("-{}", fmt_time((m.dur - m.pos).max(0.0))), font(11.0), fade(GRAY, a));
        }

        // 30px capsule buttons, 40px for play, 8px apart
        let cy = o.y + 112.0;
        let cx = (x0 + x1) / 2.0;
        let control = |ui: &mut egui::Ui, name: &str, dx: f32, size: f32, icon: fn(&egui::Painter, egui::Pos2, Color32)| {
            let r = Rect::from_center_size(pos2(cx + dx, cy), Vec2::splat(size));
            let resp = hit(ui, name, r);
            if resp.hovered() {
                p.circle_filled(r.center(), size / 2.0, fade(fill(26), a));
            }
            icon(&p, r.center(), fade(Color32::WHITE, a));
            resp.clicked()
        };
        if control(ui, "prev", -38.0, 28.0, ui::icon_prev) {
            self.media.send(media::Cmd::Prev);
        }
        let toggled = if m.playing { control(ui, "toggle", 0.0, 36.0, ui::icon_pause) } else { control(ui, "toggle", 0.0, 36.0, ui::icon_play) };
        if toggled {
            self.media.send(media::Cmd::Toggle);
        }
        if control(ui, "next", 38.0, 28.0, ui::icon_next) {
            self.media.send(media::Cmd::Next);
        }

        if let Some(l) = &m.lyrics {
            let idx = l.partition_point(|(t, _)| *t <= m.pos + 0.25);
            let text = idx.checked_sub(1).map(|i| l[i].1.as_str()).filter(|s| !s.is_empty()).unwrap_or("♪");
            let w = OPEN_W - 100.0;
            let tw = p.layout_no_wrap(text.to_owned(), bold(13.0), Color32::WHITE).size().x.min(w);
            ui::marquee(&p, pos2(o.x + (OPEN_W - tw) / 2.0, o.y + 140.0), w, text, bold(12.0), fade(Color32::WHITE, a), time);
        }
    }

    fn tab_shelf(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        let area = Rect::from_min_size(o + vec2(16.0, 38.0), vec2(OPEN_W - 32.0, 86.0));
        let dragging = ui.input(|i| !i.raw.hovered_files.is_empty());
        if self.shelf.is_empty() || dragging {
            let col = if dragging { BLUE } else { DIM };
            ui::dashed_rrect(&p, area, 12.0, Stroke::new(1.5, fade(col, a)));
            ui::icon_tray(&p, area.center() - vec2(0.0, 10.0), fade(col, a));
            p.text(area.center() + vec2(0.0, 12.0), Align2::CENTER_CENTER, "Drop files here", bold(12.0), fade(col, a));
            return;
        }

        let mut remove = None;
        let visible = 5;
        self.shelf_scroll = self.shelf_scroll.min(self.shelf.len().saturating_sub(visible));
        for (slot, idx) in (self.shelf_scroll..self.shelf.len()).take(visible).enumerate() {
            let path = &self.shelf[idx];
            let tile = Rect::from_min_size(area.min + vec2(slot as f32 * 82.0, 0.0), vec2(78.0, 84.0));
            let resp = hit(ui, &format!("shelf{idx}"), tile);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
            let hue = ext.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
            let base = hsv(hue as f32, 0.45, if resp.hovered() { 0.95 } else { 0.8 });
            let sq = Rect::from_min_size(tile.min + vec2(19.0, 2.0), Vec2::splat(40.0));
            ui::tile(&p, sq, fade(base, a));
            let label = if path.is_dir() { "DIR".to_owned() } else { ext.chars().take(4).collect::<String>() };
            p.text(sq.center(), Align2::CENTER_CENTER, label, bold(11.0), fade(Color32::from_black_alpha(190), a));
            p.with_clip_rect(tile).text(pos2(tile.center().x, tile.top() + 48.0), Align2::CENTER_TOP, shorten(&name, 12), font(10.5), fade(Color32::from_gray(220), a));

            if resp.hovered() {
                let x = Rect::from_center_size(sq.right_top() + vec2(-1.0, 1.0), Vec2::splat(16.0));
                let xr = hit(ui, &format!("shelfx{idx}"), x);
                p.circle_filled(x.center(), 8.0, fade(if xr.hovered() { Color32::from_gray(96) } else { Color32::from_gray(56) }, a));
                let c = x.center();
                for (d1, d2) in [(vec2(-2.5, -2.5), vec2(2.5, 2.5)), (vec2(-2.5, 2.5), vec2(2.5, -2.5))] {
                    p.line_segment([c + d1, c + d2], Stroke::new(1.4, fade(Color32::WHITE, a)));
                }
                if xr.clicked() {
                    remove = Some(idx);
                    continue;
                }
            }
            if resp.clicked() {
                spotlight::launch(path);
            }
            if resp.secondary_clicked() {
                remove = Some(idx);
            }
        }
        if let Some(i) = remove {
            self.shelf.remove(i);
        }
    }

    fn tab_mixer(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        if self.mixer.is_none() {
            self.mixer = mixer::Mixer::new().ok();
        }
        let p = ui.painter().clone();
        let Some(mx) = self.mixer.as_mut() else {
            p.text(o + vec2(OPEN_W / 2.0, 80.0), Align2::CENTER_CENTER, "Audio unavailable", bold(13.0), fade(GRAY, a));
            return;
        };
        // sliders show their dragged value, so only poll when no button is held
        if self.mixer_at.elapsed() > Duration::from_millis(400) && !ui.input(|i| i.pointer.any_down()) {
            self.mixer_at = Instant::now();
            let _ = mx.refresh();
        }

        let sx0 = o.x + 100.0;
        let sx1 = o.x + OPEN_W - 52.0;
        let row = |ui: &mut egui::Ui, y: f32, label: &str, value: f32, id: &str, strong: bool| -> Option<f32> {
            let clip = Rect::from_x_y_ranges(o.x + 14.0..=sx0 - 8.0, o.y..=o.y + 200.0);
            let (tint, face) = if strong { (Color32::WHITE, bold(12.0)) } else { (Color32::from_gray(200), font(12.0)) };
            ui.painter().with_clip_rect(clip).text(pos2(o.x + 16.0, y), Align2::LEFT_CENTER, label, face, fade(tint, a));
            let out = ui::slider(ui, ui.scope_id().with(id), Rect::from_x_y_ranges(sx0..=sx1, y - 3.0..=y + 3.0), value, a);
            ui.painter().text(pos2(o.x + OPEN_W - 16.0, y), Align2::RIGHT_CENTER, format!("{:.0}%", out.unwrap_or(value) * 100.0), font(11.0), fade(GRAY, a));
            out
        };

        if let Some(v) = row(ui, o.y + 50.0, "Master", mx.master, "master", true) {
            mx.set_master(v);
        }
        self.mixer_scroll = self.mixer_scroll.min(mx.apps.len().saturating_sub(3));
        if mx.apps.is_empty() {
            p.text(o + vec2(OPEN_W / 2.0, 88.0), Align2::CENTER_CENTER, "No apps playing audio", font(12.0), fade(DIM, a));
        }
        let mut changed = None;
        for (slot, i) in (self.mixer_scroll..mx.apps.len()).take(3).enumerate() {
            let (name, level) = (shorten(&mx.apps[i].name, 12), mx.apps[i].level);
            if let Some(v) = row(ui, o.y + 76.0 + slot as f32 * 22.0, &name, level, &format!("app{i}"), false) {
                changed = Some((i, v));
            }
        }
        if let Some((i, v)) = changed {
            mx.set_app(i, v);
        }
    }

    fn tab_settings(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let before = self.settings.clone();
        let s = &mut self.settings;
        let items: [(&str, &mut bool); 8] = [
            ("Hover to expand", &mut s.hover_expand),
            ("Always on top", &mut s.always_on_top),
            ("Hide on fullscreen", &mut s.hide_fullscreen),
            ("Synced lyrics", &mut s.lyrics),
            ("Clipboard toast", &mut s.clipboard_toast),
            ("Privacy indicators", &mut s.privacy_dots),
            ("Spotlight (Alt+Space)", &mut s.spotlight),
            ("Start with Windows", &mut s.autostart),
        ];
        // two inset groups of four rows, like System Settings
        const ROW: f32 = 23.0;
        let gw = (OPEN_W - 40.0) / 2.0;
        for col in 0..2 {
            let group = Rect::from_min_size(o + vec2(16.0 + col as f32 * (gw + 8.0), 36.0), vec2(gw, ROW * 4.0));
            ui.painter().rect_filled(group, 9.0, fade(fill(15), a));
            for row in 1..4 {
                let y = group.top() + row as f32 * ROW;
                ui.painter().hline(group.left() + 10.0..=group.right(), y, Stroke::new(1.0, fade(fill(18), a)));
            }
        }
        for (i, (label, value)) in items.into_iter().enumerate() {
            let (col, row) = (i / 4, i % 4);
            let left = o.x + 16.0 + col as f32 * (gw + 8.0);
            let y = o.y + 36.0 + (row as f32 + 0.5) * ROW;
            ui.painter().text(pos2(left + 10.0, y), Align2::LEFT_CENTER, label, font(12.0), fade(Color32::WHITE, a));
            if ui::toggle(ui, ui.scope_id().with(("set", i)), pos2(left + gw - 10.0 - 15.0, y), *value, a) {
                *value = !*value;
            }
        }
        if self.settings != before {
            let ctx = ui.ctx().clone();
            self.apply_settings(&ctx, &before);
        }
    }

    fn draw_spotlight(&mut self, ui: &mut egui::Ui, rect: Rect, a: f32) {
        let ctx = ui.ctx().clone();
        let o = rect.min;
        let p = ui.painter().clone();

        let (down, up, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let n = self.spot.hits.len();
        if n > 0 {
            if down {
                self.spot.sel = (self.spot.sel + 1) % n;
            }
            if up {
                self.spot.sel = (self.spot.sel + n - 1) % n;
            }
        }
        if esc {
            self.spot.open = false;
        }
        if enter && let Some(h) = self.spot.hits.get(self.spot.sel).cloned() {
            run_hit(&h);
            self.spot.open = false;
        }

        ui::icon_search(&p, o + vec2(28.0, 25.0), fade(GRAY, a));
        let edit_rect = Rect::from_min_size(o + vec2(50.0, 8.0), vec2(SPOT_W - 70.0, 36.0));
        let mut query = std::mem::take(&mut self.spot.query);
        let resp = ui.put(
            edit_rect,
            egui::TextEdit::singleline(&mut query)
                .font(font(20.0))
                .frame(egui::Frame::NONE)
                .desired_width(edit_rect.width())
                .hint_text(egui::RichText::new("Spotlight Search").color(fade(DIM, a)))
                .text_color(fade(Color32::WHITE, a))
                .align(egui::Align2::LEFT_CENTER),
        );
        if !resp.has_focus() {
            resp.request_focus();
        }
        if query != self.spot.query {
            self.spot.hits = spotlight::search(&self.index, &query);
            self.spot.sel = 0;
        }
        self.spot.query = query;

        if self.spot.hits.is_empty() {
            return;
        }
        p.line_segment([o + vec2(0.0, 52.0), o + vec2(SPOT_W, 52.0)], Stroke::new(1.0, fade(fill(22), a)));
        let mut clicked = None;
        for (i, h) in self.spot.hits.iter().enumerate() {
            let row = Rect::from_min_size(o + vec2(8.0, 58.0 + i as f32 * 40.0), vec2(SPOT_W - 16.0, 36.0));
            let selected = i == self.spot.sel;
            if selected {
                p.rect_filled(row, 9.0, fade(BLUE, a));
            }
            let ic = Rect::from_center_size(pos2(row.left() + 22.0, row.center().y), Vec2::splat(24.0));
            let (bg, glyph): (Color32, fn(&egui::Painter, egui::Pos2, Color32)) = match h.kind {
                spotlight::Kind::Calc => (ORANGE, ui::icon_calc),
                spotlight::Kind::App => (BLUE, ui::icon_app),
                spotlight::Kind::File => (Color32::from_gray(99), ui::icon_file),
            };
            ui::tile(&p, ic, fade(if selected { fill(46) } else { bg }, a));
            glyph(&p, ic.center(), fade(Color32::WHITE, a));
            let sub = if selected { fill(190) } else { GRAY };
            let q = p.with_clip_rect(Rect::from_x_y_ranges(row.left() + 42.0..=row.right() - 92.0, row.top()..=row.bottom()));
            let y = if h.detail.is_empty() { row.center().y - 8.0 } else { row.top() + 4.0 };
            q.text(pos2(row.left() + 42.0, y), Align2::LEFT_TOP, &h.title, bold(13.0), fade(Color32::WHITE, a));
            if !h.detail.is_empty() {
                q.text(pos2(row.left() + 42.0, row.top() + 20.0), Align2::LEFT_TOP, &h.detail, font(10.0), fade(sub, a));
            }
            let kind = match h.kind {
                spotlight::Kind::Calc => "Copy result",
                spotlight::Kind::App => "Application",
                spotlight::Kind::File => "File",
            };
            p.text(pos2(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, kind, font(10.0), fade(if selected { fill(190) } else { DIM }, a));

            if ui.interact(row, ui.scope_id().with(("hit", i)), Sense::click()).clicked() {
                clicked = Some(h.clone());
            }
        }
        if let Some(h) = clicked {
            run_hit(&h);
            self.spot.open = false;
        }
    }
}

fn run_hit(h: &spotlight::Hit) {
    if h.kind == spotlight::Kind::Calc {
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(h.title.clone());
        }
    } else {
        spotlight::launch(&h.path);
    }
}

fn tab_clock(p: &egui::Painter, o: egui::Pos2, a: f32) {
    let now = Local::now();
    p.text(o + vec2(20.0, 34.0), Align2::LEFT_TOP, now.format("%H:%M").to_string(), font(44.0), fade(Color32::WHITE, a));
    p.text(o + vec2(134.0, 54.0), Align2::LEFT_TOP, format!("{:02}", now.second()), font(16.0), fade(GRAY, a));
    p.text(o + vec2(22.0, 92.0), Align2::LEFT_TOP, now.format("%A").to_string().to_uppercase(), bold(11.0), fade(RED, a));
    p.text(o + vec2(22.0, 106.0), Align2::LEFT_TOP, now.format("%d %B %Y").to_string(), font(13.0), fade(Color32::WHITE, a));

    // current week, Monday first, Calendar-widget style: red month name, red today disc
    let gx = o.x + OPEN_W - 16.0 - 182.0;
    p.text(pos2(gx + 2.0, o.y + 44.0), Align2::LEFT_CENTER, now.format("%B").to_string(), bold(12.0), fade(RED, a));
    let today = now.date_naive();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    for (i, letter) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        let day = monday + chrono::Duration::days(i as i64);
        let c = pos2(gx + 13.0 + i as f32 * 26.0, o.y + 88.0);
        p.text(pos2(c.x, o.y + 66.0), Align2::CENTER_CENTER, letter, bold(9.0), fade(DIM, a));
        if day == today {
            p.circle_filled(c, 11.0, fade(RED, a));
        }
        let col = if day == today { Color32::WHITE } else if i >= 5 { GRAY } else { Color32::from_gray(225) };
        p.text(c, Align2::CENTER_CENTER, day.day(), if day == today { bold(12.0) } else { font(12.0) }, fade(col, a));
    }
}

// ---------------------------------------------------------------- helpers

fn hit(ui: &mut egui::Ui, key: &str, rect: Rect) -> Response {
    ui.interact(rect, ui.scope_id().with(key), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Seek bar. Returns the target fraction when the user clicks or releases a drag.
fn seek_bar(ui: &mut egui::Ui, rect: Rect, frac: f64, a: f32) -> Option<f64> {
    let resp = ui.interact(rect, ui.scope_id().with("seek"), Sense::click_and_drag());
    let frac_at = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
    let dragging = resp.dragged().then(|| resp.interact_pointer_pos()).flatten().map(|p| frac_at(p.x));
    let frac = dragging.unwrap_or(frac).clamp(0.0, 1.0) as f32;

    let h = ui.ctx().animate_value_with_time(resp.id.with("h"), if resp.hovered() || resp.dragged() { 8.0 } else { 4.0 }, 0.12);
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), h));
    let p = ui.painter();
    p.rect_filled(track, h / 2.0, fade(fill(40), a));
    p.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * frac, h)), h / 2.0, fade(Color32::from_gray(235), a));

    if resp.drag_stopped() || resp.clicked() {
        return resp.interact_pointer_pos().map(|p| frac_at(p.x));
    }
    None
}

fn fmt_time(s: f64) -> String {
    let s = s as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max - 1).chain(['…']).collect()
}

fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let f = |n: f32| {
        let k = (n + h / 60.0) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    Color32::from_rgb((f(5.0) * 255.0) as u8, (f(3.0) * 255.0) as u8, (f(1.0) * 255.0) as u8)
}

/// Mean art colour, brightened so it stays readable on black.
fn accent(a: &media::Art) -> Color32 {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for px in a.rgba.chunks_exact(4).step_by(7) {
        r += px[0] as u64;
        g += px[1] as u64;
        b += px[2] as u64;
        n += 1;
    }
    let n = n.max(1);
    let (r, g, b) = (r / n, g / n, b / n);
    let k = 235.0 / r.max(g).max(b).max(1) as f32;
    Color32::from_rgb((r as f32 * k).min(255.0) as u8, (g as f32 * k).min(255.0) as u8, (b as f32 * k).min(255.0) as u8)
}

fn register_hotkey(ctx: &egui::Context, enabled: bool) -> Option<(GlobalHotKeyManager, HotKey)> {
    let mgr = GlobalHotKeyManager::new().ok()?;
    let hk = HotKey::new(Some(HkMods::ALT), Code::Space);
    if enabled {
        mgr.register(hk).ok()?;
    }
    let ctx = ctx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            HOTKEY_PRESSED.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint();
    }));
    Some((mgr, hk))
}

fn make_tray() -> (Option<TrayIcon>, Option<MenuId>, Option<MenuId>) {
    // brand mark, 32 px small cut (transparent: reads on light and dark taskbars)
    let icon = image::load_from_memory(include_bytes!("../assets/brand/symbol-32.png"))
        .ok()
        .and_then(|i| tray_icon::Icon::from_rgba(i.to_rgba8().into_raw(), 32, 32).ok());
    let menu = Menu::new();
    let settings = MenuItem::new("Settings", true, None);
    let quit = MenuItem::new("Quit", true, None);
    if menu.append_items(&[&settings, &quit]).is_err() {
        return (None, None, None);
    }
    let mut b = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Camel");
    if let Some(icon) = icon {
        b = b.with_icon(icon);
    }
    (b.build().ok(), Some(settings.id().clone()), Some(quit.id().clone()))
}

/// SF Pro is not redistributable, so it is read from a `fonts/` folder (next to the exe, two levels up for
/// `cargo run`, or the source tree) and only used if found. Open Sans (OFL) is embedded as the fallback,
/// egui's default fonts stay behind both as glyph fallback.
fn setup_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontFamily};
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let dirs = [exe.join("fonts"), exe.join("../../fonts"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts")];
    let load = |file: &str| dirs.iter().find_map(|d| std::fs::read(d.join(file)).ok());
    let regular = load("SF-Pro-Display-Regular.otf").map(FontData::from_owned);
    let semibold = load("SF-Pro-Display-Semibold.otf").map(FontData::from_owned);

    let mut fonts = egui::FontDefinitions::default();
    let mut bold = fonts.families[&FontFamily::Proportional].clone();
    let regular = regular.unwrap_or_else(|| FontData::from_static(include_bytes!("../assets/OpenSans-Regular.ttf")));
    let semibold = semibold.unwrap_or_else(|| FontData::from_static(include_bytes!("../assets/OpenSans-SemiBold.ttf")));
    fonts.font_data.insert("ui-regular".into(), Arc::new(regular));
    fonts.font_data.insert("ui-bold".into(), Arc::new(semibold));
    fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "ui-regular".into());
    bold.insert(0, "ui-bold".into());
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
}

/// Windows 11 draws a 1px border and rounded corners around every top-level window, which outlines the
/// transparent margins. Ask DWM for neither.
fn strip_window_frame() {
    use windows::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::w;
    let Ok(hwnd) = (unsafe { FindWindowW(None, w!("Camel")) }) else { return };
    let corners = DWMWCP_DONOTROUND;
    let no_border = 0xFFFF_FFFEu32; // DWMWA_COLOR_NONE
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &raw const corners as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &raw const no_border as *const _, 4);
    }
}

fn main() -> eframe::Result {
    let settings = Settings::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(vec2(PILL_W, PILL_H))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false);
    if settings.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    eframe::run_native(
        "Camel",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            strip_window_frame();
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Notch::new(&cc.egui_ctx)))
        }),
    )
}
