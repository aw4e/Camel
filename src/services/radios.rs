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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    /// Connected Bluetooth devices that report a battery: (name, percent).
    pub batteries: Vec<(String, u8)>,
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
        self.shared.lock().unwrap_or_else(PoisonError::into_inner).clone()
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
    Snapshot { wifi: on(Kind::WiFi), bluetooth: on(Kind::Bluetooth), batteries: bluetooth_batteries() }
}

/// Windows keeps one battery level per Bluetooth device (`DEVPKEY` {104EA319-…} 2, written by the hands-free profile) on a
/// child node of the device, so nodes are grouped by the device address in their instance id. It reports no left/right
/// split or charging case: that is vendor data Windows does not expose.
fn bluetooth_batteries() -> Vec<(String, u8)> {
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_GETIDLIST_FILTER_ENUMERATOR, CM_Get_DevNode_PropertyW, CM_Get_Device_ID_List_SizeW, CM_Get_Device_ID_ListW, CM_LOCATE_DEVNODE_NORMAL,
        CM_Locate_DevNodeW,
    };
    use windows::Win32::Devices::Properties::{DEVPKEY_Device_FriendlyName, DEVPROPTYPE};
    use windows::Win32::Foundation::DEVPROPKEY;
    use windows::core::{GUID, PCWSTR, w};

    const BATTERY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104EA319_6EE2_4701_BD47_8DDBF425BBE5), pid: 2 };
    const CONNECTED: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x83DA6326_97A6_4088_9453_A1923F573B29), pid: 15 };
    unsafe {
        let mut len = 0u32;
        if CM_Get_Device_ID_List_SizeW(&mut len, w!("BTHENUM"), CM_GETIDLIST_FILTER_ENUMERATOR).0 != 0 {
            return Vec::new();
        }
        let mut buf = vec![0u16; len as usize];
        if CM_Get_Device_ID_ListW(w!("BTHENUM"), &mut buf, CM_GETIDLIST_FILTER_ENUMERATOR).0 != 0 {
            return Vec::new();
        }
        let prop = |inst: &[u16], key: &DEVPROPKEY, out: &mut [u8]| -> Option<(DEVPROPTYPE, usize)> {
            let mut dev = 0u32;
            if CM_Locate_DevNodeW(&mut dev, PCWSTR(inst.as_ptr()), CM_LOCATE_DEVNODE_NORMAL).0 != 0 {
                return None;
            }
            let (mut ty, mut size) = (DEVPROPTYPE::default(), out.len() as u32);
            (CM_Get_DevNode_PropertyW(dev, key, &mut ty, Some(out.as_mut_ptr()), &mut size, 0).0 == 0).then_some((ty, size as usize))
        };
        // (address, name, connected, battery) per node; the main node has the name, a profile node has the battery
        let (mut names, mut levels) = (Vec::<(String, String, bool)>::new(), Vec::<(String, u8)>::new());
        for id in buf.split(|c| *c == 0).filter(|s| !s.is_empty()) {
            let text = String::from_utf16_lossy(id);
            let mut z = id.to_vec();
            z.push(0);
            let mut b = [0u8; 1];
            if prop(&z, &BATTERY, &mut b).is_some() {
                // profile nodes end in "<12 hex address>_C00000000"
                if let Some(addr) = text.rsplit_once('_').and_then(|(head, _)| head.get(head.len().saturating_sub(12)..)) {
                    levels.push((addr.to_uppercase(), b[0].min(100)));
                }
            }
            if let Some(addr) = text.to_uppercase().split("\\DEV_").nth(1).and_then(|r| r.get(..12)).map(str::to_owned) {
                let mut raw = [0u8; 512];
                let name = prop(&z, &DEVPKEY_Device_FriendlyName, &mut raw)
                    .map(|(_, n)| {
                        String::from_utf16_lossy(&raw[..n].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
                            .trim_end_matches('\0')
                            .to_owned()
                    })
                    .unwrap_or_default();
                let mut on = [0u8; 1];
                let connected = prop(&z, &CONNECTED, &mut on).is_some() && on[0] != 0;
                names.push((addr, name, connected));
            }
        }
        names.into_iter().filter(|(_, n, c)| *c && !n.is_empty()).filter_map(|(a, n, _)| levels.iter().find(|(l, _)| *l == a).map(|(_, p)| (n, *p))).collect()
    }
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
