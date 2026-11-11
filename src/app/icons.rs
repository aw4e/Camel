//! Drawing helpers: vector icons and small widgets. All colours arrive pre-faded by the caller.

use eframe::egui::{self, Color32, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

// macOS dark system palette. GRAY / DIM are secondary / tertiary label.
pub const GRAY: Color32 = Color32::from_gray(152);
pub const DIM: Color32 = Color32::from_gray(99);
pub const BLUE: Color32 = Color32::from_rgb(10, 132, 255);
pub const GREEN: Color32 = Color32::from_rgb(48, 209, 88);
pub const ORANGE: Color32 = Color32::from_rgb(255, 159, 10);
pub const RED: Color32 = Color32::from_rgb(255, 69, 58);

/// White at `a`/255 opacity, the macOS "fill" layers drawn over the black body.
pub fn fill(a: u8) -> Color32 {
    Color32::from_white_alpha(a)
}

/// Rounded app-icon tile (corner radius 28% of the side, close to the macOS squircle).
pub fn tile(p: &Painter, r: Rect, col: Color32) {
    p.rect_filled(r, r.width() * 0.28, col);
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn fade(c: Color32, f: f32) -> Color32 {
    c.gamma_multiply(f)
}

// ---- icons: one drawing language (SF Symbols-like): 1.5 stroke, rounded corners, round caps, ~18 px box.
// They are original drawings, not Apple's symbols (those may not leave Apple platforms).

const W: f32 = 1.5;

fn at(c: Pos2, q: (f32, f32)) -> Pos2 {
    c + vec2(q.0, q.1)
}

/// Rounds every corner of a polygon / polyline with a quadratic curve of radius `r`.
fn rpoly(pts: &[Pos2], r: f32, closed: bool) -> Vec<Pos2> {
    let n = pts.len();
    let mut out = Vec::new();
    for i in 0..n {
        let v = pts[i];
        if r <= 0.01 || (!closed && (i == 0 || i == n - 1)) {
            out.push(v);
            continue;
        }
        let (d1, d2) = (pts[(i + n - 1) % n] - v, pts[(i + 1) % n] - v);
        let (l1, l2) = (d1.length().max(0.001), d2.length().max(0.001));
        let d = r.min(l1 / 2.0).min(l2 / 2.0);
        out.extend(quad(v + d1 / l1 * d, v, v + d2 / l2 * d, 6));
    }
    out
}

/// Stroked outline with rounded corners (closed) or a polyline with round caps (open).
fn path(p: &Painter, c: Pos2, pts: &[(f32, f32)], r: f32, closed: bool, w: f32, col: Color32) {
    let v = rpoly(&pts.iter().map(|q| at(c, *q)).collect::<Vec<_>>(), r, closed);
    if closed {
        p.add(Shape::closed_line(v, Stroke::new(w, col)));
    } else {
        let (a, b) = (v[0], v[v.len() - 1]);
        p.add(Shape::line(v, Stroke::new(w, col)));
        p.circle_filled(a, w / 2.0, col);
        p.circle_filled(b, w / 2.0, col);
    }
}

fn seg(p: &Painter, c: Pos2, a: (f32, f32), b: (f32, f32), w: f32, col: Color32) {
    path(p, c, &[a, b], 0.0, false, w, col);
}

/// Filled convex shape with rounded corners.
fn solid(p: &Painter, c: Pos2, pts: &[(f32, f32)], r: f32, col: Color32) {
    p.add(Shape::convex_polygon(rpoly(&pts.iter().map(|q| at(c, *q)).collect::<Vec<_>>(), r, true), col, Stroke::NONE));
}

fn arc(p: &Painter, c: Pos2, centre: (f32, f32), rad: f32, a0: f32, a1: f32, col: Color32) {
    let pts: Vec<(f32, f32)> = (0..=16)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / 16.0;
            (centre.0 + rad * a.cos(), centre.1 + rad * a.sin())
        })
        .collect();
    path(p, c, &pts, 0.0, false, W, col);
}

fn bar(p: &Painter, c: Pos2, w: f32, h: f32, col: Color32) {
    p.rect_filled(Rect::from_center_size(c, vec2(w, h)), 2.0, col);
}

