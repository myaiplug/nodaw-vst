//! Tape-style wobble: a modulated delay (wow = slow sine, flutter = smoothed noise).
//!
//! The delay is centred on `CENTRE_SECONDS` so the modulation can swing both ways; that centre
//! is a constant latency the plugin reports.

use crate::delay::DelayLine;
use crate::util::Noise;

/// Centre delay of the wobble line.
pub const CENTRE_SECONDS: f64 = 0.008;
/// Largest swing either way, in seconds.
pub const MAX_EXCURSION_SECONDS: f32 = 0.007;

/// Peak delay swing (milliseconds) that gives a pitch deviation of `cents` at LFO `rate_hz`,
/// limited to what the delay line can hold.
///
/// A sine delay modulation `e(t) = E sin(2π f t)` bends the pitch by `E·2πf` (as a ratio).
pub fn excursion_ms(cents: f32, rate_hz: f32) -> f32 {
    let eps = 2f32.powf(cents / 1200.0) - 1.0;
    (eps / (2.0 * std::f32::consts::PI * rate_hz.max(0.01)) * 1000.0).min(MAX_EXCURSION_SECONDS * 1000.0)
}

/// Stereo wobble.
#[derive(Debug, Clone)]
pub struct Wobble {
    l: DelayLine,
    r: DelayLine,
    centre: f32,
    sr: f32,
    phase: f32,
    noise_l: Noise,
    noise_r: Noise,
    flut_l: f32,
    flut_r: f32,
    flut_coef: f32,
}

impl Wobble {
    /// Allocate for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let centre = (CENTRE_SECONDS * sample_rate).round() as usize;
        let n = centre * 2 + 8;
        Self {
            l: DelayLine::new(n),
            r: DelayLine::new(n),
            centre: centre as f32,
            sr: sample_rate as f32,
            phase: 0.0,
            noise_l: Noise::new(0x1234_5678),
            noise_r: Noise::new(0x8765_4321),
            flut_l: 0.0,
            flut_r: 0.0,
            flut_coef: 1.0 - (-2.0 * std::f32::consts::PI * 9.0 / sample_rate as f32).exp(),
        }
    }

    /// Constant latency in samples.
    pub fn latency(&self) -> usize {
        self.centre as usize
    }

    /// Clear state.
    pub fn reset(&mut self) {
        self.l.clear();
        self.r.clear();
        self.phase = 0.0;
        self.flut_l = 0.0;
        self.flut_r = 0.0;
        self.noise_l = Noise::new(0x1234_5678);
        self.noise_r = Noise::new(0x8765_4321);
    }

    /// One stereo frame. `exc_ms`: wow swing (ms), `rate_hz`: wow rate, `flutter`: 0..1.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32, exc_ms: f32, rate_hz: f32, flutter: f32) -> (f32, f32) {
        self.l.push(l);
        self.r.push(r);
        self.phase += rate_hz / self.sr;
        self.phase -= self.phase.floor();
        let tau = 2.0 * std::f32::consts::PI;
        let wow_l = (tau * self.phase).sin();
        let wow_r = (tau * (self.phase + 0.19)).sin();
        self.flut_l += (self.noise_l.sample() - self.flut_l) * self.flut_coef;
        self.flut_r += (self.noise_r.sample() - self.flut_r) * self.flut_coef;
        let ms_to_smp = self.sr * 0.001;
        let fl = flutter * 0.18 * ms_to_smp * 4.0;
        let ex = exc_ms * ms_to_smp;
        let max = MAX_EXCURSION_SECONDS * self.sr;
        let dl = (self.centre + (ex * wow_l + fl * self.flut_l).clamp(-max, max)).max(1.0);
        let dr = (self.centre + (ex * wow_r + fl * self.flut_r).clamp(-max, max)).max(1.0);
        (self.l.tap_hermite(dl), self.r.tap_hermite(dr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excursion_matches_the_pitch_deviation_formula() {
        let e = excursion_ms(20.0, 1.0);
        let eps = 2f32.powf(20.0 / 1200.0) - 1.0;
        assert!((e - eps / (2.0 * std::f32::consts::PI) * 1000.0).abs() < 1e-4);
        assert_eq!(excursion_ms(100.0, 0.05), 7.0);
    }

    #[test]
    fn zero_depth_is_a_pure_delay_of_the_centre() {
        let mut w = Wobble::new(48_000.0);
        let lat = w.latency();
        let x: Vec<f32> = (0..3000).map(|i| ((i * 13 % 89) as f32 - 44.0) / 50.0).collect();
        let y: Vec<f32> = x.iter().map(|&v| w.tick(v, v, 0.0, 1.0, 0.0).0).collect();
        for i in lat..x.len() {
            assert_eq!(y[i], x[i - lat]);
        }
    }

    #[test]
    fn wobble_bends_the_pitch_by_about_the_requested_cents() {
        // 1 kHz through a 2 Hz wow of 30 cents: the instantaneous pitch swings +-30 cents.
        let sr = 48_000.0f64;
        let mut w = Wobble::new(sr);
        let exc = excursion_ms(30.0, 2.0);
        let n = 96_000;
        let y: Vec<f32> = (0..n)
            .map(|i| {
                let x = (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr as f32).sin();
                w.tick(x, x, exc, 2.0, 0.0).0
            })
            .collect();
        // instantaneous frequency from interpolated upward zero crossings, 5-cycle averages
        let mut times = vec![];
        for i in 24_000..n - 1 {
            if y[i] <= 0.0 && y[i + 1] > 0.0 {
                times.push(i as f32 + (-y[i]) / (y[i + 1] - y[i]));
            }
        }
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for k in 0..times.len() - 5 {
            let f = 5.0 * sr as f32 / (times[k + 5] - times[k]);
            lo = lo.min(f);
            hi = hi.max(f);
        }
        let cents = |f: f32| 1200.0 * (f / 1000.0).log2();
        assert!(cents(hi) > 20.0 && cents(lo) < -20.0, "{} {}", cents(lo), cents(hi));
        assert!(cents(hi) < 40.0 && cents(lo) > -40.0, "{} {}", cents(lo), cents(hi));
    }
}
