#![cfg_attr(
    all(not(debug_assertions), not(feature = "window")),
    windows_subsystem = "windows"
)]

use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chrono::Local;
#[cfg(feature = "online")]
use mac_address::get_mac_address;
use windows::{
    core::{ComInterface, Result},
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Media::Audio::{
            eMultimedia, eRender, AudioSessionStateActive, Endpoints::IAudioMeterInformation,
            IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator,
            IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        },
        System::Com::{CoCreateInstance, CoInitialize, CLSCTX_ALL, CLSCTX_INPROC_SERVER},
        UI::WindowsAndMessaging::{
            EnumWindows, PostMessageA, SetForegroundWindow, WM_KEYDOWN, WM_KEYUP,
        },
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

struct OutputVolume {
    hwnd: HWND,
    name: String,
    pid: u32,
    volume: f32,
}

fn process_session_audio(session_control: IAudioSessionControl) -> Result<Option<OutputVolume>> {
    let session_control2: IAudioSessionControl2 = session_control.cast()?;

    let session_state = unsafe { session_control.GetState() }?;
    if session_state != AudioSessionStateActive {
        return Ok(None);
    }

    let session_meter: IAudioMeterInformation = session_control.cast()?;

    let peak = unsafe { session_meter.GetPeakValue() }?;
    let session_pid = unsafe { session_control2.GetProcessId() }?;

    let session_id = unsafe { session_control2.GetSessionIdentifier()?.to_string() }?;
    if !session_id.to_lowercase().contains("warcraft") {
        return Ok(None);
    }

    let mut collector = CallbackCollector {
        hwnd: None,
        process_id: session_pid,
    };
    unsafe {
        EnumWindows(Some(callback), LPARAM(&mut collector as *mut _ as isize));
    };

    if collector.hwnd.is_none() {
        eprintln!("[WARN] No window found");
        return Ok(None);
    }

    let (hwnd, name) = collector.hwnd.unwrap();
    let output = OutputVolume {
        hwnd,
        name,
        pid: session_pid,
        volume: peak,
    };

    Ok(Some(output))
}

#[derive(Debug, Clone)]
struct Fish {
    hwnd: HWND,
    pid: u32,
    name: String,
    last_cast: Instant,
    last_reel: Instant,
    last_bite: Instant,
    stop_reel: bool,
}

impl Fish {
    fn fish(&mut self) {
        use rand::Rng;
        use windows::Win32::UI::Input::KeyboardAndMouse::VK_F8;

        let sleep_ms = rand::thread_rng().gen_range(100..200);
        std::thread::sleep(std::time::Duration::from_millis(sleep_ms));

        let key = WPARAM(usize::from(VK_F8.0));
        let flags = (VK_F8.0 as isize) << 16;
        unsafe {
            PostMessageA(self.hwnd, WM_KEYDOWN, key, LPARAM(flags));
            std::thread::sleep(std::time::Duration::from_millis(10));
            PostMessageA(self.hwnd, WM_KEYUP, key, LPARAM(flags));
        };

        self.last_cast = Instant::now();
    }

    fn debug<D: Display>(&self, msg: D) {
        println!(
            "[{}][{: >6}] {}: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            self.pid,
            self.name,
            msg
        );
    }
}

#[derive(Debug, Clone)]
struct FishingData {
    pub reel_count: u32,
    pub cast_count: u32,
    pub reel_average: Duration,
}

impl Default for FishingData {
    fn default() -> Self {
        Self {
            reel_count: 0,
            cast_count: 0,
            reel_average: Duration::from_secs(0),
        }
    }
}

struct App {
    fish_map: Arc<Mutex<HashMap<u32, Fish>>>,
    data_map: Arc<Mutex<HashMap<u32, FishingData>>>,
    is_stop: bool,
    start_time: chrono::DateTime<Local>,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut fish_map = self.fish_map.lock().unwrap();
        let data_map = self.data_map.lock().unwrap();

        let stop_start = if self.is_stop {
            "Start fishing"
        } else {
            "Stop fishing"
        };

        let fish_values = fish_map.values().cloned().collect::<Vec<_>>();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Awesome Fishing bot");
            ui.heading(format!(
                "Started {}",
                self.start_time.format("%Y-%m-%d %H:%M:%S")
            ));

            ui.add_space(16f32);

            ui.vertical(|ui| {
                for fish_window in fish_values {
                    let play_pause = if fish_window.stop_reel { "⏵" } else { "⏸" };
                    ui.horizontal(|ui| {
                        if ui.button(play_pause).clicked() {
                            fish_map.get_mut(&fish_window.pid).unwrap().stop_reel =
                                !fish_window.stop_reel;
                        }

                        ui.label(format!("{}: {}", fish_window.name, fish_window.pid));
                        if ui.button("Foreground").clicked() {
                            unsafe {
                                SetForegroundWindow(fish_window.hwnd);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        let data = data_map.get(&fish_window.pid).unwrap();
                        ui.label(format!("Reel count: {} |", data.reel_count));
                        ui.label(format!("Cast count: {} |", data.cast_count));
                        ui.label(format!("Real avg time: {:?} |", data.reel_average));
                    });
                    ui.add_space(8f32);
                }
            });

            ui.add_space(16f32);

            if ui.button(stop_start).clicked() {
                self.is_stop = !self.is_stop;
                for fish in fish_map.values_mut() {
                    fish.stop_reel = self.is_stop;
                }
            }
        });

        ctx.request_repaint_after(Duration::from_millis(2000));
    }
}