/// Icons are drawn at 16 px. Inside a button they keep one Fibonacci step (5 px) of padding above and below, never
/// scaled below half size or above full size: a 24 px pill gets a 14 px icon.
pub fn icon_fit(container_h: f32) -> f32 {
    ((container_h - 10.0) / 16.0).clamp(0.5, 1.0)
}

/// Runs `draw` and scales whatever it painted by `s` about `c`: positions, radii and stroke widths together, so the
/// icons keep their shape at any size without carrying a scale parameter.
pub fn scaled(p: &Painter, c: Pos2, s: f32, draw: impl FnOnce()) {
    let start = p.add(Shape::Noop).0;
    draw();
    let end = p.add(Shape::Noop).0;
    let t = egui::emath::TSTransform::new(c.to_vec2() * (1.0 - s), s);
    p.ctx().graphics_mut(|g| {
        let list = g.entry(p.layer_id());
        for i in start..end {
            list.mutate_shape(egui::layers::ShapeIdx(i), |cs| cs.shape.transform(t));
        }
    });
}

pub fn icon_play(p: &Painter, c: Pos2, col: Color32) {
    solid(p, c, &[(-6.0, -10.0), (-6.0, 10.0), (10.0, 0.0)], 2.6, col);
}

pub fn icon_pause(p: &Painter, c: Pos2, col: Color32) {
    bar(p, c + vec2(-5.0, 0.0), 5.5, 18.0, col);
    bar(p, c + vec2(5.0, 0.0), 5.5, 18.0, col);
}

pub fn icon_next(p: &Painter, c: Pos2, col: Color32) {
    solid(p, c, &[(-8.0, -6.5), (-8.0, 6.5), (2.0, 0.0)], 1.6, col);
    bar(p, c + vec2(5.5, 0.0), 2.8, 13.0, col);
}

pub fn icon_prev(p: &Painter, c: Pos2, col: Color32) {
    solid(p, c, &[(8.0, -6.5), (8.0, 6.5), (-2.0, 0.0)], 1.6, col);
    bar(p, c + vec2(-5.5, 0.0), 2.8, 13.0, col);
}

pub fn icon_music(p: &Painter, c: Pos2, col: Color32) {
    music_note(p, c, col, 1.0);
}

/// Pair of notes under a thick beam, at `s` times the tab-bar size. Also the placeholder where cover art would be.
pub fn music_note(p: &Painter, c: Pos2, col: Color32, s: f32) {
    let q = |x: f32, y: f32| at(c, (x * s, y * s));
    p.circle_filled(q(-4.6, 4.8), 2.6 * s, col);
    p.circle_filled(q(4.4, 3.2), 2.6 * s, col);
    seg(p, c, (-2.4 * s, 4.8 * s), (-2.4 * s, -5.0 * s), W * s, col);
    seg(p, c, (6.6 * s, 3.2 * s), (6.6 * s, -6.5 * s), W * s, col);
    p.add(Shape::convex_polygon(vec![q(-2.4, -6.6), q(6.6, -8.1), q(6.6, -4.7), q(-2.4, -3.2)], col, Stroke::NONE));
}

pub fn icon_speaker(p: &Painter, c: Pos2, col: Color32) {
    p.add(Shape::convex_polygon(
        [(-7.5, -2.6), (-4.2, -2.6), (0.5, -6.8), (0.5, 6.8), (-4.2, 2.6), (-7.5, 2.6)].iter().map(|q| at(c, *q)).collect(),
        col,
        Stroke::NONE,
    ));
    arc(p, c, (1.2, 0.0), 4.0, -0.85, 0.85, col);
    arc(p, c, (1.2, 0.0), 7.2, -0.85, 0.85, col);
}

pub fn icon_clock(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 7.4, Stroke::new(W, col));
    path(p, c, &[(0.0, -4.2), (0.0, 0.0), (3.4, 2.0)], 0.0, false, W, col);
}

pub fn icon_timer(p: &Painter, c: Pos2, col: Color32) {
    seg(p, c, (-5.8, -7.6), (5.8, -7.6), W, col);
    seg(p, c, (-5.8, 7.6), (5.8, 7.6), W, col);
    path(p, c, &[(-4.4, -7.6), (-4.4, -4.6), (0.0, 0.0), (-4.4, 4.6), (-4.4, 7.6)], 1.6, false, W, col);
    path(p, c, &[(4.4, -7.6), (4.4, -4.6), (0.0, 0.0), (4.4, 4.6), (4.4, 7.6)], 1.6, false, W, col);
}

