use crate::app::Notch;
use crate::mixer;
use crate::prelude::*;
use crate::radios::Kind;
use crate::weather::{Status, describe};

type Icon = fn(&egui::Painter, egui::Pos2, Color32);

/// Width of the weather card on the left; the tiles take the rest.
const CARD_W: f32 = 164.0;

impl Notch {
    /// One panel for the quick things, like a control center: the weather on the left, and on the right tiles for Wi-Fi,
    /// Bluetooth, the microphone, sound, keep awake and the minimal notch. A lit tile is on; a click flips it.
    pub(crate) fn tab_controls(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        self.radios.watch();
        if self.controls_at.elapsed() > Duration::from_secs(2) {
            self.controls_at = Instant::now();
            (self.mic_muted, self.speakers_muted) = (mixer::is_muted(true), mixer::is_muted(false));
        }
        let radio = self.radios.state();
        let p = ui.painter().clone();

        self.weather_card(ui, Rect::from_min_size(o + vec2(MARGIN, TOP), vec2(CARD_W, 3.0 * CONTROLS_TILE_H + 2.0 * GAP)), a);

        // (label, on or None when this PC cannot do it, lit colour, icon)
        let tiles: [(&str, Option<bool>, Color32, Icon); 6] = [
            ("Wi-Fi", radio.wifi, accent_color(), ui::icon_wifi),
            ("Bluetooth", radio.bluetooth, accent_color(), ui::icon_bluetooth),
            ("Microphone", self.mic_muted.map(|m| !m), accent_color(), ui::icon_mic),
            ("Sound", self.speakers_muted.map(|m| !m), accent_color(), ui::icon_speaker),
            ("Keep awake", Some(self.awake), accent_color(), ui::icon_sun),
            ("Minimal notch", Some(self.settings.minimal), Color32::from_rgb(94, 92, 230), ui::icon_pill),
        ];
        let left = o.x + MARGIN + CARD_W + GAP_L;
        let w = (o.x + OPEN_W - MARGIN - left - GAP) / 2.0;
        let mut clicked = None;
        for (i, (label, on, color, icon)) in tiles.iter().enumerate() {
            let (col, row) = (i % 2, i / 2);
            let tile = Rect::from_min_size(pos2(left + col as f32 * (w + GAP), o.y + TOP + row as f32 * (CONTROLS_TILE_H + GAP)), vec2(w, CONTROLS_TILE_H));
            let resp = hit(ui, &format!("ctl{i}"), tile).on_hover_text(if on.is_none() { "Not available on this PC" } else { *label });
            let lit = *on == Some(true);
            let available = on.is_some();
            let bg = if lit { fade(*color, if resp.hovered() { 0.34 } else { 0.24 }) } else { fill(if resp.hovered() && available { 20 } else { 14 }) };
            p.rect_filled(tile, 12.0, fade(bg, a));
            let badge = pos2(tile.left() + 8.0 + 12.0, tile.center().y);
            p.circle_filled(badge, 12.0, fade(if lit { *color } else { fill(26) }, a));
            icon(
                &p,
                badge,
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
            let room = tile.right() - GAP - (badge.x + 12.0 + GAP);
            let color = fade(if available { Color32::WHITE } else { DIM }, a);
            // a connected Bluetooth device with a battery: its name under the label and the level on the right
            let device = (i == 1 && lit).then(|| radio.batteries.first()).flatten();
            let galley = crate::widgets::fit_bold(&p, label, 11.5, room, color);
            let ty = if device.is_some() { tile.center().y - galley.size().y / 2.0 - 5.0 } else { tile.center().y - galley.size().y / 2.0 };
            p.galley(pos2(badge.x + 12.0 + GAP, ty), galley, color);
            if let Some((name, pct)) = device {
                let level = format!("{pct}%");
                let lw = p.layout_no_wrap(level.clone(), bold(10.0), fade(GRAY, a)).size().x;
                p.text(pos2(tile.right() - GAP, tile.center().y + 6.0), Align2::RIGHT_CENTER, level, bold(10.0), fade(if *pct <= 20 { RED } else { GRAY }, a));
                // the name never runs into the level: it is cut with an ellipsis to the room left of it
                let space = room - lw - 6.0;
                let mut shown = name.clone();
                let mut sub = p.layout_no_wrap(shown.clone(), font(9.0), fade(GRAY, a));
                while sub.size().x > space && shown.pop().is_some() {
                    sub = p.layout_no_wrap(format!("{}…", shown.trim_end()), font(9.0), fade(GRAY, a));
                }
                p.galley(pos2(badge.x + 12.0 + GAP, tile.center().y + 6.0 - sub.size().y / 2.0), sub, fade(GRAY, a));
            }
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
            Some(5) => self.set_minimal(!self.settings.minimal),
            _ => {}
        }
    }

    /// The weather where you are: the temperature large, the conditions and humidity under it. A click switches it on or off.
    fn weather_card(&mut self, ui: &mut egui::Ui, card: Rect, a: f32) {
        let p = ui.painter().clone();
        let resp = hit(ui, "weather-card", card).on_hover_text(if self.settings.weather {
            "Click to switch the weather off"
        } else {
            "Click to switch the weather on"
        });
        p.rect_filled(card, 16.0, fade(if resp.hovered() { fill(18) } else { fill(14) }, a));
        if resp.clicked() {
            self.set_weather(!self.settings.weather);
        }
        let x = card.left() + 16.0;
        let note = |title: &str, hint: &str| {
            p.text(card.center() - vec2(0.0, 7.0), Align2::CENTER_CENTER, title, bold(11.0), fade(Color32::WHITE, a));
            p.text(card.center() + vec2(0.0, 8.0), Align2::CENTER_CENTER, hint, font(10.0), fade(DIM, a));
        };
        match self.weather.status() {
            Status::Ready(w) => {
                if !w.place.is_empty() {
                    p.text(pos2(x, card.top() + 18.0), Align2::LEFT_CENTER, w.place.to_uppercase(), bold(9.5), fade(DIM, a));
                }
                p.text(pos2(x, card.top() + 50.0), Align2::LEFT_CENTER, format!("{:.0}°", w.temp), font(40.0), fade(Color32::WHITE, a));
                p.text(pos2(x, card.top() + 82.0), Align2::LEFT_CENTER, describe(w.code), bold(12.0), fade(Color32::WHITE, a));
                p.text(pos2(x, card.top() + 99.0), Align2::LEFT_CENTER, format!("{}% humidity", w.humidity), font(10.5), fade(GRAY, a));
            }
            Status::Loading => note("Weather", "Loading"),
            Status::NoLocation => note("No location", "Try: city bandung"),
            Status::Failed(_) => note("Weather offline", "Check the connection"),
            Status::Off => note("Weather is off", "Click to switch it on"),
        }
    }
}
