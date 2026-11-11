use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
}

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
        }
    }
}

fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("Camel").join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
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
}
