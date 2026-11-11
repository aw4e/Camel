use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bumped when a default changes in a way that should reach files that already exist.
const VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub hover_expand: bool,
    pub always_on_top: bool,
    pub hide_fullscreen: bool,
    pub lyrics: bool,
    pub clipboard_toast: bool,
    pub privacy_dots: bool,
    pub spotlight: bool,
    pub autostart: bool,
    pub volume_hud: bool,
    pub auto_update: bool,
    pub key_toasts: bool,
    pub device_toasts: bool,
    pub minimal: bool,
    /// Show the temperature and humidity of where you are on the Clock page. It contacts a weather service with your
    /// rounded location, so it can be switched off.
    pub weather: bool,
    /// A city to use instead of the Windows location.
    pub city: String,
    /// Apps the user moved to another mixer channel, by lowercase app name.
    pub channel_overrides: std::collections::HashMap<String, usize>,
    /// Seconds the user shifted the lyrics of a track by (Shift+click on a line), by lowercase "artist\ntitle".
    pub lyric_offsets: std::collections::HashMap<String, f64>,
    /// Which version of the defaults this file was written for; a file without one is older than any version.
    #[serde(default)]
    pub version: u32,
}

/// Tracks whose lyric shift is remembered; the file does not grow without end.
const MAX_LYRIC_OFFSETS: usize = 300;

impl Default for Settings {
    fn default() -> Self {
        Self {
            hover_expand: true,
            always_on_top: true,
            hide_fullscreen: true,
            lyrics: true,
            clipboard_toast: true,
            privacy_dots: true,
            spotlight: true,
            autostart: false,
            volume_hud: true,
            auto_update: true,
            key_toasts: true,
            device_toasts: true,
            minimal: false,
            weather: true,
            city: String::new(),
            channel_overrides: Default::default(),
            lyric_offsets: Default::default(),
            version: VERSION,
        }
    }
}

fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Camel").join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        let Some(mut s) = std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str::<Self>(&s).ok()) else { return Self::default() };
        if s.migrate() {
            s.save();
        }
        s
    }

    /// Brings a settings file from before a default changed up to date. A file from before the weather was switched on by
    /// default carries `weather: false` that nobody chose, so it is turned on once. Returns whether anything changed.
    fn migrate(&mut self) -> bool {
        if self.version >= VERSION {
            return false;
        }
        self.weather = true;
        self.version = VERSION;
        true
    }

    /// The remembered lyric shift of a track, 0 when there is none.
    pub fn lyric_offset(&self, track: &str) -> f64 {
        self.lyric_offsets.get(&track.to_lowercase()).copied().unwrap_or(0.0)
    }

    /// Remembers the lyric shift of a track; a shift under a tenth of a second is no shift and is forgotten.
    pub fn set_lyric_offset(&mut self, track: &str, seconds: f64) {
        let key = track.to_lowercase();
        if seconds.abs() < 0.1 {
            self.lyric_offsets.remove(&key);
            return;
        }
        if self.lyric_offsets.len() >= MAX_LYRIC_OFFSETS && !self.lyric_offsets.contains_key(&key) {
            self.lyric_offsets.clear(); // shortcut: forgets all, not the oldest; a map has no order, and 300 tracks is a lot of nudging
        }
        self.lyric_offsets.insert(key, seconds);
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, json);
        }
    }
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn schtasks(args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(crate::platform::system_tool("schtasks.exe")).args(args).creation_flags(0x0800_0000).output().is_ok_and(|o| o.status.success())
}

/// The real state, not the saved setting: a task or a `Run` entry exists.
pub fn autostart_enabled() -> bool {
    use winreg::{RegKey, enums::*};
    let in_run = RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN_KEY).is_ok_and(|k| k.get_value::<String, _>("Camel").is_ok());
    in_run || schtasks(&["/Query", "/TN", "Camel"])
}

/// Starts Camel at logon. A program that asks for administrator rights is silently skipped when it starts from the
/// `Run` key, so the release exe is registered as a scheduled task with the highest run level instead. The plain `Run`
/// entry is the fallback when the task cannot be created (an unelevated debug build). Returns whether the requested
/// state was reached: a task made by an elevated Camel cannot be deleted by an unelevated one.
pub fn set_autostart(on: bool) -> bool {
    use winreg::{RegKey, enums::*};
    let run = || RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY).ok().map(|(k, _)| k);
    if !on {
        schtasks(&["/Delete", "/TN", "Camel", "/F"]);
        if let Some(k) = run() {
            let _ = k.delete_value("Camel");
        }
        return !autostart_enabled();
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let file = std::env::temp_dir().join("camel-task.xml");
    // schtasks wants UTF-16 with a byte order mark
    let utf16: Vec<u8> = std::iter::once(0xFEFFu16).chain(task_xml(&exe.to_string_lossy()).encode_utf16()).flat_map(u16::to_le_bytes).collect();
    let made = std::fs::write(&file, utf16).is_ok() && schtasks(&["/Create", "/TN", "Camel", "/XML", &file.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&file);
    if let Some(k) = run() {
        if made {
            let _ = k.delete_value("Camel"); // the task replaces an older Run entry
        } else {
            let _ = k.set_value("Camel", &format!("\"{}\"", exe.display()));
        }
    }
    autostart_enabled()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Logon task, highest run level, and none of the schtasks defaults that stop a task on battery power.
fn task_xml(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
  </Settings>
  <Actions Context="Author"><Exec><Command>{}</Command></Exec></Actions>
</Task>"#,
        xml_escape(exe)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_runs_elevated_on_battery_and_escapes_the_path() {
        let x = task_xml(r"C:\Tom & Jerry\camel.exe");
        assert!(x.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(x.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(x.contains(r"<Command>C:\Tom &amp; Jerry\camel.exe</Command>"));
    }

    #[test]
    fn a_lyric_shift_is_remembered_per_track_and_bounded() {
        let mut s = Settings::default();
        assert_eq!(s.lyric_offset("A\nSong"), 0.0);
        s.set_lyric_offset("A\nSong", -2.5);
        assert_eq!(s.lyric_offset("a\nsong"), -2.5, "case does not matter");
        s.set_lyric_offset("A\nSong", 0.05);
        assert!(s.lyric_offsets.is_empty(), "a shift of nothing is forgotten");
        for i in 0..=MAX_LYRIC_OFFSETS {
            s.set_lyric_offset(&format!("t{i}"), 1.0);
        }
        assert!(s.lyric_offsets.len() <= MAX_LYRIC_OFFSETS && s.lyric_offset(&format!("t{MAX_LYRIC_OFFSETS}")) == 1.0);
        let old: Settings = serde_json::from_str(r#"{"lyrics":true}"#).unwrap();
        assert!(old.lyric_offsets.is_empty(), "a file from before this existed still loads");
    }

    #[test]
    fn an_old_file_gets_the_weather_default_once() {
        let mut old: Settings = serde_json::from_str(r#"{"lyrics":true,"weather":false}"#).unwrap();
        assert_eq!(old.version, 0, "a file without a version is older than any version");
        assert!(old.migrate() && old.weather && old.version == VERSION);
        old.weather = false;
        assert!(!old.migrate() && !old.weather, "after that the choice is the user's");
        assert!(Settings::default().weather && Settings::default().version == VERSION);
    }
}