pub fn icon_tray(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-7.5, -0.5), (-7.5, 5.0), (7.5, 5.0), (7.5, -0.5)], 2.4, false, W, col);
    seg(p, c, (0.0, -7.5), (0.0, 1.6), W, col);
    path(p, c, &[(-3.3, -1.6), (0.0, 1.7), (3.3, -1.6)], 0.0, false, W, col);
}

pub fn icon_clipboard(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 1.2), vec2(13.0, 15.5)), 2.6, Stroke::new(W, col), StrokeKind::Inside);
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, -7.0), vec2(7.2, 3.4)), 1.7, col);
    seg(p, c, (-3.0, 0.0), (3.0, 0.0), W, col);
    seg(p, c, (-3.0, 3.6), (1.4, 3.6), W, col);
}

pub fn icon_grid(p: &Painter, c: Pos2, col: Color32) {
    for (dx, dy) in [(-4.0, -4.0), (4.0, -4.0), (-4.0, 4.0), (4.0, 4.0)] {
        p.rect_filled(Rect::from_center_size(c + vec2(dx, dy), vec2(6.6, 6.6)), 2.0, col);
    }
}

pub fn icon_gear(p: &Painter, c: Pos2, col: Color32) {
    let step = std::f32::consts::FRAC_PI_4;
    let mut pts = Vec::new();
    for k in 0..8 {
        let a = k as f32 * step;
        for (da, r) in [(-0.34, 5.7), (-0.2, 7.7), (0.2, 7.7), (0.34, 5.7), (step / 2.0, 5.7)] {
            pts.push(c + vec2((a + da).cos(), (a + da).sin()) * r);
        }
    }
    p.add(Shape::closed_line(rpoly(&pts, 0.9, true), Stroke::new(W, col)));
    p.circle_stroke(c, 2.6, Stroke::new(W, col));
}

pub fn icon_folder(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-7.8, -5.4), (-2.8, -5.4), (-1.0, -3.2), (7.8, -3.2), (7.8, 5.8), (-7.8, 5.8)], 2.2, true, W, col);
}

pub fn icon_file(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-5.6, -8.0), (1.6, -8.0), (6.6, -3.0), (6.6, 8.0), (-5.6, 8.0)], 1.8, true, W, col);
    path(p, c, &[(1.6, -8.0), (1.6, -3.0), (6.6, -3.0)], 0.8, false, W, col);
}

pub fn icon_lock(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 2.5), vec2(12.0, 9.0)), 2.2, Stroke::new(W, col), StrokeKind::Inside);
    arc(p, c, (0.0, -2.0), 4.0, std::f32::consts::PI, std::f32::consts::TAU, col);
    p.circle_filled(c + vec2(0.0, 2.5), 1.2, col);
}

pub fn icon_sleep(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-6.5, -1.0), (0.5, -1.0), (-6.5, 7.0), (0.5, 7.0)], 0.0, false, W, col);
    path(p, c, &[(2.5, -8.0), (6.5, -8.0), (2.5, -4.0), (6.5, -4.0)], 0.0, false, W, col);
}

pub fn icon_power(p: &Painter, c: Pos2, col: Color32) {
    let a0 = -std::f32::consts::FRAC_PI_2 + 0.65;
    arc(p, c, (0.0, 1.0), 6.3, a0, a0 + std::f32::consts::TAU - 1.3, col);
    seg(p, c, (0.0, -7.5), (0.0, 0.0), W, col);
}

pub fn icon_sun(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 3.2, Stroke::new(W, col));
    for i in 0..8 {
        let (s, co) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
        seg(p, c, (co * 5.6, s * 5.6), (co * 7.6, s * 7.6), W, col);
    }
}

pub fn icon_close(p: &Painter, c: Pos2, col: Color32) {
    seg(p, c, (-4.5, -4.5), (4.5, 4.5), W, col);
    seg(p, c, (-4.5, 4.5), (4.5, -4.5), W, col);
}

pub fn icon_app(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c, vec2(15.0, 15.0)), 4.2, Stroke::new(W, col), StrokeKind::Inside);
    p.circle_filled(c, 2.0, col);
}

