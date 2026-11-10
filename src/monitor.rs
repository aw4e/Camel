//! Polled system state: CPU/RAM/battery, mic+camera usage, clipboard changes.

use std::time::{Duration, Instant};
use sysinfo::System;
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::Shell::{
    QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN, SHQueryUserNotificationState,
};
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
    let busy = |k: &RegKey| {
        k.get_value::<u64, _>("LastUsedTimeStart").is_ok_and(|v| v > 0)
            && k.get_value::<u64, _>("LastUsedTimeStop").is_ok_and(|v| v == 0)
    };
    root.enum_keys().flatten().any(|name| {
        let Ok(k) = root.open_subkey(&name) else { return false };
        if name == "NonPackaged" {
            k.enum_keys().flatten().any(|n| k.open_subkey(n).is_ok_and(|c| busy(&c)))
        } else {
            busy(&k)
        }
    })
}

/// Drops cold pages (GPU driver, startup allocations) from the working set. They fault back in on use.
pub fn trim_memory() {
    use windows::Win32::System::{ProcessStatus::EmptyWorkingSet, Threading::GetCurrentProcess};
    let _ = unsafe { EmptyWorkingSet(GetCurrentProcess()) };
}

pub fn clipboard_seq() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn clipboard_preview() -> String {
    match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
        Ok(t) if !t.trim().is_empty() => t.split_whitespace().collect::<Vec<_>>().join(" "),
        _ => "Image or file".to_owned(),
    }
}

pub fn fullscreen_app() -> bool {
    unsafe { SHQueryUserNotificationState() }
        .is_ok_and(|s| matches!(s, QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE))
}
