//! Microphone capture: opens default input device, streams 16kHz 16-bit mono PCM chunks.
//!
//! Uses `cpal` for cross-platform audio capture. Audio is captured at whatever
//! sample rate the device supports, then resampled to 16 kHz and downmixed to
//! mono via linear interpolation. Samples are stored as i16 in a ring buffer
//! (max 5 minutes = 4 800 000 samples).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

/// Target sample rate for all captured audio.
pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Ring buffer capacity: 5 minutes of 16 kHz mono audio.
/// Long recordings (e.g. meeting notes) won't lose the beginning.
const RING_BUFFER_CAPACITY: usize = 300 * TARGET_SAMPLE_RATE as usize;

/// Errors that can occur during audio capture.
#[derive(Debug)]
pub enum CaptureError {
    /// No suitable input device was found on the system.
    NoInputDevice,
    /// Failed to query supported input configurations.
    SupportedConfigsError(cpal::SupportedStreamConfigsError),
    /// No supported configuration was found for the device.
    NoSupportedConfig,
    /// The stream could not be built.
    BuildStreamError(cpal::BuildStreamError),
    /// The stream could not be started.
    PlayStreamError(cpal::PlayStreamError),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::NoInputDevice => write!(f, "no input device available"),
            CaptureError::SupportedConfigsError(e) => {
                write!(f, "failed to query input configs: {e}")
            }
            CaptureError::NoSupportedConfig => write!(f, "no supported input config found"),
            CaptureError::BuildStreamError(e) => write!(f, "failed to build stream: {e}"),
            CaptureError::PlayStreamError(e) => write!(f, "failed to start stream: {e}"),
        }
    }
}

impl std::error::Error for CaptureError {}

// ---------------------------------------------------------------------------
// Internal shared state
// ---------------------------------------------------------------------------

struct CaptureState {
    /// Ring buffer of resampled 16 kHz mono i16 samples.
    buffer: VecDeque<i16>,
    /// True when the stream is actively delivering samples.
    is_recording: bool,
    /// Device sample rate at which data is actually arriving (before resample).
    device_sample_rate: u32,
    /// Number of channels the device is providing.
    device_channels: u16,
    /// Fractional position tracking for linear resampling.
    resample_pos: f64,
    /// Previous sample value used for linear interpolation (per-channel average).
    prev_mono_sample: f32,
}

impl CaptureState {
    fn new(device_sample_rate: u32, device_channels: u16) -> Self {
        Self {
            buffer: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
            is_recording: false,
            device_sample_rate,
            device_channels,
            resample_pos: 0.0,
            prev_mono_sample: 0.0,
        }
    }

    /// Push a slice of raw device samples into the ring buffer.
    ///
    /// Handles mono downmix (average channels) and resampling to 16 kHz.
    fn push_samples_f32(&mut self, data: &[f32]) {
        let channels = self.device_channels as usize;
        if channels == 0 {
            return;
        }

        // Iterate over frames (one frame = one sample per channel).
        let frames: Vec<f32> = data
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect();

        // Resample from device_sample_rate to TARGET_SAMPLE_RATE using linear
        // interpolation. We maintain a fractional read position (resample_pos)
        // that advances by (device_sample_rate / TARGET_SAMPLE_RATE) per
        // output sample.
        let ratio = self.device_sample_rate as f64 / TARGET_SAMPLE_RATE as f64;

        while self.resample_pos < frames.len() as f64 {
            let idx = self.resample_pos as usize;
            let frac = self.resample_pos - idx as f64;

            let s0 = if idx < frames.len() {
                frames[idx]
            } else {
                self.prev_mono_sample
            };
            let s1 = if idx + 1 < frames.len() {
                frames[idx + 1]
            } else {
                s0
            };

            let interpolated = s0 + (s1 - s0) * frac as f32;

            // Convert f32 [-1.0, 1.0] to i16.
            let sample_i16 = (interpolated * i16::MAX as f32)
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16;

            // Enforce ring buffer capacity by dropping the oldest sample.
            if self.buffer.len() >= RING_BUFFER_CAPACITY {
                self.buffer.pop_front();
            }
            self.buffer.push_back(sample_i16);

            self.resample_pos += ratio;
        }

        // Advance position by how many source frames we consumed this batch,
        // carrying the fractional remainder into the next callback.
        self.resample_pos -= frames.len() as f64;
        if !frames.is_empty() {
            self.prev_mono_sample = *frames.last().unwrap();
        }
    }

    /// Push i16 samples, converting to f32 first.
    fn push_samples_i16(&mut self, data: &[i16]) {
        let as_f32: Vec<f32> = data
            .iter()
            .map(|&s| s as f32 / i16::MAX as f32)
            .collect();
        self.push_samples_f32(&as_f32);
    }