pub fn icon_battery(p: &Painter, c: Pos2, col: Color32) {
    let body = Rect::from_center_size(c + vec2(-0.8, 0.0), vec2(15.0, 8.6));
    p.rect_stroke(body, 2.6, Stroke::new(W, col), StrokeKind::Inside);
    p.rect_filled(body.shrink(2.2), 1.0, col);
    p.rect_filled(Rect::from_center_size(pos2(body.right() + 1.8, c.y), vec2(1.8, 3.6)), 0.9, col);
}

pub fn icon_mic(p: &Painter, c: Pos2, col: Color32) {
    p.rect_filled(Rect::from_min_max(at(c, (-2.7, -8.5)), at(c, (2.7, 0.8))), 2.7, col);
    arc(p, c, (0.0, -1.2), 5.3, 0.0, std::f32::consts::PI, col);
    seg(p, c, (0.0, 4.1), (0.0, 7.4), W, col);
    seg(p, c, (-2.7, 7.4), (2.7, 7.4), W, col);
}

pub fn icon_lyrics(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-7.5, -6.5), (7.5, -6.5), (7.5, 4.5), (-2.0, 4.5), (-5.5, 8.0), (-5.5, 4.5), (-7.5, 4.5)], 2.5, true, W, col);
    seg(p, c, (-3.8, -2.6), (3.8, -2.6), W, col);
    seg(p, c, (-3.8, 0.6), (1.0, 0.6), W, col);
}

pub fn icon_keyboard(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-8.0, -4.5), (8.0, -4.5), (8.0, 5.0), (-8.0, 5.0)], 2.5, true, W, col);
    for x in [-4.5, -1.5, 1.5, 4.5] {
        p.circle_filled(at(c, (x, -1.2)), 0.9, col);
    }
    seg(p, c, (-3.5, 2.2), (3.5, 2.2), W, col);
}

pub fn icon_wifi(p: &Painter, c: Pos2, col: Color32) {
    let (a0, a1) = (-std::f32::consts::PI * 0.75, -std::f32::consts::PI * 0.25);
    for r in [4.5, 8.0, 11.5] {
        arc(p, c, (0.0, 5.5), r, a0, a1, col);
    }
    p.circle_filled(at(c, (0.0, 5.5)), 1.3, col);
}

pub fn icon_tasks(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-7.0, -7.0), (7.0, -7.0), (7.0, 7.0), (-7.0, 7.0)], 3.0, true, W, col);
    path(p, c, &[(-3.6, 0.2), (-1.0, 2.9), (3.8, -2.8)], 0.0, false, W, col);
}

pub fn icon_bluetooth(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-4.5, -3.8), (4.5, 3.8), (0.0, 8.0), (0.0, -8.0), (4.5, -3.8), (-4.5, 3.8)], 0.0, false, W, col);
}

pub fn icon_sliders(p: &Painter, c: Pos2, col: Color32) {
    seg(p, c, (-7.0, -3.5), (7.0, -3.5), W, col);
    seg(p, c, (-7.0, 3.5), (7.0, 3.5), W, col);
    p.circle_filled(at(c, (-2.5, -3.5)), 2.4, col);
    p.circle_filled(at(c, (3.0, 3.5)), 2.4, col);
}

pub fn icon_cloud(p: &Painter, c: Pos2, col: Color32) {
    path(p, c, &[(-7.5, 4.5), (-7.5, 0.5), (-4.5, -1.5), (-3.0, -5.0), (1.0, -6.5), (4.5, -4.0), (5.5, -1.5), (7.8, 0.0), (7.8, 4.5)], 2.5, true, W, col);
}

/// A small battery with the charge drawn inside; green while charging, red when low.
pub fn battery_gauge(p: &Painter, left_center: Pos2, pct: u8, charging: bool, f: f32) {
    let body = Rect::from_min_size(left_center - vec2(0.0, 5.5), vec2(22.0, 11.0));
    let edge = fade(Color32::from_gray(160), f);
    p.rect_stroke(body, 3.0, Stroke::new(1.0, edge), StrokeKind::Inside);
    p.rect_filled(Rect::from_center_size(pos2(body.right() + 2.0, body.center().y), vec2(2.0, 4.0)), 1.0, edge);
    let level = if charging {
        GREEN
    } else if pct <= 20 {
        RED
    } else {
        Color32::WHITE
    };
    let inner = body.shrink(2.0);
    p.rect_filled(Rect::from_min_size(inner.min, vec2(inner.width() * pct as f32 / 100.0, inner.height())), 1.5, fade(level, f));
}

