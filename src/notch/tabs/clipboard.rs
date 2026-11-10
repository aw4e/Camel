use crate::clipboard;
use crate::notch::{Notch, ToastIcon};
use crate::prelude::*;

impl Notch {
    pub(crate) fn tab_clipboard(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        p.text(o + vec2(MARGIN + ROW_PAD, TOP + 6.0), Align2::LEFT_CENTER, "Clipboard", bold(11.0), fade(GRAY, a));
        let n = self.history.items().len();
        if n == 0 {
            p.text(o + vec2(OPEN_W / 2.0, 86.0), Align2::CENTER_CENTER, "Text and pictures you copy show up here", font(12.0), fade(DIM, a));
            return;
        }
        p.text(o + vec2(MARGIN + ROW_PAD + 56.0 + GAP_L, TOP + 6.0), Align2::LEFT_CENTER, "click to copy, right-click to remove", font(10.5), fade(DIM, a));
        let clear = Rect::from_min_size(o + vec2(OPEN_W - MARGIN - 56.0, TOP), vec2(56.0, 12.0));
        let c = hit(ui, "clipclear", clear);
        p.text(
            pos2(o.x + OPEN_W - MARGIN - ROW_PAD, clear.center().y),
            Align2::RIGHT_CENTER,
            "Clear",
            bold(11.0),
            fade(if c.hovered() { Color32::WHITE } else { GRAY }, a),
        );

        self.clip_scroll = self.clip_scroll.min(n.saturating_sub(3));
        // thumbnails of pictures that left the history are dropped
        let alive: std::collections::HashSet<u64> =
            self.history.items().iter().filter_map(|s| if let clipboard::Item::Image(p) = &s.item { Some(p.id) } else { None }).collect();
        self.clip_thumbs.retain(|k, _| alive.contains(k));
        let (mut pick, mut remove, mut pin) = (None, None, None);
        for (slot, idx) in (self.clip_scroll..n).take(3).enumerate() {
            let row = Rect::from_min_size(o + vec2(MARGIN, CLIP_TOP + slot as f32 * ROW_PITCH), vec2(OPEN_W - 2.0 * MARGIN, ROW_H));
            let resp = hit(ui, &format!("clip{idx}"), row);
            let hover = over(ui, row);
            if hover {
                p.rect_filled(row, 8.0, fade(fill(22), a));
            }
            let pinned = self.history.items()[idx].pinned;
            match &self.history.items()[idx].item {
                clipboard::Item::Text(t) => {
                    let line = clipboard::one_line(t, 90);
                    p.with_clip_rect(row.shrink2(vec2(ROW_PAD, 0.0))).text(
                        pos2(row.left() + ROW_PAD, row.center().y),
                        Align2::LEFT_CENTER,
                        line,
                        font(12.0),
                        fade(Color32::WHITE, a),
                    );
                }
                clipboard::Item::Image(pic) => {
                    let [tw, th] = pic.thumb.0;
                    let tex = self.clip_thumbs.entry(pic.id).or_insert_with(|| {
                        ui.ctx().load_texture(
                            format!("clip-{}", pic.id),
                            egui::ColorImage::from_rgba_unmultiplied(pic.thumb.0, &pic.thumb.1),
                            egui::TextureOptions::LINEAR,
                        )
                    });
                    // the thumbnail is fitted into a 30 x 16 box, keeping its proportions
                    let s = (30.0 / tw as f32).min(16.0 / th as f32);
                    let thumb = Rect::from_center_size(pos2(row.left() + ROW_PAD + 15.0, row.center().y), vec2(tw as f32 * s, th as f32 * s));
                    egui::Image::new(&*tex).corner_radius(3).tint(fade(Color32::WHITE, a)).paint_at(ui, thumb);
                    let text_x = row.left() + ROW_PAD + 30.0 + GAP;
                    p.text(
                        pos2(text_x, row.center().y),
                        Align2::LEFT_CENTER,
                        format!("Image  {} \u{d7} {}", pic.w, pic.h),
                        font(12.0),
                        fade(Color32::WHITE, a),
                    );
                    if !hover {
                        p.text(pos2(row.right() - ROW_PAD, row.center().y), Align2::RIGHT_CENTER, format!("{} KB", pic.kb()), font(10.5), fade(DIM, a));
                    }
                }
            }
            // a pinned row carries a blue dot; hovering shows the Pin / Unpin button
            if pinned {
                p.circle_filled(pos2(row.left() + 4.0, row.center().y), 1.5, fade(BLUE, a));
            }
            if hover {
                let b = Rect::from_center_size(pos2(row.right() - ROW_PAD - 16.0, row.center().y), vec2(40.0, ROW_H));
                let r = hit(ui, &format!("pin{idx}"), b);
                let col = if r.hovered() { Color32::WHITE } else { GRAY };
                p.text(b.right_center(), Align2::RIGHT_CENTER, if pinned { "Unpin" } else { "Pin" }, bold(10.5), fade(col, a));
                if r.clicked() {
                    pin = Some(idx);
                }
            }
            if resp.clicked() && pin != Some(idx) {
                pick = Some(idx);
            }
            if resp.secondary_clicked() {
                remove = Some(idx);
            }
        }
        if let Some(i) = pick
            && let Ok(mut cb) = arboard::Clipboard::new()
        {
            match &self.history.items()[i].item {
                clipboard::Item::Text(t) => {
                    let _ = cb.set_text(t.clone());
                }
                clipboard::Item::Image(pic) => {
                    if let Some((w, h, rgba)) = pic.to_rgba() {
                        let _ = cb.set_image(arboard::ImageData { width: w as usize, height: h as usize, bytes: std::borrow::Cow::Owned(rgba) });
                    }
                }
            }
        }
        if let Some(i) = pin
            && !self.history.toggle_pin(i)
        {
            self.show_toast(ToastIcon::Clipboard, "Pin limit reached", "Unpin one first".to_owned());
        }
        if let Some(i) = remove {
            self.history.remove(i);
        }
        if c.clicked() {
            self.history.clear();
        }
    }
}
