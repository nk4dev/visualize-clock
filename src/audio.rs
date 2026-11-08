use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[repr(C)]
struct PaSampleSpec {
    format: u32,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct PaBufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

const PA_SAMPLE_S16LE: u32 = 3;
const PA_STREAM_RECORD: u32 = 2;

#[link(name = "pulse-simple")]
extern "C" {
    fn pa_simple_new(
        server: *const c_char,
        name: *const c_char,
        dir: u32,
        dev: *const c_char,
        stream_name: *const c_char,
        ss: *const PaSampleSpec,
        map: *const c_void,
        attr: *const PaBufferAttr,
        error: *mut c_int,
    ) -> *mut c_void;

    fn pa_simple_free(s: *mut c_void);

    fn pa_simple_read(s: *mut c_void, data: *mut c_void, bytes: usize, error: *mut c_int) -> c_int;
}

#[link(name = "pulse")]
extern "C" {
    fn pa_strerror(error: c_int) -> *const c_char;
}

pub struct AudioState {
    pub samples: Vec<f32>,
    pub is_active: bool,
    pub rms: f32,
    pub source_name: String,
}

pub struct AudioCapture {
    state: Arc<Mutex<AudioState>>,
    running: Arc<AtomicBool>,
}

impl AudioCapture {
    pub const BUFFER_SIZE: usize = 1024;

    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(AudioState {
            samples: vec![0.0; Self::BUFFER_SIZE],
            is_active: false,
            rms: 0.0,
            source_name: "Initializing...".to_string(),
        }));

        let running = Arc::new(AtomicBool::new(true));
        let state_clone = Arc::clone(&state);
        let running_clone = Arc::clone(&running);

        thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || {
                Self::run_capture_loop(state_clone, running_clone);
            })
            .expect("Failed to spawn audio capture thread");

        Self { state, running }
    }

    pub fn get_samples(&self, out: &mut [f32]) -> (bool, f32, String) {
        if let Ok(st) = self.state.lock() {
            let len = out.len().min(st.samples.len());
            out[..len].copy_from_slice(&st.samples[..len]);
            (st.is_active, st.rms, st.source_name.clone())
        } else {
            (false, 0.0, "Locked".to_string())
        }
    }

    fn run_capture_loop(state: Arc<Mutex<AudioState>>, running: Arc<AtomicBool>) {
        let app_name = CString::new("sound-clock-widget").unwrap();
        let stream_name = CString::new("Audio Visualizer").unwrap();
        let ss = PaSampleSpec {
            format: PA_SAMPLE_S16LE,
            rate: 44100,
            channels: 2,
        };

        // Buffer attributes for low latency
        let buffer_attr = PaBufferAttr {
            maxlength: u32::MAX,
            tlength: u32::MAX,
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: (Self::BUFFER_SIZE * 4) as u32,
        };

        let mut handle: *mut c_void = std::ptr::null_mut();
        let mut source_desc = String::new();

        // 1. Try default sink monitor (records desktop audio output)
        let monitor_target = CString::new("@DEFAULT_MONITOR@").unwrap();
        let mut err: c_int = 0;
        let p_handle = unsafe {
            pa_simple_new(
                std::ptr::null(),
                app_name.as_ptr(),
                PA_STREAM_RECORD,
                monitor_target.as_ptr(),
                stream_name.as_ptr(),
                &ss,
                std::ptr::null(),
                &buffer_attr,
                &mut err,
            )
        };

        if !p_handle.is_null() {
            handle = p_handle;
            source_desc = "Desktop Audio".to_string();
        } else {
            // 2. Fall back to default recording source (e.g. microphone)
            let p_handle2 = unsafe {
                pa_simple_new(
                    std::ptr::null(),
                    app_name.as_ptr(),
                    PA_STREAM_RECORD,
                    std::ptr::null(),
                    stream_name.as_ptr(),
                    &ss,
                    std::ptr::null(),
                    &buffer_attr,
                    &mut err,
                )
            };
            if !p_handle2.is_null() {
                handle = p_handle2;
                source_desc = "Microphone".to_string();
            }
        }

        if handle.is_null() {
            // Fallback to simulated audio mode if pulse connection fails
            let err_msg = unsafe {
                CStr::from_ptr(pa_strerror(err))
                    .to_string_lossy()
                    .into_owned()
            };
            eprintln!(
                "PulseAudio connection failed ({}). Using ambient demo signal.",
                err_msg
            );
            Self::run_simulated_loop(state, running);
            return;
        }

        if let Ok(mut st) = state.lock() {
            st.source_name = source_desc;
        }

        // Stereo buffer: 1024 frames = 2048 i16 samples
        let mut raw_buf = vec![0i16; Self::BUFFER_SIZE * 2];
        let bytes_to_read = raw_buf.len() * std::mem::size_of::<i16>();
        let mut converted = vec![0.0f32; Self::BUFFER_SIZE];

        let mut silence_counter = 0;

        while running.load(Ordering::Relaxed) {
            let res = unsafe {
                pa_simple_read(
                    handle,
                    raw_buf.as_mut_ptr() as *mut c_void,
                    bytes_to_read,
                    &mut err,
                )
            };

            if res < 0 {
                // Reconnect or sleep on error
                thread::sleep(Duration::from_millis(50));
                continue;
            }

            // Downmix stereo i16 to mono f32 (-1.0 to 1.0)
            let mut sum_sq = 0.0f32;
            for i in 0..Self::BUFFER_SIZE {
                let left = raw_buf[i * 2] as f32 / 32768.0;
                let right = raw_buf[i * 2 + 1] as f32 / 32768.0;
                let mono = (left + right) * 0.5;
                converted[i] = mono;
                sum_sq += mono * mono;
            }

            let rms = (sum_sq / Self::BUFFER_SIZE as f32).sqrt();
            let is_sound_present = rms > 0.003;

            if is_sound_present {
                silence_counter = 0;
            } else {
                silence_counter = (silence_counter + 1).min(100);
            }

            let is_active = silence_counter < 30; // Active if sound was heard within last 30 frames (~0.5s)

            if let Ok(mut st) = state.lock() {
                st.samples.copy_from_slice(&converted);
                st.rms = rms;
                st.is_active = is_active;
            }
        }

        unsafe {
            pa_simple_free(handle);
        }
    }

    fn run_simulated_loop(state: Arc<Mutex<AudioState>>, running: Arc<AtomicBool>) {
        if let Ok(mut st) = state.lock() {
            st.source_name = "Demo Mode (Simulated)".to_string();
        }

        let mut phase = 0.0f32;
        let mut buf = vec![0.0f32; Self::BUFFER_SIZE];

        while running.load(Ordering::Relaxed) {
            for i in 0..Self::BUFFER_SIZE {
                let t = phase + (i as f32) / 44100.0;
                // Complex harmonic signal for aesthetic visualization
                let s1 = (t * 220.0 * 2.0 * std::f32::consts::PI).sin() * 0.3;
                let s2 = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.2;
                let s3 = (t * 880.0 * 2.0 * std::f32::consts::PI).sin() * 0.15;
                let s4 = (t * 1760.0 * 2.0 * std::f32::consts::PI).sin() * 0.1;
                buf[i] = s1 + s2 + s3 + s4;
            }
            phase += 0.03;
            if phase > 1000.0 {
                phase = 0.0;
            }

            if let Ok(mut st) = state.lock() {
                st.samples.copy_from_slice(&buf);
                st.rms = 0.25;
                st.is_active = true;
            }

            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
