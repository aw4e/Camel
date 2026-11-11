use crate::prelude::*;
use chrono::{Datelike, Local, NaiveDate, Timelike};

/// First weekday (Monday = 0) and length of the month `d` is in.
pub(crate) fn month_grid(d: NaiveDate) -> (u32, u32) {
    let first = d.with_day(1).unwrap_or(d);
    let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    let days = NaiveDate::from_ymd_opt(y, m, 1).map_or(30, |n| (n - first).num_days() as u32);
    (first.weekday().num_days_from_monday(), days)
}

/// Rows the month grid needs: 4 to 6.
pub(crate) fn month_rows(d: NaiveDate) -> u32 {
    let (lead, days) = month_grid(d);
    (lead + days).div_ceil(7)
}

pub(crate) fn tab_clock(p: &egui::Painter, o: egui::Pos2, a: f32, h24: bool) {
    let now = Local::now();
    // seconds sit after the measured width of the time, on the same baseline
    let digits = if h24 { now.format("%H:%M") } else { now.format("%-I:%M") }.to_string();
    let time = p.layout_no_wrap(digits, font(44.0), fade(Color32::WHITE, a));
    let suffix = if h24 { String::new() } else { now.format(" %p").to_string() };
    let secs = p.layout_no_wrap(format!("{:02}{suffix}", now.second()), font(16.0), fade(GRAY, a));
    let origin = o + vec2(MARGIN, TOP - 1.0);
    let secs_at = pos2(origin.x + time.size().x + 5.0, origin.y + baseline(&time) - baseline(&secs));
    p.galley(secs_at, secs, Color32::WHITE);
    p.galley(origin, time, Color32::WHITE);
    p.text(o + vec2(MARGIN, 92.0), Align2::LEFT_TOP, now.format("%A").to_string().to_uppercase(), bold(11.0), fade(RED, a));
    p.text(o + vec2(MARGIN, 106.0), Align2::LEFT_TOP, now.format("%d %B %Y").to_string(), font(13.0), fade(Color32::WHITE, a));

    // the whole month, Monday first, in the narrow golden column: red month name, red disc on today, weekends dimmer
    let today = now.date_naive();
    let (lead, days) = month_grid(today);
    let (_, minor) = phi_columns();
    let cell = minor / 7.0;
    let gx = o.x + OPEN_W - MARGIN - minor;
    p.text(pos2(gx + 2.0, o.y + TOP + 8.0), Align2::LEFT_CENTER, now.format("%B").to_string(), bold(12.0), fade(RED, a));
    p.text(pos2(o.x + OPEN_W - MARGIN, o.y + TOP + 8.0), Align2::RIGHT_CENTER, now.format("%Y").to_string(), font(11.0), fade(DIM, a));
    for (i, letter) in ["M", "T", "W", "T", "F", "S", "S"].iter().enumerate() {
        p.text(pos2(gx + cell / 2.0 + i as f32 * cell, o.y + TOP + 21.0), Align2::CENTER_CENTER, letter, bold(9.0), fade(DIM, a));
    }
    for d in 1..=days {
        let idx = lead + d - 1;
        let c = pos2(gx + cell / 2.0 + (idx % 7) as f32 * cell, o.y + CAL_TOP + (idx / 7) as f32 * CAL_ROW_H);
        let is_today = d == today.day();
        if is_today {
            p.circle_filled(c, CAL_DISC, fade(RED, a));
        }
        let col = if is_today {
            Color32::WHITE
        } else if idx % 7 >= 5 {
            GRAY
        } else {
            Color32::from_gray(225)
        };
        p.text(c, Align2::CENTER_CENTER, d, if is_today { bold(10.0) } else { font(10.0) }, fade(col, a));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_grid_knows_its_rows() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(month_grid(d(2026, 11, 11)), (6, 30)); // November 2026 starts on a Sunday
        assert_eq!(month_rows(d(2026, 11, 11)), 6);
        assert_eq!(month_rows(d(2026, 8, 3)), 6);
        assert_eq!(month_rows(d(2026, 12, 5)), 5);
        assert_eq!(month_rows(d(2026, 2, 1)), 5);
        assert_eq!(month_rows(d(2027, 2, 9)), 4); // 28 days starting on a Monday
        assert_eq!(month_grid(d(2026, 12, 31)).1, 31); // December rolls into the next year
    }
}
