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
        let pos = m.position();
        let has_lyrics = self.settings.lyrics && m.lyrics.is_some();
        if has_lyrics {
            let button = Rect::from_center_size(pos2(x1 - 11.0, o.y + TOP + 9.0), Vec2::splat(22.0));
            let resp = hit(ui, "lyrics-toggle", button).on_hover_text(if self.lyrics_view { "Show the player" } else { "Show lyrics" });
            if resp.hovered() {
                p.circle_filled(button.center(), 11.0, fade(fill(26), a));
            }
            ui::icon_lyrics(&p, button.center(), fade(if self.lyrics_view { accent } else { Color32::WHITE }, a));
            if resp.clicked() {
                self.lyrics_view = !self.lyrics_view;
            }
        }
        if has_lyrics && self.lyrics_view {
            self.draw_lyrics(ui, o, (x0, x1 - 26.0), m, a);
            return;
        }
        let (title, artist) = if m.title.is_empty() { ("Not playing", "") } else { (m.title.as_str(), m.artist.as_str()) };
        let text_w = x1 - x0 - if has_lyrics { 26.0 } else { 0.0 };
        ui::marquee(&p, pos2(x0, o.y + TOP), text_w, title, bold(14.0), fade(Color32::WHITE, a), time);
        ui::marquee(&p, pos2(x0, o.y + TOP + 20.0), text_w, artist, font(12.0), fade(GRAY, a), time);

        if m.dur > 0.0 {
            let cy = o.y + TOP + 50.0;
            let bar = Rect::from_x_y_ranges(x0 + 36.0..=x1 - 40.0, cy - 6.0..=cy + 6.0);
            if let Some(f) = seek_bar(ui, bar, pos / m.dur, a) {
                self.media.send(media::Cmd::Seek(f * m.dur));
            }
            p.text(pos2(x0, cy), Align2::LEFT_CENTER, fmt_time(pos), font(11.0), fade(GRAY, a));
            p.text(pos2(x1, cy), Align2::RIGHT_CENTER, format!("-{}", fmt_time((m.dur - pos).max(0.0))), font(11.0), fade(GRAY, a));
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
    }

    /// Spotify-style lyrics: the current line is large and white, its neighbours fade out, the list glides from line to
    /// line, and a click on a line jumps there.
    fn draw_lyrics(&mut self, ui: &mut egui::Ui, o: egui::Pos2, (x0, x1): (f32, f32), m: &media::State, a: f32) {
        let Some(lines) = m.lyrics.as_deref() else { return };
        let (video, dur) = (m.position(), m.dur);
        // music videos often start with an intro the audio-only lyrics do not have: the offset shifts the lyrics
        let track = format!(
            "{}
{}",
            m.artist, m.title
        );
        if self.lyric_track != track {
            self.lyric_track = track;
            self.lyric_offset = 0.0;
        }
        let pos = video - self.lyric_offset;
        const LINE_H: f32 = 24.0;
        let p = ui.painter().clone();
        let center = o.y + TOP + 44.0;
        let region = Rect::from_x_y_ranges(x0 - 4.0..=x1 + 4.0, o.y + TOP - 4.0..=o.y + TOP + 92.0);
        let clip = p.with_clip_rect(region);

        let current = lines.partition_point(|(t, _)| *t <= pos + 0.25) as f32 - 1.0;
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.lyric_pos =
            if (current - self.lyric_pos).abs() > 6.0 { current } else { self.lyric_pos + (current - self.lyric_pos) * (1.0 - (-dt * 10.0).exp()) };
        if (current - self.lyric_pos).abs() > 0.01 {
            ui.ctx().request_repaint();
        }

        let first = (self.lyric_pos - 3.0).floor().max(0.0) as usize;
        let last = ((self.lyric_pos + 3.0).ceil().max(0.0) as usize).min(lines.len().saturating_sub(1));
        for (i, (start, text)) in lines.iter().enumerate().take(last + 1).skip(first) {
            let d = i as f32 - self.lyric_pos;
            let near = (1.0 - d.abs()).clamp(0.0, 1.0);
            let text = if text.is_empty() && near > 0.5 { "♪" } else { text.as_str() };
            if text.is_empty() {
                continue;
            }
            let y = center + d * LINE_H;
            let hover_rect = Rect::from_min_size(pos2(x0 - 4.0, y - LINE_H / 2.0), vec2(x1 - x0 + 8.0, LINE_H)).intersect(region);
            let tip = if dur > 0.0 { "Click to jump here. Shift+click if the lyrics are early or late." } else { "Click the line you hear to sync." };
            let resp = hit(ui, &format!("line{i}"), hover_rect).on_hover_text(tip);
            if resp.clicked() {
                if dur > 0.0 && ui.input(|i| i.modifiers.shift) {
                    self.lyric_offset = video - *start;
                } else {
                    self.media.send(media::Cmd::Seek(*start + self.lyric_offset));
                }
            }
            let shade = if resp.hovered() { 255 } else { (150.0 + 105.0 * near) as u8 };
            let alpha = (1.0 - 0.45 * d.abs()).clamp(0.0, 1.0) * a;
            let color = fade(Color32::from_gray(shade), alpha);
            let mut size = 12.5 + 3.0 * near;
            let mut galley = p.layout_no_wrap(text.to_owned(), bold(size), color);
            if galley.size().x > x1 - x0 {
                size = (size * (x1 - x0) / galley.size().x).max(size * 0.75);
                galley = p.layout_no_wrap(text.to_owned(), bold(size), color);
            }
            clip.galley(pos2(x0, y - galley.size().y / 2.0), galley, color);
        }

        if self.lyric_offset != 0.0 {
            let label = Rect::from_center_size(pos2(x1 + 26.0 - 40.0, o.y + TOP + 86.0), vec2(80.0, 12.0));
            let reset = hit(ui, "lyric-offset", label).on_hover_text("Click to remove the shift");
            let col = if reset.hovered() { Color32::WHITE } else { DIM };
            p.text(label.right_center(), Align2::RIGHT_CENTER, format!("shifted {:+.1} s", -self.lyric_offset), font(9.5), fade(col, a));
            if reset.clicked() {
                self.lyric_offset = 0.0;
            }
        }
        if dur <= 0.0 {
            // no timeline from the player: the position is counted here, and a click on a line corrects it
            p.text(pos2(x0, o.y + TOP + 96.0), Align2::LEFT_CENTER, "No timeline from this player. Click the line you hear to sync.", font(9.5), fade(DIM, a));
            return;
        }
        let bar = Rect::from_x_y_ranges(x0..=x1 + 26.0, o.y + TOP + 95.0..=o.y + TOP + 97.0);
        p.rect_filled(bar, 1.0, fade(fill(24), a));
        let done = (video / dur).clamp(0.0, 1.0) as f32;
        p.rect_filled(Rect::from_min_size(bar.min, vec2(bar.width() * done, bar.height())), 1.0, fade(Color32::from_gray(235), a));
    }
}
