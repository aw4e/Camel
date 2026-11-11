//! What Spotlight can do besides search: system actions, a keep-awake switch, timers, quitting programs and unit
//! conversion. Parsing is pure and tested; only `power`, `lock`, `sleep`, `kill` and `count_processes` touch the system.

use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Lock,
    Sleep,
    Restart,
    Shutdown,
    SignOut,
    KeepAwake,
    /// Mute the speakers, or the microphone when true.
    Mute(bool),
    /// Save a quick note.
    Note(String),
    OpenNotes,
    /// Add a reminder or to-do.
    Remind(crate::reminders::Spec),
    /// Tick a reminder off by id.
    Done(u64),
    /// Ask again about the last reminder in ten minutes.
    Snooze,
    /// Switch the weather on the Clock page on or off.
    Weather,
    /// Use this city for the weather instead of the Windows location.
    City(String),
    /// Check for a newer version, or install the one that was found.
    Update,
    Timer(Duration),
    Kill(String),
}

impl Action {
    /// Things that lose unsaved work ask for a second Enter.
    pub fn needs_confirm(&self) -> bool {
        matches!(self, Action::Restart | Action::Shutdown | Action::SignOut | Action::Kill(_))
    }
}

pub struct Suggestion {
    pub action: Action,
    pub title: String,
    pub detail: String,
}

/// "1 h 30 min", "25 min", "90 s".
pub fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    match (s / 3600, s % 3600 / 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, 0) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

/// Programs that must not be closed from a search box, however the name is typed.
const PROTECTED: [&str; 14] =
    ["system", "registry", "smss", "csrss", "wininit", "winlogon", "services", "lsass", "svchost", "dwm", "fontdrvhost", "explorer", "camel", "msmpeng"];

fn clean_name(n: &str) -> String {
    n.to_lowercase().trim_end_matches(".exe").to_owned()
}

fn protected(name: &str) -> bool {
    PROTECTED.contains(&name)
}

/// Names (without .exe) of running programs that start with `prefix`, never the protected ones.
fn matching_processes(prefix: &str) -> Vec<sysinfo::Pid> {
    use sysinfo::{ProcessesToUpdate, System};
    let prefix = clean_name(prefix);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .filter(|(_, p)| {
            let n = clean_name(&p.name().to_string_lossy());
            n.starts_with(&prefix) && !protected(&n)
        })
        .map(|(pid, _)| *pid)
        .collect()
}

pub fn count_processes(prefix: &str) -> usize {
    if prefix.trim().chars().count() < 2 || protected(&clean_name(prefix.trim())) {
        return 0;
    }
    matching_processes(prefix.trim()).len()
}

/// Ends every matching program and returns how many it ended.
pub fn kill(prefix: &str) -> usize {
    use sysinfo::{ProcessesToUpdate, System};
    if prefix.trim().chars().count() < 2 {
        return 0;
    }
    let pids = matching_processes(prefix.trim());
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    pids.iter().filter(|pid| sys.process(**pid).is_some_and(|p| p.kill())).count()
}

/// `shutdown.exe` does the work, so Camel needs no shutdown privilege of its own.
pub fn power(flag: &str) {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new(crate::platform::system_tool("shutdown.exe"));
    c.arg(flag);
    if flag != "/l" {
        c.args(["/t", "0"]);
    }
    let _ = c.creation_flags(0x0800_0000).spawn();
}

pub fn lock() {
    let _ = unsafe { windows::Win32::System::Shutdown::LockWorkStation() };
}

pub fn sleep() {
    // sleep, not hibernate; do not force running programs, do not disable wake events
    let _ = unsafe { windows::Win32::System::Power::SetSuspendState(false, false, false) };
}

/// Keywords that name a system action. Typing three letters or more of one is enough.
const KEYWORDS: [(&str, &[&str]); 10] = [
    ("lock", &["lock", "lock screen"]),
    ("sleep", &["sleep", "suspend"]),
    ("restart", &["restart", "reboot"]),
    ("shutdown", &["shutdown", "shut down", "power off", "poweroff"]),
    ("signout", &["sign out", "signout", "log out", "logout", "log off"]),
    ("awake", &["keep awake", "awake", "caffeine", "stay awake"]),
    ("mute", &["mute", "unmute", "silence"]),
    ("mic", &["mic", "microphone", "mute mic", "mic mute"]),
    ("notes", &["notes", "my notes", "open notes"]),
    ("update", &["update", "updates", "check for updates", "check updates", "version"]),
];

