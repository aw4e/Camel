use crate::app::Notch;
use crate::prelude::*;

/// The Settings page: three groups, one shown at a time, each a short list of switches in two columns.
const PAGES: [&str; 3] = ["General", "Media and data", "Alerts"];
const ROW: f32 = 21.0;

impl Notch {
    pub(crate) fn tab_settings(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();

        let mut x = o.x + MARGIN;
        for (i, name) in PAGES.iter().enumerate() {
            let galley = p.layout_no_wrap((*name).to_owned(), bold(11.0), Color32::WHITE);
            let chip = Rect::from_min_size(pos2(x, o.y + TOP), vec2(galley.size().x + 2.0 * GAP_L, 20.0));
            let resp = hit(ui, &format!("settings-page{i}"), chip);
            let chosen = self.settings_page == i;
            p.rect_filled(
                chip,
                10.0,
                fade(
                    if chosen {
                        fill(36)
                    } else if resp.hovered() {
                        fill(20)
                    } else {
                        fill(14)
                    },
                    a,
                ),
            );
            p.text(chip.center(), Align2::CENTER_CENTER, name, bold(11.0), fade(if chosen || resp.hovered() { Color32::WHITE } else { GRAY }, a));
            if resp.clicked() {
                self.settings_page = i;
            }
            x = chip.right() + GAP_S;
        }

        let before = self.settings.clone();
        let s = &mut self.settings;
        let items: Vec<(&str, &mut bool)> = match self.settings_page {
            0 => vec![
                ("Start with Windows", &mut s.autostart),
                ("Hover to expand", &mut s.hover_expand),
                ("Always on top", &mut s.always_on_top),
                ("Hide on fullscreen", &mut s.hide_fullscreen),
                ("Spotlight (Alt+Space)", &mut s.spotlight),
                ("Check for updates", &mut s.auto_update),
            ],
            1 => vec![("Synced lyrics", &mut s.lyrics), ("Weather", &mut s.weather), ("Clipboard history", &mut s.clipboard_toast)],
            _ => vec![
                ("Privacy indicators", &mut s.privacy_dots),
                ("Volume HUD", &mut s.volume_hud),
                ("Caps Lock and language", &mut s.key_toasts),
                ("Network and audio alerts", &mut s.device_toasts),
            ],
        };

        let gw = (OPEN_W - 2.0 * MARGIN - GAP) / 2.0;
        let per_column = items.len().div_ceil(2);
        let mut columns: [Vec<(&str, &mut bool)>; 2] = [Vec::new(), Vec::new()];
        for (i, item) in items.into_iter().enumerate() {
            columns[usize::from(i >= per_column)].push(item);
        }
        for (col, rows) in columns.into_iter().enumerate() {
            if rows.is_empty() {
                continue;
            }
            let left = o.x + MARGIN + col as f32 * (gw + GAP);
            let group = Rect::from_min_size(pos2(left, o.y + APPS_TOP), vec2(gw, ROW * rows.len() as f32));
            p.rect_filled(group, 9.0, fade(fill(15), a));
            for (row, (label, value)) in rows.into_iter().enumerate() {
                let y = group.top() + (row as f32 + 0.5) * ROW;
                if row > 0 {
                    p.hline(group.left() + ROW_PAD..=group.right(), group.top() + row as f32 * ROW, Stroke::new(1.0, fade(fill(16), a)));
                }
                p.text(pos2(left + ROW_PAD, y), Align2::LEFT_CENTER, label, font(12.0), fade(Color32::WHITE, a));
                if ui::toggle(ui, ui.scope_id().with(("set", col, row)), pos2(left + gw - ROW_PAD - 14.0, y), *value, a) {
                    *value = !*value;
                }
            }
        }
        if self.settings_page == 1 {
            // under the second column's switch: where the sound of all apps is played
            let left = o.x + MARGIN + gw + GAP;
            let row = Rect::from_min_size(pos2(left, o.y + APPS_TOP + ROW), vec2(gw, ROW));
            p.rect_filled(row, 9.0, fade(fill(15), a));
            p.hline(row.left() + ROW_PAD..=row.right(), row.top(), Stroke::new(1.0, fade(fill(16), a)));
            let resp = hit(ui, "route", row);
            let (shown, dot) = match (self.settings.route_to.as_str(), self.router.as_ref().map(crate::router::Router::status)) {
                ("", _) => ("Off".to_owned(), None),
                (id, status) => {
                    let name = self.route_devices.iter().find(|(d, _)| d == id).map_or("Not connected", |(_, n)| n.as_str());
                    let dot = match status {
                        Some(crate::router::Status::Running(_)) => GREEN,
                        Some(crate::router::Status::SameDevice) => ORANGE,
                        Some(crate::router::Status::Failed(_)) => RED,
                        _ => GRAY,
                    };
                    (shorten(name, 14), Some(dot))
                }
            };
            p.text(pos2(row.left() + ROW_PAD, row.center().y), Align2::LEFT_CENTER, "Mix to", font(12.0), fade(Color32::WHITE, a));
            let color = if resp.hovered() { Color32::WHITE } else { GRAY };
            let end = pos2(row.right() - ROW_PAD, row.center().y);
            let text = p.text(end, Align2::RIGHT_CENTER, shown, font(11.0), fade(color, a));
            if let Some(dot) = dot {
                p.circle_filled(pos2(text.left() - 8.0, end.y), 3.0, fade(dot, a));
            }
            let tip = match self.router.as_ref().map(crate::router::Router::status) {
                Some(crate::router::Status::Running(n)) => format!("Playing {n} app(s) through this output. Click for the next output."),
                Some(crate::router::Status::SameDevice) => {
                    "This is also your default output, so nothing is routed. Make another output the default in Windows.".to_owned()
                }
                Some(crate::router::Status::Failed(e)) => e,
                _ => {
                    "Plays everything through another output. Set an output nobody listens to as the Windows default, and pick your headphones here.".to_owned()
                }
            };
            let resp = resp.on_hover_text(tip);
            if resp.clicked() {
                let at = self.route_devices.iter().position(|(d, _)| *d == self.settings.route_to);
                let next = match (self.settings.route_to.is_empty(), at) {
                    (true, _) => 0,
                    (false, Some(i)) => i + 1,
                    (false, None) => 0,
                };
                self.settings.route_to = self.route_devices.get(next).map(|(d, _)| d.clone()).unwrap_or_default();
            }
        }
        if self.settings != before {
            let ctx = ui.ctx().clone();
            self.apply_settings(&ctx, &before);
        }
    }
}
