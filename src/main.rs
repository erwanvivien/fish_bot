use windows::{
    core::Result,
    Win32::{
        Media::Audio::{
            eConsole, eRender, Endpoints::IAudioMeterInformation, IMMDevice, IMMDeviceEnumerator,
            MMDeviceEnumerator,
        },
        System::Com::{CoCreateInstance, CoInitialize, CLSCTX_ALL, CLSCTX_INPROC_SERVER},
    },
};

fn main() {
    let _ = unsafe { CoInitialize(None) };
    let device_enumerator: Result<IMMDeviceEnumerator> =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER) };
    let device_enumerator = device_enumerator.expect("Failed to create device enumerator");

    let device: Result<IMMDevice> =
        unsafe { device_enumerator.GetDefaultAudioEndpoint(eRender, eConsole) };
    let device = device.expect("Failed to get default audio endpoint");

    let meter: Result<IAudioMeterInformation> = unsafe { device.Activate(CLSCTX_ALL, None) };
    let meter = meter.expect("Failed to activate audio meter");

    loop {
        let value = unsafe { meter.GetPeakValue() }.expect("Failed to get peak value");
        dbg!(value);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
