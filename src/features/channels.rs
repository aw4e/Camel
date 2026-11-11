//! The mixer's channels. Apps that play audio are grouped by what they are for, the way Elgato Wave Link groups them: a
//! fader per channel sets the volume of every app in it. The grouping starts from a list of well-known apps and anything
//! the user moved is remembered by app name. Pure logic; the audio calls live in `services::mixer`.

use std::collections::HashMap;
use std::sync::LazyLock;

pub const NAMES: [&str; 5] = ["System", "Music", "Browser", "Voice", "Game"];

/// Which app belongs where, as data: `assets/mixer-apps.json`. Anything not listed lands in Game, and hidden apps never show.
struct Catalog {
    channels: [Vec<String>; 5],
    hidden: Vec<String>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| parse_catalog(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/mixer-apps.json"))));

fn parse_catalog(json: &str) -> Catalog {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let list = |key: &str| -> Vec<String> { v[key].as_array().into_iter().flatten().filter_map(|x| x.as_str()).map(str::to_lowercase).collect() };
    Catalog { channels: [list("system"), list("music"), list("browser"), list("voice"), Vec::new()], hidden: list("hidden") }
}

pub fn is_hidden(app: &str) -> bool {
    let name = app.to_lowercase();
    CATALOG.hidden.contains(&name)
}

pub fn default_channel(app: &str) -> usize {
    let name = app.to_lowercase();
    CATALOG.channels.iter().position(|apps| apps.contains(&name)).unwrap_or(4)
}

/// The channel of an app: where the user put it, otherwise where it belongs by default.
pub fn channel_of(app: &str, moved: &HashMap<String, usize>) -> usize {
    moved.get(&app.to_lowercase()).copied().filter(|c| *c < NAMES.len()).unwrap_or_else(|| default_channel(app))
}

/// The next channel, for the click that moves an app along.
pub fn next(channel: usize) -> usize {
    (channel + 1) % NAMES.len()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Strip {
    /// Indexes into the list of apps given to `summarize`.
    pub apps: Vec<usize>,
    /// The average level of the apps; 1.0 for an empty channel.
    pub level: f32,
    /// True when the channel has apps and all of them are muted.
    pub muted: bool,
    /// The loudest app right now, 0..1.
    pub peak: f32,
}

/// One entry per app name: a browser with many helper processes has many audio sessions, but it is one app. Returns the
/// first index of each name, in order.
pub fn one_per_app(indexes: &[usize], name_of: impl Fn(usize) -> String) -> Vec<usize> {
    let mut seen = Vec::new();
    indexes
        .iter()
        .copied()
        .filter(|i| {
            let name = name_of(*i).to_lowercase();
            let fresh = !seen.contains(&name);
            seen.push(name);
            fresh
        })
        .collect()
}

/// A plain text report for a bug report: the output, and every audio session with its channel, level and mute state.
pub fn report(version: &str, os: &str, output: &str, sessions: &[(&str, usize, f32, bool)], moved: usize) -> String {
    let mut out = format!(
        "Camel {version}
Windows {os}
Default output: {output}
Apps you moved to another channel: {moved}
Audio sessions: {}
",
        sessions.len()
    );
    for (name, channel, level, muted) in sessions {
        out.push_str(&format!(
            "  {name:<24} {:<8} {:>3.0}%{}{}
",
            NAMES[*channel % NAMES.len()],
            level * 100.0,
            if *muted { "  muted" } else { "" },
            if is_hidden(name) { "  (hidden)" } else { "" }
        ));
    }
    out
}

/// One strip per channel from the apps' (name, level, muted, peak).
pub fn summarize(apps: &[(&str, f32, bool, f32)], moved: &HashMap<String, usize>) -> [Strip; 5] {
    let mut strips: [Strip; 5] = Default::default();
    for (i, (name, ..)) in apps.iter().enumerate().filter(|(_, (name, ..))| !is_hidden(name)) {
        strips[channel_of(name, moved)].apps.push(i);
    }
    for strip in &mut strips {
        let members: Vec<_> = strip.apps.iter().map(|i| apps[*i]).collect();
        strip.level = if members.is_empty() { 1.0 } else { members.iter().map(|m| m.1).sum::<f32>() / members.len() as f32 };
        strip.muted = !members.is_empty() && members.iter().all(|m| m.2);
        strip.peak = members.iter().map(|m| m.3).fold(0.0, f32::max);
    }
    strips
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_apps_land_in_their_channel_and_the_rest_in_game() {
        for (app, channel) in
            [("Spotify", 1), ("chrome", 2), ("Discord", 3), ("System sounds", 0), ("ZenlessZoneZero", 4), ("RobloxPlayerBeta", 4), ("Unknown", 4)]
        {
            assert_eq!(default_channel(app), channel, "{app}");
        }
    }

    #[test]
    fn the_app_list_is_valid_data_and_hides_helpers() {
        let all: Vec<&String> = CATALOG.channels.iter().flatten().chain(CATALOG.hidden.iter()).collect();
        assert!(all.len() > 30, "the shipped list was not read");
        let mut sorted = all.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "an app is listed twice");
        assert!(all.iter().all(|a| **a == a.to_lowercase() && !a.ends_with(".exe")));
        assert!(is_hidden("Camel") && is_hidden("AudioDG") && !is_hidden("spotify"));
        let apps = [("camel", 1.0, false, 0.5), ("spotify", 1.0, false, 0.2)];
        let s = summarize(&apps, &HashMap::new());
        assert_eq!((s[1].apps.clone(), s[4].apps.len()), (vec![1], 0), "a hidden app is in no channel");
        let broken = parse_catalog("not json");
        assert!(broken.hidden.is_empty() && broken.channels.iter().all(Vec::is_empty), "bad data means no lists, not a crash");
    }

    #[test]
    fn the_report_lists_every_session() {
        let r = report("1.2.3", "11", "Speakers", &[("spotify", 1, 0.5, false), ("camel", 4, 1.0, true)], 2);
        assert!(r.starts_with(
            "Camel 1.2.3
Windows 11
Default output: Speakers
"
        ));
        assert!(r.contains("Audio sessions: 2") && r.contains("moved to another channel: 2"));
        assert!(r.contains("spotify") && r.contains("Music") && r.contains(" 50%"));
        assert!(r.contains("camel") && r.contains("muted") && r.contains("(hidden)"));
    }

    #[test]
    fn many_sessions_of_one_app_count_as_one() {
        let names = ["Chrome", "chrome", "Discord", "CHROME"];
        assert_eq!(one_per_app(&[0, 1, 2, 3], |i| names[i].to_owned()), [0, 2]);
        assert!(one_per_app(&[], |_| String::new()).is_empty());
    }

    #[test]
    fn a_moved_app_stays_where_the_user_put_it() {
        let moved = HashMap::from([("spotify".to_owned(), 4), ("odd".to_owned(), 99)]);
        assert_eq!(channel_of("Spotify", &moved), 4);
        assert_eq!(channel_of("odd", &moved), 4, "a channel that does not exist is ignored");
        assert_eq!(channel_of("chrome", &moved), 2);
        assert_eq!((next(0), next(4)), (1, 0));
    }

    #[test]
    fn a_strip_sums_up_its_apps() {
        let apps = [("spotify", 0.5, true, 0.2), ("vlc", 1.0, true, 0.6), ("chrome", 0.8, false, 0.1), ("game", 0.4, false, 0.9)];
        let s = summarize(&apps, &HashMap::new());
        assert_eq!(s[1].apps, [0, 1]);
        assert_eq!((s[1].level, s[1].muted, s[1].peak), (0.75, true, 0.6));
        assert!(!s[2].muted && s[2].peak == 0.1);
        assert_eq!((s[0].apps.len(), s[0].level, s[0].muted, s[0].peak), (0, 1.0, false, 0.0), "an empty channel is neither muted nor loud");
        assert_eq!(s[4].apps, [3]);
    }
}
