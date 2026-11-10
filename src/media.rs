//! Now-playing via Windows SMTC. Worker thread polls twice a second and executes commands.

use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;
use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status};
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
    pub lyrics: Option<Arc<Lyrics>>,
}

/// (start seconds, text), sorted by time
pub type Lyrics = Vec<(f64, String)>;

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
            if tick(&mut mgr, &shared, cmds, &worker_lyr).is_err() {
                mgr = None;
            }
        }
    });
    Handle { state, tx, lyrics: lyr }
}

fn tick(mgr: &mut Option<Manager>, shared: &Mutex<State>, cmds: Vec<Cmd>, lyr: &Arc<LyricsCtx>) -> windows::core::Result<()> {
    let mgr = match mgr {
        Some(m) => m,
        None => mgr.insert(Manager::RequestAsync()?.join()?),
    };
    let Ok(session) = mgr.GetCurrentSession() else {
        *shared.lock().unwrap_or_else(PoisonError::into_inner) = State::default();
        return Ok(());
    };

    for c in cmds {
        match c {
            Cmd::Prev => session.TrySkipPreviousAsync()?.join()?,
            Cmd::Toggle => session.TryTogglePlayPauseAsync()?.join()?,
            Cmd::Next => session.TrySkipNextAsync()?.join()?,
            Cmd::Seek(secs) => session.TryChangePlaybackPositionAsync((secs * 1e7) as i64)?.join()?,
        };
    }

    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    let title = props.Title()?.to_string();
    let artist = props.Artist()?.to_string();
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;

    // Only re-fetch art when the track changes; reuse the old Arc otherwise.
    let prev = shared.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let art = if prev.title == title && prev.artist == artist { prev.art } else { fetch_art(&props).ok().map(Arc::new) };
    let (pos, dur) = timeline(&session, playing).unwrap_or((0.0, 0.0));
    let lyrics = lyrics_for(lyr, &artist, &title, dur);
    *shared.lock().unwrap_or_else(PoisonError::into_inner) = State { title, artist, playing, art, pos, dur, lyrics };
    Ok(())
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

/// Kicks off one fetch per track and returns the lyrics once they belong to the current track.
fn lyrics_for(lyr: &Arc<LyricsCtx>, artist: &str, title: &str, dur: f64) -> Option<Arc<Lyrics>> {
    if !lyr.enabled.load(Ordering::Relaxed) || dur <= 0.0 {
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
            std::thread::spawn(move || {
                if let Some(v) = fetch_lyrics(&a, &t, dur) {
                    *l.slot.lock().unwrap_or_else(PoisonError::into_inner) = Some((k, Arc::new(v)));
                }
            });
        }
    }
    lyr.slot.lock().unwrap_or_else(PoisonError::into_inner).as_ref().filter(|(k, _)| *k == key).map(|(_, v)| v.clone())
}

#[derive(Deserialize)]
struct LrcResp {
    #[serde(rename = "syncedLyrics")]
    synced: Option<String>,
}

fn fetch_lyrics(artist: &str, title: &str, dur: f64) -> Option<Lyrics> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(8))).build().into();
    let mut resp = agent
        .get("https://lrclib.net/api/get")
        .query("artist_name", artist)
        .query("track_name", title)
        .query("duration", (dur.round() as u64).to_string())
        .call()
        .ok()?;
    parse_lrc(&resp.body_mut().read_json::<LrcResp>().ok()?.synced?)
}

/// "[01:23.45] text" lines, possibly with several timestamps per line.
fn parse_lrc(src: &str) -> Option<Lyrics> {
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
    use super::parse_lrc;

    #[test]
    fn parses_lrc() {
        let l = parse_lrc(
            "[00:01.50] hi
[00:03.00][00:05.25] again
[ar:x]",
        )
        .unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!(l[0], (1.5, "hi".to_owned()));
        assert_eq!(l[2], (5.25, "again".to_owned()));
    }
}
