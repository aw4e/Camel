use crate::notch::Notch;
use crate::prelude::*;

impl Notch {
    pub(crate) fn tab_settings(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let before = self.settings.clone();
        let s = &mut self.settings;
        let groups: [(&str, [(&str, &mut bool); 5]); 2] = [
            (
                "GENERAL",
                [
                    ("Start with Windows", &mut s.autostart),
                    ("Hover to expand", &mut s.hover_expand),
                    ("Always on top", &mut s.always_on_top),
                    ("Hide on fullscreen", &mut s.hide_fullscreen),
                    ("Check for updates", &mut s.auto_update),
                ],
            ),
            (
                "FEATURES",
                [
                    ("Synced lyrics", &mut s.lyrics),
                    ("Clipboard history", &mut s.clipboard_toast),
                    ("Privacy indicators", &mut s.privacy_dots),
                    ("Volume HUD", &mut s.volume_hud),
                    ("Spotlight (Alt+Space)", &mut s.spotlight),
                ],
            ),
        ];
        const ROW: f32 = 21.0;
        let gw = (OPEN_W - 2.0 * MARGIN - GAP) / 2.0;
        for (col, (title, items)) in groups.into_iter().enumerate() {
            let left = o.x + MARGIN + col as f32 * (gw + GAP);
            ui.painter().text(pos2(left + 4.0, o.y + 36.0), Align2::LEFT_CENTER, title, bold(9.5), fade(DIM, a));
            let group = Rect::from_min_size(pos2(left, o.y + 45.0), vec2(gw, ROW * 5.0));
            ui.painter().rect_filled(group, 9.0, fade(fill(15), a));
            for (row, (label, value)) in items.into_iter().enumerate() {
                let y = group.top() + (row as f32 + 0.5) * ROW;
                if row > 0 {
                    ui.painter().hline(group.left() + ROW_PAD..=group.right(), group.top() + row as f32 * ROW, Stroke::new(1.0, fade(fill(16), a)));
                }
                ui.painter().text(pos2(left + ROW_PAD, y), Align2::LEFT_CENTER, label, font(12.0), fade(Color32::WHITE, a));
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
