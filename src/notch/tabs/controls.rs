use crate::mixer;
use crate::notch::Notch;
use crate::prelude::*;
use crate::radios::Kind;

type Icon = fn(&egui::Painter, egui::Pos2, Color32);

impl Notch {
    /// Quick controls in a grid of tiles, like a control center: a lit tile is on, a dim one is off, and a click flips it.
    pub(crate) fn tab_controls(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        self.radios.watch();
        if self.controls_at.elapsed() > Duration::from_secs(2) {
            self.controls_at = Instant::now();
            (self.mic_muted, self.speakers_muted) = (mixer::is_muted(true), mixer::is_muted(false));
        }
        let radio = self.radios.state();

        // (label, on or None when this PC cannot do it, subtitle, lit colour, icon)
        let yes_no = |on: Option<bool>, yes: &'static str, no: &'static str| match on {
            Some(true) => yes,
            Some(false) => no,
            None => "Not available",
        };
        let tiles: [(&str, Option<bool>, &str, Color32, Icon); 6] = [
            ("Wi-Fi", radio.wifi, yes_no(radio.wifi, "On", "Off"), BLUE, ui::icon_wifi),
            ("Bluetooth", radio.bluetooth, yes_no(radio.bluetooth, "On", "Off"), BLUE, ui::icon_bluetooth),
            ("Microphone", self.mic_muted.map(|m| !m), yes_no(self.mic_muted.map(|m| !m), "Live", "Muted"), BLUE, ui::icon_mic),
            ("Sound", self.speakers_muted.map(|m| !m), yes_no(self.speakers_muted.map(|m| !m), "On", "Muted"), BLUE, ui::icon_speaker),
            ("Keep awake", Some(self.awake), if self.awake { "On" } else { "Off" }, GREEN, ui::icon_sun),
            ("Weather", Some(self.settings.weather), if self.settings.weather { "On" } else { "Off" }, Color32::from_rgb(94, 92, 230), ui::icon_cloud),
        ];

        let p = ui.painter().clone();
        let (cols, gap) = (3, GAP);
        let w = (OPEN_W - 2.0 * MARGIN - gap * (cols as f32 - 1.0)) / cols as f32;
        let mut clicked = None;
        for (i, (label, on, sub, color, icon)) in tiles.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let tile = Rect::from_min_size(o + vec2(MARGIN + col as f32 * (w + gap), TOP + row as f32 * (40.0 + gap)), vec2(w, 40.0));
            let resp = hit(ui, &format!("ctl{i}"), tile);
            let lit = *on == Some(true);
            let available = on.is_some();
            let bg = if lit { fade(*color, if resp.hovered() { 0.34 } else { 0.24 }) } else { fill(if resp.hovered() && available { 20 } else { 14 }) };
            p.rect_filled(tile, 12.0, fade(bg, a));
            let badge = Rect::from_center_size(pos2(tile.left() + 8.0 + 14.0, tile.center().y), Vec2::splat(28.0));
            p.circle_filled(badge.center(), 14.0, fade(if lit { *color } else { fill(26) }, a));
            icon(
                &p,
                badge.center(),
                fade(
                    if lit {
                        Color32::WHITE
                    } else if available {
                        GRAY
                    } else {
                        DIM
                    },
                    a,
                ),
            );
            let text_x = badge.right() + GAP;
            p.with_clip_rect(tile).text(
                pos2(text_x, tile.center().y - 6.5),
                Align2::LEFT_CENTER,
                label,
                bold(12.0),
                fade(if available { Color32::WHITE } else { DIM }, a),
            );
            p.with_clip_rect(tile).text(
                pos2(text_x, tile.center().y + 7.5),
                Align2::LEFT_CENTER,
                sub,
                font(10.5),
                fade(if lit { Color32::from_gray(210) } else { DIM }, a),
            );
            if resp.clicked() && available {
                clicked = Some(i);
            }
        }
        match clicked {
            Some(0) => self.radios.set(Kind::WiFi, radio.wifi != Some(true)),
            Some(1) => self.radios.set(Kind::Bluetooth, radio.bluetooth != Some(true)),
            Some(2) => self.mic_muted = mixer::toggle_mute(true),
            Some(3) => self.speakers_muted = mixer::toggle_mute(false),
            Some(4) => self.set_awake(!self.awake),
            Some(5) => self.set_weather(!self.settings.weather),
            _ => {}
        }
    }
}
