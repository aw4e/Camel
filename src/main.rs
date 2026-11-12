#![windows_subsystem = "windows"]

//! Camel: a macOS-style notch for Windows. Entry point: opens one borderless, transparent, always-on-top egui
//! window. `app` draws it, `features` holds pure logic (testable without Windows), `services` talks to the OS and network.

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
    // release builds abort on panic with no console, so keep the reason in %APPDATA%\Camel\crash.log
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = settings::data_dir() {
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
                let _ = std::io::Write::write_all(
                    &mut f,
                    format!(
                        "{} v{}: {info}
",
                        chrono::Local::now().to_rfc3339(),
                        update::VERSION
                    )
                    .as_bytes(),
                );
            }
        }
        default_hook(info);
    }));
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
