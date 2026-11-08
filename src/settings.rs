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

pub fn set_autostart(on: bool) {
    use winreg::{RegKey, enums::*};
    let Ok((run, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") else {
        return;
    };
    if on {
        if let Ok(exe) = std::env::current_exe() {
            let _ = run.set_value("Camel", &format!("\"{}\"", exe.display()));
        }
    } else {
        let _ = run.delete_value("Camel");
    }
}
