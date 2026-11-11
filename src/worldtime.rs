//! "time tokyo" and "until 25 dec" answers for Spotlight: the time in another city, and the days left to a date.
//! Time zones come from Windows, so daylight saving is right without shipping a time zone database.

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};

/// (names people type, display name, Windows time zone key).
const CITIES: &[(&[&str], &str, &str)] = &[
    (&["utc", "gmt"], "UTC", "UTC"),
    (&["jakarta", "bangkok", "hanoi"], "Jakarta", "SE Asia Standard Time"),
    (&["singapore", "kuala lumpur", "manila"], "Singapore", "Singapore Standard Time"),
    (&["tokyo", "osaka"], "Tokyo", "Tokyo Standard Time"),
    (&["seoul"], "Seoul", "Korea Standard Time"),
    (&["beijing", "shanghai", "hong kong"], "Beijing", "China Standard Time"),
    (&["delhi", "mumbai", "new delhi", "kolkata"], "Delhi", "India Standard Time"),
    (&["karachi"], "Karachi", "Pakistan Standard Time"),
    (&["dubai", "abu dhabi"], "Dubai", "Arabian Standard Time"),
    (&["riyadh", "mecca", "jeddah"], "Riyadh", "Arab Standard Time"),
    (&["istanbul"], "Istanbul", "Turkey Standard Time"),
    (&["moscow"], "Moscow", "Russian Standard Time"),
    (&["cairo"], "Cairo", "Egypt Standard Time"),
    (&["johannesburg", "cape town"], "Johannesburg", "South Africa Standard Time"),
    (&["lagos"], "Lagos", "W. Central Africa Standard Time"),
    (&["london", "dublin", "lisbon"], "London", "GMT Standard Time"),
    (&["paris", "madrid", "brussels", "copenhagen"], "Paris", "Romance Standard Time"),
    (&["berlin", "amsterdam", "rome", "vienna", "zurich", "stockholm", "oslo"], "Berlin", "W. Europe Standard Time"),
    (&["new york", "toronto", "washington", "miami"], "New York", "Eastern Standard Time"),
    (&["chicago", "dallas", "houston"], "Chicago", "Central Standard Time"),
    (&["mexico city"], "Mexico City", "Central Standard Time (Mexico)"),
    (&["denver"], "Denver", "Mountain Standard Time"),
    (&["los angeles", "san francisco", "seattle", "vancouver"], "Los Angeles", "Pacific Standard Time"),
    (&["honolulu"], "Honolulu", "Hawaiian Standard Time"),
    (&["sao paulo"], "Sao Paulo", "E. South America Standard Time"),
    (&["sydney", "melbourne", "canberra"], "Sydney", "AUS Eastern Standard Time"),
    (&["auckland", "wellington"], "Auckland", "New Zealand Standard Time"),
];

fn city(name: &str) -> Option<(&'static str, &'static str)> {
    let name = name.trim().to_lowercase();
    CITIES.iter().find(|(names, ..)| names.contains(&name.as_str())).map(|(_, display, zone)| (*display, *zone))
}

/// The wall clock in a Windows time zone right now.
fn now_in(zone_key: &str) -> Option<NaiveDateTime> {
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::SystemInformation::GetSystemTime;
    use windows::Win32::System::Time::{DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation, SystemTimeToTzSpecificLocalTimeEx};
    for index in 0.. {
        let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
        if unsafe { EnumDynamicTimeZoneInformation(index, &mut info) } != 0 {
            return None;
        }
        let len = info.TimeZoneKeyName.iter().position(|c| *c == 0).unwrap_or(info.TimeZoneKeyName.len());
        if String::from_utf16_lossy(&info.TimeZoneKeyName[..len]) != zone_key {
            continue;
        }
        let utc = unsafe { GetSystemTime() };
        let mut local = SYSTEMTIME::default();
        unsafe { SystemTimeToTzSpecificLocalTimeEx(Some(&info), &utc, &mut local) }.ok()?;
        return NaiveDate::from_ymd_opt(i32::from(local.wYear), u32::from(local.wMonth), u32::from(local.wDay))?.and_hms_opt(
            u32::from(local.wHour),
            u32::from(local.wMinute),
            u32::from(local.wSecond),
        );
    }
    None
}

/// "+2 h", "-13 h", "+5:30 h" or "same time", the gap between two clocks.
fn gap_label(minutes: i64) -> String {
    if minutes == 0 {
        return "same time as here".to_owned();
    }
    let (sign, m) = (if minutes < 0 { '-' } else { '+' }, minutes.abs());
    if m % 60 == 0 { format!("{sign}{} h from here", m / 60) } else { format!("{sign}{}:{:02} h from here", m / 60, m % 60) }
}

