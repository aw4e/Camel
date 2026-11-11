use crate::app::Notch;
use crate::channels;
use crate::mixer;
use crate::prelude::*;

/// Height of a strip: name, level, fader with its meter, mute button and the apps in the channel.
const STRIP_H: f32 = 152.0;
const FADER: (f32, f32) = (34.0, 100.0);

impl Notch {
    /// A channel mixer in the style of Elgato Wave Link: apps are grouped into channels, a fader per channel sets the
    /// volume of every app in it, and a click on an app name moves it to the next channel. The last strip is the master.
    pub(crate) fn tab_mixer(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        if self.mixer.is_none() {
            self.mixer = mixer::Mixer::new().ok();
        }
        let p = ui.painter().clone();
        let Some(mx) = self.mixer.as_mut() else {
            p.text(o + vec2(OPEN_W / 2.0, 90.0), Align2::CENTER_CENTER, "Audio unavailable", bold(13.0), fade(GRAY, a));
            return;
        };
        // the faders show their dragged value, so the list of apps is only read again when no button is held
        if self.mixer_at.elapsed() > Duration::from_millis(400) && !ui.input(|i| i.pointer.any_down()) {
            self.mixer_at = Instant::now();
            let _ = mx.refresh();
            self.speakers_muted = mixer::is_muted(false);
        }
        mx.read_peaks();
        ui.ctx().request_repaint_after(Duration::from_millis(50));

        let snapshot: Vec<(String, f32, bool, f32)> = mx.apps.iter().map(|app| (app.name.clone(), app.level, app.muted, app.peak)).collect();
        let refs: Vec<(&str, f32, bool, f32)> = snapshot.iter().map(|(n, l, m, k)| (n.as_str(), *l, *m, *k)).collect();
        let strips = channels::summarize(&refs, &self.settings.channel_overrides);
        let loudest = strips.iter().map(|s| s.peak).fold(0.0, f32::max);

        let count = channels::NAMES.len() + 1;
        let w = (OPEN_W - 2.0 * MARGIN - GAP_S * (count as f32 - 1.0)) / count as f32;
        let (mut set_level, mut set_mute, mut move_app) = (None, None, None);
        for i in 0..count {
            let strip = strips.get(i);
            let master = strip.is_none();
            let card = Rect::from_min_size(o + vec2(MARGIN + i as f32 * (w + GAP_S), TOP), vec2(w, STRIP_H));
            let empty = strip.is_some_and(|s| s.apps.is_empty());
            let (name, level, muted, peak) = match strip {
                Some(s) => (channels::NAMES[i], s.level, s.muted, s.peak),
                None => ("Master", mx.master, self.speakers_muted == Some(true), loudest),
            };
            p.rect_filled(card, 12.0, fade(fill(14), a));
            let cx = card.center().x;
            let dim = if empty { DIM } else { Color32::WHITE };
            p.text(pos2(cx, card.top() + 11.0), Align2::CENTER_CENTER, name, bold(10.5), fade(if master { Color32::WHITE } else { dim }, a));

            let fader = Rect::from_x_y_ranges(cx - 6.0..=cx + 6.0, card.top() + FADER.0..=card.top() + FADER.1);
            let shown = if empty { 1.0 } else { level };
            let moved = ui::vslider(ui, ui.scope_id().with(("fader", i)), fader, shown, !empty, a);
            if let Some(v) = moved
                && !empty
            {
                set_level = Some((i, v));
            }
            p.text(pos2(cx, card.top() + 23.0), Align2::CENTER_CENTER, format!("{:.0}", moved.unwrap_or(shown) * 100.0), font(9.0), fade(DIM, a));

            // the level meter beside the fader
            let meter = Rect::from_x_y_ranges(cx + 12.0..=cx + 16.0, fader.top()..=fader.bottom());
            p.rect_filled(meter, 2.0, fade(fill(30), a));
            let height = meter.height() * peak.sqrt().clamp(0.0, 1.0);
            let col = if peak > 0.9 {
                RED
            } else if peak > 0.6 {
                ORANGE
            } else {
                GREEN
            };
            p.rect_filled(Rect::from_x_y_ranges(meter.x_range(), meter.bottom() - height..=meter.bottom()), 2.0, fade(col, a));

            // mute
            let mute = Rect::from_center_size(pos2(cx, card.top() + 112.0), vec2(w - 14.0, 18.0));
            let resp = hit(ui, &format!("mute{i}"), mute);
            let (bg, fg) =
                if muted { (RED.gamma_multiply(0.35), RED) } else { (fill(if resp.hovered() && !empty { 24 } else { 18 }), if empty { DIM } else { GRAY }) };
            p.rect_filled(mute, 9.0, fade(bg, a));
            p.text(mute.center(), Align2::CENTER_CENTER, if muted { "Muted" } else { "Mute" }, bold(9.5), fade(fg, a));
            if resp.clicked() && !empty {
                set_mute = Some((i, !muted));
            }

            // the apps in the channel; a click moves one along
            if let Some(strip) = strip {
                let shown: Vec<usize> = strip.apps.iter().copied().take(2).collect();
                for (line, app) in shown.iter().enumerate() {
                    let y = card.top() + 130.0 + line as f32 * 10.5;
                    let label = if line == 1 && strip.apps.len() > 2 { format!("+{} more", strip.apps.len() - 1) } else { shorten(&snapshot[*app].0, 9) };
                    let row = Rect::from_center_size(pos2(cx, y), vec2(w - 6.0, 10.0));
                    let r = hit(ui, &format!("app{i}-{line}"), row).on_hover_text(format!(
                        "{}. Click to move it to {}.",
                        snapshot[*app].0,
                        channels::NAMES[channels::next(i)]
                    ));
                    p.text(row.center(), Align2::CENTER_CENTER, label, font(9.0), fade(if r.hovered() { Color32::WHITE } else { GRAY }, a));
                    if r.clicked() {
                        move_app = Some((snapshot[*app].0.to_lowercase(), channels::next(i)));
                    }
                }
                if shown.is_empty() {
                    p.text(pos2(cx, card.top() + 135.0), Align2::CENTER_CENTER, "-", font(9.0), fade(DIM, a));
                }
            }
        }

        if let Some((i, v)) = set_level {
            if i == channels::NAMES.len() {
                mx.set_master(v);
            } else {
                for app in &strips[i].apps {
                    mx.set_app(*app, v);
                }
            }
        }
        if let Some((i, muted)) = set_mute {
            if i == channels::NAMES.len() {
                self.speakers_muted = mixer::toggle_mute(false);
            } else {
                for app in &strips[i].apps {
                    mx.set_app_muted(*app, muted);
                }
            }
        }
        if let Some((key, channel)) = move_app {
            self.settings.channel_overrides.insert(key, channel);
            self.settings.save();
        }
    }
}