pub fn icon_pill(p: &Painter, c: Pos2, col: Color32) {
    p.rect_filled(Rect::from_center_size(c, vec2(12.0, 3.0)), 1.5, col);
}

pub fn icon_search(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c + vec2(-1.5, -1.5), 6.0, Stroke::new(1.8, col));
    seg(p, c, (2.9, 2.9), (7.8, 7.8), 1.9, col);
}

pub fn icon_calc(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c, vec2(13.5, 16.0)), 3.0, Stroke::new(W, col), StrokeKind::Inside);
    seg(p, c, (-3.6, -4.6), (3.6, -4.6), W, col);
    for q in [(-3.0, 0.6), (3.0, 0.6), (-3.0, 4.6), (3.0, 4.6)] {
        p.circle_filled(at(c, q), 1.1, col);
    }
}

/// Equalizer bars, driven by time while playing.
pub fn equalizer(p: &Painter, center: Pos2, bars: usize, playing: bool, time: f32, color: Color32, max_h: f32) {
    for i in 0..bars {
        let h = if playing { 3.0 + (max_h - 3.0) * (0.5 + 0.5 * (time * (5.0 + i as f32 * 1.9) + i as f32 * 2.3).sin()) } else { 3.0 };
        let x = center.x + (i as f32 - (bars - 1) as f32 / 2.0) * 5.0;
        p.rect_filled(Rect::from_center_size(pos2(x, center.y), vec2(3.0, h)), 1.5, color);
    }
}

/// Text that scrolls sideways when wider than `max_w`.
pub fn marquee(p: &Painter, pos: Pos2, max_w: f32, text: &str, font: FontId, col: Color32, time: f32) {
    let clip = Rect::from_min_size(pos, vec2(max_w, font.size * 1.5));
    let p = p.with_clip_rect(clip.intersect(p.clip_rect()));
    let galley = p.layout_no_wrap(text.to_owned(), font, col);
    let w = galley.size().x;
    if w <= max_w {
        p.galley(pos, galley, col);
        return;
    }
    // pause at the start, then glide one full cycle
    let gap = 36.0;
    let cycle = (w + gap) / 32.0;
    let ph = (time % (cycle + 1.5) - 1.5).max(0.0);
    let off = ph * 32.0;
    p.galley(pos - vec2(off, 0.0), galley.clone(), col);
    p.galley(pos - vec2(off - w - gap, 0.0), galley, col);
}

/// Horizontal slider. Returns the new value while the user is dragging or clicking.
pub fn slider(ui: &mut Ui, id: egui::Id, rect: Rect, value: f32, f: f32) -> Option<f32> {
    let resp = ui.interact(rect.expand2(vec2(0.0, 6.0)), id, Sense::click_and_drag());
    let v_at = |x: f32| ((x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    let new = (resp.dragged() || resp.clicked()).then(|| resp.interact_pointer_pos().map(|p| v_at(p.x))).flatten();
    let shown = new.unwrap_or(value).clamp(0.0, 1.0);

    let track = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    let x = track.left() + track.width() * shown;
    let r = if resp.hovered() || resp.dragged() { 7.0 } else { 6.0 };
    let p = ui.painter();
    p.rect_filled(track, 2.0, fade(fill(46), f));
    p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.bottom())), 2.0, fade(Color32::from_gray(235), f));
    p.circle_filled(pos2(x, track.center().y + 0.8), r + 0.5, fade(Color32::from_black_alpha(110), f));
    p.circle_filled(pos2(x, track.center().y), r, fade(Color32::WHITE, f));
    new
}

/// iOS style switch. Returns true when clicked.
pub fn toggle(ui: &mut Ui, id: egui::Id, c: Pos2, on: bool, f: f32) -> bool {
    let rect = Rect::from_center_size(c, vec2(28.0, 16.0));
    let resp = ui.interact(rect.expand(3.0), id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let k = ui.ctx().animate_bool_with_time(id.with("k"), on, 0.18);
    let bg = Color32::from_gray(62).lerp_to_gamma(GREEN, k);
    let knob = pos2(rect.left() + 8.0 + 12.0 * k, rect.center().y);
    let p = ui.painter();
    p.rect_filled(rect, 8.0, fade(bg, f));
    p.circle_filled(knob + vec2(0.0, 0.7), 6.7, fade(Color32::from_black_alpha(80), f));
    p.circle_filled(knob, 6.2, fade(Color32::WHITE, f));
    resp.clicked()
}

fn quad(a: Pos2, c: Pos2, b: Pos2, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            pos2(u * u * a.x + 2.0 * u * t * c.x + t * t * b.x, u * u * a.y + 2.0 * u * t * c.y + t * t * b.y)
        })
        .collect()
}

