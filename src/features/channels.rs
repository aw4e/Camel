//! The mixer's channels. Apps that play audio are grouped by what they are for, the way Elgato Wave Link groups them: a
//! fader per channel sets the volume of every app in it. The grouping starts from a list of well-known apps and anything
//! the user moved is remembered by app name. Pure logic; the audio calls live in `services::mixer`.

use std::collections::HashMap;

pub const NAMES: [&str; 5] = ["System", "Music", "Browser", "Voice", "Game"];

/// Process names (lowercase, without .exe) that belong in a channel. Anything not listed lands in Game.
const KNOWN: [(usize, &[&str]); 3] = [
    (1, &["spotify", "vlc", "musicbee", "foobar2000", "itunes", "applemusic", "wmplayer", "tidal", "deezer", "aimp", "winamp", "potplayer", "mpc-hc64", "mpv"]),
    (2, &["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "zen", "iexplore", "waterfox", "librewolf", "chromium"]),
    (
        3,
        &[
            "discord",
            "discordptb",
            "discordcanary",
            "teams",
            "ms-teams",
            "zoom",
            "skype",
            "telegram",
            "slack",
            "whatsapp",
            "webex",
            "signal",
            "mumble",
            "teamspeak",
        ],
    ),
];

pub fn default_channel(app: &str) -> usize {
    let name = app.to_lowercase();
    if name == "system sounds" {
        return 0;
    }
    KNOWN.iter().find(|(_, apps)| apps.contains(&name.as_str())).map_or(4, |(channel, _)| *channel)
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

/// One strip per channel from the apps' (name, level, muted, peak).
pub fn summarize(apps: &[(&str, f32, bool, f32)], moved: &HashMap<String, usize>) -> [Strip; 5] {
    let mut strips: [Strip; 5] = Default::default();
    for (i, (name, ..)) in apps.iter().enumerate() {
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
