//! Polled system state: CPU/RAM/battery, mic+camera usage, clipboard changes.

use std::time::{Duration, Instant};
use sysinfo::System;
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::Shell::{QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState};
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

#[derive(Default)]
pub struct Stats {
    pub cpu: f32,
    pub ram: f32,
    pub battery: Option<u8>,
    pub charging: bool,
    pub mic: bool,
    pub cam: bool,
}

pub struct Monitor {
    sys: System,
    pub stats: Stats,
    at: Instant,
    ticks: u32,
}

impl Monitor {
    pub fn new() -> Self {
        Self { sys: System::new(), stats: Stats::default(), at: Instant::now() - Duration::from_secs(10), ticks: 0 }
    }

    /// Returns true when the 1 Hz refresh actually ran. The registry walk behind the mic/camera dots is
    /// the priciest part, so it runs every 2 s and only while `privacy` is on.
    pub fn refresh(&mut self, privacy: bool) -> bool {
        if self.at.elapsed() < Duration::from_secs(1) {
            return false;
        }
        self.at = Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        let s = &mut self.stats;
        s.cpu = self.sys.global_cpu_usage();
        s.ram = 100.0 * self.sys.used_memory() as f32 / self.sys.total_memory().max(1) as f32;

        let mut p = SYSTEM_POWER_STATUS::default();
        // 255 = battery level unknown (desktop PC)
        if unsafe { GetSystemPowerStatus(&mut p) }.is_ok() && p.BatteryLifePercent != 255 {
            s.battery = Some(p.BatteryLifePercent);
            s.charging = p.ACLineStatus == 1;
        } else {
            s.battery = None;
        }
        self.ticks = self.ticks.wrapping_add(1);
        if !privacy {
            (s.mic, s.cam) = (false, false);
        } else if self.ticks % 2 == 1 {
            s.mic = capability_in_use("microphone");
            s.cam = capability_in_use("webcam");
        }
        true
    }
}

/// Windows records capability use per app; a start time with no stop time means "in use right now".
fn capability_in_use(kind: &str) -> bool {
    let path = format!(r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{kind}");
    let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&path) else {
        return false;
    };
    let busy = |k: &RegKey| k.get_value::<u64, _>("LastUsedTimeStart").is_ok_and(|v| v > 0) && k.get_value::<u64, _>("LastUsedTimeStop").is_ok_and(|v| v == 0);
    root.enum_keys().flatten().any(|name| {
        let Ok(k) = root.open_subkey(&name) else { return false };
        if name == "NonPackaged" { k.enum_keys().flatten().any(|n| k.open_subkey(n).is_ok_and(|c| busy(&c))) } else { busy(&k) }
    })
}

/// Drops cold pages (GPU driver, startup allocations) from the working set. They fault back in on use.
pub fn trim_memory() {
    use windows::Win32::System::{ProcessStatus::EmptyWorkingSet, Threading::GetCurrentProcess};
    let _ = unsafe { EmptyWorkingSet(GetCurrentProcess()) };
}

/// Keeps the screen and the system awake until switched off. The state belongs to the calling thread, so this must be
/// called from the UI thread, which lives as long as Camel does; it ends with the process.
pub fn set_awake(on: bool) {
    use windows::Win32::System::Power::{ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
    let _ = unsafe { SetThreadExecutionState(if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED } else { ES_CONTINUOUS }) };
}

pub fn clipboard_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub enum Clip {
    Text(String),
    /// The source app asked clipboard managers to ignore it (password managers do). Not read, not stored.
    Hidden,
    /// A picture, as RGBA pixels.
    Image {
        w: usize,
        h: usize,
        rgba: Vec<u8>,
    },
    /// Files, or anything else that is not text or a picture.
    Other,
}

pub fn read_clipboard() -> Clip {
    use windows::Win32::System::DataExchange::{IsClipboardFormatAvailable, RegisterClipboardFormatW};
    let format = unsafe { RegisterClipboardFormatW(windows::core::w!("ExcludeClipboardContentFromMonitorProcessing")) };
    if format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok() {
        return Clip::Hidden;
    }
    let Ok(mut cb) = arboard::Clipboard::new() else { return Clip::Other };
    if let Ok(t) = cb.get_text()
        && !t.trim().is_empty()
    {
        return Clip::Text(t);
    }
    match cb.get_image() {
        Ok(img) => Clip::Image { w: img.width, h: img.height, rgba: img.bytes.into_owned() },
        Err(_) => Clip::Other,
    }
}

pub fn beep() {
    use windows::Win32::System::Diagnostics::Debug::MessageBeep;
    use windows::Win32::UI::WindowsAndMessaging::MB_ICONASTERISK;
    let _ = unsafe { MessageBeep(MB_ICONASTERISK) };
}

/// A full-screen app is running. The Windows notification state alone is too eager (it reports "busy" for long
/// stretches with nothing full-screen, and around Alt+Tab), so it must also be confirmed by the foreground window
/// really covering its whole monitor.
pub fn fullscreen_app() -> bool {
    unsafe { SHQueryUserNotificationState() }.is_ok_and(|s| matches!(s, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE))
        && foreground_covers_monitor()
}

/// Window classes that fill the screen without being a full-screen app: desktop, taskbar, Alt+Tab, Task View.
const SHELL_CLASSES: [&str; 9] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "XamlExplorerHostIslandWindow",
    "MultitaskingViewFrame",
    "TaskSwitcherWnd",
    "Windows.UI.Core.CoreWindow",
    "ForegroundStaging",
];

fn foreground_covers_monitor() -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return false;
        }
        let mut name = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut name).max(0) as usize;
        if SHELL_CLASSES.contains(&String::from_utf16_lossy(&name[..n]).as_str()) {
            return false;
        }
        let mut r = RECT::default();
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetWindowRect(hwnd, &mut r).is_err() || !GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

/// Re-asserts topmost: Windows can demote a topmost window after full-screen apps, UAC prompts and Alt+Tab.
pub fn keep_on_top() {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos};
    if let Ok(hwnd) = unsafe { FindWindowW(None, windows::core::w!("Camel")) } {
        let _ = unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn foreground_check_does_not_panic() {
        // the answer depends on what is on screen; this only proves the Win32 calls are wired correctly
        let _ = super::foreground_covers_monitor();
        let _ = super::fullscreen_app();
    }
}
