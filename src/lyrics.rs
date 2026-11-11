//! Synced lyrics from lrclib.net. Players report messy names (a YouTube title is "Artist - Song (Official Video)", its
//! artist the channel, "Artist - Topic" or "ArtistVEVO"), so the names are cleaned and several guesses are tried: an
//! exact lookup first, then a search whose results are checked against the title and the length of the track.

use serde::Deserialize;
use std::time::Duration;

/// (start seconds, text), sorted by time
pub type Lyrics = Vec<(f64, String)>;

const NOISE: [&str; 17] = [
    "official",
    "video",
    "audio",
    "lyric",
    "lyrics",
    "visualizer",
    "visualiser",
    "remastered",
    "remaster",
    "hd",
    "hq",
    "4k",
    "mv",
    "clip",
    "explicit",
    "music",
    "topic",
];

/// How far a result's length may differ from the playing track and still count as the same recording.
const DURATION_SLACK: f64 = 3.0;

fn words(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase)
}

/// Lowercase letters and digits only, for comparing two names.
fn key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Removes "(...)" and "[...]" groups that only say what kind of upload it is, such as "(Official Video)".
fn strip_noise_groups(s: &str) -> String {
    let (mut out, mut group, mut close) = (String::new(), String::new(), None);
    for c in s.chars() {
        match (close, c) {
            (None, '(') => (close, group) = (Some(')'), String::new()),
            (None, '[') => (close, group) = (Some(']'), String::new()),
            (Some(end), c) if c == end => {
                let open = if end == ')' { '(' } else { '[' };
                if !words(&group).any(|w| NOISE.contains(&w.as_str())) {
                    out.push(open);
                    out.push_str(&group);
                    out.push(end);
                }
                close = None;
            }
            (Some(_), c) => group.push(c),
            (None, c) => out.push(c),
        }
    }
    if let Some(end) = close {
        out.push(if end == ')' { '(' } else { '[' });
        out.push_str(&group);
    }
    out
}

/// Cuts at the first " feat. ", " ft. " or " featuring " (also inside an open bracket) and at " | ".
fn strip_trailers(s: &str) -> String {
    let lower = s.to_lowercase();
    let cut = [" feat.", " feat ", " ft.", " ft ", " featuring ", "(feat", "(ft", "[feat", "[ft", " | "]
        .iter()
        .filter_map(|m| lower.find(m))
        .min()
        .unwrap_or(s.len());
    s[..cut].trim().trim_end_matches(['(', '[']).trim().to_owned()
}

pub fn clean_title(title: &str) -> String {
    strip_trailers(&strip_noise_groups(title)).trim().to_owned()
}

/// The first artist, without the suffixes YouTube channels carry.
pub fn clean_artist(artist: &str) -> String {
    let mut a = artist.trim().to_owned();
    for suffix in [" - Topic", "VEVO", "Vevo", " Official", " Music", " Records"] {
        if let Some(rest) = a.strip_suffix(suffix) {
            a = rest.trim().to_owned();
        }
    }
    let lower = a.to_lowercase();
    let cut = [",", ";", " & ", " feat.", " ft.", " featuring ", " x ", " × ", " and "].iter().filter_map(|m| lower.find(m)).min().unwrap_or(a.len());
    a[..cut].trim().to_owned()
}

/// Name pairs worth asking about, best guess first: what the player says, then "Artist - Song" read out of the title.
pub fn guesses(artist: &str, title: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |a: String, t: String| {
        if !t.is_empty() && !out.iter().any(|(x, y)| key(x) == key(&a) && key(y) == key(&t)) {
            out.push((a, t));
        }
    };
    let title = strip_noise_groups(title);
    if let Some((left, right)) = title.split_once(" - ") {
        let (left, right) = (clean_artist(left), clean_title(right));
        add(left.clone(), right.clone());
        let player = clean_artist(artist);
        if key(&player).contains(&key(&left)) || key(&left).contains(&key(&player)) {
            add(player, right);
        }
    }
    add(clean_artist(artist), clean_title(&title));
    out
}

#[derive(Deserialize, Default)]
struct Entry {
    #[serde(rename = "trackName", default)]
    track: String,
    #[serde(default)]
    duration: f64,
    #[serde(rename = "syncedLyrics", default)]
    synced: Option<String>,
}

