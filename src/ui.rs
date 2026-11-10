//! Drawing helpers: vector icons and small widgets. All colours arrive pre-faded by the caller.

use eframe::egui::{
    self, Color32, FontFamily, FontId, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

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

fn tri(p: &Painter, pts: [Pos2; 3], col: Color32) {
    p.add(Shape::convex_polygon(pts.to_vec(), col, Stroke::NONE));
}

fn bar(p: &Painter, c: Pos2, w: f32, h: f32, col: Color32) {
    p.rect_filled(Rect::from_center_size(c, vec2(w, h)), 1.5, col);
}

pub fn icon_play(p: &Painter, c: Pos2, col: Color32) {
    tri(p, [c + vec2(-6.0, -10.0), c + vec2(-6.0, 10.0), c + vec2(10.0, 0.0)], col);
}

pub fn icon_pause(p: &Painter, c: Pos2, col: Color32) {
    bar(p, c + vec2(-5.0, 0.0), 5.5, 18.0, col);
    bar(p, c + vec2(5.0, 0.0), 5.5, 18.0, col);
}

pub fn icon_next(p: &Painter, c: Pos2, col: Color32) {
    tri(p, [c + vec2(-8.0, -6.5), c + vec2(-8.0, 6.5), c + vec2(2.0, 0.0)], col);
    bar(p, c + vec2(5.5, 0.0), 2.5, 13.0, col);
}

pub fn icon_prev(p: &Painter, c: Pos2, col: Color32) {
    tri(p, [c + vec2(8.0, -6.5), c + vec2(8.0, 6.5), c + vec2(-2.0, 0.0)], col);
    bar(p, c + vec2(-5.5, 0.0), 2.5, 13.0, col);
}

fn line(p: &Painter, pts: &[Pos2], w: f32, col: Color32) {
    p.add(Shape::line(pts.to_vec(), Stroke::new(w, col)));
}

pub fn icon_home(p: &Painter, c: Pos2, col: Color32) {
    line(p, &[c + vec2(-7.0, 0.0), c + vec2(0.0, -6.5), c + vec2(7.0, 0.0)], 1.6, col);
    line(p, &[c + vec2(-5.0, -1.5), c + vec2(-5.0, 6.0), c + vec2(5.0, 6.0), c + vec2(5.0, -1.5)], 1.6, col);
}

pub fn icon_tray(p: &Painter, c: Pos2, col: Color32) {
    line(p, &[c + vec2(-7.0, -1.0), c + vec2(-7.0, 6.0), c + vec2(7.0, 6.0), c + vec2(7.0, -1.0)], 1.6, col);
    line(p, &[c + vec2(-3.0, -2.0), c + vec2(0.0, 1.5), c + vec2(3.0, -2.0)], 1.6, col);
    line(p, &[c + vec2(0.0, -7.0), c + vec2(0.0, 1.5)], 1.6, col);
}

pub fn icon_clock(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 7.0, Stroke::new(1.6, col));
    line(p, &[c + vec2(0.0, -4.0), c, c + vec2(3.5, 2.0)], 1.6, col);
}

pub fn icon_speaker(p: &Painter, c: Pos2, col: Color32) {
    p.add(Shape::convex_polygon(
        vec![c + vec2(-7.0, -2.5), c + vec2(-3.5, -2.5), c + vec2(0.0, -6.0), c + vec2(0.0, 6.0), c + vec2(-3.5, 2.5), c + vec2(-7.0, 2.5)],
        col,
        Stroke::NONE,
    ));
    for (r, s) in [(3.5_f32, 1.4_f32), (6.5, 1.4)] {
        let arc: Vec<Pos2> = (-4..=4).map(|i| {
            let a = i as f32 * 0.2;
            c + vec2(1.0 + r * a.cos(), r * a.sin())
        }).collect();
        line(p, &arc, s, col);
    }
}

pub fn icon_gear(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c, 3.0, Stroke::new(1.6, col));
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        let (s, co) = a.sin_cos();
        line(p, &[c + vec2(co, s) * 5.2, c + vec2(co, s) * 7.2], 2.2, col);
    }
}

