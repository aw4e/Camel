use eframe::egui::{Vec2, vec2};

pub(crate) const PILL_W: f32 = 196.0;
pub(crate) const PILL_H: f32 = 34.0;
pub(crate) const OPEN_W: f32 = 460.0;
pub(crate) const OPEN_H: f32 = 136.0;
/// Settings lists six switches per column, two rows (21 px each) more than the other pages.
pub(crate) const SETTINGS_H: f32 = OPEN_H + 42.0;
/// Transparent margin each side of the body, room for the flared top corners.
pub(crate) const EAR: f32 = 19.0;
/// The closed notch in minimal mode: a thin line in a taller, transparent window so it is easy to hover.
pub(crate) const MINIMAL: Vec2 = vec2(72.0, 5.0);
pub(crate) const MINIMAL_WIN_H: f32 = 14.0;
pub(crate) const TOAST: Vec2 = vec2(300.0, 44.0);
// Outer margins sit on a 4 px grid. Proportions use the golden ratio, and inner gaps come from the Fibonacci scale
// (3, 5, 8, 13, 21, 34), whose neighbouring steps keep that ratio, so nested paddings never look arbitrary.
// The tests at the bottom of this file pin these numbers.
pub(crate) const PHI: f32 = 1.618_034;
/// Left and right margin of every screen.
pub(crate) const MARGIN: f32 = 16.0;
/// Content starts one Fibonacci step (5) below the tab bar, which ends at y = 30.
pub(crate) const TOP: f32 = 35.0;
/// Fibonacci steps for the gaps between things.
pub(crate) const GAP_S: f32 = 5.0;
pub(crate) const GAP: f32 = 8.0;
pub(crate) const GAP_L: f32 = 13.0;
/// Text inset inside a list row or a settings group.
pub(crate) const ROW_PAD: f32 = 8.0;
/// List rows are 22 px tall on a 24 px pitch.
pub(crate) const ROW_H: f32 = 22.0;
pub(crate) const ROW_PITCH: f32 = 24.0;
/// First row of the clipboard list (below its 12 px caption and a gap) and of the apps list (below 20 px chips).
pub(crate) const CLIP_TOP: f32 = TOP + 12.0 + GAP;
pub(crate) const APPS_TOP: f32 = TOP + 20.0 + GAP_S;
/// Month grid: weekday letters, first row, row pitch and the radius of the disc behind today.
pub(crate) const CAL_TOP: f32 = TOP + 35.0;
pub(crate) const CAL_ROW_H: f32 = 13.0;
pub(crate) const CAL_DISC: f32 = 8.5;

/// The content width cut in the golden ratio with a Fibonacci gutter between the parts: (major, minor).
pub(crate) fn phi_columns() -> (f32, f32) {
    let w = OPEN_W - 2.0 * MARGIN - GAP_L;
    let minor = w / (1.0 + PHI);
    (w - minor, minor)
}
/// A tab icon is 14 px (a 16 px drawing scaled by `ui::icon_fit`), with one Fibonacci step (5 px) above and below: 14 + 2 x 5.
pub(crate) const TAB_PILL_H: f32 = 24.0;
/// A tab that is not chosen is an icon in a pill the height plus one Fibonacci step (5) wide. The chosen tab grows to also
/// show its name beside the icon.
pub(crate) const TAB_W: f32 = 29.0;
/// Extra space after every second tab (audio, time, storage, apps): a Fibonacci step.
pub(crate) const GROUP_GAP: f32 = 8.0;
/// The track around the pills: the pill inset is 2 px, so its height is the pill height + 4.
pub(crate) const TAB_INSET: f32 = 2.0;

/// The width of the chosen tab: the icon cell, the name, and one standard gap after it.
pub(crate) fn tab_open_w(label_w: f32) -> f32 {
    TAB_W - GAP_S + 2.0 + label_w + GAP
}

/// Left edge of each tab inside the track, with the extra gap after every second one.
pub(crate) fn tab_offsets(widths: &[f32]) -> Vec<f32> {
    let mut x = 0.0;
    widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let at = x + (i / 2) as f32 * GROUP_GAP;
            x += w;
            at
        })
        .collect()
}