#[cfg(feature = "online")]
// const UUID: &str = "DEFAULT0-DEFA-DEFA-DEFA-DEFAULT00000";
const UUID: &str = "db67dcec-be9c-4247-8f23-ca52603bf89c";

fn main_loop(
    audio_session_pointer: usize,
    fish_map: Arc<Mutex<HashMap<u32, Fish>>>,
    data_map: Arc<Mutex<HashMap<u32, FishingData>>>,
) -> Result<()> {
    let audio_session_enumerator: IAudioSessionEnumerator =
        unsafe { std::mem::transmute(audio_session_pointer) };

    #[cfg(feature = "online")]
    let client = reqwest::blocking::Client::new();
    #[cfg(feature = "online")]
    let mac_address = get_mac_address()
        .expect("InternalError while retrieving mac address")
        .expect("No mac address found");

    #[cfg(feature = "online")]
    let mut last_check = Instant::now() - Duration::from_secs(10);

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[cfg(feature = "online")]
    enum LastCheckStatus {
        Ok,
        Failed,
        Rejected,
    }
    #[cfg(feature = "online")]
    let mut last_check_status = LastCheckStatus::Ok;

    loop {
        #[cfg(feature = "online")]
        if last_check.elapsed().as_secs() > 5 {
            last_check = Instant::now();

            let Ok(resp) = client
                .post("http://localhost:3000/api/ping")
                .body(format!(
                    r#"{{ "macAddress": "{}", "botId": "{}" }}"#,
                    mac_address, UUID
                )).send() else {
                std::thread::sleep(Duration::from_secs(1));
                eprintln!("Failed to ping server");
                last_check_status = LastCheckStatus::Failed;
                continue;
            };
            if !resp.status().is_success() {
                eprintln!("Failed to ping server");
                std::thread::sleep(Duration::from_secs(1));
                last_check_status = LastCheckStatus::Rejected;
                continue;
            }

            last_check_status = LastCheckStatus::Ok;
        }

        #[cfg(feature = "online")]
        if last_check_status != LastCheckStatus::Ok {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }

        let session_count = unsafe { audio_session_enumerator.GetCount() }?;
        for i in 0..session_count {
            let session_control: IAudioSessionControl =
                unsafe { audio_session_enumerator.GetSession(i) }?;

            let Some(output) = process_session_audio(session_control)? else {
                continue
            };

            let mut fish_map = fish_map.lock().unwrap();
            // Window has been discarded
            if !fish_map.contains_key(&output.pid) {
                continue;
            }

            let current_fish = fish_map.get_mut(&output.pid).unwrap();
            let mut fish_data = data_map.lock().unwrap();

            // Fish is waiting to be reeled in
            if current_fish.last_bite > Instant::now() || current_fish.stop_reel {
                continue;
            }

            // Fish has waited enough time to be reeled in
            if current_fish.last_bite > current_fish.last_reel {
                current_fish.debug("Fish reeled in");

                let reel_time = current_fish.last_cast.elapsed().as_secs_f32();
                current_fish.fish();
                current_fish.last_reel = Instant::now();

                let FishingData {
                    reel_average,
                    reel_count,
                    ..
                } = fish_data.get(&output.pid).unwrap();
                let reel_average = (reel_average.as_secs_f32() * *reel_count as f32 + reel_time)
                    / (reel_count + 1) as f32;
                fish_data.get_mut(&output.pid).unwrap().reel_count += 1;
                fish_data.get_mut(&output.pid).unwrap().reel_average =
                    Duration::from_secs_f32(reel_average);

                continue;
            }

            // Wait 1 second between reel and cast
            if current_fish.last_reel.elapsed() < Duration::from_secs(1) {
                continue;
            }

            // Cast if we have reel'd in
            if current_fish.last_cast < current_fish.last_reel {
                fish_data.get_mut(&output.pid).unwrap().cast_count += 1;
                current_fish.fish();
                continue;
            }
            // Reset cast if we haven't reeled in for 30 seconds
            if current_fish.last_cast.elapsed() > Duration::from_secs(30) {
                current_fish.debug("RESET CAST");
                fish_data.get_mut(&output.pid).unwrap().cast_count += 1;
                current_fish.fish();
                continue;
            }
            // Don't process volume for 5 seconds after cast
            if current_fish.last_cast.elapsed() < Duration::from_secs(5) {
                continue;
            }

            #[cfg(debug_assertions)]
            if output.volume >= 0.01 {
                current_fish.debug(output.volume);
            }

            if output.volume > 0.035 {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let random = rng.gen_range(100..500);
                current_fish.debug(format!("BITE! Reel in {}ms", random));
                current_fish.last_bite = Instant::now() + Duration::from_millis(random);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }
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

    let fish_map = Arc::new(Mutex::new(HashMap::new()));
    let fish_map_ui = fish_map.clone();
    let data_map = Arc::new(Mutex::new(HashMap::new()));
    let data_map_ui = data_map.clone();
    let data_map_ctrlc = data_map.clone();

    ctrlc::set_handler(move || {
        dbg!(&data_map_ctrlc.lock().unwrap());
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let session_count = unsafe { audio_session_enumerator.GetCount() }?;
    for i in 0..session_count {
        let session_control: IAudioSessionControl =
            unsafe { audio_session_enumerator.GetSession(i) }?;

        let Some(output) = process_session_audio(session_control)? else {
            continue
        };

        let mut fish_map = fish_map.lock().unwrap();
        if !fish_map.contains_key(&output.pid) {
            fish_map.insert(
                output.pid,
                Fish {
                    hwnd: output.hwnd,
                    name: output.name,
                    pid: output.pid,
                    last_bite: Instant::now() - Duration::from_secs(7200),
                    last_cast: Instant::now() - Duration::from_secs(3600),
                    last_reel: Instant::now() - Duration::from_secs(3600),
                    stop_reel: false,
                },
            );

            if let Ok(mut data_map) = data_map.lock() {
                data_map.insert(output.pid, FishingData::default());
            }
        }
    }

    // This is a hack to get around the fact that we can't pass a pointer to a thread
    let audio_session_pointer: usize = unsafe { std::mem::transmute(audio_session_enumerator) };

    let main_thread = std::thread::spawn(move || {
        let _ = main_loop(audio_session_pointer, fish_map, data_map);
    });

    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::Vec2::new(400.0, 400.0)),
        ..Default::default()
    };
    let app = App {
        is_stop: false,
        data_map: data_map_ui,
        fish_map: fish_map_ui,
        start_time: chrono::Local::now(),
    };

    let _ = eframe::run_native(
        "Awesome Fishing Bot",
        options,
        Box::new(|_cc| Box::new(app)),
    );
    main_thread.join().unwrap();

    Ok(())
}