/// The best entry for `title` and `dur` (0 when unknown): it has synced lyrics, a matching title and a length within a
/// few seconds, the closest one wins.
fn best(entries: Vec<Entry>, title: &str, dur: f64) -> Option<String> {
    let wanted = key(title);
    entries
        .into_iter()
        .filter(|e| e.synced.as_deref().is_some_and(|s| !s.trim().is_empty()))
        .filter(|e| {
            let got = key(&clean_title(&e.track));
            !wanted.is_empty() && (got == wanted || (wanted.len() >= 6 && (got.contains(&wanted) || wanted.contains(&got) && got.len() >= 6)))
        })
        .map(|e| ((if dur > 0.0 { (e.duration - dur).abs() } else { 0.0 }), e))
        .filter(|(diff, _)| *diff <= DURATION_SLACK)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .and_then(|(_, e)| e.synced)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(8))).build().into()
}

fn user_agent() -> String {
    format!("Camel/{} (https://github.com/aw4e/Camel)", crate::update::VERSION)
}

/// Ok(None) when the server answered and has nothing; Err when it could not be reached, so the caller may retry.
fn get<T: for<'de> Deserialize<'de>>(agent: &ureq::Agent, path: &str, query: &[(&str, String)]) -> Result<Option<T>, ()> {
    let mut req = agent.get(format!("https://lrclib.net/api/{path}")).header("User-Agent", user_agent());
    for (k, v) in query {
        req = req.query(k, v);
    }
    match req.call() {
        Ok(mut resp) => Ok(resp.body_mut().read_json::<T>().ok()),
        Err(ureq::Error::StatusCode(code)) if (400..500).contains(&code) => Ok(None),
        Err(_) => Err(()),
    }
}

/// Looks the track up under every guess. `dur` is 0 when the player reports no timeline.
pub fn fetch(artist: &str, title: &str, dur: f64) -> Result<Option<Lyrics>, ()> {
    let agent = agent();
    let guesses = guesses(artist, title);
    let mut tried = 0;
    for (a, t) in &guesses {
        let mut q = vec![("artist_name", a.clone()), ("track_name", t.clone())];
        if dur > 0.0 {
            q.push(("duration", (dur.round() as u64).to_string()));
        }
        tried += 1;
        if let Some(e) = get::<Entry>(&agent, "get", &q)?
            && let Some(l) = e.synced.as_deref().and_then(parse_lrc)
        {
            return Ok(Some(l));
        }
    }
    for (a, t) in guesses.iter().take(2) {
        for q in [vec![("track_name", t.clone()), ("artist_name", a.clone())], vec![("q", format!("{a} {t}"))]] {
            tried += 1;
            if tried > 8 {
                return Ok(None);
            }
            if let Some(entries) = get::<Vec<Entry>>(&agent, "search", &q)?
                && let Some(l) = best(entries, t, dur).as_deref().and_then(parse_lrc)
            {
                return Ok(Some(l));
            }
        }
    }
    Ok(None)
}