pub fn icon_note(p: &Painter, c: Pos2, col: Color32, scale: f32) {
    let s = |v: Vec2| c + v * scale;
    p.circle_filled(s(vec2(-4.0, 6.0)), 3.5 * scale, col);
    line(p, &[s(vec2(-0.7, 6.0)), s(vec2(-0.7, -8.0)), s(vec2(6.0, -6.0))], 2.0 * scale, col);
}

pub fn icon_clipboard(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 1.0), vec2(14.0, 16.0)), 3.0, Stroke::new(1.6, col), StrokeKind::Inside);
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, -7.0), vec2(7.0, 4.0)), 1.5, col);
}

pub fn icon_battery(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c, vec2(14.0, 8.0)), 2.0, Stroke::new(1.6, col), StrokeKind::Inside);
    p.rect_filled(Rect::from_center_size(c + vec2(8.0, 0.0), vec2(2.0, 4.0)), 1.0, col);
}

pub fn icon_mic(p: &Painter, c: Pos2, col: Color32) {
    p.rect_filled(Rect::from_center_size(c + vec2(0.0, -2.0), vec2(6.0, 11.0)), 3.0, col);
    line(p, &[c + vec2(-5.0, 1.0), c + vec2(-5.0, 3.0), c + vec2(0.0, 7.0), c + vec2(5.0, 3.0), c + vec2(5.0, 1.0)], 1.4, col);
}

pub fn icon_search(p: &Painter, c: Pos2, col: Color32) {
    p.circle_stroke(c + vec2(-1.5, -1.5), 6.0, Stroke::new(2.0, col));
    line(p, &[c + vec2(3.0, 3.0), c + vec2(8.0, 8.0)], 2.2, col);
}

pub fn icon_calc(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c, vec2(14.0, 16.0)), 3.0, Stroke::new(1.6, col), StrokeKind::Inside);
    line(p, &[c + vec2(-3.0, -3.0), c + vec2(3.0, -3.0)], 1.6, col);
    line(p, &[c + vec2(-3.0, 2.0), c + vec2(3.0, 2.0)], 1.6, col);
}

pub fn icon_file(p: &Painter, c: Pos2, col: Color32) {
    line(p, &[c + vec2(-5.0, -8.0), c + vec2(2.0, -8.0), c + vec2(6.0, -4.0), c + vec2(6.0, 8.0), c + vec2(-5.0, 8.0), c + vec2(-5.0, -8.0)], 1.6, col);
}

pub fn icon_app(p: &Painter, c: Pos2, col: Color32) {
    p.rect_stroke(Rect::from_center_size(c, vec2(15.0, 15.0)), 4.0, Stroke::new(1.6, col), StrokeKind::Inside);
    p.circle_filled(c, 2.0, col);
}

/// Equalizer bars, driven by time while playing.
pub fn equalizer(p: &Painter, center: Pos2, bars: usize, playing: bool, time: f32, color: Color32, max_h: f32) {
    for i in 0..bars {
        let h = if playing {
            3.0 + (max_h - 3.0) * (0.5 + 0.5 * (time * (5.0 + i as f32 * 1.9) + i as f32 * 2.3).sin())
        } else {
            3.0
        };
        let x = center.x + (i as f32 - (bars - 1) as f32 / 2.0) * 5.0;
        p.rect_filled(Rect::from_center_size(pos2(x, center.y), vec2(3.0, h)), 1.5, color);
    }
}

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
    let rect = Rect::from_center_size(c, vec2(30.0, 18.0));
    let resp = ui.interact(rect.expand(3.0), id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let k = ui.ctx().animate_bool_with_time(id.with("k"), on, 0.18);
    let bg = Color32::from_gray(62).lerp_to_gamma(GREEN, k);
    let knob = pos2(rect.left() + 9.0 + 12.0 * k, rect.center().y);
    let p = ui.painter();
    p.rect_filled(rect, 9.0, fade(bg, f));
    p.circle_filled(knob + vec2(0.0, 0.8), 7.5, fade(Color32::from_black_alpha(80), f));
    p.circle_filled(knob, 7.0, fade(Color32::WHITE, f));
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

/// Atoll's `NotchShape`: quadratic top corners flaring outward into the screen edge (the "ears",
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
        let curve = if left {
            quad(pos2(l - ear, t), pos2(l, t), pos2(l, t + ear), 10)
        } else {
            quad(pos2(r, t + ear), pos2(r, t), pos2(r + ear, t), 10)
        };
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