fn keyword_match(q: &str) -> Option<&'static str> {
    KEYWORDS.iter().find(|(_, words)| words.iter().any(|w| *w == q || (q.len() >= 3 && w.starts_with(q)))).map(|(k, _)| *k)
}

/// Rows to offer for what has been typed. `awake` is the current keep-awake state, so the row can say "off".
pub fn suggest(query: &str, awake: bool, pending: &[crate::reminders::Reminder]) -> Vec<Suggestion> {
    let q = query.trim().to_lowercase();
    let mut out = Vec::new();
    let mut add = |action: Action, title: &str, detail: &str| out.push(Suggestion { action, title: title.to_owned(), detail: detail.to_owned() });

    if let Some(rest) = q.strip_prefix("timer ").or_else(|| q.strip_prefix("focus ")) {
        if let Some(d) = crate::focus::parse_spoken(rest) {
            add(Action::Timer(d), &format!("Start a {} timer", format_duration(d)), "Focus timer");
        }
    } else if let Some(rest) = ["remind ", "todo ", "task "].iter().find(|p| q.starts_with(**p)).and_then(|p| query.trim().get(p.len()..)) {
        let now = chrono::Local::now();
        if let Some(spec) = crate::reminders::parse(rest, now) {
            let detail = crate::reminders::describe(spec.due, spec.every, now);
            let title = format!("Remind: {}", spec.text);
            add(Action::Remind(spec), &title, &detail);
        }
    } else if matches!(q.as_str(), "remind" | "reminders" | "todo" | "todos" | "task" | "tasks") {
        let now = chrono::Local::now();
        for r in pending.iter().take(5) {
            add(Action::Done(r.id), &r.text, &format!("{} · Enter to tick off", crate::reminders::describe(r.due, r.every, now)));
        }
    } else if let Some(city) = q.strip_prefix("city ").and_then(|_| query.trim().get(5..)).map(str::trim).filter(|c| !c.is_empty()) {
        add(Action::City(city.to_owned()), &format!("Use {city} for the weather"), "Instead of the Windows location");
    } else if q == "weather" {
        add(Action::Weather, "Switch the weather on or off", "Temperature and humidity on the Clock page");
    } else if q == "snooze" {
        add(Action::Snooze, "Snooze the last reminder", "Ask again in 10 minutes");
    } else if let Some(text) = query.trim().get(5..).filter(|_| q.starts_with("note ")).map(str::trim).filter(|t| !t.is_empty()) {
        add(Action::Note(text.to_owned()), "Save note", text);
    } else if let Some(name) = q.strip_prefix("kill ").or_else(|| q.strip_prefix("quit ")) {
        let n = count_processes(name);
        if n > 0 {
            add(
                Action::Kill(name.trim().to_owned()),
                &format!("Quit {}", name.trim()),
                &format!("{n} running {}", if n == 1 { "program" } else { "programs" }),
            );
        }
    } else if let Some(k) = keyword_match(&q) {
        match k {
            "lock" => add(Action::Lock, "Lock this PC", "Action"),
            "sleep" => add(Action::Sleep, "Sleep", "Action"),
            "restart" => add(Action::Restart, "Restart", "Action"),
            "shutdown" => add(Action::Shutdown, "Shut down", "Action"),
            "signout" => add(Action::SignOut, "Sign out", "Action"),
            "mute" => add(Action::Mute(false), "Mute or unmute the speakers", "Action"),
            "mic" => add(Action::Mute(true), "Mute or unmute the microphone", "Action"),
            "notes" => add(Action::OpenNotes, "Open notes", "Your quick notes"),
            "update" => add(Action::Update, "Check for updates", &format!("Camel {}", crate::update::VERSION)),
            _ if awake => add(Action::KeepAwake, "Let this PC sleep again", "Keep awake is on"),
            _ => add(Action::KeepAwake, "Keep this PC awake", "Stops the screen and sleep from turning off"),
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Dim {
    Length,
    Mass,
    Volume,
    Time,
    Speed,
    Temperature,
}

/// (names, dimension, factor to the dimension's base unit: m, g, l, s, m/s). Temperature is handled apart.
const UNITS: &[(&[&str], Dim, f64)] = &[
    (&["mm", "millimeter", "millimeters", "millimetre", "millimetres"], Dim::Length, 0.001),
    (&["cm", "centimeter", "centimeters", "centimetre", "centimetres"], Dim::Length, 0.01),
    (&["m", "meter", "meters", "metre", "metres"], Dim::Length, 1.0),
    (&["km", "kilometer", "kilometers", "kilometre", "kilometres"], Dim::Length, 1000.0),
    (&["in", "inch", "inches"], Dim::Length, 0.0254),
    (&["ft", "foot", "feet"], Dim::Length, 0.3048),
    (&["yd", "yard", "yards"], Dim::Length, 0.9144),
    (&["mi", "mile", "miles"], Dim::Length, 1609.344),
    (&["mg", "milligram", "milligrams"], Dim::Mass, 0.001),
    (&["g", "gram", "grams"], Dim::Mass, 1.0),
    (&["kg", "kilogram", "kilograms", "kilo", "kilos"], Dim::Mass, 1000.0),
    (&["oz", "ounce", "ounces"], Dim::Mass, 28.349_523_125),
    (&["lb", "lbs", "pound", "pounds"], Dim::Mass, 453.592_37),
    (&["ml", "milliliter", "milliliters", "millilitre", "millilitres"], Dim::Volume, 0.001),
    (&["l", "liter", "liters", "litre", "litres"], Dim::Volume, 1.0),
    (&["gal", "gallon", "gallons"], Dim::Volume, 3.785_411_784),
    (&["cup", "cups"], Dim::Volume, 0.236_588_236_5),
    (&["s", "sec", "secs", "second", "seconds"], Dim::Time, 1.0),
    (&["min", "mins", "minute", "minutes"], Dim::Time, 60.0),
    (&["h", "hr", "hrs", "hour", "hours"], Dim::Time, 3600.0),
    (&["d", "day", "days"], Dim::Time, 86_400.0),
    (&["wk", "week", "weeks"], Dim::Time, 604_800.0),
    (&["ms", "m/s"], Dim::Speed, 1.0),
    (&["kmh", "km/h", "kph"], Dim::Speed, 1.0 / 3.6),
    (&["mph"], Dim::Speed, 0.447_04),
    (&["c", "celsius"], Dim::Temperature, 0.0),
    (&["f", "fahrenheit"], Dim::Temperature, 0.0),
    (&["k", "kelvin"], Dim::Temperature, 0.0),
];

fn unit(name: &str) -> Option<(Dim, f64, &'static str)> {
    let n = name.trim();
    UNITS.iter().find(|(names, ..)| names.contains(&n)).map(|(names, dim, f)| (*dim, *f, names[0]))
}

fn to_celsius(v: f64, canon: &str) -> f64 {
    match canon {
        "f" => (v - 32.0) * 5.0 / 9.0,
        "k" => v - 273.15,
        _ => v,
    }
}

fn from_celsius(c: f64, canon: &str) -> f64 {
    match canon {
        "f" => c * 9.0 / 5.0 + 32.0,
        "k" => c + 273.15,
        _ => c,
    }
}

fn fmt_num(v: f64) -> String {
    if v != 0.0 && (v.abs() >= 1e9 || v.abs() < 1e-4) {
        return format!("{v:.4e}");
    }
    format!("{v:.5}").trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// "5 km in miles", "100 f to c", "90 min to h": returns the result and the question it answers.
pub fn convert(query: &str) -> Option<(String, String)> {
    let q = query.trim().to_lowercase();
    // "in" is both a separator and a unit (inches), so every split point is tried, not just the first
    [" in ", " to ", " into ", " = ", "->"]
        .iter()
        .flat_map(|sep| q.match_indices(sep).map(move |(i, _)| (i, sep.len())))
        .find_map(|(i, len)| convert_parts(q[..i].trim(), q[i + len..].trim()))
}

fn convert_parts(left: &str, right: &str) -> Option<(String, String)> {
    let split = left.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | ',')))?;
    let value: f64 = left[..split].replace(',', ".").parse().ok()?;
    let (dim_a, fa, name_a) = unit(&left[split..])?;
    let (dim_b, fb, name_b) = unit(right)?;
    if dim_a != dim_b {
        return None;
    }
    let result = if dim_a == Dim::Temperature { from_celsius(to_celsius(value, name_a), name_b) } else { value * fa / fb };
    Some((format!("{} {}", fmt_num(result), right), format!("{} {} =", fmt_num(value), left[split..].trim())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn converts_units() {
        let r = |q: &str| convert(q).map(|(t, _)| t);
        assert_eq!(r("5 km in miles").as_deref(), Some("3.10686 miles"));
        assert_eq!(r("100 f to c").as_deref(), Some("37.77778 c"));
        assert_eq!(r("0 c to k").as_deref(), Some("273.15 k"));
        assert_eq!(r("2 kg to lb").as_deref(), Some("4.40925 lb"));
        assert_eq!(r("90 min to h").as_deref(), Some("1.5 h"));
        assert_eq!(r("60 mph in km/h").as_deref(), Some("96.56064 km/h"));
        assert_eq!(r("5km in mi").as_deref(), Some("3.10686 mi"));
        assert_eq!(r("12 in to cm").as_deref(), Some("30.48 cm"));
        assert_eq!(convert("5 km in miles").unwrap().1, "5 km =");
        // different kinds of thing, unknown units, and plain text are not conversions
        assert!(convert("5 km in kg").is_none());
        assert!(convert("5 foo in bar").is_none());
        assert!(convert("5 km").is_none());
        assert!(convert("hello in world").is_none());
    }

    #[test]
    fn suggests_actions_from_keywords() {
        let one = |q: &str| suggest(q, false, &[]).into_iter().next().map(|s| s.action);
        assert_eq!(one("lock"), Some(Action::Lock));
        assert_eq!(one("shu"), Some(Action::Shutdown));
        assert_eq!(one("Shut Down"), Some(Action::Shutdown));
        assert_eq!(one("reboot"), Some(Action::Restart));
        assert_eq!(one("log out"), Some(Action::SignOut));
        assert_eq!(one("caffeine"), Some(Action::KeepAwake));
        assert_eq!(one("mute"), Some(Action::Mute(false)));
        assert_eq!(one("mic"), Some(Action::Mute(true)));
        assert_eq!(one("mute mic"), Some(Action::Mute(true)));
        assert_eq!(one("notes"), Some(Action::OpenNotes));
        assert!(matches!(one("remind 15m stretch"), Some(Action::Remind(spec)) if spec.text == "stretch"));
        assert!(matches!(one("TODO Buy Milk"), Some(Action::Remind(spec)) if spec.text == "Buy Milk" && spec.due.is_none()));
        assert_eq!(one("remind 15m"), None, "a reminder needs text");
        assert_eq!(one("snooze"), Some(Action::Snooze));
        assert_eq!(one("weather"), Some(Action::Weather));
        assert_eq!(one("City  Bandung "), Some(Action::City("Bandung".into())));
        assert_eq!(one("city "), None);
        let pending = [crate::reminders::Reminder { id: 7, text: "call mom".into(), due: None, every: None }];
        let listed = suggest("todo", false, &pending);
        assert_eq!((listed.len(), &listed[0].action), (1, &Action::Done(7)));
        assert_eq!(one("check for updates"), Some(Action::Update));
        assert_eq!(one("version"), Some(Action::Update));
        assert_eq!(one("note Buy  Milk "), Some(Action::Note("Buy  Milk".into())));
        assert_eq!(one("note "), Some(Action::OpenNotes), "no text yet: offer the notes file, not an empty note");
        // two letters are too little, and unrelated words match nothing
        assert_eq!(one("sl"), None);
        assert_eq!(one("firefox"), None);
        assert_eq!(suggest("awake", true, &[])[0].title, "Let this PC sleep again");
        assert_eq!(suggest("awake", false, &[])[0].title, "Keep this PC awake");
    }

    #[test]
    fn timer_rows_parse_the_time() {
        let s = suggest("timer 10m", false, &[]);
        assert_eq!(s[0].action, Action::Timer(secs(600)));
        assert_eq!(s[0].title, "Start a 10 min timer");
        assert_eq!(suggest("focus 1h30m", false, &[])[0].action, Action::Timer(secs(5400)));
        assert!(suggest("timer soon", false, &[]).is_empty());
    }

    #[test]
    fn only_risky_actions_need_a_second_enter() {
        for a in [Action::Restart, Action::Shutdown, Action::SignOut, Action::Kill("x".into())] {
            assert!(a.needs_confirm());
        }
        for a in [Action::Lock, Action::Sleep, Action::KeepAwake, Action::Timer(secs(60))] {
            assert!(!a.needs_confirm());
        }
    }

    #[test]
    fn will_not_offer_to_close_system_programs() {
        assert_eq!(count_processes("explorer"), 0);
        assert_eq!(count_processes("camel"), 0);
        assert_eq!(count_processes("svchost.exe"), 0);
        assert_eq!(count_processes("x"), 0); // one letter would match far too much
        assert!(suggest("kill explorer", false, &[]).is_empty());
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(secs(90)), "1 min 30 s");
        assert_eq!(format_duration(secs(45)), "45 s");
        assert_eq!(format_duration(secs(3600)), "1 h");
        assert_eq!(format_duration(secs(5400)), "1 h 30 min");
    }
}
