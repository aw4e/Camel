use crate::mixer;
use crate::notch::Notch;
use crate::prelude::*;

impl Notch {
    pub(crate) fn tab_mixer(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
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

        let sx0 = o.x + MARGIN + 84.0;
        let sx1 = o.x + OPEN_W - MARGIN - 36.0;
        let row = |ui: &mut egui::Ui, y: f32, label: &str, value: f32, id: &str, strong: bool| -> Option<f32> {
            let clip = Rect::from_x_y_ranges(o.x + MARGIN..=sx0 - GAP, o.y..=o.y + 200.0);
            let (tint, face) = if strong { (Color32::WHITE, bold(12.0)) } else { (Color32::from_gray(200), font(12.0)) };
            ui.painter().with_clip_rect(clip).text(pos2(o.x + MARGIN, y), Align2::LEFT_CENTER, label, face, fade(tint, a));
            let out = ui::slider(ui, ui.scope_id().with(id), Rect::from_x_y_ranges(sx0..=sx1, y - 3.0..=y + 3.0), value, a);
            ui.painter().text(pos2(o.x + OPEN_W - MARGIN, y), Align2::RIGHT_CENTER, format!("{:.0}%", out.unwrap_or(value) * 100.0), font(11.0), fade(GRAY, a));
            out
        };

        if let Some(v) = row(ui, o.y + TOP + 12.0, "Master", mx.master, "master", true) {
            mx.set_master(v);
        }
        self.mixer_scroll = self.mixer_scroll.min(mx.apps.len().saturating_sub(3));
        if mx.apps.is_empty() {
            p.text(o + vec2(OPEN_W / 2.0, 88.0), Align2::CENTER_CENTER, "No apps playing audio", font(12.0), fade(DIM, a));
        }
        let mut changed = None;
        for (slot, i) in (self.mixer_scroll..mx.apps.len()).take(3).enumerate() {
            let (name, level) = (shorten(&mx.apps[i].name, 12), mx.apps[i].level);
            if let Some(v) = row(ui, o.y + TOP + 38.0 + slot as f32 * ROW_PITCH, &name, level, &format!("app{i}"), false) {
                changed = Some((i, v));
            }
        }
        if let Some((i, v)) = changed {
            mx.set_app(i, v);
        }
    }
}
