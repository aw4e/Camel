use crate::notch::Notch;
use crate::prelude::*;
use crate::{actions, spotlight};

impl Notch {
    pub(crate) fn draw_spotlight(&mut self, ui: &mut egui::Ui, rect: Rect, a: f32) {
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
                (self.spot.sel, self.spot.armed) = ((self.spot.sel + 1) % n, None);
            }
            if up {
                (self.spot.sel, self.spot.armed) = ((self.spot.sel + n - 1) % n, None);
            }
        }
        if esc {
            self.spot.open = false;
        }
        if enter {
            self.activate(self.spot.sel);
        }

        ui::icon_search(&p, o + vec2(28.0, 25.0), fade(GRAY, a));
        let edit_rect = Rect::from_min_size(o + vec2(50.0, 8.0), vec2(SPOT_W - 70.0, 36.0));
        // a copy, so the comparison below sees whether the user typed. Taking the string out left self.spot.query
        // empty, which made every frame look like a keystroke: it re-ran the search and reset the selection.
        let mut query = self.spot.query.clone();
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
            self.spot.hits = spotlight::search(&self.index, &query, self.awake, &self.reminders.list());
            self.spot.armed = None;
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
            let armed = self.spot.armed == Some(i);
            if selected {
                p.rect_filled(row, 9.0, fade(if armed { RED } else { BLUE }, a));
            }
            let ic = Rect::from_center_size(pos2(row.left() + 22.0, row.center().y), Vec2::splat(24.0));
            let (bg, glyph): (Color32, fn(&egui::Painter, egui::Pos2, Color32)) = match h.kind {
                spotlight::Kind::Calc => (ORANGE, ui::icon_calc),
                spotlight::Kind::App => (BLUE, ui::icon_app),
                spotlight::Kind::File => (Color32::from_gray(99), ui::icon_file),
                spotlight::Kind::Web => (GREEN, ui::icon_search),
                spotlight::Kind::Action => h.action.as_ref().map_or((Color32::from_gray(99), ui::icon_close), action_style),
            };
            ui::tile(&p, ic, fade(if selected { fill(46) } else { bg }, a));
            ui::scaled(&p, ic.center(), ui::icon_fit(ic.height()), || glyph(&p, ic.center(), fade(Color32::WHITE, a)));
            let sub = if selected { fill(190) } else { GRAY };
            let q = p.with_clip_rect(Rect::from_x_y_ranges(row.left() + 42.0..=row.right() - 92.0, row.top()..=row.bottom()));
            let y = if h.detail.is_empty() { row.center().y - 8.0 } else { row.top() + 4.0 };
            q.text(pos2(row.left() + 42.0, y), Align2::LEFT_TOP, &h.title, bold(13.0), fade(Color32::WHITE, a));
            if armed {
                q.text(pos2(row.left() + 42.0, row.top() + 20.0), Align2::LEFT_TOP, "Press Enter again to confirm", font(10.0), fade(Color32::WHITE, a));
            } else if !h.detail.is_empty() {
                q.text(pos2(row.left() + 42.0, row.top() + 20.0), Align2::LEFT_TOP, &h.detail, font(10.0), fade(sub, a));
            }
            let kind = match h.kind {
                spotlight::Kind::Calc => "Copy result",
                spotlight::Kind::App => "Application",
                spotlight::Kind::File => "File",
                spotlight::Kind::Web => "Web",
                spotlight::Kind::Action => {
                    if armed {
                        "Confirm"
                    } else {
                        "Action"
                    }
                }
            };
            p.text(pos2(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, kind, font(10.0), fade(if selected { fill(190) } else { DIM }, a));

            if ui.interact(row, ui.scope_id().with(("hit", i)), Sense::click()).clicked() {
                clicked = Some(i);
            }
        }
        if let Some(i) = clicked {
            self.activate(i);
        }
    }
}

fn action_style(a: &actions::Action) -> (Color32, fn(&egui::Painter, egui::Pos2, Color32)) {
    use crate::actions::Action::*;
    match a {
        Lock => (BLUE, ui::icon_lock),
        Sleep => (Color32::from_rgb(94, 92, 230), ui::icon_sleep),
        Restart => (ORANGE, ui::icon_power),
        Shutdown => (RED, ui::icon_power),
        SignOut => (Color32::from_gray(99), ui::icon_power),
        KeepAwake => (GREEN, ui::icon_sun),
        Timer(_) => (ORANGE, ui::icon_timer),
        Kill(_) => (RED, ui::icon_close),
        Mute(false) => (Color32::from_gray(99), ui::icon_speaker),
        Mute(true) => (RED, ui::icon_mic),
        Note(_) | OpenNotes => (ORANGE, ui::icon_clipboard),
        Update => (GREEN, ui::icon_app),
        Weather | City(_) => (BLUE, ui::icon_cloud),
        Remind(_) | Snooze => (ORANGE, ui::icon_timer),
        Done(_) => (GREEN, ui::icon_timer),
    }
}
