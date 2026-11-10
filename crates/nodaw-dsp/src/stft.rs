//! A streaming short-time Fourier transform: Hann analysis and synthesis windows, 75 %
//! overlap, overlap-add, an exact latency of `size` samples. A [`SpectralProcessor`] edits
//! each frame (magnitude and phase per bin) between the analysis and the resynthesis.
//!
//! Identity (a processor that changes nothing) reconstructs the input exactly, delayed by
//! [`Stft::latency`] (tested). No allocation after construction.

use crate::fft::Fft;
use std::f32::consts::PI;

/// Edits one spectral frame in place. `mag` and `phase` have `size / 2 + 1` bins.
pub trait SpectralProcessor {
    /// Called once per hop, on the audio thread: must not allocate.
    fn process(&mut self, ch: usize, mag: &mut [f32], phase: &mut [f32]);
}

/// The identity processor.
pub struct Passthrough;
impl SpectralProcessor for Passthrough {
    fn process(&mut self, _ch: usize, _mag: &mut [f32], _phase: &mut [f32]) {}
}

/// One channel's buffers.
#[derive(Debug, Clone)]
struct Chan {
    inp: Vec<f32>,
    out: Vec<f32>,
}

/// The STFT engine for up to two channels.
#[derive(Debug, Clone)]
pub struct Stft {
    n: usize,
    hop: usize,
    fft: Fft,
    win: Vec<f32>,
    scale: f32,
    ch: [Chan; 2],
    pos: usize,
    re: Vec<f32>,
    im: Vec<f32>,
    /// Magnitudes of the last frame (per channel), for analysers.
    pub mag: [Vec<f32>; 2],
    /// Phases of the last frame (per channel).
    pub phase: [Vec<f32>; 2],
}

impl Stft {
    /// An STFT of `size` points (a power of two, >= 64).
    pub fn new(size: usize) -> Self {
        assert!(size.is_power_of_two() && size >= 64);
        let hop = size / 4;
        let win: Vec<f32> = (0..size).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / size as f32).cos()).collect();
        // Hann analysis x Hann synthesis at 75 % overlap sums to 1.5.
        let scale = 1.0 / (1.5 * size as f32);
        let bins = size / 2 + 1;
        let chan = || Chan { inp: vec![0.0; size], out: vec![0.0; size] };
        Self { n: size, hop, fft: Fft::new(size), win, scale, ch: [chan(), chan()], pos: 0, re: vec![0.0; size], im: vec![0.0; size], mag: [vec![0.0; bins], vec![0.0; bins]], phase: [vec![0.0; bins], vec![0.0; bins]] }
    }
    /// FFT size.
    pub fn size(&self) -> usize {
        self.n
    }
    /// Hop in samples.
    pub fn hop(&self) -> usize {
        self.hop
    }
    /// Bins per frame (`size / 2 + 1`).
    pub fn bins(&self) -> usize {
        self.n / 2 + 1
    }
    /// Latency in samples.
    pub fn latency(&self) -> usize {
        self.n
    }
    /// Clear all state.
    pub fn reset(&mut self) {
        for c in &mut self.ch {
            c.inp.fill(0.0);
            c.out.fill(0.0);
        }
        for m in self.mag.iter_mut().chain(self.phase.iter_mut()) {
            m.fill(0.0);
        }
        self.pos = 0;
    }

    /// One stereo frame in, one out (delayed by `latency()`).
    #[inline]
    pub fn tick<P: SpectralProcessor>(&mut self, l: f32, r: f32, p: &mut P) -> (f32, f32) {
        let n = self.n;
        let i = self.pos;
        // the input ring holds the last `n` samples; out holds the overlap-add accumulator
        let yl = self.ch[0].out[i];
        let yr = self.ch[1].out[i];
        self.ch[0].out[i] = 0.0;
        self.ch[1].out[i] = 0.0;
        self.ch[0].inp[i] = l;
        self.ch[1].inp[i] = r;
        self.pos = (i + 1) % n;
        if self.pos.is_multiple_of(self.hop) {
            for c in 0..2 {
                self.frame(c, p);
            }
        }
        (yl, yr)
    }

    fn frame<P: SpectralProcessor>(&mut self, c: usize, p: &mut P) {
        let n = self.n;
        let start = self.pos; // oldest sample of the ring
        for k in 0..n {
            self.re[k] = self.ch[c].inp[(start + k) % n] * self.win[k];
            self.im[k] = 0.0;
        }
        self.fft.forward(&mut self.re, &mut self.im);
        let bins = n / 2 + 1;
        for k in 0..bins {
            let (a, b) = (self.re[k], self.im[k]);
            self.mag[c][k] = (a * a + b * b).sqrt();
            self.phase[c][k] = b.atan2(a);
        }
        {
            let (mag, phase) = (&mut self.mag[c], &mut self.phase[c]);
            p.process(c, mag, phase);
        }
        // Back to a Hermitian spectrum; the inverse is conj(FFT(conj(X))) / n.
        for k in 0..bins {
            let m = if self.mag[c][k].is_finite() { self.mag[c][k] } else { 0.0 };
            let ph = self.phase[c][k];
            let (s, co) = ph.sin_cos();
            self.re[k] = m * co;
            self.im[k] = -(m * s);
        }
        self.im[0] = 0.0;
        self.im[n / 2] = 0.0;
        for k in bins..n {
            self.re[k] = self.re[n - k];
            self.im[k] = -self.im[n - k];
        }
        self.fft.forward(&mut self.re, &mut self.im);
        // re now holds n * x[k] (conjugation leaves the real part); window and add. The frame
        // covers the samples ring[start .. start+n]; they come out one block later.
        for k in 0..n {
            let v = self.re[k] * self.win[k] * self.scale * n as f32 / n as f32;
            let j = (start + k) % n;
            self.ch[c].out[j] += crate::util::undenorm(v);
        }
    }
}

/// Wrap a phase to -pi..pi.
#[inline]
pub fn wrap(p: f32) -> f32 {
    let t = p + PI;
    t - (t / (2.0 * PI)).floor() * 2.0 * PI - PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_reconstructs_the_input_delayed_by_the_latency() {
        let mut s = Stft::new(1024);
        let n = 20_000;
        let x: Vec<f32> = (0..n).map(|i| 0.5 * (i as f32 * 0.013).sin() + 0.2 * (i as f32 * 0.31).cos()).collect();
        let mut y = Vec::with_capacity(n);
        for &v in &x {
            y.push(s.tick(v, -v, &mut Passthrough));
        }
        let lat = s.latency();
        let worst = (lat + 10..n).map(|i| (y[i].0 - x[i - lat]).abs().max((y[i].1 + x[i - lat]).abs())).fold(0.0f32, f32::max);
        assert!(worst < 1e-4, "{worst}");
    }

    #[test]
    fn wrap_stays_in_range() {
        for i in -100..100 {
            let w = wrap(i as f32 * 0.77);
            assert!((-PI - 1e-5..=PI + 1e-5).contains(&w));
        }
    }
}
