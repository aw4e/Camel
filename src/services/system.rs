//! Small questions to Windows: clock format, animation setting, keyboard state, connection and audio output.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Globalization::{GetLocaleInfoEx, GetLocaleInfoW, LOCALE_ITIME, LOCALE_SISO639LANGNAME};
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::Networking::NetworkListManager::{INetworkListManager, NetworkListManager};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, STGM_READ};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};
use windows::core::w;

/// True when Windows shows the time on a 24-hour clock.
pub fn use_24h() -> bool {
    let mut buf = [0u16; 4];
    let n = unsafe { GetLocaleInfoEx(w!(""), LOCALE_ITIME, Some(&mut buf)) };
    n <= 0 || buf[0] != u16::from(b'0')
}

/// "Show animations in Windows" is off.
pub fn reduced_motion() -> bool {
    let mut on = windows::core::BOOL(1);
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some((&raw mut on).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    ok.is_ok() && !on.as_bool()
}

pub fn caps_lock() -> bool {
    unsafe { GetKeyState(i32::from(VK_CAPITAL.0)) & 1 != 0 }
}

/// Two-letter code of the keyboard layout of the window you are typing in, "EN" or "ID".
pub fn keyboard_language() -> Option<String> {
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let layout = GetKeyboardLayout(thread).0 as usize;
        let mut buf = [0u16; 9];
        let n = GetLocaleInfoW((layout & 0xFFFF) as u32, LOCALE_SISO639LANGNAME, Some(&mut buf));
        (n > 1).then(|| String::from_utf16_lossy(&buf[..(n - 1) as usize]).to_uppercase())
    }
}

/// Asks Windows whether the PC is online. Keep one and call `online` on it.
pub struct Network(INetworkListManager);

impl Network {
    pub fn new() -> Option<Self> {
        unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL).ok().map(Self) }
    }

    pub fn online(&self) -> Option<bool> {
        unsafe { self.0.IsConnectedToInternet().ok().map(|b| b.as_bool()) }
    }
}

/// Id and friendly name of the default speakers or headphones.
pub fn audio_output() -> Option<(String, String)> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let id = device.GetId().ok()?.to_string().ok()?;
        let name = device.OpenPropertyStore(STGM_READ).ok().and_then(|s| s.GetValue(&PKEY_Device_FriendlyName).ok()).map(|v| v.to_string()).unwrap_or_default();
        Some((id, name))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_system_answers() {
        let _ = super::use_24h();
        let _ = super::reduced_motion();
        let _ = super::caps_lock();
        assert!(super::keyboard_language().is_none_or(|l| l.len() == 2));
        assert!(super::audio_output().is_none_or(|(id, _)| !id.is_empty()));
    }
}
