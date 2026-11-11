//! Captures the sound of one app (and its child processes) with the documented Windows "process loopback" API, no driver
//! needed. This is the first block of a driverless mixer: capture each channel's apps, mix them, play the mix elsewhere.
//! Format of what comes out: interleaved stereo f32 at 48 kHz.

use std::mem::ManuallyDrop;
use std::sync::{Arc, Condvar, Mutex};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
    IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::BLOB;
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{Interface, Ref, implement};

pub const RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Handler(Arc<(Mutex<bool>, Condvar)>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Handler_Impl {
    fn ActivateCompleted(&self, _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        let (done, signal) = &*self.0;
        *done.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        signal.notify_all();
        Ok(())
    }
}

pub struct ProcessCapture {
    client: IAudioClient,
    capture: IAudioCaptureClient,
}

impl ProcessCapture {
    /// Starts capturing the sound `pid` and its child processes play. Needs a COM-initialised thread.
    pub fn start(pid: u32) -> windows::core::Result<Self> {
        let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: pid,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        // PROPVARIANT's destructor would free the blob, which lives on this stack, so it must never run
        let blob = ManuallyDrop::new(PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                    vt: VT_BLOB,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: PROPVARIANT_0_0_0 {
                        blob: BLOB { cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32, pBlobData: (&raw mut params).cast() },
                    },
                }),
            },
        });
        let done = Arc::new((Mutex::new(false), Condvar::new()));
        let handler: IActivateAudioInterfaceCompletionHandler = Handler(done.clone()).into();
        let operation = unsafe { ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IAudioClient::IID, Some(&*blob), &handler)? };
        {
            let (flag, signal) = &*done;
            let mut finished = flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*finished {
                finished = signal.wait(finished).unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        let mut result = windows::core::HRESULT(0);
        let mut unknown = None;
        unsafe { operation.GetActivateResult(&mut result, &mut unknown)? };
        result.ok()?;
        let client: IAudioClient = unknown.ok_or_else(windows::core::Error::empty)?.cast()?;

        // process loopback has no mix format of its own, so the format is asked for and Windows converts to it
        let format = WAVEFORMATEX {
            wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
            nChannels: CHANNELS as u16,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * CHANNELS as u32 * 4,
            nBlockAlign: CHANNELS as u16 * 4,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                1_000_000, // 100 ms, in 100 ns units
                0,
                &format,
                None,
            )?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Self { client, capture })
        }
    }

    /// Appends what has been captured since the last call to `out` and returns how many frames that was.
    pub fn read(&mut self, out: &mut Vec<f32>) -> windows::core::Result<usize> {
        let mut frames_total = 0;
        unsafe {
            while self.capture.GetNextPacketSize()? > 0 {
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let samples = frames as usize * CHANNELS;
                // flag 0x2 is AUDCLNT_BUFFERFLAGS_SILENT: the buffer holds no usable data
                if flags & 0x2 != 0 || data.is_null() {
                    out.extend(std::iter::repeat_n(0.0, samples));
                } else {
                    out.extend_from_slice(std::slice::from_raw_parts(data.cast::<f32>(), samples));
                }
                self.capture.ReleaseBuffer(frames)?;
                frames_total += frames as usize;
            }
        }
        Ok(frames_total)
    }
}

impl Drop for ProcessCapture {
    fn drop(&mut self) {
        let _ = unsafe { self.client.Stop() };
    }
}

/// Root mean square of interleaved samples, 0 for silence.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_silence_and_of_a_square_wave() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0; 8]), 0.0);
        assert!((rms(&[0.5, -0.5, 0.5, -0.5]) - 0.5).abs() < 1e-6);
    }

    /// Does muting an app's audio session also silence what process loopback captures of it? The answer decides how a
    /// driverless mixer can keep the original sound from playing twice. Plays a quiet looped sound from a child
    /// PowerShell for about five seconds and changes only that child's own session volume and mute.
    /// Run it by hand: `cargo test --bin camel session_mute_and_process_loopback -- --ignored --nocapture`
    #[test]
    #[ignore = "plays a quiet sound for a few seconds on the default output"]
    fn session_mute_and_process_loopback() {
        use crate::services::mixer::Mixer;
        use std::time::{Duration, Instant};
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};

        let sound = r"C:\Windows\Media\Alarm01.wav";
        assert!(std::path::Path::new(sound).exists(), "needs {sound}");
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap() };
        let script = format!("$p = New-Object Media.SoundPlayer '{sound}'; $p.PlayLooping(); Start-Sleep 25");
        let mut child = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).spawn().unwrap();
        let mut mixer = Mixer::new().unwrap();
        // wait until the child's audio session shows up, and turn it down to a whisper
        let started = Instant::now();
        let slot = loop {
            mixer.refresh().unwrap();
            if let Some(i) = mixer.apps.iter().position(|a| a.name.eq_ignore_ascii_case("powershell")) {
                break i;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "no audio session from the child");
            std::thread::sleep(Duration::from_millis(200));
        };
        mixer.set_app(slot, 0.05);

        let mut cap = ProcessCapture::start(child.id()).expect("process loopback");
        let mut measure = |label: &str| {
            let mut samples = Vec::new();
            let t = Instant::now();
            while t.elapsed() < Duration::from_millis(1500) {
                std::thread::sleep(Duration::from_millis(50));
                cap.read(&mut samples).unwrap();
            }
            let level = rms(&samples);
            println!("{label:<28} frames={:<6} rms={level:.5}", samples.len() / CHANNELS);
            level
        };
        let loud = measure("session volume 5%, unmuted");
        mixer.set_app_muted(slot, true);
        let muted = measure("session muted");
        mixer.set_app_muted(slot, false);
        let back = measure("session unmuted again");
        let _ = child.kill();
        let _ = child.wait();
        println!("capture while muted is {:.0}% of the unmuted level", 100.0 * muted / loud.max(1e-9));
        assert!(loud > 0.0001, "nothing was captured at all");
        let _ = back;
    }
}
