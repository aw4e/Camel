//! Installed programs, uninstall, leftover cleanup and startup entries. Everything here reads the registry and the
//! file system locally; nothing touches the network.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::UI::Shell::{FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW, SHFileOperationW, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_BINARY};
use winreg::{RegKey, RegValue};

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
const RUN: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Scope {
    Machine,
    User,
}

impl Scope {
    fn root(self) -> RegKey {
        RegKey::predef(if self == Scope::Machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    /// Lowercased name and publisher, for the search box.
    pub haystack: String,
    pub location: String,
    pub uninstall: String,
    pub size_kb: u64,
    scope: Scope,
    key_path: String,
}

/// Entries without a name or an uninstall command, hidden system components, updates and hotfixes are not programs.
pub fn is_listed(name: &str, uninstall: &str, system_component: bool, parent: &str, release: &str) -> bool {
    !name.trim().is_empty()
        && !uninstall.trim().is_empty()
        && !system_component
        && parent.is_empty()
        && !matches!(release, "Update" | "Hotfix" | "Security Update" | "Service Pack")
}

pub fn list() -> Vec<App> {
    let mut out = Vec::new();
    for (scope, base) in [(Scope::Machine, UNINSTALL), (Scope::Machine, UNINSTALL_32), (Scope::User, UNINSTALL)] {
        let Ok(root) = scope.root().open_subkey(base) else { continue };
        for name in root.enum_keys().flatten() {
            let Ok(k) = root.open_subkey(&name) else { continue };
            let s = |n: &str| expand_env(&k.get_value::<String, _>(n).unwrap_or_default());
            let (display, uninstall) = (s("DisplayName"), s("UninstallString"));
            let system = k.get_value::<u32, _>("SystemComponent").unwrap_or(0) == 1;
            if !is_listed(&display, &uninstall, system, &s("ParentKeyName"), &s("ReleaseType")) {
                continue;
            }
            let publisher = s("Publisher");
            out.push(App {
                haystack: format!("{display} {publisher}").to_lowercase(),
                name: display,
                location: s("InstallLocation"),
                uninstall,
                size_kb: u64::from(k.get_value::<u32, _>("EstimatedSize").unwrap_or(0)),
                scope,
                key_path: format!(r"{base}\{name}"),
            });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// "245 MB", or an empty string when the program did not report a size.
pub fn format_size(kb: u64) -> String {
    match kb {
        0 => String::new(),
        k if k >= 1024 * 1024 => format!("{:.1} GB", k as f64 / 1048576.0),
        k if k >= 1024 => format!("{} MB", k / 1024),
        k => format!("{k} KB"),
    }
}

/// Expands `%NAME%` references, as registry REG_EXPAND_SZ values contain them.
pub fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find('%').and_then(|j| std::env::var(&tail[..j]).ok().map(|v| (j, v))) {
            Some((j, v)) => {
                out.push_str(&v);
                rest = &tail[j + 1..];
            }
            None => {
                out.push('%');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Splits an UninstallString into program and arguments: `"C:\A B\un.exe" /S`, `MsiExec.exe /X{id}` or a bare path.
pub fn split_command(cmd: &str) -> (String, String) {
    let c = cmd.trim();
    if let Some(rest) = c.strip_prefix('"')
        && let Some(end) = rest.find('"')
    {
        return (rest[..end].to_owned(), rest[end + 1..].trim().to_owned());
    }
    if let Some(i) = c.to_lowercase().find(".exe") {
        return (c[..i + 4].to_owned(), c[i + 4..].trim().to_owned());
    }
    match c.split_once(' ') {
        Some((a, b)) => (a.to_owned(), b.trim().to_owned()),
        None => (c.to_owned(), String::new()),
    }
}

/// ShellExecute (not CreateProcess) so an uninstaller that needs administrator rights can show its UAC prompt.
fn shell_open(target: &str, args: &str) -> bool {
    let r = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(target), &HSTRING::from(args), None, SW_SHOWNORMAL) };
    r.0 as isize > 32
}

/// Starts the program's own uninstaller.
pub fn uninstall(app: &App) -> bool {
    let (exe, args) = split_command(&app.uninstall);
    shell_open(&exe, &args)
}

pub fn open_location(app: &App) -> bool {
    let dir = PathBuf::from(app.location.trim_matches('"'));
    dir.is_dir() && shell_open(&dir.to_string_lossy(), "")
}

/// True while the program's registry entry exists, so false once its uninstaller has finished.
pub fn installed(app: &App) -> bool {
    app.scope.root().open_subkey(&app.key_path).is_ok()
}

/// Folder names that are never a program's own folder, whatever a registry entry claims.
const GENERIC: [&str; 14] =
    ["microsoft", "windows", "common files", "system32", "temp", "packages", "programs", "apps", "data", "users", "user", "default", "public", "program files"];

fn safe_name(n: &str) -> bool {
    let n = n.trim();
    n.chars().count() >= 4 && !n.ends_with('.') && !n.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']) && !GENERIC.contains(&n.to_lowercase().as_str())
}

/// A folder deep enough to be one program's own, and not one of the shared roots.
fn safe_dir(p: &Path) -> bool {
    let shared = ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "USERPROFILE", "APPDATA", "LOCALAPPDATA"];
    p.is_absolute()
        && p.components().count() >= 4
        && !shared.iter().filter_map(std::env::var_os).any(|d| Path::new(&d) == p || p.starts_with(Path::new(&d).join("System32")))
}

/// Folders that may be left behind after the uninstaller finished: the install folder itself, and folders named exactly
/// like the program directly under the usual data roots. Nothing is returned for registry keys.
pub fn leftovers(app: &App) -> Vec<PathBuf> {
    let mut names = vec![app.name.clone()];
    let mut found = Vec::new();
    let loc = PathBuf::from(app.location.trim_matches('"'));
    if !app.location.trim().is_empty() && safe_dir(&loc) && loc.is_dir() {
        if let Some(leaf) = loc.file_name() {
            names.push(leaf.to_string_lossy().into_owned());
        }
        found.push(loc);
    }
    for var in ["APPDATA", "LOCALAPPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)"] {
        let Some(base) = std::env::var_os(var) else { continue };
        for n in names.iter().filter(|n| safe_name(n)) {
            let p = Path::new(&base).join(n.trim());
            if p.is_dir() && !found.contains(&p) {
                found.push(p);
            }
        }
    }
    found
}

/// Sends the folders to the Recycle Bin, so it can always be undone.
pub fn recycle(paths: &[PathBuf]) -> bool {
    let mut wide: Vec<u16> = Vec::new();
    for p in paths {
        wide.extend(p.as_os_str().encode_wide());
        wide.push(0);
    }
    wide.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(wide.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
        ..Default::default()
    };
    unsafe { SHFileOperationW(&mut op) == 0 && !op.fAnyOperationsAborted.as_bool() }
}

/// Contents first, folders after, so a deletion queue never meets a non-empty folder. Links are queued themselves,
/// never followed.
fn post_order(p: &Path, out: &mut Vec<PathBuf>) {
    const CAP: usize = 5000;
    if out.len() >= CAP {
        return;
    }
    if std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_dir())
        && let Ok(rd) = std::fs::read_dir(p)
    {
        for e in rd.flatten() {
            post_order(&e.path(), out);
        }
    }
    out.push(p.to_path_buf());
}

/// For what cannot be removed now, such as a shell extension Explorer still has loaded: Windows deletes it at the
/// next restart. Needs administrator rights. This deletes for good, it does not use the Recycle Bin.
pub fn delete_on_reboot(paths: &[PathBuf]) -> bool {
    use windows::Win32::Storage::FileSystem::{MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};
    let mut queue = Vec::new();
    for p in paths {
        post_order(p, &mut queue);
    }
    !queue.is_empty() && queue.iter().all(|p| unsafe { MoveFileExW(&HSTRING::from(p.as_os_str()), PCWSTR::null(), MOVEFILE_DELAY_UNTIL_REBOOT) }.is_ok())
}

/// What Camel remembers between runs: an uninstaller it started and is waiting on, and the leftovers found after one
/// finished. Without this, closing Camel (or a failed Clean up) lost the list for good.
#[derive(Serialize, Deserialize, Default)]
pub struct Record {
    pub pending: Option<App>,
    pub leftover_name: String,
    pub leftovers: Vec<PathBuf>,
}

fn record_path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Camel").join("uninstall.json")
}

/// A saved leftover is only trusted if it still looks like what `leftovers` would have produced: a folder directly
/// under one of the usual data roots, with a non-generic name. The file is user-writable and Camel may run elevated,
/// so a hand-edited path must not be able to point Clean up at anything else.
fn trusted_leftover(p: &Path) -> bool {
    let roots = ["APPDATA", "LOCALAPPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)"];
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    p.is_dir() && safe_name(&name) && p.parent().is_some_and(|d| roots.iter().filter_map(std::env::var_os).any(|r| Path::new(&r) == d))
}

pub fn load_record() -> Record {
    let mut r: Record = std::fs::read_to_string(record_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    r.leftovers.retain(|p| trusted_leftover(p));
    r
}

pub fn save_record(r: &Record) {
    let p = record_path();
    if r.pending.is_none() && r.leftovers.is_empty() {
        let _ = std::fs::remove_file(p);
    } else if let Ok(json) = serde_json::to_string_pretty(r) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, json);
    }
}

/// Where a startup item comes from. Each source keeps its on/off flag in its own StartupApproved key, in the same
/// hive as the item itself: Task Manager writes the same places.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    UserRun,
    MachineRun,
    MachineRun32,
    UserFolder,
    CommonFolder,
}

