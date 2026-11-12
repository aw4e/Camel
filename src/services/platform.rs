//! Process-level Windows glue: elevation, global hotkey, tray menu, fonts, window frame.

use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers as HkMods};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// A Windows tool by absolute path. Camel runs elevated, and a program started by bare name is looked up in Camel's own
/// folder first, where a file dropped by a less privileged process would win.
pub(crate) fn system_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join("System32").join(name)
}

/// Set by the hotkey callback. With an event handler installed the crate's channel stays empty.
pub(crate) static HOTKEY_PRESSED: AtomicBool = AtomicBool::new(false);

pub(crate) fn register_hotkey(ctx: &egui::Context, enabled: bool) -> Option<(GlobalHotKeyManager, HotKey)> {
    let mgr = GlobalHotKeyManager::new().ok()?;
    let hk = HotKey::new(Some(HkMods::ALT), Code::Space);
    if enabled {
        mgr.register(hk).ok()?;
    }
    let ctx = ctx.clone();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            HOTKEY_PRESSED.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint();
    }));
    Some((mgr, hk))
}

pub(crate) struct Tray {
    _icon: Option<TrayIcon>,
    pub(crate) settings: Option<MenuId>,
    pub(crate) quit: Option<MenuId>,
    pub(crate) awake: Option<CheckMenuItem>,
    pub(crate) minimal: Option<CheckMenuItem>,
}

pub(crate) fn make_tray(minimal: bool) -> Tray {
    // brand mark, 32 px small cut (transparent: reads on light and dark taskbars)
    let icon = image::load_from_memory(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/brand/symbol-32.png")))
        .ok()
        .and_then(|i| tray_icon::Icon::from_rgba(i.to_rgba8().into_raw(), 32, 32).ok());
    let menu = Menu::new();
    let awake = CheckMenuItem::new("Keep awake", true, false, None);
    let minimal = CheckMenuItem::new("Minimal notch", true, minimal, None);
    let settings = MenuItem::new("Settings", true, None);
    let quit = MenuItem::new("Quit", true, None);
    if menu.append_items(&[&awake, &minimal, &settings, &quit]).is_err() {
        return Tray { _icon: None, settings: None, quit: None, awake: None, minimal: None };
    }
    let mut b = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("Camel");
    if let Some(icon) = icon {
        b = b.with_icon(icon);
    }
    Tray { _icon: b.build().ok(), settings: Some(settings.id().clone()), quit: Some(quit.id().clone()), awake: Some(awake), minimal: Some(minimal) }
}

/// SF Pro is not redistributable, so it is read from a `fonts/` folder (next to the exe, two levels up for
/// `cargo run`, or the source tree) and only used if found. Open Sans (OFL) is embedded as the fallback,
/// egui's default fonts stay behind both as glyph fallback.
pub(crate) fn setup_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontFamily};
    let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let dirs = [exe.join("fonts"), exe.join("../../fonts"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts")];
    let load = |file: &str| dirs.iter().find_map(|d| std::fs::read(d.join(file)).ok());
    let regular = load("SF-Pro-Display-Regular.otf").map(FontData::from_owned);
    let semibold = load("SF-Pro-Display-Semibold.otf").map(FontData::from_owned);

    let mut fonts = egui::FontDefinitions::default();
    let mut bold = fonts.families[&FontFamily::Proportional].clone();
    let regular = regular.unwrap_or_else(|| FontData::from_static(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/OpenSans-Regular.ttf"))));
    let semibold = semibold.unwrap_or_else(|| FontData::from_static(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/OpenSans-SemiBold.ttf"))));
    fonts.font_data.insert("ui-regular".into(), Arc::new(regular));
    fonts.font_data.insert("ui-bold".into(), Arc::new(semibold));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui-regular".into());
    bold.insert(0, "ui-bold".into());
    // Windows ships emoji and symbol fonts; as the last fallback they replace the empty boxes a title with an emoji would show
    let windows = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("Fonts");
    for (name, file) in [("system-emoji", "seguiemj.ttf"), ("system-symbols", "seguisym.ttf")] {
        if let Ok(bytes) = std::fs::read(windows.join(file)) {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_owned(bytes)));
            for family in [fonts.families.entry(FontFamily::Proportional).or_default(), &mut bold] {
                family.push(name.into());
            }
        }
    }
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    ctx.set_fonts(fonts);
}

/// Removes everything Windows draws around the window itself: the 1px non-client edge at the top that shows when the
/// window floats away from the screen edge (Spotlight), the Windows 11 border and the rounded corners. They would
/// outline the transparent margins. Must run on the window's own thread.
pub(crate) fn strip_window_frame() {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos, WM_NCCALCSIZE,
    };
    use windows::core::w;

    // No non-client area at all: the client area is the whole window.
    unsafe extern "system" fn no_frame(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if msg == WM_NCCALCSIZE && wparam.0 != 0 {
            return LRESULT(0);
        }
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    let Ok(hwnd) = (unsafe { FindWindowW(None, w!("Camel")) }) else { return };
    let corners = DWMWCP_DONOTROUND;
    let no_border = 0xFFFF_FFFEu32; // DWMWA_COLOR_NONE
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &raw const corners as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &raw const no_border as *const _, 4);
        if SetWindowSubclass(hwnd, Some(no_frame), 1, 0).as_bool() {
            let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

/// Release builds run elevated, so Camel can move leftovers out of Program Files and write machine-wide startup flags.
/// A manifest would make `cargo run` fail with "requires elevation", so the exe asks for it itself: it reopens
/// itself through the UAC prompt and exits. Declining the prompt just keeps this unelevated copy running.
/// `--no-elevate` skips the request; `--elevated` marks the copy that was already asked, so it can never loop.
#[cfg(not(debug_assertions))]
pub(crate) fn ensure_admin() {
    use windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, w};
    let args: Vec<String> = std::env::args().skip(1).collect();
    if unsafe { IsUserAnAdmin() }.as_bool() || args.iter().any(|a| a == "--elevated" || a == "--no-elevate") {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let params = args.iter().map(|a| format!("\"{a}\"")).chain(["--elevated".to_owned()]).collect::<Vec<_>>().join(" ");
    let r = unsafe { ShellExecuteW(None, w!("runas"), &HSTRING::from(exe.as_os_str()), &HSTRING::from(params), None, SW_SHOWNORMAL) };
    if r.0 as isize > 32 {
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn system_tools_resolve_to_real_files_under_system32() {
        for name in ["shutdown.exe", "schtasks.exe"] {
            let path = super::system_tool(name);
            assert!(path.is_absolute() && path.exists(), "{path:?}");
            assert!(path.to_string_lossy().to_lowercase().contains("system32"));
        }
    }
}
