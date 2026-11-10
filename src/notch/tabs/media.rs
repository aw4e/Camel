use crate::media;
use crate::notch::Notch;
use crate::prelude::*;

impl Notch {
    pub(crate) fn tab_media(&mut self, ui: &mut egui::Ui, o: egui::Pos2, m: &media::State, a: f32, time: f32) {
        let p = ui.painter().clone();
        let accent = self.accent();
        // art shrinks to 85% while paused and its glow fades
        let play = ui.ctx().animate_bool_with_time(ui.scope_id().with("play"), m.playing, 0.3);
        let side = 72.0 * (0.85 + 0.15 * play);
        // centred on the whole text block (title top to the bottom of the transport buttons), not pinned to its top
        let block_h = 75.0 + 18.0;
        let art = Rect::from_center_size(o + vec2(MARGIN + 36.0, TOP + block_h / 2.0), Vec2::splat(side));
        if self.art_tex.is_some() {
            ui::glow(&p, art.center(), 58.0, fade(accent, 0.42 * play * a));
        }
        match &self.art_tex {
            Some((_, tex, _)) => egui::Image::new(tex).corner_radius(11).tint(fade(Color32::WHITE, a)).paint_at(ui, art),
            None => {
                p.rect_filled(art, 11.0, fade(fill(14), a));
                ui::music_note(&p, art.center(), fade(DIM, a), art.width() / 72.0 * 2.3);
            }
        }
        if self.art_tex.is_some() {
            p.rect_stroke(art, 11.0, Stroke::new(1.0, fade(fill(28), a)), StrokeKind::Inside);
        }

        let x0 = o.x + MARGIN + 72.0 + GAP_L;
        let x1 = o.x + OPEN_W - MARGIN;
        let (title, artist) = if m.title.is_empty() { ("Not playing", "") } else { (m.title.as_str(), m.artist.as_str()) };
        ui::marquee(&p, pos2(x0, o.y + TOP), x1 - x0, title, bold(14.0), fade(Color32::WHITE, a), time);
        ui::marquee(&p, pos2(x0, o.y + TOP + 20.0), x1 - x0, artist, font(12.0), fade(GRAY, a), time);

        if m.dur > 0.0 {
            let cy = o.y + TOP + 50.0;
            let bar = Rect::from_x_y_ranges(x0 + 36.0..=x1 - 40.0, cy - 6.0..=cy + 6.0);
            if let Some(f) = seek_bar(ui, bar, m.pos / m.dur, a) {
                self.media.send(media::Cmd::Seek(f * m.dur));
            }
            p.text(pos2(x0, cy), Align2::LEFT_CENTER, fmt_time(m.pos), font(11.0), fade(GRAY, a));
            p.text(pos2(x1, cy), Align2::RIGHT_CENTER, format!("-{}", fmt_time((m.dur - m.pos).max(0.0))), font(11.0), fade(GRAY, a));
        }

        let cy = o.y + TOP + 75.0;
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
            ui::marquee(&p, pos2(o.x + (OPEN_W - tw) / 2.0, o.y + TOP + 105.0), w, text, bold(12.0), fade(Color32::WHITE, a), time);
        }
    }
}