pub(crate) const SPOT_W: f32 = 480.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::MAIN_TABS;
    use crate::ui;

    #[test]
    fn content_columns_follow_the_golden_ratio() {
        let (major, minor) = phi_columns();
        assert!((major / minor - PHI).abs() < 0.005, "{major} : {minor}");
        assert!((major + minor + GAP_L - (OPEN_W - 2.0 * MARGIN)).abs() < 0.01);
        // the timer's controls (presets, stepper, reset) end inside the wide column
        let controls_end = MARGIN + (3.0 * 38.0 + 4.0) + GAP + (2.0 * 26.0 + 4.0) + GAP_L + 44.0;
        assert!(controls_end <= MARGIN + major, "controls end at {controls_end}");
    }

    #[test]
    fn spacing_comes_from_the_scale() {
        for g in [GAP_S, GAP, GAP_L, TOP - 30.0] {
            assert!([3.0, 5.0, 8.0, 13.0, 21.0, 34.0].contains(&g), "{g}");
        }
        assert_eq!(MARGIN % 4.0, 0.0);
        assert_eq!(ROW_H + 2.0, ROW_PITCH);
    }

    #[test]
    fn every_screen_keeps_a_bottom_margin() {
        let room = |bottom: f32, height: f32| height - bottom;
        let list = |top: f32| top + 2.0 * ROW_PITCH + ROW_H;
        assert!(room(list(CLIP_TOP), OPEN_H) >= GAP_S, "clipboard");
        assert!(room(list(APPS_TOP), OPEN_H) >= GAP_S, "apps");
        assert!(room(list(APPS_TOP + ROW_PITCH) - ROW_PITCH, OPEN_H) >= GAP_S, "apps with the leftovers banner");
        assert!(room(TOP + 90.0, OPEN_H) >= GAP_S, "shelf");
        assert!(room(45.0 + 6.0 * 21.0, SETTINGS_H) >= GAP_S, "settings, six rows");
        assert!(room(TOP + 75.0 + 18.0, OPEN_H) >= GAP_S, "media controls");
        assert!(room(TOP + 38.0 + 2.0 * ROW_PITCH + 7.0, OPEN_H) >= GAP_S, "mixer");
        let month = |rows: f32| CAL_TOP + (rows - 1.0) * CAL_ROW_H + CAL_DISC;
        const { assert!(CAL_DISC * 2.0 >= 13.0, "the disc is wider than a row of text") };
        const { assert!(CAL_DISC <= CAL_ROW_H - 4.0, "the disc stays clear of the digits in the rows above and below") };
        assert!(room(month(5.0), OPEN_H) >= GAP_S, "calendar, five rows");
        assert!(room(month(6.0), OPEN_H + 14.0) >= GAP_S, "calendar, six rows");
    }

    #[test]
    fn tab_bar_follows_the_rules() {
        // a 16 px drawing scaled to fit leaves one Fibonacci step (5 px) above and below
        assert_eq!(TAB_PILL_H - 16.0 * ui::icon_fit(TAB_PILL_H), 2.0 * GAP_S);
        // a collapsed pill is the height plus one Fibonacci step
        assert_eq!(TAB_W, TAB_PILL_H + GAP_S);
        // gaps come from the Fibonacci scale
        assert!([3.0, 5.0, 8.0, 13.0, 21.0, 34.0].contains(&GROUP_GAP));
        // the bar leaves room for the battery readout on its right (leftmost at x = 358), even with the widest name chosen
        // ("Clipboard" in bold 11 px is about 56 px wide)
        let widest = tab_open_w(56.0);
        let widths: Vec<f32> = (0..MAIN_TABS.len()).map(|i| if i == 0 { widest } else { TAB_W }).collect();
        let offsets = tab_offsets(&widths);
        let track_right = 12.0 + offsets[MAIN_TABS.len() - 1] + TAB_W + 2.0 * TAB_INSET;
        assert!(track_right + 8.0 <= 358.0, "tab bar ends at {track_right}");
        assert_eq!(tab_offsets(&[10.0, 10.0, 10.0]), [0.0, 10.0, 28.0], "the gap comes after every second tab");
    }
}
