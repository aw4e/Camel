//! Wi-Fi and Bluetooth on or off, through the Windows radio API. The calls are asynchronous and can take a moment, so a
//! worker thread does them and the notch only reads the last known state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::Duration;
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    WiFi,
    Bluetooth,
}

impl Kind {
    fn radio_kind(self) -> RadioKind {
        match self {
            Kind::WiFi => RadioKind::WiFi,
            Kind::Bluetooth => RadioKind::Bluetooth,
        }
    }
}

/// Last known state: None when the PC has no such radio or Windows refused access to it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

pub struct Radios {
    shared: Arc<Mutex<Snapshot>>,
    tx: mpsc::Sender<(Kind, bool)>,
    /// The worker only polls while the controls page is showing.
    watching: Arc<AtomicBool>,
}

impl Radios {
    pub fn spawn() -> Self {
        let shared = Arc::new(Mutex::new(Snapshot::default()));
        let watching = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<(Kind, bool)>();
        let (state, watch) = (shared.clone(), watching.clone());
        std::thread::spawn(move || {
            unsafe { RoInitialize(RO_INIT_MULTITHREADED).ok() };
            loop {
                match rx.recv_timeout(Duration::from_secs(2)) {
                    Ok((kind, on)) => {
                        let _ = set(kind, on);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) if !watch.load(Ordering::Relaxed) => continue,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                *state.lock().unwrap_or_else(PoisonError::into_inner) = read();
            }
        });
        Self { shared, tx, watching }
    }

    /// Call while the controls page is showing so the state stays fresh.
    pub fn watch(&self) {
        self.watching.store(true, Ordering::Relaxed);
    }

    pub fn unwatch(&self) {
        self.watching.store(false, Ordering::Relaxed);
    }

    pub fn state(&self) -> Snapshot {
        *self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(&self, kind: Kind, on: bool) {
        let _ = self.tx.send((kind, on));
        // show the new state at once; the worker corrects it if Windows disagrees
        let mut s = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        match kind {
            Kind::WiFi => s.wifi = s.wifi.map(|_| on),
            Kind::Bluetooth => s.bluetooth = s.bluetooth.map(|_| on),
        }
    }
}

fn find(kind: Kind) -> Option<Radio> {
    Radio::GetRadiosAsync().ok()?.join().ok()?.into_iter().find(|r| r.Kind().is_ok_and(|k| k == kind.radio_kind()))
}

fn read() -> Snapshot {
    let on =
        |kind| find(kind).and_then(|r| r.State().ok()).and_then(|s| (s != RadioState::Unknown && s != RadioState::Disabled).then_some(s == RadioState::On));
    Snapshot { wifi: on(Kind::WiFi), bluetooth: on(Kind::Bluetooth) }
}

fn set(kind: Kind, on: bool) -> Option<()> {
    if Radio::RequestAccessAsync().ok()?.join().ok()? != RadioAccessStatus::Allowed {
        return None;
    }
    let radio = find(kind)?;
    let status = radio.SetStateAsync(if on { RadioState::On } else { RadioState::Off }).ok()?.join().ok()?;
    (status == RadioAccessStatus::Allowed).then_some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the real radios; never switches one, which could cut a connection the test run depends on.
    #[test]
    fn reads_the_radios_without_touching_them() {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED).ok() };
        let s = read();
        println!("{s:?}");
    }
}
