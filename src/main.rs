use windows::{
    core::{ComInterface, Result},
    Win32::{
        Foundation::LPARAM,
        Media::Audio::{
            eMultimedia, eRender, AudioSessionStateActive, Endpoints::IAudioMeterInformation,
            IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator,
            IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        },
        System::Com::{CoCreateInstance, CoInitialize, CLSCTX_ALL, CLSCTX_INPROC_SERVER},
        UI::WindowsAndMessaging::EnumWindows,
    },
};

use crate::window_callback::{callback, CallbackCollector};

mod window_callback {
    use windows::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{GetWindowTextLengthA, GetWindowTextW, GetWindowThreadProcessId},
    };

    #[derive(Debug)]
    pub struct CallbackCollector {
        pub hwnd: Option<(HWND, String)>,
        pub process_id: u32,
    }

    pub unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let hwnds_collector = &mut *(lparam.0 as *mut CallbackCollector);

        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        let name_length = GetWindowTextLengthA(hwnd);
        let mut buffer = vec![0u16; name_length as usize + 1];
        GetWindowTextW(hwnd, &mut buffer);
        let Ok(name) = String::from_utf16(&buffer) else {
            return BOOL::from(true);
        };

        if process_id != hwnds_collector.process_id {
            return BOOL::from(true);
        }

        hwnds_collector.hwnd = Some((hwnd, name.to_string()));

        // Return false to stop enumeration
        BOOL::from(false)
    }
}

fn process_session_audio(session_control: IAudioSessionControl) -> Result<()> {
    let session_control2: IAudioSessionControl2 = session_control.cast()?;

    let session_state = unsafe { session_control.GetState() }?;
    if session_state != AudioSessionStateActive {
        return Ok(());
    }

    let session_meter: IAudioMeterInformation = session_control.cast()?;

    let peak = unsafe { session_meter.GetPeakValue() }?;
    let session_pid = unsafe { session_control2.GetProcessId() }?;

    let mut collector = CallbackCollector {
        hwnd: None,
        process_id: session_pid,
    };
    unsafe {
        EnumWindows(Some(callback), LPARAM(&mut collector as *mut _ as isize));
    };

    if collector.hwnd.is_none() {
        eprintln!("[WARN] No window found");
        return Ok(());
    }

    println!("{}: {}", collector.hwnd.unwrap().1, peak);
    Ok(())
}

fn main() -> Result<()> {
    let _ = unsafe { CoInitialize(None) };
    let device_enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) }?;

    // eRender is playback
    let device: IMMDevice =
        unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) }?;

    let audio_session_manager: IAudioSessionManager2 =
        unsafe { device.Activate(CLSCTX_ALL, None) }?;
    let audio_session_enumerator: IAudioSessionEnumerator =
        unsafe { audio_session_manager.GetSessionEnumerator() }?;

    let session_count = unsafe { audio_session_enumerator.GetCount() }?;
    for i in 0..session_count {
        let session_control: IAudioSessionControl =
            unsafe { audio_session_enumerator.GetSession(i) }?;

        process_session_audio(session_control)?;
    }

    Ok(())
}
