//! Reminders and to-dos typed into Spotlight ("remind 15m stretch", "remind every 45m drink water", "todo send invoice").
//! They are saved to `%APPDATA%\Camel\reminders.json`, so they survive a restart, and the parsing is pure and tested.

use chrono::{DateTime, Duration as Span, Local, NaiveTime, TimeZone};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A reminder that repeats cannot be shorter than this.
const MIN_EVERY: u64 = 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: u64,
    pub text: String,
    /// Unix seconds; a to-do without a time has none.
    pub due: Option<i64>,
    /// Seconds between repeats.
    pub every: Option<u64>,
}

/// What was typed, before it gets an id.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub text: String,
    pub due: Option<i64>,
    pub every: Option<u64>,
}

/// "15m", "1h30m", "90s" or "25" (minutes), as the focus timer reads them.
fn duration(word: &str) -> Option<u64> {
    crate::focus::parse_spoken(word).map(|d| d.as_secs())
}

/// "17:30", "5pm", "5:30pm" as a time of day.
fn clock(word: &str) -> Option<NaiveTime> {
    let w = word.to_lowercase();
    let (digits, pm) = match (w.strip_suffix("pm"), w.strip_suffix("am")) {
        (Some(d), _) => (d, Some(true)),
        (_, Some(d)) => (d, Some(false)),
        _ => (w.as_str(), None),
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if pm.is_some() => (digits.parse::<u32>().ok()?, 0),
        None => return None,
    };
    let h = match pm {
        Some(true) if (1..=12).contains(&h) => h % 12 + 12,
        Some(false) if (1..=12).contains(&h) => h % 12,
        Some(_) => return None,
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// The next moment the clock shows `t`: today if it is still ahead, otherwise tomorrow.
fn next_at(now: DateTime<Local>, t: NaiveTime) -> Option<DateTime<Local>> {
    let today = Local.from_local_datetime(&now.date_naive().and_time(t)).single()?;
    if today > now { Some(today) } else { Local.from_local_datetime(&(now.date_naive() + Span::days(1)).and_time(t)).single() }
}

/// Reads "15m stretch", "17:30 call mom", "every 45m drink water" or just "buy milk".
pub fn parse(input: &str, now: DateTime<Local>) -> Option<Spec> {
    let input = input.trim();
    let first = input.split_whitespace().next()?;
    let rest = |skip: usize| input.split_whitespace().skip(skip).collect::<Vec<_>>().join(" ");
    let (text, due, every) = if first.eq_ignore_ascii_case("every") {
        let secs = duration(input.split_whitespace().nth(1)?)?.max(MIN_EVERY);
        (rest(2), Some(now.timestamp() + secs as i64), Some(secs))
    } else if let Some(t) = clock(first) {
        (rest(1), Some(next_at(now, t)?.timestamp()), None)
    } else if first.eq_ignore_ascii_case("at") {
        let t = clock(input.split_whitespace().nth(1)?)?;
        (rest(2), Some(next_at(now, t)?.timestamp()), None)
    } else if let Some(secs) = duration(first) {
        (rest(1), Some(now.timestamp() + secs as i64), None)
    } else {
        (rest(0), None, None)
    };
    (!text.is_empty()).then_some(Spec { text, due, every }).map(|s| Spec { text: s.text.chars().take(120).collect(), ..s })
}

/// "in 15 min", "at 17:30", "every 45 min" or "to-do", for the row in Spotlight.
pub fn describe(spec_due: Option<i64>, every: Option<u64>, now: DateTime<Local>) -> String {
    if let Some(secs) = every {
        return format!("every {}", crate::actions::format_duration(std::time::Duration::from_secs(secs)));
    }
    match spec_due.and_then(|d| Local.timestamp_opt(d, 0).single()) {
        Some(due) if due.date_naive() == now.date_naive() => format!("at {}", due.format("%H:%M")),
        Some(due) => format!("tomorrow {}", due.format("%H:%M")),
        None => "to-do".to_owned(),
    }
}

/// The reminders in memory. Only a store made by `load` writes to disk; tests use `Default`, which does not.
#[derive(Default)]
pub struct Reminders {
    items: Vec<Reminder>,
    next_id: u64,
    file: Option<PathBuf>,
}

fn path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Camel").join("reminders.json")
}

impl Reminders {
    pub fn load() -> Self {
        let items: Vec<Reminder> = std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        Self { next_id: items.iter().map(|r| r.id + 1).max().unwrap_or(1), items, file: Some(path()) }
    }

    fn save(&self) {
        let Some(p) = &self.file else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.items) {
            let _ = std::fs::write(p, json);
        }
    }

    /// Soonest first, to-dos without a time last.
    pub fn list(&self) -> Vec<Reminder> {
        let mut v = self.items.clone();
        v.sort_by_key(|r| (r.due.is_none(), r.due, r.id));
        v
    }

    pub fn add(&mut self, spec: Spec) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Reminder { id, text: spec.text, due: spec.due, every: spec.every });
        self.save();
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.items.retain(|r| r.id != id);
        self.save();
    }

    /// The reminders that are due at `now`. A repeating one is moved to its next time, the others are removed.
    pub fn take_due(&mut self, now: i64) -> Vec<Reminder> {
        let due: Vec<Reminder> = self.items.iter().filter(|r| r.due.is_some_and(|d| d <= now)).cloned().collect();
        if due.is_empty() {
            return due;
        }
        self.items.retain_mut(|r| match (r.due, r.every) {
            (Some(d), Some(every)) if d <= now => {
                let every = every.max(MIN_EVERY) as i64;
                r.due = Some(d + ((now - d) / every + 1) * every);
                true
            }
            (Some(d), None) => d > now,
            _ => true,
        });
        self.save();
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 11, h, m, 0).unwrap()
    }

    #[test]
    fn reads_durations_clock_times_repeats_and_plain_text() {
        let now = at(14, 0);
        let spec = |i: &str| parse(i, now).unwrap();
        assert_eq!(spec("15m stretch"), Spec { text: "stretch".into(), due: Some(now.timestamp() + 900), every: None });
        assert_eq!(spec("1h30m call mom").due, Some(now.timestamp() + 5400));
        assert_eq!(spec("17:30 call mom"), Spec { text: "call mom".into(), due: Some(at(17, 30).timestamp()), every: None });
        assert_eq!(spec("at 5pm review").due, Some(at(17, 0).timestamp()));
        assert_eq!(spec("5:30pm review").due, Some(at(17, 30).timestamp()));
        assert_eq!(spec("12am sleep").due, Some((at(0, 0) + Span::days(1)).timestamp()), "midnight has passed, so tomorrow");
        assert_eq!(spec("9:00 standup").due, Some((at(9, 0) + Span::days(1)).timestamp()), "already past today");
        let every = spec("every 45m drink water");
        assert_eq!((every.every, every.due), (Some(2700), Some(now.timestamp() + 2700)));
        assert_eq!(spec("every 10s too often").every, Some(60), "no faster than a minute");
        assert_eq!(spec("buy milk"), Spec { text: "buy milk".into(), due: None, every: None });
    }

    #[test]
    fn needs_some_text() {
        let now = at(14, 0);
        for bad in ["", "   ", "15m", "17:30", "every 45m", "at 5pm", "every"] {
            assert_eq!(parse(bad, now), None, "{bad}");
        }
        assert!(parse("25:70 not a time", now).is_some_and(|s| s.due.is_some() || s.text.contains("not a time")));
        assert_eq!(parse(&format!("x {}", "y".repeat(500)), now).unwrap().text.chars().count(), 120);
    }

    #[test]
    fn describes_what_was_asked() {
        let now = at(14, 0);
        assert_eq!(describe(Some(at(17, 30).timestamp()), None, now), "at 17:30");
        assert_eq!(describe(Some((at(9, 0) + Span::days(1)).timestamp()), None, now), "tomorrow 09:00");
        assert_eq!(describe(None, Some(2700), now), "every 45 min");
        assert_eq!(describe(None, None, now), "to-do");
    }

    #[test]
    fn due_reminders_fire_once_and_repeats_move_on() {
        let items = vec![
            Reminder { id: 1, text: "once".into(), due: Some(100), every: None },
            Reminder { id: 2, text: "repeat".into(), due: Some(100), every: Some(60) },
            Reminder { id: 3, text: "later".into(), due: Some(10_000), every: None },
            Reminder { id: 4, text: "no time".into(), due: None, every: None },
        ];
        let mut r = Reminders { items, next_id: 5, file: None };
        let fired = r.take_due(500);
        assert_eq!(fired.iter().map(|f| f.id).collect::<Vec<_>>(), [1, 2]);
        let left = r.list();
        assert_eq!(left.iter().map(|f| f.id).collect::<Vec<_>>(), [2, 3, 4], "soonest first, to-dos last");
        assert_eq!(left[0].due, Some(520), "the repeat skips the times that were missed: 100, 160, ... 460, then 520");
        assert!(r.take_due(500).is_empty(), "nothing fires twice");
    }
}
