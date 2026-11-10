use crate::focus;
use crate::notch::Notch;
use crate::prelude::*;

impl Notch {
    pub(crate) fn tab_timer(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        let now = Instant::now();
        let running = self.focus.running();
        let left = self.focus.remaining(now).as_secs_f32().ceil() as u64;
        let shown = format!("{:02}:{:02}", left / 60, left % 60);

        // the time: click it (while stopped) and type a length, "30" or "25:30"
        let clock = Rect::from_min_size(o + vec2(MARGIN, TOP), vec2(phi_columns().0, 56.0));
        if let Some((mut buf, fresh)) = self.timer_edit.take() {
            let resp = ui.put(
                clock,
                egui::TextEdit::singleline(&mut buf)
                    .font(font(44.0))
                    .frame(egui::Frame::NONE)
                    .desired_width(clock.width())
                    .hint_text(egui::RichText::new(&shown).color(fade(DIM, a)))
                    .text_color(fade(Color32::WHITE, a))
                    .align(Align2::LEFT_CENTER),
            );
            if fresh {
                resp.request_focus();
            }
            let (enter, esc) = ui.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
            if esc {
                // cancelled: drop the text
            } else if enter || (resp.lost_focus() && !fresh) {
                if let Some(d) = focus::parse_duration(&buf) {
                    self.focus.set_duration(d);
                }
            } else {
                self.timer_edit = Some((buf, false));
            }
        } else {
            p.text(clock.min + vec2(0.0, 4.0), Align2::LEFT_TOP, &shown, font(44.0), fade(Color32::WHITE, a));
            if !running && hit(ui, "timeedit", clock).on_hover_text("Click to type a time").clicked() {
                self.timer_edit = Some((String::new(), true));
                // the overlay does not take keyboard focus by being clicked, so ask for it (Spotlight does the same)
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // length: two grouped controls (presets, stepper) and a plain reset, inside the wide golden column.
        // Each group is one track with segments, like the tab bar. Locked while a session runs.
        let track_y = o.y + TOP + 64.0;
        let segments = |ui: &mut egui::Ui, x: f32, seg_w: f32, labels: &[&str], selected: Option<usize>, key: &str, base: Color32| -> Option<usize> {
            let track = Rect::from_min_size(pos2(o.x + x, track_y), vec2(seg_w * labels.len() as f32 + 4.0, 24.0));
            p.rect_filled(track, 12.0, fade(fill(14), a));
            let mut clicked = None;
            for (i, label) in labels.iter().enumerate() {
                let r = Rect::from_min_size(track.min + vec2(2.0 + i as f32 * seg_w, 2.0), vec2(seg_w, 20.0));
                let resp = hit(ui, &format!("{key}{i}"), r);
                let chosen = selected == Some(i);
                let hot = resp.hovered() && !running;
                if chosen {
                    p.rect_filled(r, 10.0, fade(fill(36), a));
                } else if hot {
                    p.rect_filled(r, 10.0, fade(fill(18), a));
                }
                p.text(r.center(), Align2::CENTER_CENTER, label, bold(11.0), fade(if chosen || hot { Color32::WHITE } else { base }, a));
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
            clicked
        };
        let presets = [15u64, 25, 45];
        let current = presets.iter().position(|m| self.focus.total().as_secs() == m * 60);
        if let Some(i) = segments(ui, MARGIN, 38.0, &["15m", "25m", "45m"], current, "preset", GRAY) {
            self.focus.set_minutes(presets[i]);
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let step_x = MARGIN + 3.0 * 38.0 + 4.0 + GAP;
        if let Some(i) = segments(ui, step_x, 26.0, &["\u{2212}", "+"], None, "step", if running { DIM } else { Color32::WHITE }) {
            self.focus.adjust(if i == 0 { -1 } else { 1 } * if shift { 60 } else { 300 });
        }
        let reset = Rect::from_min_size(pos2(o.x + step_x + 2.0 * 26.0 + 4.0 + GAP_L, track_y), vec2(44.0, 24.0));
        let rr = hit(ui, "timerreset", reset);
        p.text(reset.center(), Align2::CENTER_CENTER, "Reset", bold(11.0), fade(if rr.hovered() { Color32::WHITE } else { GRAY }, a));
        if rr.clicked() {
            self.focus.reset();
        }

        let c = pos2(o.x + OPEN_W - MARGIN - phi_columns().1 / 2.0, o.y + TOP + 49.0);
        p.circle_stroke(c, 38.0, Stroke::new(6.0, fade(fill(24), a)));
        let prog = self.focus.progress(now).clamp(0.0, 1.0);
        if prog > 0.002 {
            let steps = ((prog * 72.0).ceil() as usize).max(2);
            let pts: Vec<egui::Pos2> = (0..=steps)
                .map(|i| {
                    let ang = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * prog * i as f32 / steps as f32;
                    c + vec2(ang.cos(), ang.sin()) * 38.0
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(6.0, fade(ORANGE, a))));
        }
        let btn = Rect::from_center_size(c, Vec2::splat(56.0));
        let b = hit(ui, "timertoggle", btn);
        if b.hovered() {
            p.circle_filled(c, 28.0, fade(fill(20), a));
        }
        if running {
            ui::icon_pause(&p, c, fade(Color32::WHITE, a));
        } else {
            ui::icon_play(&p, c, fade(Color32::WHITE, a));
        }
        if b.clicked() {
            self.focus.toggle(now);
        }
    }
}
