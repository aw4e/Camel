use crate::apps;
use crate::notch::{Notch, ToastIcon};
use crate::prelude::*;
use std::path::PathBuf;

impl Notch {
    pub(crate) fn tab_apps(&mut self, ui: &mut egui::Ui, o: egui::Pos2, a: f32) {
        let p = ui.painter().clone();
        if self.startups.is_none() {
            self.can_admin = apps::can_write_machine();
        }
        let apps_all = self.apps.get_or_insert_with(apps::list);
        let startups = self.startups.get_or_insert_with(apps::startup_list);

        for (i, (label, startup)) in [("Programs", false), ("Startup", true)].into_iter().enumerate() {
            let r = Rect::from_min_size(o + vec2(MARGIN + i as f32 * 72.0, TOP), vec2(64.0, 20.0));
            let resp = hit(ui, &format!("appsview{i}"), r);
            let sel = self.apps_startup == startup;
            p.rect_filled(
                r,
                10.0,
                fade(
                    fill(if sel {
                        46
                    } else if resp.hovered() {
                        26
                    } else {
                        16
                    }),
                    a,
                ),
            );
            p.text(r.center(), Align2::CENTER_CENTER, label, bold(11.0), fade(if sel { Color32::WHITE } else { GRAY }, a));
            if resp.clicked() {
                (self.apps_startup, self.apps_scroll) = (startup, 0);
            }
        }

        // search box, programs view only
        if !self.apps_startup {
            let sr = Rect::from_min_max(o + vec2(MARGIN + 144.0 + GAP_L, TOP), o + vec2(OPEN_W - MARGIN, TOP + 20.0));
            p.rect_filled(sr, 10.0, fade(fill(16), a));
            let mut q = std::mem::take(&mut self.apps_query);
            ui.put(
                sr.shrink2(vec2(10.0, 0.0)),
                egui::TextEdit::singleline(&mut q)
                    .font(font(12.0))
                    .frame(egui::Frame::NONE)
                    .desired_width(sr.width() - 20.0)
                    .hint_text(egui::RichText::new("Search programs").color(fade(DIM, a)))
                    .text_color(fade(Color32::WHITE, a)),
            );
            if q != self.apps_query {
                self.apps_scroll = 0;
            }
            self.apps_query = q;
        }

        // leftovers from the last uninstall: shown only after its uninstaller finished, moved to the Recycle Bin on click
        let mut top = APPS_TOP;
        let mut visible = 3;
        let (mut clean, mut dismiss) = (false, false);
        if let Some(found) = &self.leftovers {
            let row = Rect::from_min_size(o + vec2(MARGIN, top), vec2(OPEN_W - 2.0 * MARGIN, ROW_H));
            p.rect_filled(row, 8.0, fade(ORANGE.gamma_multiply(0.22), a));
            let first = found[0].to_string_lossy();
            let text = format!(
                "{} leftover(s): {}",
                found.len(),
                if first.chars().count() > 38 {
                    format!("…{}", first.chars().skip(first.chars().count() - 37).collect::<String>())
                } else {
                    first.into_owned()
                }
            );
            p.with_clip_rect(Rect::from_x_y_ranges(row.left() + 8.0..=row.right() - 92.0, row.y_range())).text(
                pos2(row.left() + ROW_PAD, row.center().y),
                Align2::LEFT_CENTER,
                text,
                font(11.0),
                fade(Color32::WHITE, a),
            );
            let b = Rect::from_center_size(pos2(row.right() - 50.0, row.center().y), vec2(64.0, 18.0));
            let br = hit(ui, "leftclean", b);
            p.rect_filled(b, 9.0, fade(if br.hovered() { ORANGE } else { ORANGE.gamma_multiply(0.8) }, a));
            p.text(b.center(), Align2::CENTER_CENTER, "Clean up", bold(10.5), fade(Color32::BLACK, a));
            let x = Rect::from_center_size(pos2(row.right() - 10.0, row.center().y), vec2(16.0, 18.0));
            let xr = hit(ui, "leftdismiss", x);
            p.text(x.center(), Align2::CENTER_CENTER, "×", bold(13.0), fade(if xr.hovered() { Color32::WHITE } else { GRAY }, a));
            (clean, dismiss) = (br.clicked(), xr.clicked());
            top += ROW_PITCH;
            visible = 2;
        }

        let mut act: Option<(usize, bool)> = None; // (index, true = uninstall / false = open folder)
        let mut toggle: Option<usize> = None;
        if self.apps_startup {
            self.apps_n = startups.len();
            self.apps_scroll = self.apps_scroll.min(startups.len().saturating_sub(visible));
            if startups.is_empty() {
                p.text(o + vec2(OPEN_W / 2.0, 96.0), Align2::CENTER_CENTER, "No startup programs", font(12.0), fade(DIM, a));
            }
            for (slot, i) in (self.apps_scroll..startups.len()).take(visible).enumerate() {
                let row = Rect::from_min_size(o + vec2(MARGIN, top + slot as f32 * ROW_PITCH), vec2(OPEN_W - 2.0 * MARGIN, ROW_H));
                let item = &startups[i];
                p.with_clip_rect(Rect::from_x_y_ranges(row.left() + 8.0..=row.left() + 160.0, row.y_range())).text(
                    pos2(row.left() + ROW_PAD, row.center().y),
                    Align2::LEFT_CENTER,
                    &item.name,
                    font(12.0),
                    fade(Color32::WHITE, a),
                );
                p.with_clip_rect(Rect::from_x_y_ranges(row.left() + 166.0..=row.right() - 50.0, row.y_range())).text(
                    pos2(row.left() + 168.0, row.center().y),
                    Align2::LEFT_CENTER,
                    &item.command,
                    font(10.5),
                    fade(DIM, a),
                );
                if item.machine_wide() && !self.can_admin {
                    p.text(pos2(row.right() - 10.0, row.center().y), Align2::RIGHT_CENTER, "Admin", font(10.0), fade(DIM, a));
                } else if ui::toggle(ui, ui.scope_id().with(("startup", i)), pos2(row.right() - 24.0, row.center().y), item.enabled, a) {
                    toggle = Some(i);
                }
            }
        } else {
            let q = self.apps_query.trim().to_lowercase();
            let idx: Vec<usize> = apps_all.iter().enumerate().filter(|(_, x)| q.is_empty() || x.haystack.contains(&q)).map(|(i, _)| i).collect();
            self.apps_n = idx.len();
            self.apps_scroll = self.apps_scroll.min(idx.len().saturating_sub(visible));
            if idx.is_empty() {
                p.text(
                    o + vec2(OPEN_W / 2.0, 96.0),
                    Align2::CENTER_CENTER,
                    if q.is_empty() { "No programs found" } else { "No programs match" },
                    font(12.0),
                    fade(DIM, a),
                );
            }
            for (slot, &i) in idx.iter().skip(self.apps_scroll).take(visible).enumerate() {
                let app = &apps_all[i];
                let row = Rect::from_min_size(o + vec2(MARGIN, top + slot as f32 * ROW_PITCH), vec2(OPEN_W - 2.0 * MARGIN, ROW_H));
                let hovered = over(ui, row);
                if hovered {
                    p.rect_filled(row, 8.0, fade(fill(22), a));
                }
                p.with_clip_rect(Rect::from_x_y_ranges(row.left() + 8.0..=row.right() - 104.0, row.y_range())).text(
                    pos2(row.left() + ROW_PAD, row.center().y),
                    Align2::LEFT_CENTER,
                    &app.name,
                    font(12.0),
                    fade(Color32::WHITE, a),
                );
                if hovered {
                    let un = Rect::from_center_size(pos2(row.right() - 40.0, row.center().y), vec2(68.0, 20.0));
                    let ur = hit(ui, &format!("appun{i}"), un);
                    p.rect_filled(un, 9.0, fade(if ur.hovered() { RED } else { RED.gamma_multiply(0.75) }, a));
                    p.text(un.center(), Align2::CENTER_CENTER, "Uninstall", bold(10.5), fade(Color32::WHITE, a));
                    if ur.clicked() {
                        act = Some((i, true));
                    }
                    if !app.location.trim().is_empty() {
                        let fo = Rect::from_center_size(pos2(row.right() - 88.0, row.center().y), vec2(28.0, 20.0));
                        let fr = hit(ui, &format!("appfolder{i}"), fo);
                        p.rect_filled(fo, 9.0, fade(fill(if fr.hovered() { 40 } else { 24 }), a));
                        ui::scaled(&p, fo.center(), ui::icon_fit(fo.height()), || ui::icon_folder(&p, fo.center(), fade(Color32::WHITE, a)));
                        if fr.clicked() {
                            act = Some((i, false));
                        }
                    }
                } else {
                    p.text(pos2(row.right() - 10.0, row.center().y), Align2::RIGHT_CENTER, apps::format_size(app.size_kb), font(11.0), fade(GRAY, a));
                }
            }
        }

        if clean && let Some(found) = self.leftovers.clone() {
            apps::recycle(&found);
            // keep whatever is still there, so a failed Clean up can simply be tried again
            let mut left: Vec<PathBuf> = found.iter().filter(|p| p.exists()).cloned().collect();
            // a file Explorer still has loaded (a shell extension) cannot go to the Recycle Bin: queue it for the next restart
            let queued = !left.is_empty() && apps::delete_on_reboot(&left);
            if queued {
                left.clear();
            }
            if left.is_empty() {
                self.leftovers = None;
                let (title, body) = if queued {
                    ("Finishes after restart", "Locked files are removed on the next restart".to_owned())
                } else {
                    ("Moved to Recycle Bin", format!("{} folder(s)", found.len()))
                };
                self.show_toast(ToastIcon::Apps, title, body);
            } else {
                let body = format!("{} folder(s) left, try again", left.len());
                self.leftovers = Some(left);
                self.show_toast(ToastIcon::Apps, "Could not clean up", body);
            }
            self.save_record();
        }
        if dismiss {
            self.leftovers = None;
            self.save_record();
        }
        if let Some((i, remove)) = act
            && let Some(app) = self.apps.as_ref().and_then(|l| l.get(i)).cloned()
        {
            if !remove {
                apps::open_location(&app);
            } else if apps::uninstall(&app) {
                self.pending = Some((app, Instant::now()));
                self.save_record();
            } else {
                self.show_toast(ToastIcon::Apps, "Could not start the uninstaller", shorten(&app.name, 30));
            }
        }
        if let Some(i) = toggle
            && let Some(item) = self.startups.as_ref().and_then(|l| l.get(i)).cloned()
        {
            if apps::set_startup(&item, !item.enabled) {
                self.startups = None;
            } else {
                let why = if item.machine_wide() { "Needs administrator rights" } else { "Windows did not accept the change" };
                self.show_toast(ToastIcon::Apps, &format!("Could not change {}", shorten(&item.name, 18)), why.to_owned());
            }
        }
    }
}
