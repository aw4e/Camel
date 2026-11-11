//! Now-playing via Windows SMTC. Worker thread polls twice a second and executes commands.

use crate::lyrics::{self, Lyrics};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

pub struct Art {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}

#[derive(Default, Clone)]
pub struct State {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub art: Option<Arc<Art>>,
    /// seconds; dur == 0 means the player reports no timeline
    pub pos: f64,
    pub dur: f64,
    /// How many players Windows lists, and the name of the one shown.
    pub players: usize,
    pub source: String,
    /// When `pos` was read; the worker only polls twice a second.
    pub at: Option<Instant>,
    pub lyrics: Option<Arc<Lyrics>>,
}

impl State {
    /// Where the track is now: the last reading plus the time since, while it plays.
    pub fn position(&self) -> f64 {
        match (self.playing, self.at) {
            (true, Some(at)) if self.dur > 0.0 => (self.pos + at.elapsed().as_secs_f64()).min(self.dur),
            (true, Some(at)) => self.pos + at.elapsed().as_secs_f64(),
            _ => self.pos,
        }
    }

    /// Names the track for things remembered per track, such as a lyric shift.
    pub fn track_key(&self) -> String {
        format!(
            "{}
{}",
            self.artist, self.title
        )
    }

    /// The lyric line to show now, if the track plays and its lyrics are loaded. `shift` is how far the lyrics run ahead of
    /// the player; a line switches a quarter second early so it is read in time. None before the first line and on empty lines.
    pub fn lyric_line(&self, shift: f64) -> Option<&str> {
        let lines = self.lyrics.as_deref()?;
        if !self.playing {
            return None;
        }
        let pos = self.position() - shift + 0.25;
        let current = lines.partition_point(|(t, _)| *t <= pos).checked_sub(1)?;
        Some(lines[current].1.as_str()).filter(|t| !t.is_empty())
    }
}

pub struct LyricsCtx {
    pub enabled: AtomicBool,
    requested: Mutex<String>,
    slot: Mutex<Option<(String, Arc<Lyrics>)>>,
}

pub enum Cmd {
    Prev,
    Toggle,
    Next,
    Seek(f64),
    /// Show the next player when several are open.
    NextPlayer,
}

pub struct Handle {
    state: Arc<Mutex<State>>,
    tx: mpsc::Sender<Cmd>,
    pub lyrics: Arc<LyricsCtx>,
}

