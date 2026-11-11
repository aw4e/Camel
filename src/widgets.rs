use crate::media;
use crate::prelude::*;

/// Baseline of a single-line galley, measured from its top edge.
pub(crate) fn baseline(g: &egui::Galley) -> f32 {
    g.rows.first().and_then(|r| r.row.glyphs.first().map(|gl| r.pos.y + gl.pos.y)).unwrap_or(g.size().y * 0.8)
}

/// True while the pointer is over `rect`, even when a child widget sits on top of it.
pub(crate) fn over(ui: &egui::Ui, rect: Rect) -> bool {
    ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| rect.contains(p))
}

pub(crate) fn hit(ui: &mut egui::Ui, key: &str, rect: Rect) -> Response {
    ui.interact(rect, ui.scope_id().with(key), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Seek bar. Returns the target fraction when the user clicks or releases a drag.
pub(crate) fn seek_bar(ui: &mut egui::Ui, rect: Rect, frac: f64, a: f32) -> Option<f64> {
    let resp = ui.interact(rect, ui.scope_id().with("seek"), Sense::click_and_drag());
    let frac_at = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
    let dragging = resp.dragged().then(|| resp.interact_pointer_pos()).flatten().map(|p| frac_at(p.x));
    let frac = dragging.unwrap_or(frac).clamp(0.0, 1.0) as f32;

    let h = ui.ctx().animate_value_with_time(resp.id.with("h"), if resp.hovered() || resp.dragged() { 8.0 } else { 4.0 }, 0.12);
    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), h));
    let p = ui.painter();
    p.rect_filled(track, h / 2.0, fade(fill(40), a));
    p.rect_filled(Rect::from_min_size(track.min, vec2(track.width() * frac, h)), h / 2.0, fade(Color32::from_gray(235), a));

    if resp.drag_stopped() || resp.clicked() {
        return resp.interact_pointer_pos().map(|p| frac_at(p.x));
    }
    None
}

pub(crate) fn fmt_time(s: f64) -> String {
    let s = s as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

pub(crate) fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    s.chars().take(max - 1).chain(['…']).collect()
}

pub(crate) fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let f = |n: f32| {
        let k = (n + h / 60.0) % 6.0;
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0)
    };
    Color32::from_rgb((f(5.0) * 255.0) as u8, (f(3.0) * 255.0) as u8, (f(1.0) * 255.0) as u8)
}

/// Mean art colour, brightened so it stays readable on black.
pub(crate) fn accent(a: &media::Art) -> Color32 {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for px in a.rgba.chunks_exact(4).step_by(7) {
        r += px[0] as u64;
        g += px[1] as u64;
        b += px[2] as u64;
        n += 1;
    }
    let n = n.max(1);
    let (r, g, b) = (r / n, g / n, b / n);
    let k = 235.0 / r.max(g).max(b).max(1) as f32;
    Color32::from_rgb((r as f32 * k).min(255.0) as u8, (g as f32 * k).min(255.0) as u8, (b as f32 * k).min(255.0) as u8)
}

/// "14:05", or "2:05 PM" on a 12-hour clock.
pub(crate) fn clock_text(now: &chrono::DateTime<chrono::Local>, h24: bool) -> String {
    now.format(if h24 { "%H:%M" } else { "%-I:%M %p" }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn the_clock_follows_the_hour_format() {
        let t = chrono::Local.with_ymd_and_hms(2026, 11, 11, 14, 5, 9).unwrap();
        assert_eq!(clock_text(&t, true), "14:05");
        assert_eq!(clock_text(&t, false), "2:05 PM");
        let midnight = chrono::Local.with_ymd_and_hms(2026, 11, 11, 0, 7, 0).unwrap();
        assert_eq!(clock_text(&midnight, false), "12:07 AM");
    }
}