impl Source {
    fn scope(self) -> Scope {
        if matches!(self, Source::UserRun | Source::UserFolder) { Scope::User } else { Scope::Machine }
    }

    fn approved(self) -> String {
        let leaf = match self {
            Source::UserRun | Source::MachineRun => "Run",
            Source::MachineRun32 => "Run32",
            Source::UserFolder | Source::CommonFolder => "StartupFolder",
        };
        format!(r"{APPROVED}\{leaf}")
    }
}

#[derive(Clone)]
pub struct Startup {
    pub name: String,
    pub command: String,
    pub enabled: bool,
    source: Source,
    /// Name the on/off flag is stored under: the Run value name, or the shortcut's file name.
    key: String,
}

impl Startup {
    /// Machine-wide items keep their flag in HKLM, which only an elevated Camel may write.
    pub fn machine_wide(&self) -> bool {
        self.source.scope() == Scope::Machine
    }
}

/// What Task Manager lists: Run keys (user, machine, 32-bit) and the two Startup folders, each with its on/off flag.
pub fn startup_list() -> Vec<Startup> {
    let flag =
        |s: Source, key: &str| s.scope().root().open_subkey(s.approved()).ok().and_then(|k| k.get_raw_value(key).ok()).is_none_or(|v| is_enabled(&v.bytes));
    let mut out = Vec::new();
    for (source, path) in [(Source::UserRun, RUN), (Source::MachineRun, RUN), (Source::MachineRun32, RUN32)] {
        let Ok(run) = source.scope().root().open_subkey(path) else { continue };
        for name in run.enum_values().flatten().map(|(n, _)| n) {
            let command = expand_env(&run.get_value::<String, _>(&name).unwrap_or_default());
            out.push(Startup { enabled: flag(source, &name), key: name.clone(), name, command, source });
        }
    }
    for (source, var) in [(Source::UserFolder, "APPDATA"), (Source::CommonFolder, "ProgramData")] {
        let Some(base) = std::env::var_os(var) else { continue };
        let Ok(rd) = std::fs::read_dir(Path::new(&base).join(r"Microsoft\Windows\Start Menu\Programs\Startup")) else { continue };
        for e in rd.flatten().filter(|e| e.path().is_file()) {
            let file = e.file_name().to_string_lossy().into_owned();
            if file.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let name = Path::new(&file).file_stem().map_or_else(|| file.clone(), |n| n.to_string_lossy().into_owned());
            out.push(Startup { enabled: flag(source, &file), key: file, name, command: e.path().to_string_lossy().into_owned(), source });
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

/// Whether Camel may write machine-wide flags, that is, whether it runs elevated.
pub fn can_write_machine() -> bool {
    Scope::Machine.root().open_subkey_with_flags(Source::MachineRun.approved(), KEY_SET_VALUE).is_ok()
}

/// StartupApproved values: first byte 2 or 6 means enabled, 3 means disabled; a missing value means enabled.
pub fn is_enabled(bytes: &[u8]) -> bool {
    bytes.first().is_none_or(|b| b & 1 == 0)
}

fn approval(on: bool) -> [u8; 12] {
    let mut b = [0u8; 12];
    if on {
        b[0] = 2;
    } else {
        b[0] = 3;
        // disable time as a FILETIME: 100 ns ticks since 1601
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        b[4..].copy_from_slice(&((secs + 11_644_473_600) * 10_000_000).to_le_bytes());
    }
    b
}

/// Writes the flag and reads it back: only a change Windows will actually honour counts as done.
pub fn set_startup(item: &Startup, on: bool) -> bool {
    let root = item.source.scope().root();
    let Ok((key, _)) = root.create_subkey_with_flags(item.source.approved(), KEY_SET_VALUE) else { return false };
    let value = RegValue { bytes: Cow::Owned(approval(on).to_vec()), vtype: REG_BINARY };
    key.set_raw_value(&item.key, &value).is_ok()
        && root.open_subkey(item.source.approved()).ok().and_then(|k| k.get_raw_value(&item.key).ok()).is_some_and(|v| is_enabled(&v.bytes) == on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_uninstall_commands() {
        assert_eq!(split_command(r#""C:\A B\unins000.exe" /SILENT"#), (r"C:\A B\unins000.exe".into(), "/SILENT".into()));
        assert_eq!(split_command("MsiExec.exe /X{1234}"), ("MsiExec.exe".into(), "/X{1234}".into()));
        assert_eq!(split_command(r"C:\Program Files\X\un.exe -u"), (r"C:\Program Files\X\un.exe".into(), "-u".into()));
        assert_eq!(split_command("uninst"), ("uninst".into(), String::new()));
    }

    #[test]
    fn filters_non_programs() {
        assert!(is_listed("App", "x.exe", false, "", ""));
        assert!(!is_listed("", "x.exe", false, "", ""));
        assert!(!is_listed("App", "", false, "", ""));
        assert!(!is_listed("App", "x.exe", true, "", ""));
        assert!(!is_listed("App", "x.exe", false, "Parent", ""));
        assert!(!is_listed("App", "x.exe", false, "", "Hotfix"));
    }

    #[test]
    fn formats_sizes_and_env() {
        assert_eq!(format_size(0), "");
        assert_eq!(format_size(512), "512 KB");
        assert_eq!(format_size(250 * 1024), "250 MB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.0 GB");
        assert_eq!(expand_env("100%"), "100%");
        // set_var is unsafe in edition 2024, so use a variable every Windows session already has
        let sys = std::env::var("SystemRoot").unwrap();
        assert_eq!(expand_env("%SystemRoot%\\x"), format!("{sys}\\x"));
        assert_eq!(expand_env("%NO_SUCH_VAR_CAMEL%"), "%NO_SUCH_VAR_CAMEL%");
    }

    #[test]
    fn refuses_generic_or_shared_folders() {
        assert!(!safe_name("Microsoft"));
        assert!(!safe_name("abc"));
        assert!(!safe_name("a/b"));
        assert!(safe_name("Discord"));
        assert!(!safe_dir(Path::new(r"C:\Windows")));
        assert!(!safe_dir(Path::new(r"C:\Program Files")));
        assert!(!safe_dir(Path::new("relative\\dir")));
        assert!(safe_dir(Path::new(r"C:\Program Files\Some App")));
    }

    #[test]
    fn record_survives_a_round_trip_and_rejects_foreign_paths() {
        let app = App {
            name: "Demo".into(),
            haystack: "demo".into(),
            location: String::new(),
            uninstall: "x.exe".into(),
            size_kb: 1,
            scope: Scope::User,
            key_path: "k".into(),
        };
        let r = Record { pending: Some(app), leftover_name: "Demo".into(), leftovers: vec![PathBuf::from(r"C:\Windows")] };
        let back: Record = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back.pending.unwrap().name, "Demo");
        assert_eq!(back.leftover_name, "Demo");
        assert!(!trusted_leftover(&back.leftovers[0]));
        assert!(!trusted_leftover(Path::new(r"C:\Users")));
    }

    #[test]
    fn deletion_queue_lists_contents_before_folders() {
        let root = std::env::temp_dir().join(format!("camel-queue-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("a").join("b")).unwrap();
        std::fs::write(root.join("a").join("b").join("f.txt"), "x").unwrap();
        std::fs::write(root.join("top.txt"), "x").unwrap();
        let mut q = Vec::new();
        post_order(&root, &mut q);
        let pos = |p: PathBuf| q.iter().position(|x| *x == p).unwrap();
        assert!(pos(root.join("a").join("b").join("f.txt")) < pos(root.join("a").join("b")));
        assert!(pos(root.join("a").join("b")) < pos(root.join("a")));
        assert!(pos(root.join("a")) < pos(root.clone()));
        assert_eq!(q.last(), Some(&root));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn startup_sources_use_the_hive_of_the_item() {
        assert_eq!(Source::UserRun.scope(), Scope::User);
        assert_eq!(Source::UserFolder.scope(), Scope::User);
        assert_eq!(Source::MachineRun.scope(), Scope::Machine);
        assert_eq!(Source::MachineRun32.scope(), Scope::Machine);
        assert_eq!(Source::CommonFolder.scope(), Scope::Machine);
        assert!(Source::MachineRun32.approved().ends_with(r"StartupApproved\Run32"));
        assert!(Source::CommonFolder.approved().ends_with(r"StartupApproved\StartupFolder"));
    }

    #[test]
    fn startup_list_reads_this_machine_without_failing() {
        for s in startup_list() {
            assert!(!s.name.is_empty() && !s.key.is_empty());
        }
    }

    #[test]
    fn startup_flags() {
        assert!(is_enabled(&[]));
        assert!(is_enabled(&[2, 0, 0, 0]));
        assert!(is_enabled(&[6, 0, 0, 0]));
        assert!(!is_enabled(&[3, 0, 0, 0]));
        assert_eq!(approval(true)[0], 2);
        assert_eq!(approval(false)[0], 3);
        assert!(approval(false)[4..].iter().any(|b| *b != 0));
    }
}
