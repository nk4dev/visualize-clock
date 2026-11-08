use num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

pub const NUM_BARS: usize = 28;

pub struct VisualizerDsp {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    fft_buffer: Vec<Complex<f32>>,
    output_buffer: Vec<Complex<f32>>,
    pub smooth_bars: [f32; NUM_BARS],
    pub peak_bars: [f32; NUM_BARS],
    pub peak_speeds: [f32; NUM_BARS],
    bin_ranges: Vec<(usize, usize)>,
    pub gain: f32,
}

impl VisualizerDsp {
    pub fn new(buffer_size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(buffer_size);

        // Precompute Hann window
        let mut window = Vec::with_capacity(buffer_size);
        for i in 0..buffer_size {
            let val = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (buffer_size - 1) as f32).cos());
            window.push(val);
        }

        let fft_buffer = vec![Complex::new(0.0, 0.0); buffer_size];
        let output_buffer = vec![Complex::new(0.0, 0.0); buffer_size];

        // Calculate logarithmically spaced bin ranges for NUM_BARS
        // Sampling rate = 44100Hz, Nyquist = 22050Hz, buffer_size = 1024 -> ~43Hz per FFT bin
        let min_freq = 45.0f32;
        let max_freq = 16000.0f32;
        let mut bin_ranges = Vec::with_capacity(NUM_BARS);

        let bin_width = 44100.0 / (buffer_size as f32);

        for i in 0..NUM_BARS {
            let t0 = i as f32 / NUM_BARS as f32;
            let t1 = (i + 1) as f32 / NUM_BARS as f32;

            let f0 = min_freq * (max_freq / min_freq).powf(t0);
            let f1 = min_freq * (max_freq / min_freq).powf(t1);

            let mut b0 = (f0 / bin_width).floor() as usize;
            let mut b1 = (f1 / bin_width).ceil() as usize;

            if b0 < 1 {
                b0 = 1;
            }
            if b1 <= b0 {
                b1 = b0 + 1;
            }
            if b1 > buffer_size / 2 {
                b1 = buffer_size / 2;
            }

            bin_ranges.push((b0, b1));
        }

        Self {
            fft,
            window,
            fft_buffer,
            output_buffer,
            smooth_bars: [0.0; NUM_BARS],
            peak_bars: [0.0; NUM_BARS],
            peak_speeds: [0.0; NUM_BARS],
            bin_ranges,
            gain: 1.2,
        }
    }

    pub fn process(&mut self, samples: &[f32], dt: f32) {
        let n = samples.len().min(self.window.len());

        // Apply Hann window and fill complex buffer
        for i in 0..n {
            self.fft_buffer[i] = Complex::new(samples[i] * self.window[i], 0.0);
        }
        for i in n..self.fft_buffer.len() {
            self.fft_buffer[i] = Complex::new(0.0, 0.0);
        }

        self.output_buffer.copy_from_slice(&self.fft_buffer);
        self.fft.process(&mut self.output_buffer);

        let gravity = 1.8 * dt;

        for (i, &(b0, b1)) in self.bin_ranges.iter().enumerate() {
            let mut max_mag = 0.0f32;
            for b in b0..b1 {
                if b < self.output_buffer.len() / 2 {
                    let c = self.output_buffer[b];
                    let mag = (c.re * c.re + c.im * c.im).sqrt();
                    if mag > max_mag {
                        max_mag = mag;
                    }
                }
            }

            // Normalize and apply dB-like non-linear scaling with frequency tilt
            // Higher frequencies naturally have lower power, so apply high-shelf boost
            let freq_boost = 1.0 + (i as f32 / NUM_BARS as f32) * 1.8;
            let raw_val = (max_mag / (n as f32 * 0.12) * self.gain * freq_boost).sqrt();
            let target = raw_val.clamp(0.0, 1.0);

            // Fast attack, smooth decay
            if target > self.smooth_bars[i] {
                self.smooth_bars[i] = self.smooth_bars[i] * 0.2 + target * 0.8;
            } else {
                let decay = (0.08 * dt * 60.0).clamp(0.01, 0.4);
                self.smooth_bars[i] = (self.smooth_bars[i] - decay).max(target);
            }

            // Peak hold logic
            if self.smooth_bars[i] >= self.peak_bars[i] {
                self.peak_bars[i] = self.smooth_bars[i];
                self.peak_speeds[i] = 0.0;
            } else {
                self.peak_speeds[i] += gravity;
                self.peak_bars[i] = (self.peak_bars[i] - self.peak_speeds[i]).max(0.0);
            }
        }
    }
}
