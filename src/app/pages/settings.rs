use crate::app::Notch;
use crate::prelude::*;

/// The Settings page: three groups, one shown at a time, each a short list of switches in two columns.
const PAGES: [&str; 3] = ["General", "Media and data", "Alerts"];
const ROW: f32 = 21.0;

impl Notch {
    pub(crate) fn tab_settings(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();

        // the groups, as chips in a row
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

        // two columns, filled top to bottom
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
        if self.settings != before {
            let ctx = ui.ctx().clone();
            self.apply_settings(&ctx, &before);
        }
    }
}