/// A notch: quadratic top corners flaring outward into the screen edge (the "ears",
/// drawn outside `body`'s left and right edges) and quadratic bottom corners.
pub fn notch_shape(p: &Painter, body: Rect, ear: f32, bottom: f32) {
    let (l, r, t, b) = (body.left(), body.right(), body.top(), body.bottom());
    let bottom = bottom.min(body.height() / 2.0);
    let mut pts = vec![pos2(l, t)];
    pts.extend(quad(pos2(l, b - bottom), pos2(l, b), pos2(l + bottom, b), 8));
    pts.extend(quad(pos2(r - bottom, b), pos2(r, b), pos2(r, b - bottom), 8));
    pts.push(pos2(r, t));
    p.add(Shape::convex_polygon(pts, Color32::BLACK, Stroke::NONE));

    for left in [true, false] {
        let curve = if left { quad(pos2(l - ear, t), pos2(l, t), pos2(l, t + ear), 10) } else { quad(pos2(r, t + ear), pos2(r, t), pos2(r + ear, t), 10) };
        let ox = if left { l + 1.0 } else { r - 1.0 };
        let mut m = Mesh::default();
        m.colored_vertex(pos2(ox, t), Color32::BLACK);
        for q in &curve {
            m.colored_vertex(*q, Color32::BLACK);
        }
        for i in 1..curve.len() as u32 {
            m.add_triangle(0, i, i + 1);
        }
        p.add(Shape::mesh(m));
        // hairline stroke antialiases the curved edge, meshes are not feathered
        p.add(Shape::line(curve, Stroke::new(1.0, Color32::BLACK)));
    }
}

/// Dashed rounded rectangle for drop zones.
pub fn dashed_rrect(p: &Painter, r: Rect, rad: f32, stroke: Stroke) {
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    let mut pts = vec![pos2(l + rad, t), pos2(rt - rad, t)];
    pts.extend(quad(pos2(rt - rad, t), pos2(rt, t), pos2(rt, t + rad), 6));
    pts.extend([pos2(rt, t + rad), pos2(rt, b - rad)]);
    pts.extend(quad(pos2(rt, b - rad), pos2(rt, b), pos2(rt - rad, b), 6));
    pts.extend([pos2(rt - rad, b), pos2(l + rad, b)]);
    pts.extend(quad(pos2(l + rad, b), pos2(l, b), pos2(l, b - rad), 6));
    pts.extend([pos2(l, b - rad), pos2(l, t + rad)]);
    pts.extend(quad(pos2(l, t + rad), pos2(l, t), pos2(l + rad, t), 6));
    p.extend(Shape::dashed_line(&pts, stroke, 6.0, 5.0));
}

/// Soft radial glow, fading to transparent at `r`.
pub fn glow(p: &Painter, c: Pos2, r: f32, col: Color32) {
    const N: u32 = 28;
    let mut m = Mesh::default();
    m.colored_vertex(c, col);
    for i in 0..=N {
        let a = i as f32 * std::f32::consts::TAU / N as f32;
        m.colored_vertex(c + vec2(a.cos(), a.sin()) * r, Color32::TRANSPARENT);
    }
    for i in 1..=N {
        m.add_triangle(0, i, i + 1);
    }
    p.add(Shape::mesh(m));
}

/// A thin progress ring: a faint track with `frac` of it drawn from the top, clockwise.
pub fn ring(p: &Painter, c: Pos2, r: f32, frac: f32, col: Color32, track: Color32) {
    p.circle_stroke(c, r, Stroke::new(2.0, track));
    let frac = frac.clamp(0.0, 1.0);
    let n = (frac * 40.0).ceil() as usize;
    if n == 0 {
        return;
    }
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * frac * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::line(pts, Stroke::new(2.0, col)));
}
