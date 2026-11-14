use crate::app::Notch;
use crate::monitor::format_rate;
use crate::prelude::*;
use std::collections::VecDeque;

const TILES: usize = 5;
/// A tile: label, value, graph (its history and the value at its top), colour and hover text.
type Tile<'a> = (&'a str, String, Option<(&'a VecDeque<f32>, f32)>, Color32, String);
const INDIGO: Color32 = Color32::from_rgb(94, 92, 230);

/// The history as a line across `rect`, newest on the right. `max` is the value at the top.
fn spark(p: &egui::Painter, rect: Rect, hist: &VecDeque<f32>, max: f32, color: Color32, a: f32) {
    if hist.len() < 2 {
        return;
    }
    let step = rect.width() / (crate::monitor::HISTORY - 1) as f32;
    let first = rect.right() - step * (hist.len() - 1) as f32;
    let points = hist.iter().enumerate().map(|(i, v)| pos2(first + step * i as f32, rect.bottom() - rect.height() * (v / max).clamp(0.0, 1.0))).collect();
    p.add(egui::Shape::line(points, Stroke::new(1.3, fade(color, a))));
}

/// The top of a graph that shows bytes: the biggest sample, but never so small that a trickle looks like a flood.
fn rate_max(hist: &VecDeque<f32>) -> f32 {
    hist.iter().copied().fold(64.0 * 1024.0, f32::max)
}

impl Notch {
    /// How the PC is doing: CPU, memory, disk, network and battery as tiles with a small graph, the last minute of CPU and
    /// memory, and the programs that use the CPU most. The extra readings only run while this page is open.
    pub(crate) fn tab_stats(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        self.monitor.refresh_detail();
        let p = ui.painter().clone();
        let (s, d) = (&self.monitor.stats, &self.monitor.detail);

        let w = (OPEN_W - 2.0 * MARGIN - GAP_S * (TILES - 1) as f32) / TILES as f32;
        let battery_color = match s.battery {
            Some(b) if s.charging || b > 20 => GREEN,
            Some(b) if b > 15 => ORANGE,
            Some(_) => RED,
            None => DIM, // a desktop PC: no battery to warn about
        };
        // (label, value, history and its top, colour, hover text)
        let tiles: [Tile; TILES] = [
            ("CPU", format!("{:.0}%", s.cpu), Some((&s.cpu_hist, 100.0)), BLUE, format!("{:.0}% of the processor in use", s.cpu)),
            ("Memory", format!("{:.0}%", s.ram), Some((&s.ram_hist, 100.0)), ORANGE, format!("{:.1} of {:.1} GB in use", s.ram_gb.0, s.ram_gb.1)),
            ("Disk", format_rate(d.disk), Some((&d.disk_hist, rate_max(&d.disk_hist))), GREEN, "Read and write, all drives".to_owned()),
            (
                "Network",
                format_rate(d.net_down + d.net_up),
                Some((&d.net_hist, rate_max(&d.net_hist))),
                INDIGO,
                format!("Down {}, up {}", format_rate(d.net_down), format_rate(d.net_up)),
            ),
            (
                "Battery",
                s.battery.map_or("AC".to_owned(), |b| format!("{b}%")),
                None,
                battery_color,
                s.battery.map_or("No battery".to_owned(), |b| format!("{b}%{}", if s.charging { ", charging" } else { "" })),
            ),
        ];
        for (i, (label, value, hist, color, tip)) in tiles.iter().enumerate() {
            let tile = Rect::from_min_size(o + vec2(MARGIN + i as f32 * (w + GAP_S), TOP), vec2(w, STATS_TILE_H));
            p.rect_filled(tile, 10.0, fade(fill(14), a));
            p.text(tile.left_top() + vec2(8.0, 10.0), Align2::LEFT_CENTER, label, bold(9.5), fade(GRAY, a));
            p.text(tile.left_top() + vec2(8.0, 23.0), Align2::LEFT_CENTER, value, bold(12.5), fade(Color32::WHITE, a));
            let strip = Rect::from_x_y_ranges(tile.left() + 8.0..=tile.right() - 8.0, tile.bottom() - 15.0..=tile.bottom() - 6.0);
            match hist {
                Some((h, max)) => spark(&p, strip, h, *max, *color, a),
                None => {
                    // battery: a bar instead of a graph
                    let bar = Rect::from_x_y_ranges(strip.x_range(), strip.bottom() - 3.0..=strip.bottom());
                    p.rect_filled(bar, 1.5, fade(fill(30), a));
                    let level = f32::from(s.battery.unwrap_or(100)) / 100.0;
                    p.rect_filled(Rect::from_x_y_ranges(bar.left()..=bar.left() + bar.width() * level, bar.y_range()), 1.5, fade(*color, a));
                }
            }
            ui.interact(tile, ui.scope_id().with(("stat", i)), Sense::hover()).on_hover_text(tip.as_str());
        }

        // below: the last minute, and the busiest programs
        let (major, minor) = phi_columns();
        let top = o.y + TOP + STATS_TILE_H + GAP;
        let history = Rect::from_min_size(pos2(o.x + MARGIN, top), vec2(major, STATS_PANEL_H));
        let busiest = Rect::from_min_size(pos2(history.right() + GAP_L, top), vec2(minor, STATS_PANEL_H));
        for panel in [history, busiest] {
            p.rect_filled(panel, 10.0, fade(fill(14), a));
        }

        p.text(history.left_top() + vec2(10.0, 11.0), Align2::LEFT_CENTER, "Usage history", bold(9.5), fade(GRAY, a));
        for (n, (name, color)) in [("CPU", BLUE), ("Memory", ORANGE)].iter().enumerate() {
            let x = history.right() - 10.0 - (1 - n) as f32 * 52.0;
            p.circle_filled(pos2(x - 40.0, history.top() + 11.0), 2.5, fade(*color, a));
            p.text(pos2(x - 34.0, history.top() + 11.0), Align2::LEFT_CENTER, name, font(9.0), fade(GRAY, a));
        }
        let graph = Rect::from_x_y_ranges(history.left() + 10.0..=history.right() - 10.0, history.top() + 24.0..=history.bottom() - 9.0);
        p.hline(graph.x_range(), graph.bottom(), Stroke::new(1.0, fade(fill(24), a)));
        p.hline(graph.x_range(), graph.center().y, Stroke::new(1.0, fade(fill(18), a)));
        spark(&p, graph, &s.ram_hist, 100.0, ORANGE, a);
        spark(&p, graph, &s.cpu_hist, 100.0, BLUE, a);

        p.text(busiest.left_top() + vec2(10.0, 11.0), Align2::LEFT_CENTER, "Top processes", bold(9.5), fade(GRAY, a));
        for (i, (name, cpu)) in d.top.iter().enumerate() {
            let y = busiest.top() + 26.0 + i as f32 * 10.5;
            p.text(pos2(busiest.left() + 10.0, y), Align2::LEFT_CENTER, shorten(name, 13), font(9.0), fade(Color32::WHITE, a));
            p.text(pos2(busiest.right() - 10.0, y), Align2::RIGHT_CENTER, format!("{cpu:.1}%"), font(9.0), fade(GRAY, a));
        }
        if d.top.is_empty() {
            p.text(busiest.center(), Align2::CENTER_CENTER, "Reading...", font(9.0), fade(DIM, a));
        }
    }
}