impl Handle {
    pub fn state(&self) -> State {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

pub fn spawn(lyrics_enabled: bool) -> Handle {
    let state = Arc::new(Mutex::new(State::default()));
    let (tx, rx) = mpsc::channel();
    let shared = state.clone();
    let lyr = Arc::new(LyricsCtx { enabled: AtomicBool::new(lyrics_enabled), requested: Mutex::new(String::new()), slot: Mutex::new(None) });
    let worker_lyr = lyr.clone();
    std::thread::spawn(move || {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED).ok() };
        let mut mgr = None;
        let mut preferred = None;
        loop {
            // A command wakes the worker at once; otherwise poll twice a second.
            let mut cmds = Vec::new();
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(c) => {
                    cmds.push(c);
                    cmds.extend(rx.try_iter());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            // errors (no session, manager gone) just mean "nothing playing"; drop the manager and retry
            if tick(&mut mgr, &shared, cmds, &worker_lyr, &mut preferred).is_err() {
                mgr = None;
            }
        }
    });
    Handle { state, tx, lyrics: lyr }
}

fn tick(mgr: &mut Option<Manager>, shared: &Mutex<State>, cmds: Vec<Cmd>, lyr: &Arc<LyricsCtx>, preferred: &mut Option<String>) -> windows::core::Result<()> {
    let mgr = match mgr {
        Some(m) => m,
        None => mgr.insert(Manager::RequestAsync()?.join()?),
    };
    let switch = cmds.iter().any(|c| matches!(c, Cmd::NextPlayer));
    let Some((session, players, source)) = pick_session(mgr, preferred, switch) else {
        *shared.lock().unwrap_or_else(PoisonError::into_inner) = State::default();
        return Ok(());
    };

    let mut seek_to = None;
    for c in cmds {
        match c {
            Cmd::Prev => session.TrySkipPreviousAsync()?.join()?,
            Cmd::Toggle => session.TryTogglePlayPauseAsync()?.join()?,
            Cmd::Next => session.TrySkipNextAsync()?.join()?,
            Cmd::NextPlayer => false,
            Cmd::Seek(secs) => {
                seek_to = Some(secs);
                // a player without a timeline may refuse; the estimate below still follows the request
                session.TryChangePlaybackPositionAsync((secs * 1e7) as i64).ok().and_then(|a| a.join().ok()).unwrap_or(false)
            }
        };
    }

    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let title = crate::widgets::clean_text(&props.Title()?.to_string());
    let artist = crate::widgets::clean_text(&props.Artist()?.to_string());
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;

    // Only re-fetch art when the track changes; reuse the old Arc otherwise.
    let prev = shared.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let same_track = prev.title == title && prev.artist == artist;
    let estimate = prev.position();
    let art = if same_track { prev.art } else { fetch_art(&props).ok().map(Arc::new) };
    // Some players (browsers, mostly) report no timeline at all. Then the position is counted here: from the start of
    // the track, or from the last line the user clicked, so lyrics can still follow along.
    let (pos, dur) = match timeline(&session, playing) {
        Ok((pos, dur)) if dur > 0.0 => (pos, dur),
        _ => (seek_to.unwrap_or(if same_track { estimate } else { 0.0 }), 0.0),
    };
    let lyrics = lyrics_for(lyr, &artist, &title, dur);
    *shared.lock().unwrap_or_else(PoisonError::into_inner) = State { title, artist, playing, art, pos, dur, players, source, at: Some(Instant::now()), lyrics };
    Ok(())
}

fn session_id(s: &Session) -> String {
    s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

/// The player to show: the one the user switched to while it is still open, otherwise the one Windows calls current.
/// Returns it with the number of open players and its display name.
fn pick_session(mgr: &Manager, preferred: &mut Option<String>, switch: bool) -> Option<(Session, usize, String)> {
    let sessions: Vec<Session> = mgr.GetSessions().ok()?.into_iter().collect();
    let mut ids: Vec<String> = sessions.iter().map(session_id).collect();
    ids.sort();
    let mut shown = preferred.clone().filter(|p| ids.contains(p)).or_else(|| mgr.GetCurrentSession().ok().map(|s| session_id(&s)))?;
    if switch && ids.len() > 1 {
        let at = ids.iter().position(|i| *i == shown).unwrap_or(0);
        shown = ids[(at + 1) % ids.len()].clone();
        *preferred = Some(shown.clone());
    }
    let session = sessions.into_iter().find(|s| session_id(s) == shown)?;
    Some((session, ids.len(), app_name(&shown)))
}

/// A readable name for an app id: "Spotify.exe" is "Spotify", a Store id keeps its last part, and the hashed id of a
/// browser is just "Browser".
pub fn app_name(id: &str) -> String {
    let id = id.rsplit('!').next().unwrap_or(id);
    let id = id.rsplit(['\\', '/']).next().unwrap_or(id);
    let id = id.strip_suffix(".exe").unwrap_or(id);
    let id = if id.contains('.') { id.rsplit('.').next().unwrap_or(id) } else { id };
    if id.is_empty() || (id.len() >= 12 && id.chars().all(|c| c.is_ascii_hexdigit())) { "Browser".to_owned() } else { id.to_owned() }
}

fn fetch_art(props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties) -> Result<Art, Box<dyn std::error::Error>> {
    let stream = props.Thumbnail()?.OpenReadAsync()?.join()?;
    let len = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(len)?.join()?;
    let mut buf = vec![0u8; len as usize];
    reader.ReadBytes(&mut buf)?;

    // Players hand over 16:9 thumbnails as often as square art; the UI draws a square, so cover-crop the centre
    // instead of stretching.
    let full = image::load_from_memory(&buf)?;
    let side = full.width().min(full.height());
    let img = full.crop_imm((full.width() - side) / 2, (full.height() - side) / 2, side, side).thumbnail(256, 256).to_rgba8();
    Ok(Art { size: [img.width() as usize, img.height() as usize], rgba: img.into_raw() })
}

/// Position is a snapshot taken at LastUpdatedTime, so add the time elapsed since while playing.
fn timeline(session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession, playing: bool) -> windows::core::Result<(f64, f64)> {
    let t = session.GetTimelineProperties()?;
    let secs = |ticks: i64| ticks as f64 / 1e7;
    let dur = secs(t.EndTime()?.Duration - t.StartTime()?.Duration);
    let mut pos = secs(t.Position()?.Duration - t.StartTime()?.Duration);
    if playing {
        // DateTime ticks are 100ns since 1601-01-01; Unix epoch is 11_644_473_600s later.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64();
        pos += (now + 11_644_473_600.0 - secs(t.LastUpdatedTime()?.UniversalTime)).max(0.0);
    }
    Ok((pos.clamp(0.0, dur.max(0.0)), dur.max(0.0)))
}

/// Kicks off one fetch per track and returns the lyrics once they belong to the current track. A fetch that failed
/// because the server could not be reached is tried again after a pause.
fn lyrics_for(lyr: &Arc<LyricsCtx>, artist: &str, title: &str, dur: f64) -> Option<Arc<Lyrics>> {
    if !lyr.enabled.load(Ordering::Relaxed) || title.trim().is_empty() {
        return None;
    }
    let key = format!(
        "{artist}
{title}"
    );
    {
        let mut req = lyr.requested.lock().unwrap_or_else(PoisonError::into_inner);
        if *req != key {
            req.clone_from(&key);
            let (l, a, t, k) = (lyr.clone(), artist.to_owned(), title.to_owned(), key.clone());
            std::thread::spawn(move || match lyrics::fetch(&a, &t, dur) {
                Ok(Some(v)) => *l.slot.lock().unwrap_or_else(PoisonError::into_inner) = Some((k, Arc::new(v))),
                Ok(None) => {}
                Err(()) => {
                    std::thread::sleep(Duration::from_secs(15));
                    let mut req = l.requested.lock().unwrap_or_else(PoisonError::into_inner);
                    if *req == k {
                        req.clear();
                    }
                }
            });
        }
    }
    lyr.slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref().filter(|(k, _)| *k == key).map(|(_, v)| v.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_are_readable() {
        assert_eq!(app_name("Spotify.exe"), "Spotify");
        assert_eq!(app_name("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic"), "ZuneMusic");
        assert_eq!(app_name("chrome.exe"), "chrome");
        assert_eq!(app_name("F0DC299D809B9700"), "Browser");
        assert_eq!(app_name(""), "Browser");
    }

    #[test]
    fn the_lyric_line_follows_the_position_and_the_shift() {
        let lyrics = Arc::new(vec![(1.0, "one".to_owned()), (5.0, String::new()), (9.0, "nine".to_owned())]);
        let mut s = State { playing: true, pos: 0.0, dur: 100.0, at: Some(Instant::now()), lyrics: Some(lyrics), ..State::default() };
        assert_eq!(s.lyric_line(0.0), None, "before the first line");
        s.pos = 2.0;
        assert_eq!(s.lyric_line(0.0), Some("one"));
        assert_eq!(s.lyric_line(-8.0), Some("nine"), "negative shift: the lyrics are late, so they are looked up further on");
        s.pos = 6.0;
        assert_eq!(s.lyric_line(0.0), None, "an empty line shows nothing");
        s.pos = 8.8;
        assert_eq!(s.lyric_line(0.0), Some("nine"), "a quarter second early");
        s.playing = false;
        assert_eq!(s.lyric_line(0.0), None, "paused");
        assert_eq!(
            s.track_key(),
            "
"
        );
    }

    #[test]
    fn position_keeps_moving_only_while_playing_and_stops_at_the_end() {
        let mut s = State { pos: 10.0, dur: 100.0, at: Some(Instant::now()), ..State::default() };
        assert_eq!(s.position(), 10.0, "paused: the last reading");
        s.playing = true;
        std::thread::sleep(Duration::from_millis(30));
        assert!(s.position() > 10.0 && s.position() < 11.0);
        s.pos = 99.99;
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(s.position(), 100.0, "never past the end");
        assert_eq!(State { playing: true, pos: 5.0, ..State::default() }.position(), 5.0, "no reading yet");
        let unknown_length = State { playing: true, pos: 5.0, dur: 0.0, at: Some(Instant::now()), ..State::default() };
        std::thread::sleep(Duration::from_millis(30));
        assert!(unknown_length.position() > 5.0, "a track without a length is counted, not frozen");
    }
}