/// "[01:23.45] text" lines, possibly with several timestamps per line.
pub fn parse_lrc(src: &str) -> Option<Lyrics> {
    let mut out = Lyrics::new();
    for line in src.lines() {
        let mut rest = line.trim();
        let mut times = Vec::new();
        while let Some(tail) = rest.strip_prefix('[') {
            let Some((stamp, after)) = tail.split_once(']') else { break };
            let Some((m, s)) = stamp.split_once(':') else { break };
            let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) else { break };
            times.push(m * 60.0 + s);
            rest = after;
        }
        out.extend(times.into_iter().map(|t| (t, rest.trim().to_owned())));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lrc() {
        let l = parse_lrc("[00:01.50] hi\n[00:03.00][00:05.25] again\n[ar:x]").unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!(l[0], (1.5, "hi".to_owned()));
        assert_eq!(l[2], (5.25, "again".to_owned()));
    }

    #[test]
    fn cleans_youtube_artists() {
        assert_eq!(clean_artist("The Weeknd - Topic"), "The Weeknd");
        assert_eq!(clean_artist("AdeleVEVO"), "Adele");
        assert_eq!(clean_artist("Daft Punk, Pharrell Williams"), "Daft Punk");
        assert_eq!(clean_artist("Calvin Harris feat. Rihanna"), "Calvin Harris");
        assert_eq!(clean_artist("Simon & Garfunkel"), "Simon");
        assert_eq!(clean_artist("Coldplay"), "Coldplay");
    }

    #[test]
    fn cleans_upload_noise_from_titles_but_keeps_real_parts() {
        assert_eq!(clean_title("Blinding Lights (Official Video)"), "Blinding Lights");
        assert_eq!(clean_title("Blinding Lights [Official Audio] | The Weeknd"), "Blinding Lights");
        assert_eq!(clean_title("Song (Lyrics) [HD]"), "Song");
        assert_eq!(clean_title("Get Lucky (feat. Pharrell Williams)"), "Get Lucky");
        assert_eq!(clean_title("Hello (Acoustic)"), "Hello (Acoustic)");
        assert_eq!(clean_title("Plain Title"), "Plain Title");
        assert_eq!(clean_title("Broken (Official"), "Broken (Official");
    }

    #[test]
    fn reads_artist_and_song_out_of_a_youtube_title() {
        let g = guesses("TheWeekndVEVO", "The Weeknd - Blinding Lights (Official Video)");
        assert!(g.contains(&("The Weeknd".to_owned(), "Blinding Lights".to_owned())), "{g:?}");
        let g = guesses("The Weeknd", "Blinding Lights");
        assert_eq!(g, [("The Weeknd".to_owned(), "Blinding Lights".to_owned())]);
        let g = guesses("Adele", "Adele - Hello");
        assert_eq!(g[0], ("Adele".to_owned(), "Hello".to_owned()));
        assert_eq!(g.len(), 2, "the same pair is not listed twice; the whole title stays as the last resort");
        assert!(guesses("x", "").is_empty());
    }

    fn entry(track: &str, duration: f64, lrc: Option<&str>) -> Entry {
        Entry { track: track.to_owned(), duration, synced: lrc.map(str::to_owned) }
    }

    #[test]
    fn search_results_must_match_the_title_and_the_length() {
        let lrc = Some("[00:01.00] a");
        let pick = |entries: Vec<Entry>, dur: f64| best(entries, "Blinding Lights", dur);
        // the closest length wins
        let r = pick(vec![entry("Blinding Lights", 263.0, Some("[00:09.00] far")), entry("Blinding Lights", 201.0, Some("[00:01.00] near"))], 200.0);
        assert_eq!(r.as_deref(), Some("[00:01.00] near"));
        // a different song, a very different length, or no synced lyrics never count
        assert!(pick(vec![entry("Save Your Tears", 200.0, lrc)], 200.0).is_none());
        assert!(pick(vec![entry("Blinding Lights", 263.0, lrc)], 200.0).is_none());
        assert!(pick(vec![entry("Blinding Lights", 200.0, None), entry("Blinding Lights", 200.0, Some("  "))], 200.0).is_none());
        // an upload title with noise still matches
        assert!(pick(vec![entry("Blinding Lights (Official Video)", 200.0, lrc)], 200.0).is_some());
        // without a known length any length is fine
        assert!(pick(vec![entry("Blinding Lights", 263.0, lrc)], 0.0).is_some());
    }

    /// Asks the real server with the kind of names YouTube, YouTube Music and Spotify report.
    /// Run it by hand: `cargo test --bin camel finds_lyrics_online -- --ignored --nocapture`
    #[test]
    #[ignore = "needs the network"]
    fn finds_lyrics_online() {
        let cases = [
            ("The Weeknd", "Blinding Lights", 200.0),
            ("TheWeekndVEVO", "The Weeknd - Blinding Lights (Official Video)", 262.0),
            ("The Weeknd - Topic", "Blinding Lights", 200.0),
            ("Coldplay", "Coldplay - Yellow (Official Video)", 0.0),
            ("Adele", "Hello", 295.0),
            ("Sal Priadi", "Sal Priadi - Ada titik-titik di ujung doa (Official Music Video)", 0.0),
        ];
        for (artist, title, dur) in cases {
            let found = fetch(artist, title, dur);
            println!("{artist} / {title} / {dur}: {:?}", found.as_ref().map(|r| r.as_ref().map(|l| l.len())));
            assert!(matches!(found, Ok(Some(_))), "{artist} / {title}");
        }
    }
}
