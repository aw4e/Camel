use crate::app::{Notch, ToastIcon};
use crate::prelude::*;
use crate::reminders;
use chrono::Local;

impl Notch {
    /// Reminders and to-dos: a field to add one ("17:30 call mom", "every 45m drink water", "buy milk") and the pending list.
    /// Click the circle in front of a row to tick it off.
    pub(crate) fn tab_tasks(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        let field = Rect::from_min_size(o + vec2(MARGIN, TOP), vec2(OPEN_W - 2.0 * MARGIN, 20.0));
        p.rect_filled(field, 10.0, fade(fill(14), a));
        let edit = ui.put(
            field.shrink2(vec2(ROW_PAD, 0.0)),
            egui::TextEdit::singleline(&mut self.task_input)
                .font(font(12.0))
                .frame(egui::Frame::NONE)
                .desired_width(field.width() - 2.0 * ROW_PAD)
                .hint_text(egui::RichText::new("Add a task: 17:30 call mom, every 45m drink water, buy milk").color(fade(DIM, a)))
                .text_color(fade(Color32::WHITE, a))
                .align(Align2::LEFT_CENTER),
        );
        if edit.clicked() {
            // the overlay does not take keyboard focus by being clicked, so ask for it (the timer field does the same)
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            match reminders::parse(&self.task_input, Local::now()) {
                Some(spec) => {
                    self.reminders.add(spec);
                    self.task_input.clear();
                }
                None => self.show_toast(ToastIcon::Timer, "Write what to remember", "Time first if you want a reminder".to_owned()),
            }
            edit.request_focus();
        }

        let pending = self.reminders.list();
        if pending.is_empty() {
            p.text(o + vec2(OPEN_W / 2.0, 92.0), Align2::CENTER_CENTER, "Nothing to do. Type a task above.", font(12.0), fade(DIM, a));
            return;
        }
        self.tasks_scroll = self.tasks_scroll.min(pending.len().saturating_sub(3));
        let now = Local::now();
        let mut done = None;
        for (slot, task) in pending.iter().skip(self.tasks_scroll).take(3).enumerate() {
            let row = Rect::from_min_size(o + vec2(MARGIN, APPS_TOP + slot as f32 * ROW_PITCH), vec2(OPEN_W - 2.0 * MARGIN, ROW_H));
            if over(ui, row) {
                p.rect_filled(row, 8.0, fade(fill(18), a));
            }
            let circle = Rect::from_center_size(pos2(row.left() + ROW_PAD + 7.0, row.center().y), Vec2::splat(18.0));
            let tick = hit(ui, &format!("task{}", task.id), circle).on_hover_text("Tick off");
            p.circle_stroke(circle.center(), 6.0, Stroke::new(1.5, fade(if tick.hovered() { GREEN } else { GRAY }, a)));
            if tick.hovered() {
                p.circle_filled(circle.center(), 3.0, fade(GREEN, a));
            }
            if tick.clicked() {
                done = Some(task.id);
            }
            let text_x = row.left() + ROW_PAD + 22.0;
            let due = reminders::describe(task.due, task.every, now);
            let due_w = p.layout_no_wrap(due.clone(), font(10.5), Color32::WHITE).size().x;
            p.with_clip_rect(Rect::from_x_y_ranges(text_x..=row.right() - ROW_PAD - due_w - GAP, row.y_range())).text(
                pos2(text_x, row.center().y),
                Align2::LEFT_CENTER,
                &task.text,
                font(12.0),
                fade(Color32::WHITE, a),
            );
            p.text(pos2(row.right() - ROW_PAD, row.center().y), Align2::RIGHT_CENTER, due, font(10.5), fade(DIM, a));
        }
        if let Some(id) = done {
            self.reminders.remove(id);
        }
    }
}
