//! Plays everything the apps play on the default output through another output instead, driverless. Each playing app is
//! captured with the process loopback API (see `loopback`), the captures are mixed, and the mix is rendered on the chosen
//! output. The default output then only has to be a device nobody listens to, such as an unplugged second jack: apps play
//! there, Camel plays the same sound in your headphones, and this is the place where a channel gets its own processing.

use crate::loopback::{CHANNELS, ProcessCapture, RATE};
use crate::mixer::Mixer;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessesToUpdate, System};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient,
    IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
};
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::core::PCWSTR;

/// What the router is doing, for the UI.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Starting,
    /// Mixing this many apps.
    Running(usize),
    /// The chosen output is also the default one: the apps already play there, so nothing is routed.
    SameDevice,
    Failed(String),
}

/// Frames of sound the output holds back so a late wake-up does not make a gap.
const CUSHION_FRAMES: u32 = RATE / 1000 * 30;
/// More than this queued on the output and new sound is dropped, so the delay cannot grow.
const MAX_QUEUED_FRAMES: u32 = RATE / 1000 * 90;

pub struct Router {
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<Status>>,
    thread: Option<JoinHandle<()>>,
}

impl Router {
    /// Starts mixing to the output with id `target`.
    pub fn start(target: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(Status::Starting));
        let (s, st, id) = (stop.clone(), status.clone(), target.to_owned());
        let thread = std::thread::Builder::new().name("camel-router".into()).spawn(move || run(&id, &s, &st)).ok();
        Self { stop, status, thread }
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl Drop for Router {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// An output we render on.
struct Output {
    client: IAudioClient,
    render: IAudioRenderClient,
    frames: u32,
}

impl Output {
    fn open(id: &str) -> windows::core::Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
            let device = enumerator.GetDevice(PCWSTR(wide.as_ptr()))?;
            let client = device.Activate::<IAudioClient>(CLSCTX_ALL, None)?;
            let format = WAVEFORMATEX {
                wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
                nChannels: CHANNELS as u16,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * CHANNELS as u32 * 4,
                nBlockAlign: CHANNELS as u16 * 4,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                2_000_000, // 200 ms, in 100 ns units
                0,
                &format,
                None,
            )?;
            let frames = client.GetBufferSize()?;
            let render: IAudioRenderClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, render, frames })
        }
    }

    /// Frames waiting to be played.
    fn queued(&self) -> u32 {
        unsafe { self.client.GetCurrentPadding().unwrap_or(0) }
    }

    /// Queues as much of `samples` (interleaved stereo) as fits.
    fn write(&self, samples: &[f32]) {
        let free = self.frames.saturating_sub(self.queued()) as usize;
        let frames = (samples.len() / CHANNELS).min(free);
        if frames == 0 {
            return;
        }
        unsafe {
            if let Ok(buffer) = self.render.GetBuffer(frames as u32) {
                std::ptr::copy_nonoverlapping(samples.as_ptr(), buffer.cast::<f32>(), frames * CHANNELS);
                let _ = self.render.ReleaseBuffer(frames as u32, 0);
            }
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
    }
}

/// Adds `input` into `mix`, growing `mix` with silence when `input` is longer.
pub fn add_into(mix: &mut Vec<f32>, input: &[f32]) {
    if mix.len() < input.len() {
        mix.resize(input.len(), 0.0);
    }
    for (m, s) in mix.iter_mut().zip(input) {
        *m += s;
    }
}

/// Keeps the sum of several apps inside the range the output accepts. Quiet sound passes untouched; only what would
/// clip is bent, so one app alone is never changed.
pub fn limit(samples: &mut [f32]) {
    for s in samples {
        if s.abs() > 0.9 {
            // from 0.9 up, ease towards 1.0, kept just under it
            *s = s.signum() * (0.9 + 0.1 * ((s.abs() - 0.9) / 0.1).tanh()).min(0.999);
        }
    }
}

