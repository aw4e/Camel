use crate::app::{GROUPS, MAIN_TABS, Notch, Tab, ToastIcon, group_of, tab_icon, tab_label, tab_name};
use crate::prelude::*;
use crate::{media, update};
use chrono::Local;

impl Notch {
    pub(crate) fn accent(&self) -> Color32 {
        self.art_tex.as_ref().map_or(Color32::from_gray(200), |a| a.2)
    }

    /// Orange while focusing, green during a break.
    pub(crate) fn phase_color(&self) -> Color32 {
        if self.focus.phase() == crate::focus::Phase::Focus { ORANGE } else { GREEN }
    }

    pub(crate) fn dots_active(&self) -> bool {
        let s = &self.monitor.stats;
        self.settings.privacy_dots && (s.mic || s.cam)
    }

    pub(crate) fn draw_compact(&self, ui: &mut egui::Ui, rect: Rect, m: &media::State, a: f32) {
        if self.settings.minimal {
            return;
        }
        let p = ui.painter().clone();
        let half = (PILL_W + if self.dots_active() { 30.0 } else { 0.0 }) / 2.0;
        let c = rect.center();
        let art = Rect::from_center_size(pos2(c.x - half + 21.0, c.y), Vec2::splat(22.0));
        match &self.art_tex {
            Some((_, tex, _)) => egui::Image::new(tex).corner_radius(5).tint(fade(Color32::WHITE, a)).paint_at(ui, art),
            None => {
                p.rect_filled(art, 5.0, fade(fill(16), a));
                ui::music_note(&p, art.center(), fade(GRAY, a), 0.8);
            }
        }
        let time = ui.input(|i| i.time) as f32;
        ui::equalizer(&p, pos2(c.x + half - 23.0, c.y), 4, m.playing, time, fade(self.accent(), a), 15.0);

        if !self.dots_active() {
            let running = self.focus.running();
            let (text, color) = if running {
                let left = self.focus.remaining(Instant::now()).as_secs_f32().ceil() as u64;
                (format!("{:02}:{:02}", left / 60, left % 60), self.phase_color())
            } else {
                (clock_text(&Local::now(), self.h24), Color32::WHITE.gamma_multiply(0.9))
            };
            let galley = p.layout_no_wrap(text, bold(12.0), fade(color, a));
            let half_w = galley.size().x / 2.0;
            p.galley(pos2(c.x - half_w, c.y - galley.size().y / 2.0), galley, color);
            if self.awake {
                let sun = pos2(c.x - half_w - 12.0, c.y);
                ui::scaled(&p, sun, 0.6, || ui::icon_sun(&p, sun, fade(Color32::from_rgb(255, 214, 10), a)));
            }
            // a ring beside the clock: the session while the timer runs, otherwise the battery
            let ring = if running {
                Some((self.focus.progress(Instant::now()), self.phase_color()))
            } else {
                self.monitor.stats.battery.map(|b| {
                    (
                        f32::from(b) / 100.0,
                        if self.monitor.stats.charging || b > 20 {
                            GREEN
                        } else if b > 15 {
                            ORANGE
                        } else {
                            RED
                        },
                    )
                })
            };
            if let Some((frac, col)) = ring {
                ui::ring(&p, pos2(c.x + half_w + 12.0, c.y), 5.5, frac, fade(col, a), fade(fill(30), a));
            }
        }
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

    /// Version number next to Quit. It turns into an Update button when a newer release was found.
    fn draw_version(&mut self, ui: &mut egui::Ui, right_center: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        let status = self.updater.status();
        let version = format!("v{}", update::VERSION);
        let (label, color, tip) = match &status {
            update::Status::Available(r) => {
                let pill = Rect::from_center_size(pos2(right_center.x - 22.0, right_center.y), vec2(44.0, 18.0));
                let resp = hit(ui, "update", pill).on_hover_text(format!("Install Camel {} and restart", r.version));
                p.rect_filled(pill, 9.0, fade(BLUE.gamma_multiply(if resp.hovered() { 1.0 } else { 0.8 }), a));
                p.text(pill.center(), Align2::CENTER_CENTER, "Update", bold(11.0), fade(Color32::WHITE, a));
                if resp.clicked() {
                    self.update_action();
                }
                return;
            }
            update::Status::Checking => ("Checking".to_owned(), DIM, "Looking for a newer version".to_owned()),
            update::Status::Installing => ("Installing".to_owned(), DIM, "Camel restarts when it is done".to_owned()),
            update::Status::Failed(e) => (version, ORANGE, format!("Update failed: {e}. Click to retry")),
            _ => (version, DIM, "Click to check for updates".to_owned()),
        };
        let galley = p.layout_no_wrap(label, font(10.5), fade(color, a));
        let area = Rect::from_min_size(pos2(right_center.x - galley.size().x - 4.0, right_center.y - 9.0), vec2(galley.size().x + 8.0, 18.0));
        let resp = hit(ui, "version", area).on_hover_text(tip);
        let col = if resp.hovered() { Color32::WHITE } else { color };
        p.galley(pos2(area.left() + 4.0, right_center.y - galley.size().y / 2.0), galley, fade(col, a));
        if resp.clicked() {
            self.update_action();
        }
    }

    pub(crate) fn draw_toast(&self, ui: &mut egui::Ui, rect: Rect, a: f32) {
        let Some(t) = &self.toast else { return };
        let p = ui.painter().clone();
        let o = rect.min;
        let icon = Rect::from_center_size(pos2(o.x + 26.0, o.y + TOAST.y / 2.0), Vec2::splat(28.0));
        let (bg, glyph): (Color32, fn(&egui::Painter, egui::Pos2, Color32)) = match t.icon {
            ToastIcon::Clipboard => (BLUE, ui::icon_clipboard),
            ToastIcon::Awake => (GREEN, ui::icon_sun),
            ToastIcon::Volume => (BLUE, ui::icon_speaker),
            ToastIcon::Timer => (ORANGE, ui::icon_timer),
            ToastIcon::Apps => (BLUE, ui::icon_app),
            ToastIcon::Keyboard => (Color32::from_rgb(94, 92, 230), ui::icon_keyboard),
            ToastIcon::Online => (GREEN, ui::icon_wifi),
            ToastIcon::Offline => (RED, ui::icon_wifi),
            ToastIcon::Battery => (GREEN, ui::icon_battery),
            ToastIcon::BatteryLow => (RED, ui::icon_battery),
        };
        ui::tile(&p, icon, fade(bg, a));
        glyph(&p, icon.center(), fade(Color32::WHITE, a));
        let clip = Rect::from_x_y_ranges(o.x + 48.0..=o.x + TOAST.x - 16.0, o.y..=o.y + TOAST.y);
        let q = p.with_clip_rect(clip);
        q.text(pos2(o.x + 48.0, o.y + 7.0), Align2::LEFT_TOP, &t.title, bold(12.0), fade(Color32::WHITE, a));
        if let Some(l) = t.level {
            q.text(pos2(o.x + TOAST.x - 16.0, o.y + 7.0), Align2::RIGHT_TOP, &t.body, font(11.0), fade(GRAY, a));
            let track = Rect::from_min_size(o + vec2(48.0, 28.0), vec2(TOAST.x - 48.0 - 16.0, 6.0));
            p.rect_filled(track, 3.0, fade(fill(30), a));
            p.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * l.clamp(0.0, 1.0), 6.0)), 3.0, fade(Color32::WHITE, a));
        } else {
            q.text(pos2(o.x + 48.0, o.y + 23.0), Align2::LEFT_TOP, &t.body, font(11.0), fade(GRAY, a));
        }
    }

    pub(crate) fn draw_open(&mut self, ui: &mut egui::Ui, rect: Rect, m: &media::State, a: f32) {
        let p = ui.painter().clone();
        let o = rect.min;
        let time = ui.input(|i| i.time) as f32;

        // The tab bar: four groups. A closed group is its first page's icon; the chosen group opens to show its pages and
        // the page you are on shows its name.
        let names: Vec<_> = MAIN_TABS.iter().map(|t| p.layout_no_wrap(tab_name(*t).to_owned(), bold(11.0), Color32::WHITE)).collect();
        let speed = if self.reduce_motion { 0.0 } else { 0.18 };
        let chosen_group = group_of(self.tab);
        let group_ids: Vec<usize> = MAIN_TABS.iter().map(|t| group_of(*t).unwrap_or(0)).collect();
        let widths: Vec<f32> = MAIN_TABS
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let g = group_ids[i];
                let target = if Some(g) == chosen_group {
                    if self.tab == *t { tab_open_w(names[i].size().x) } else { TAB_W }
                } else if GROUPS[g].1[0] == *t {
                    TAB_W
                } else {
                    0.0
                };
                ui.ctx().animate_value_with_time(ui.scope_id().with(("tabw", i)), target, speed)
            })
            .collect();
        let offsets = tab_offsets(&widths, &group_ids);
        let last = MAIN_TABS.len() - 1;
        let track_h = TAB_PILL_H + 2.0 * TAB_INSET;
        let track = Rect::from_min_size(o + vec2(12.0, 16.0 - track_h / 2.0), vec2(offsets[last] + widths[last] + 2.0 * TAB_INSET, track_h));
        p.rect_filled(track, track_h / 2.0, fade(fill(14), a));
        let tab_rect = |i: usize| Rect::from_min_size(track.min + vec2(TAB_INSET + offsets[i], TAB_INSET), vec2(widths[i], TAB_PILL_H));
        // a lighter backing behind the open group, and a thin line between groups
        if let Some(g) = chosen_group {
            let members: Vec<usize> = (0..MAIN_TABS.len()).filter(|i| group_ids[*i] == g).collect();
            let (first, end) = (tab_rect(members[0]), tab_rect(*members.last().unwrap_or(&0)));
            p.rect_filled(Rect::from_min_max(first.min, end.max).expand2(vec2(1.0, 0.0)), TAB_PILL_H / 2.0, fade(fill(22), a));
        }
        for (_, pages) in GROUPS.iter().skip(1) {
            let first = MAIN_TABS.iter().position(|t| *t == pages[0]).unwrap_or(0);
            let x = tab_rect(first).left() - GROUP_GAP / 2.0;
            p.line_segment([pos2(x, track.top() + 8.0), pos2(x, track.bottom() - 8.0)], Stroke::new(1.0, fade(fill(30), a)));
        }
        for (i, tab) in MAIN_TABS.iter().enumerate() {
            if widths[i] < 2.0 {
                continue;
            }
            let r = tab_rect(i);
            let g = group_ids[i];
            let in_open_group = Some(g) == chosen_group;
            let tip = if in_open_group { tab_label(*tab).to_owned() } else { format!("{}: {}", GROUPS[g].0, tab_label(self.group_last[g])) };
            let resp = hit(ui, &format!("tab{i}"), r).on_hover_text(tip);
            let chosen = self.tab == *tab;
            if chosen {
                p.rect_filled(r, TAB_PILL_H / 2.0, fade(fill(40), a));
            } else if resp.hovered() {
                p.rect_filled(r, TAB_PILL_H / 2.0, fade(fill(20), a));
            }
            let col = if chosen || resp.hovered() { Color32::WHITE } else { GRAY };
            let icon_at = pos2(r.left() + TAB_W / 2.0, r.center().y);
            ui::scaled(&p, icon_at, ui::icon_fit(TAB_PILL_H), || tab_icon(*tab)(&p, icon_at, fade(col, a)));
            // the name fades in as the pill opens
            let open = ((widths[i] - TAB_W) / (tab_open_w(names[i].size().x) - TAB_W)).clamp(0.0, 1.0);
            if open > 0.05 {
                let text_at = pos2(r.left() + TAB_W - GAP_S + 2.0, r.center().y - names[i].size().y / 2.0);
                p.with_clip_rect(r).galley(text_at, names[i].clone(), fade(Color32::WHITE, a * open * open));
            }
            if resp.clicked() {
                // a closed group returns to the page you last had open in it
                self.tab = if in_open_group { *tab } else { self.group_last[g] };
            }
        }
        // page content fades in after a tab switch
        let ease = (self.tab_at.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0);
        let content = a * ease * (2.0 - ease);

        let right = o.x + OPEN_W - 14.0;
        let gear = Rect::from_center_size(pos2(right - 2.0, o.y + 16.0), vec2(24.0, 22.0));
        let g = hit(ui, "gear", gear);
        let on = self.tab == Tab::Settings;
        if on || g.hovered() {
            p.rect_filled(gear, 12.0, fade(fill(if on { 46 } else { 24 }), a));
        }
        ui::scaled(&p, gear.center(), ui::icon_fit(gear.height()), || {
            ui::icon_gear(&p, gear.center(), fade(if on || g.hovered() { Color32::WHITE } else { GRAY }, a))
        });
        if g.clicked() {
            self.tab = if on { Tab::Media } else { Tab::Settings };
        }

        if on {
            let q = Rect::from_center_size(pos2(right - 44.0, o.y + 16.0), vec2(46.0, 18.0));
            let r = hit(ui, "quit", q);
            p.rect_filled(q, 9.0, fade(RED.gamma_multiply(if r.hovered() { 0.38 } else { 0.24 }), a));
            p.text(q.center(), Align2::CENTER_CENTER, "Quit", bold(11.0), fade(RED, a));
            if r.clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            self.draw_version(ui, pos2(q.left() - GAP, q.center().y), a);
        } else {
            let s = &self.monitor.stats;
            let dots = self.settings.privacy_dots && (s.mic || s.cam);
            let mut x = right - 28.0;
            if let Some(b) = s.battery {
                ui::battery_gauge(&p, pos2(x - 24.0, o.y + 16.0), b, s.charging, a);
                p.text(pos2(x - 31.0, o.y + 16.0), Align2::RIGHT_CENTER, format!("{b}%"), bold(12.0), fade(Color32::WHITE, a));
                x -= 76.0;
            } else if !dots {
                p.text(pos2(x, o.y + 10.0), Align2::RIGHT_CENTER, format!("CPU {:.0}%", s.cpu), font(9.5), fade(GRAY, a));
                p.text(pos2(x, o.y + 22.0), Align2::RIGHT_CENTER, format!("RAM {:.0}%", s.ram), font(9.5), fade(GRAY, a));
            }
            for (on, col, is_mic) in [(s.mic, GREEN, true), (s.cam, ORANGE, false)] {
                if dots && on {
                    if is_mic {
                        ui::icon_mic(&p, pos2(x, o.y + 16.0), fade(col, a));
                    } else {
                        p.circle_filled(pos2(x, o.y + 16.0), 4.0, fade(col, a));
                    }
                    x -= 20.0;
                }
            }
        }

        match self.tab {
            Tab::Media => self.tab_media(ui, o, m, content, time),
            Tab::Shelf => self.tab_shelf(ui, o, content),
            Tab::Clipboard => self.tab_clipboard(ui, o, content),
            Tab::Timer => self.tab_timer(ui, o, content),
            Tab::Apps => self.tab_apps(ui, o, content),
            Tab::Tasks => self.tab_tasks(ui, o, content),
            Tab::Controls => self.tab_controls(ui, o, content),
            Tab::Clock => super::pages::clock::tab_clock(&p, o, content, self.h24, &self.weather.status()),
            Tab::Mixer => self.tab_mixer(ui, o, content),
            Tab::Settings => self.tab_settings(ui, o, content),
        }
    }
}
