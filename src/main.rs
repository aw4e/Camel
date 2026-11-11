#![windows_subsystem = "windows"]

mod app;
mod features;
mod services;

// The modules live in folders; these names keep `crate::media`, `crate::ui` and so on short for every file.
use app::{icons as ui, layout, prelude, widgets};
use features::{actions, apps, channels, clipboard, currency, focus, notes, reminders, settings, spotlight, worldtime};
use services::{loopback, lyrics, media, mixer, monitor, platform, radios, rates, router, system, update, weather};

use app::Notch;
use eframe::egui::{self, vec2};
use layout::{PILL_H, PILL_W};
use settings::Settings;

fn main() -> eframe::Result {
    #[cfg(not(debug_assertions))]
    platform::ensure_admin();
    update::cleanup();
    let settings = Settings::load();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(vec2(PILL_W, PILL_H))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false);
    if settings.always_on_top {
        viewport = viewport.with_always_on_top();
    }
    eframe::run_native(
        "Camel",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| {
            platform::setup_fonts(&cc.egui_ctx);
            platform::strip_window_frame();
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Notch::new(&cc.egui_ctx)))
        }),
    )
}