/// Of the processes that play, the ones to capture. A capture takes a process and all its children, so a process whose
/// parent plays as well is left out, or the same sound would be mixed twice. `procs` is (pid, parent pid).
pub fn capture_roots(procs: &[(u32, Option<u32>)]) -> Vec<u32> {
    let all: HashSet<u32> = procs.iter().map(|(p, _)| *p).collect();
    let mut roots: Vec<u32> = procs.iter().filter(|(_, parent)| !parent.is_some_and(|p| all.contains(&p))).map(|(p, _)| *p).collect();
    roots.sort_unstable();
    roots.dedup();
    roots
}

fn run(target: &str, stop: &AtomicBool, status: &Mutex<Status>) {
    let set = |s: Status| *status.lock().unwrap_or_else(PoisonError::into_inner) = s;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        timeBeginPeriod(1); // without it a 5 ms sleep takes up to 16 ms, and the sound breaks up
    }
    work(target, stop, &set);
    unsafe {
        timeEndPeriod(1);
        CoUninitialize();
    }
}

fn work(target: &str, stop: &AtomicBool, set: &dyn Fn(Status)) {
    let output = match Output::open(target) {
        Ok(o) => o,
        Err(e) => return set(Status::Failed(format!("Could not open the output: {e}"))),
    };
    let mut captures: HashMap<u32, ProcessCapture> = HashMap::new();
    let mut mixer: Option<Mixer> = None;
    let mut procs = System::new();
    let me = std::process::id();
    let mut last_scan: Option<Instant> = None;
    let (mut mix, mut part) = (Vec::<f32>::new(), Vec::<f32>::new());

    while !stop.load(Ordering::Relaxed) {
        // once a second: who is playing on the default output
        if last_scan.is_none_or(|t| t.elapsed() > Duration::from_secs(1)) {
            last_scan = Some(Instant::now());
            let default = crate::system::audio_output().map(|(id, _)| id);
            if default.as_deref() == Some(target) {
                captures.clear();
                set(Status::SameDevice);
            } else {
                if mixer.as_ref().is_some_and(|m| default.as_deref() != Some(m.device_id())) {
                    mixer = None;
                }
                if mixer.is_none() {
                    mixer = Mixer::new().ok();
                }
                let playing: Vec<u32> = mixer
                    .as_mut()
                    .map(|m| {
                        let _ = m.refresh();
                        m.apps.iter().map(|a| a.pid).filter(|p| *p != 0 && *p != me).collect()
                    })
                    .unwrap_or_default();
                let ids: Vec<Pid> = playing.iter().map(|p| Pid::from_u32(*p)).collect();
                procs.refresh_processes(ProcessesToUpdate::Some(&ids), true);
                let pairs: Vec<(u32, Option<u32>)> =
                    playing.iter().map(|p| (*p, procs.process(Pid::from_u32(*p)).and_then(|x| x.parent()).map(Pid::as_u32))).collect();
                let wanted = capture_roots(&pairs);
                captures.retain(|pid, _| wanted.contains(pid));
                for pid in wanted {
                    if let std::collections::hash_map::Entry::Vacant(slot) = captures.entry(pid)
                        && let Ok(c) = ProcessCapture::start(pid)
                    {
                        slot.insert(c);
                    }
                }
                set(Status::Running(captures.len()));
            }
        }

        mix.clear();
        let mut dead = Vec::new();
        for (pid, capture) in &mut captures {
            part.clear();
            if capture.read(&mut part).is_err() {
                dead.push(*pid); // the app closed or the device changed
            } else {
                add_into(&mut mix, &part);
            }
        }
        for pid in dead {
            captures.remove(&pid);
        }

        let queued = output.queued();
        if queued == 0 && !captures.is_empty() {
            output.write(&vec![0.0; CUSHION_FRAMES as usize * CHANNELS]); // after a gap, rebuild the cushion
        }
        if !mix.is_empty() && output.queued() < MAX_QUEUED_FRAMES {
            limit(&mut mix);
            output.write(&mix);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixing_adds_and_grows_to_the_longest() {
        let mut mix = vec![0.1, 0.2];
        add_into(&mut mix, &[0.1, 0.1, 0.5, 0.5]);
        assert_eq!(mix, [0.2, 0.3, 0.5, 0.5]);
        add_into(&mut mix, &[]);
        assert_eq!(mix.len(), 4);
    }

    #[test]
    fn the_limiter_leaves_normal_sound_alone_and_never_clips() {
        let mut quiet = [0.0, 0.5, -0.5, 0.9];
        limit(&mut quiet);
        assert_eq!(quiet, [0.0, 0.5, -0.5, 0.9], "one app at a normal level is not changed");
        let mut loud = [1.0, -1.7, 4.0, 0.95];
        limit(&mut loud);
        assert!(loud.iter().all(|s| s.abs() < 1.0), "{loud:?}");
        assert!(loud[0] > 0.9 && loud[1] < -0.9 && loud[3] > 0.9, "bent, not flattened");
        assert!(loud[2] > loud[0] && loud[3] < loud[0], "louder in, louder out");
    }

    #[test]
    fn a_child_of_a_playing_process_is_not_captured_twice() {
        // browser (10) with an audio helper (11) that plays too, a player (20) alone, and a helper whose parent does not play
        let procs = [(10, Some(1)), (11, Some(10)), (20, Some(1)), (30, Some(99)), (30, Some(99))];
        assert_eq!(capture_roots(&procs), [10, 20, 30]);
        assert!(capture_roots(&[]).is_empty());
        assert_eq!(capture_roots(&[(5, None)]), [5]);
    }

    /// Plays a quiet looped sound from a child PowerShell on the default output and routes it to the output named by
    /// the environment variable `CAMEL_ROUTE_TO` (part of its name, e.g. `Headphones`) for six seconds. Needs a default
    /// output other than that one, and you should hear the sound on the chosen output only.
    /// Run it by hand: `$env:CAMEL_ROUTE_TO='Headphones'; cargo test --bin camel route_a_sound -- --ignored --nocapture`
    #[test]
    #[ignore = "plays a quiet sound for a few seconds and needs a second output"]
    fn route_a_sound() {
        let want = std::env::var("CAMEL_ROUTE_TO").expect("set CAMEL_ROUTE_TO to part of an output's name").to_lowercase();
        unsafe { windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED).ok().unwrap() };
        let devices = crate::system::render_devices();
        println!("outputs: {devices:?}");
        let (id, name) = devices.iter().find(|(_, n)| n.to_lowercase().contains(&want)).expect("no such output");
        println!("routing to {name}");
        let sound = r"C:\Windows\Media\Alarm01.wav";
        let script = format!("$p = New-Object Media.SoundPlayer '{sound}'; $p.PlayLooping(); Start-Sleep 9");
        let mut child = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).spawn().unwrap();
        // quiet: turn the child's own session down as soon as it shows up
        let mut mixer = crate::mixer::Mixer::new().unwrap();
        let started = Instant::now();
        loop {
            mixer.refresh().unwrap();
            if let Some(i) = mixer.apps.iter().position(|a| a.name.eq_ignore_ascii_case("powershell")) {
                mixer.set_app(i, 0.05);
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "no audio session from the child");
            std::thread::sleep(Duration::from_millis(200));
        }
        // what the output itself reports as playing on it
        let meter = unsafe {
            let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
            let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
            e.GetDevice(PCWSTR(wide.as_ptr())).unwrap().Activate::<windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation>(CLSCTX_ALL, None).unwrap()
        };
        let before = unsafe { meter.GetPeakValue().unwrap() };
        let router = Router::start(id);
        let (mut seen, mut peak) = (Vec::new(), 0.0f32);
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(100));
            peak = peak.max(unsafe { meter.GetPeakValue().unwrap() });
            if seen.len() > 3 && peak > 0.0 {
                break;
            }
            let s = router.status();
            if seen.last() != Some(&s) {
                println!("{s:?}");
                seen.push(s);
            }
        }
        drop(router);
        let _ = child.kill();
        let _ = child.wait();
        println!("peak on the output: before {before:.4}, while routing {peak:.4}");
        assert!(seen.iter().any(|s| matches!(s, Status::Running(n) if *n > 0)), "no app was captured: {seen:?}");
        assert!(peak > 0.0, "the output played nothing");
    }
}