    /// Push i32 samples, converting to f32 first.
    fn push_samples_i32(&mut self, data: &[i32]) {
        let as_f32: Vec<f32> = data
            .iter()
            .map(|&s| s as f32 / i32::MAX as f32)
            .collect();
        self.push_samples_f32(&as_f32);
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Manages microphone capture using the default system input device.
pub struct AudioCapture {
    /// The live cpal stream. Kept alive as long as recording.
    _stream: Option<Stream>,
    /// Shared buffer / state between callback thread and API callers.
    state: Arc<Mutex<CaptureState>>,
    /// Actual sample rate of the device (stored for reference).
    sample_rate: u32,
}

// Safety: `cpal::Stream` is not `Send` by default on some platforms, but we
// hold it purely for its lifetime (drop = stop). We never send the stream
// itself across threads — only the Arc<Mutex<CaptureState>>.
unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

#[allow(dead_code)]
impl AudioCapture {
    /// Create a new `AudioCapture` connected to the default input device.
    ///
    /// This does **not** start recording; call [`start`](AudioCapture::start)
    /// to begin streaming audio.
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(CaptureError::NoInputDevice)?;

        // Pick the best supported config: prefer f32, then i16, then i32.
        // We'll take whatever sample rate the device offers — resampling
        // happens in the callback.
        let supported_configs = device
            .supported_input_configs()
            .map_err(CaptureError::SupportedConfigsError)?;

        // Collect all configs and rank them.
        let mut configs: Vec<cpal::SupportedStreamConfigRange> = supported_configs.collect();
        if configs.is_empty() {
            return Err(CaptureError::NoSupportedConfig);
        }

        // Sort by preference: f32 > i16 > i32, then by channel count (prefer mono).
        configs.sort_by_key(|c| {
            let fmt_rank = match c.sample_format() {
                SampleFormat::F32 => 0,
                SampleFormat::I16 => 1,
                SampleFormat::I32 => 2,
                _ => 3,
            };
            let ch_rank = (c.channels() as i32 - 1).abs(); // prefer 1 channel
            (fmt_rank, ch_rank)
        });

        let chosen_range = &configs[0];

        // Request the sample rate closest to TARGET_SAMPLE_RATE within the
        // supported range.
        let desired_rate = cpal::SampleRate(TARGET_SAMPLE_RATE);
        let clamped_rate = desired_rate
            .0
            .clamp(chosen_range.min_sample_rate().0, chosen_range.max_sample_rate().0);
        let sample_rate = cpal::SampleRate(clamped_rate);

        let config = StreamConfig {
            channels: chosen_range.channels(),
            sample_rate,
            buffer_size: cpal::BufferSize::Default,
        };

        let device_sample_rate = config.sample_rate.0;
        let device_channels = config.channels;

        let state = Arc::new(Mutex::new(CaptureState::new(
            device_sample_rate,
            device_channels,
        )));

        Ok(Self {
            _stream: None,
            state,
            sample_rate: device_sample_rate,
        })
    }

    /// Start recording audio from the microphone.
    ///
    /// Returns an error if the stream cannot be opened or started.
    pub fn start(&mut self) -> Result<(), CaptureError> {
        // If already recording, do nothing.
        if self._stream.is_some() {
            if let Ok(s) = self.state.lock() {
                if s.is_recording {
                    return Ok(());
                }
            }
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(CaptureError::NoInputDevice)?;

        let supported_configs = device
            .supported_input_configs()
            .map_err(CaptureError::SupportedConfigsError)?;

        let mut configs: Vec<cpal::SupportedStreamConfigRange> = supported_configs.collect();
        if configs.is_empty() {
            return Err(CaptureError::NoSupportedConfig);
        }

        configs.sort_by_key(|c| {
            let fmt_rank = match c.sample_format() {
                SampleFormat::F32 => 0,
                SampleFormat::I16 => 1,
                SampleFormat::I32 => 2,
                _ => 3,
            };
            let ch_rank = (c.channels() as i32 - 1).abs();
            (fmt_rank, ch_rank)
        });

        let chosen_range = &configs[0];
        let desired_rate = cpal::SampleRate(TARGET_SAMPLE_RATE);
        let clamped_rate = desired_rate
            .0
            .clamp(chosen_range.min_sample_rate().0, chosen_range.max_sample_rate().0);

        let config = StreamConfig {
            channels: chosen_range.channels(),
            sample_rate: cpal::SampleRate(clamped_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let sample_format = chosen_range.sample_format();
        let device_sample_rate = config.sample_rate.0;
        let device_channels = config.channels;

        // Update shared state with confirmed device parameters.
        {
            let mut s = self.state.lock().unwrap();
            s.device_sample_rate = device_sample_rate;
            s.device_channels = device_channels;
            s.resample_pos = 0.0;
        }

        self.sample_rate = device_sample_rate;

        let state_clone = Arc::clone(&self.state);
        let err_state = Arc::clone(&self.state);

        let stream = match sample_format {
            SampleFormat::F32 => device
                .build_input_stream(
                    &config,
                    move |data: &[f32], _info| {
                        let mut s = state_clone.lock().unwrap();
                        if s.is_recording {
                            s.push_samples_f32(data);
                        }
                    },
                    move |err| {
                        eprintln!("[fonos] audio capture error: {err}");
                        let mut s = err_state.lock().unwrap();
                        s.is_recording = false;
                    },
                    None,
                )
                .map_err(CaptureError::BuildStreamError)?,

            SampleFormat::I16 => {
                let state_clone2 = Arc::clone(&self.state);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i16], _info| {
                            let mut s = state_clone2.lock().unwrap();
                            if s.is_recording {
                                s.push_samples_i16(data);
                            }
                        },
                        move |err| {
                            eprintln!("[fonos] audio capture error: {err}");
                            let mut s = state_clone.lock().unwrap();
                            s.is_recording = false;
                        },
                        None,
                    )
                    .map_err(CaptureError::BuildStreamError)?
            }

            SampleFormat::I32 => {
                let state_clone2 = Arc::clone(&self.state);
                device
                    .build_input_stream(
                        &config,
                        move |data: &[i32], _info| {
                            let mut s = state_clone2.lock().unwrap();
                            if s.is_recording {
                                s.push_samples_i32(data);
                            }
                        },
                        move |err| {
                            eprintln!("[fonos] audio capture error: {err}");
                            let mut s = err_state.lock().unwrap();
                            s.is_recording = false;
                        },
                        None,
                    )
                    .map_err(CaptureError::BuildStreamError)?
            }

            other => {
                // For any other format, fall back to f32 by requesting a
                // new config that forces f32.
                eprintln!("[fonos] unsupported sample format {other:?}, falling back to f32");
                return Err(CaptureError::NoSupportedConfig);
            }
        };

        stream
            .play()
            .map_err(CaptureError::PlayStreamError)?;

        {
            let mut s = self.state.lock().unwrap();
            s.is_recording = true;
        }

        self._stream = Some(stream);
        Ok(())
    }

    /// Stop recording. The internal buffer retains its contents.
    pub fn stop(&mut self) {
        // Dropping the stream stops the cpal callback.
        self._stream = None;
        if let Ok(mut s) = self.state.lock() {
            s.is_recording = false;
        }
    }

    /// Returns `true` if audio is actively being captured.
    pub fn is_recording(&self) -> bool {
        self.state
            .lock()
            .map(|s| s.is_recording)
            .unwrap_or(false)
    }

    /// Extract the oldest `duration_ms` milliseconds of samples (at 16 kHz)
    /// from the front of the ring buffer.
    ///
    /// Returns `None` if there are not enough samples buffered yet.
    ///
    /// Example: `take_chunk(200)` returns `Some(Vec<i16>)` with 3 200 samples.
    pub fn take_chunk(&self, duration_ms: u32) -> Option<Vec<i16>> {
        let required = (TARGET_SAMPLE_RATE as u64 * duration_ms as u64 / 1000) as usize;
        let mut s = self.state.lock().unwrap();
        if s.buffer.len() < required {
            return None;
        }
        let chunk: Vec<i16> = s.buffer.drain(..required).collect();
        Some(chunk)
    }

    /// RMS amplitude of the most recent ~100ms of audio (1 600 samples at
    /// 16 kHz), normalised to the range `[0.0, 1.0]`.
    ///
    /// Useful for waveform visualisation.
    pub fn get_level(&self) -> f32 {
        const WINDOW: usize = 1_600; // 100ms at 16 kHz
        let s = self.state.lock().unwrap();
        let len = s.buffer.len();
        if len == 0 {
            return 0.0;
        }
        let window = WINDOW.min(len);
        let start = len - window;
        let sum_sq: f64 = s
            .buffer
            .range(start..)
            .map(|&x| {
                let v = x as f64 / i16::MAX as f64;
                v * v
            })
            .sum();
        let rms = (sum_sq / window as f64).sqrt() as f32;
        rms.clamp(0.0, 1.0)
    }

    /// The actual device sample rate (may differ from `TARGET_SAMPLE_RATE`
    /// if the device doesn't support 16 kHz natively).
    pub fn device_sample_rate(&self) -> u32 {
        self.sample_rate
    }
}
