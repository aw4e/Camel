use crate::app::Notch;
use crate::prelude::*;
use crate::spotlight;

impl Notch {
    pub(crate) fn tab_shelf(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        let area = Rect::from_min_size(o + vec2(MARGIN, TOP), vec2(OPEN_W - 2.0 * MARGIN, 90.0));
        let dragging = ui.input(|i| !i.raw.hovered_files.is_empty());
        if self.shelf.is_empty() || dragging {
            let col = if dragging { BLUE } else { DIM };
            ui::dashed_rrect(&p, area, 12.0, Stroke::new(1.5, fade(col, a)));
            ui::icon_tray(&p, area.center() - vec2(0.0, 10.0), fade(col, a));
            p.text(area.center() + vec2(0.0, 12.0), Align2::CENTER_CENTER, "Drop files here", bold(12.0), fade(col, a));
            return;
        }

        let mut remove = None;
        let visible = 5;
        self.shelf_scroll = self.shelf_scroll.min(self.shelf.len().saturating_sub(visible));
        for (slot, idx) in (self.shelf_scroll..self.shelf.len()).take(visible).enumerate() {
            let path = &self.shelf[idx];
            let tile = Rect::from_min_size(area.min + vec2(slot as f32 * 82.0, 0.0), vec2(78.0, 84.0));
            let resp = hit(ui, &format!("shelf{idx}"), tile);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let ext = path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
            let hue = ext.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
            let base = hsv(hue as f32, 0.45, if over(ui, tile) { 0.95 } else { 0.8 });
            let sq = Rect::from_min_size(tile.min + vec2(19.0, 2.0), Vec2::splat(40.0));
            ui::tile(&p, sq, fade(base, a));
            let label = if path.is_dir() { "DIR".to_owned() } else { ext.chars().take(4).collect::<String>() };
            p.text(sq.center(), Align2::CENTER_CENTER, label, bold(11.0), fade(Color32::from_black_alpha(190), a));
            p.with_clip_rect(tile).text(
                pos2(tile.center().x, tile.top() + 48.0),
                Align2::CENTER_TOP,
                shorten(&name, 12),
                font(10.5),
                fade(Color32::from_gray(220), a),
            );

            if over(ui, tile) {
                let x = Rect::from_center_size(sq.right_top() + vec2(-1.0, 1.0), Vec2::splat(16.0));
                let xr = hit(ui, &format!("shelfx{idx}"), x);
                p.circle_filled(x.center(), 8.0, fade(if xr.hovered() { Color32::from_gray(96) } else { Color32::from_gray(56) }, a));
                let c = x.center();
                for (d1, d2) in [(vec2(-2.5, -2.5), vec2(2.5, 2.5)), (vec2(-2.5, 2.5), vec2(2.5, -2.5))] {
                    p.line_segment([c + d1, c + d2], Stroke::new(1.4, fade(Color32::WHITE, a)));
                }
                if xr.clicked() {
                    remove = Some(idx);
                    continue;
                }
            }
            if resp.clicked() {
                spotlight::launch(path);
            }
            if resp.secondary_clicked() {
                remove = Some(idx);
            }
        }
        if let Some(i) = remove {
            self.shelf.remove(i);
        }
    }
}
