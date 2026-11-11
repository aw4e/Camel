//! Master + per-app volume through WASAPI. Must be used from a COM-initialised thread (the UI thread).

use std::ptr::null;
use sysinfo::{Pid, ProcessesToUpdate, System};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, eCapture, eConsole,
    eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::Interface;

pub struct AppVolume {
    pub name: String,
    pub level: f32,
    pub muted: bool,
    /// How loud the app is right now, 0..1; updated by `read_peaks`.
    pub peak: f32,
    ctl: ISimpleAudioVolume,
    meter: Option<IAudioMeterInformation>,
}

pub struct Mixer {
    device_id: String,
    endpoint: IAudioEndpointVolume,
    manager: IAudioSessionManager2,
    names: System,
    pub master: f32,
    pub apps: Vec<AppVolume>,
}

impl Mixer {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let endpoint = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)?;
            let manager = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)?;
            let device_id = device.GetId()?.to_string()?;
            let mut m = Self { device_id, endpoint, manager, names: System::new(), master: 0.0, apps: Vec::new() };
            m.refresh()?;
            Ok(m)
        }
    }

    /// The id of the output this mixer was made for; when the default output changes, the mixer has to be made again.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn refresh(&mut self) -> windows::core::Result<()> {
        unsafe {
            self.master = self.endpoint.GetMasterVolumeLevelScalar()?;
            let list = self.manager.GetSessionEnumerator()?;
            let mut apps = Vec::new();
            for i in 0..list.GetCount()? {
                let session = list.GetSession(i)?;
                if session.GetState()? != AudioSessionStateActive {
                    continue;
                }
                let ctl2: IAudioSessionControl2 = session.cast()?;
                // returns S_OK (0) for system sounds, S_FALSE (1) for others; both count as is_ok()
                let name = if ctl2.IsSystemSoundsSession().0 == 0 { "System sounds".to_owned() } else { self.process_name(ctl2.GetProcessId()?) };
                let ctl: ISimpleAudioVolume = session.cast()?;
                let (level, muted) = (ctl.GetMasterVolume()?, ctl.GetMute()?.as_bool());
                apps.push(AppVolume { name, level, muted, peak: 0.0, ctl, meter: session.cast().ok() });
            }
            self.apps = apps;
        }
        Ok(())
    }

    fn process_name(&mut self, pid: u32) -> String {
        let pid = Pid::from_u32(pid);
        self.names.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        self.names.process(pid).map_or_else(
            || "Unknown".to_owned(),
            |p| {
                let n = p.name().to_string_lossy().into_owned();
                n.strip_suffix(".exe").map_or(n.clone(), str::to_owned)
            },
        )
    }

    pub fn set_master(&mut self, v: f32) {
        self.master = v;
        let _ = unsafe { self.endpoint.SetMasterVolumeLevelScalar(v, null()) };
    }

    /// The current loudness of every app, a cheap read that can run every frame.
    pub fn read_peaks(&mut self) {
        for a in &mut self.apps {
            a.peak = a.meter.as_ref().and_then(|m| unsafe { m.GetPeakValue().ok() }).unwrap_or(0.0);
        }
    }

    pub fn set_app_muted(&mut self, i: usize, muted: bool) {
        if let Some(a) = self.apps.get_mut(i) {
            a.muted = muted;
            let _ = unsafe { a.ctl.SetMute(muted, null()) };
        }
    }

    pub fn set_app(&mut self, i: usize, v: f32) {
        if let Some(a) = self.apps.get_mut(i) {
            a.level = v;
            let _ = unsafe { a.ctl.SetMasterVolume(v, null()) };
        }
    }
}

/// Master volume and mute only: one COM call per read, so Camel can watch it all the time for the volume HUD.
pub struct Master(IAudioEndpointVolume);

impl Master {
    pub fn new() -> Option<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
            Some(Self(device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()?))
        }
    }

    /// (level 0..1, muted); None once the device is gone.
    pub fn read(&self) -> Option<(f32, bool)> {
        unsafe { Some((self.0.GetMasterVolumeLevelScalar().ok()?, self.0.GetMute().ok()?.as_bool())) }
    }
}

/// Flips mute on the default speakers (or microphone) and returns the new state; None when there is no such device.
pub fn toggle_mute(mic: bool) -> Option<bool> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(if mic { eCapture } else { eRender }, eConsole).ok()?;
        let volume = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()?;
        let muted = !volume.GetMute().ok()?.as_bool();
        volume.SetMute(muted, null()).ok()?;
        Some(muted)
    }
}

/// Whether the default speakers (or microphone) are muted; None when there is no such device.
pub fn is_muted(mic: bool) -> Option<bool> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(if mic { eCapture } else { eRender }, eConsole).ok()?;
        Some(device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()?.GetMute().ok()?.as_bool())
    }
}