/// Which city a query asks the time of: "time tokyo", "time in tokyo", "tokyo time".
fn time_query(q: &str) -> Option<&str> {
    let q = q.trim();
    let lower = q.to_lowercase();
    let tail = |n: &str| q.get(q.len().checked_sub(n.len())?..);
    let name = lower
        .strip_prefix("time in ")
        .or_else(|| lower.strip_prefix("time "))
        .and_then(tail)
        .or_else(|| lower.strip_suffix(" time").and_then(|n| q.get(..n.len())))?;
    Some(name.trim())
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

fn month(word: &str) -> Option<u32> {
    let w = word.to_lowercase();
    (w.len() >= 3).then(|| MONTHS.iter().position(|m| w.starts_with(m)).map(|i| i as u32 + 1)).flatten()
}

/// "2026-12-25", "25 dec", "dec 25", "25 december 2026". Without a year it is the next such day.
pub fn parse_date(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = s.trim().to_lowercase().replace(',', " ");
    if let Ok(d) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
        return Some(d);
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    let (day, mon, year) = match words.as_slice() {
        [a, b] | [a, b, _] if month(b).is_some() => (a.parse().ok()?, month(b)?, words.get(2).and_then(|y| y.parse().ok())),
        [a, b] | [a, b, _] if month(a).is_some() => (b.parse().ok()?, month(a)?, words.get(2).and_then(|y| y.parse().ok())),
        _ => return None,
    };
    match year {
        Some(y) => NaiveDate::from_ymd_opt(y, mon, day),
        None => {
            let this_year = NaiveDate::from_ymd_opt(today.year(), mon, day)?;
            if this_year >= today { Some(this_year) } else { NaiveDate::from_ymd_opt(today.year() + 1, mon, day) }
        }
    }
}

/// "until 25 dec", "days until 2026-12-25", "countdown 25 dec 2026".
fn countdown_query(q: &str) -> Option<&str> {
    let q = q.trim();
    let lower = q.to_lowercase();
    ["days until ", "days to ", "countdown ", "until "].iter().find_map(|p| lower.strip_prefix(p).and_then(|rest| q.get(q.len().checked_sub(rest.len())?..)))
}

/// "Today", "Tomorrow", "1 day", "44 days", or "N days ago" for the past.
fn days_label(days: i64) -> String {
    match days {
        0 => "Today".to_owned(),
        1 => "Tomorrow".to_owned(),
        -1 => "Yesterday".to_owned(),
        d if d > 0 => format!("{d} days"),
        d => format!("{} days ago", -d),
    }
}

/// (title, detail) for a query that asks the time somewhere or the days to a date; None for anything else.
pub fn answer(query: &str) -> Option<(String, String)> {
    if let Some(name) = time_query(query) {
        let (display, zone) = city(name)?;
        let there = now_in(zone)?;
        let here = Local::now().naive_local();
        let minutes = (there - here).num_minutes();
        let day = match (there.date() - here.date()).num_days() {
            0 => "",
            1 => " tomorrow",
            -1 => " yesterday",
            _ => "",
        };
        return Some((format!("{}{day}", there.format("%H:%M")), format!("{display}, {}", gap_label(minutes))));
    }
    let date = parse_date(countdown_query(query)?, Local::now().date_naive())?;
    let days = (date - Local::now().date_naive()).num_days();
    Some((days_label(days), format!("until {}", date.format("%d %b %Y"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 11).unwrap()
    }

    #[test]
    fn finds_cities_by_name() {
        assert_eq!(city("Tokyo").map(|c| c.1), Some("Tokyo Standard Time"));
        assert_eq!(city(" new york ").map(|c| c.0), Some("New York"));
        assert_eq!(city("atlantis"), None);
    }

    #[test]
    fn reads_which_city_a_query_asks_about() {
        assert_eq!(time_query("time tokyo"), Some("tokyo"));
        assert_eq!(time_query("Time in New York"), Some("New York"));
        assert_eq!(time_query("london time"), Some("london"));
        assert_eq!(time_query("sometime"), None);
        assert_eq!(time_query("timer 10m"), None);
    }

    #[test]
    fn windows_knows_every_zone_in_the_table() {
        for (_, display, zone) in CITIES {
            assert!(now_in(zone).is_some(), "{display}: {zone}");
        }
        assert!(now_in("Not A Zone").is_none());
    }

    #[test]
    fn a_time_query_gives_a_time_and_a_gap() {
        let (title, detail) = answer("time utc").unwrap();
        assert!(title.len() >= 5 && title.as_bytes()[2] == b':', "{title}");
        assert!(detail.starts_with("UTC, "), "{detail}");
        assert!(answer("time atlantis").is_none());
    }

    #[test]
    fn labels_the_gap_between_clocks() {
        assert_eq!(gap_label(0), "same time as here");
        assert_eq!(gap_label(120), "+2 h from here");
        assert_eq!(gap_label(-780), "-13 h from here");
        assert_eq!(gap_label(330), "+5:30 h from here");
    }

    #[test]
    fn reads_dates_in_the_common_forms() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day);
        assert_eq!(parse_date("2026-12-25", today()), d(2026, 12, 25));
        assert_eq!(parse_date("25 dec", today()), d(2026, 12, 25));
        assert_eq!(parse_date("Dec 25", today()), d(2026, 12, 25));
        assert_eq!(parse_date("25 December 2027", today()), d(2027, 12, 25));
        assert_eq!(parse_date("1 jan", today()), d(2027, 1, 1), "no year: the next one");
        assert_eq!(parse_date("11 nov", today()), d(2026, 11, 11), "today counts");
        for bad in ["", "soon", "31 feb", "25", "dec"] {
            assert_eq!(parse_date(bad, today()), None, "{bad}");
        }
    }

    #[test]
    fn counts_days() {
        assert_eq!(days_label(0), "Today");
        assert_eq!(days_label(1), "Tomorrow");
        assert_eq!(days_label(44), "44 days");
        assert_eq!(days_label(-3), "3 days ago");
        assert_eq!(countdown_query("days until 25 dec"), Some("25 dec"));
        assert_eq!(countdown_query("Until 2026-12-25"), Some("2026-12-25"));
        assert_eq!(countdown_query("untilt"), None);
        let (title, detail) = answer("until 2999-01-01").unwrap();
        assert!(title.ends_with(" days") && detail == "until 01 Jan 2999", "{title} / {detail}");
    }
}
