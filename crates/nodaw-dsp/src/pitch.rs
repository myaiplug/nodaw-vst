//! The "screw": a SOLA (synchronous overlap-add) varispeed pitch shifter.
//!
//! A single read pointer travels through a ring buffer of the input at `ratio` samples per
//! sample, so every tone is exactly transposed. Reading slower (or faster) than real time makes
//! the pointer drift away from the write head; when the gap leaves its allowed band the pointer
//! *jumps* back into it. The jump target is chosen by normalised cross-correlation so the
//! waveform on both sides of the splice lines up, and the two read points are cross-faded over
//! a few milliseconds. That is what keeps tonal material free of the comb and flutter that
//! fixed two-tap shifters produce.
//!
//! Latency: the gap (the "lag") is in a band, so the true delay swings inside it. The plugin
//! reports its middle, [`PitchShifter::latency`]. At (almost) unity the shifter cross-fades to
//! a plain delay of exactly that length, so the host's delay compensation is exact there.

use crate::util::{next_pow2, Smooth};

/// A stereo SOLA pitch shifter. Supports ratios 0.5 ..= 2.0 (one octave each way).
#[derive(Debug, Clone)]
pub struct PitchShifter {
    l: Vec<f32>,
    r: Vec<f32>,
    mono: Vec<f32>,
    mask: usize,
    /// Number of samples written so far; the newest sample is at `n - 1`.
    n: u64,
    rp: f64,
    rp2: f64,
    xf: usize,
    xf_len: usize,
    corr_len: usize,
    search: usize,
    lag_floor: usize,
    lag_max: usize,
    lag_mean: usize,
    scratch: Vec<f32>,
    active: Smooth,
}

impl PitchShifter {
    /// Allocate for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let ms = |t: f64| ((t * 0.001 * sample_rate).round() as usize).max(8);
        let corr_len = ms(5.33);
        let xf_len = ms(8.0);
        let search = ms(7.5);
        let lag_floor = corr_len + xf_len + 16;
        let lag_max = lag_floor + 2 * search + ms(10.0);
        let lag_mean = (lag_floor + lag_max) / 2;
        let size = next_pow2(lag_max + corr_len + 2 * search + xf_len + 64);
        let mut s = Self {
            l: vec![0.0; size],
            r: vec![0.0; size],
            mono: vec![0.0; size],
            mask: size - 1,
            n: 0,
            rp: 0.0,
            rp2: 0.0,
            xf: 0,
            xf_len,
            corr_len,
            search,
            lag_floor,
            lag_max,
            lag_mean,
            scratch: vec![0.0; corr_len],
            active: Smooth::new(0.0, sample_rate, 15.0, 15.0),
        };
        s.reset();
        s
    }

    /// The latency reported to the host, in samples: the middle of the lag band.
    pub fn latency(&self) -> usize {
        self.lag_mean
    }

    /// The current gap between the read pointer and the newest sample, in samples (a test hook).
    pub fn lag(&self) -> f64 {
        (self.n as f64 - 1.0) - self.rp
    }

    /// The band the lag stays in.
    pub fn lag_band(&self) -> (usize, usize) {
        (self.lag_floor, self.lag_max)
    }

    /// Clear the buffers and return to the resting state.
    pub fn reset(&mut self) {
        self.l.fill(0.0);
        self.r.fill(0.0);
        self.mono.fill(0.0);
        // Start with enough history that the very first reads are of silence, not of the
        // future: pretend `lag_max + margin` samples of silence were written.
        self.n = (self.lag_max + self.corr_len + 2 * self.search + self.xf_len + 32) as u64;
        self.rp = (self.n - 1) as f64 - self.lag_mean as f64;
        self.rp2 = self.rp;
        self.xf = 0;
        self.active.reset(0.0);
    }

    #[inline]
    fn at(&self, buf: &[f32], i: i64) -> f32 {
        buf[(i as usize) & self.mask]
    }

    #[inline]
    fn read(&self, buf: &[f32], pos: f64) -> f32 {
        let i = pos.floor() as i64;
        let f = (pos - i as f64) as f32;
        let (ym1, y0, y1, y2) = (
            self.at(buf, i - 1),
            self.at(buf, i),
            self.at(buf, i + 1),
            self.at(buf, i + 2),
        );
        let c1 = 0.5 * (y1 - ym1);
        let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
        let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
        ((c3 * f + c2) * f + c1) * f + y0
    }

    /// Choose where the read pointer should land: the lag nearest `nominal` ± `search` whose
    /// following `corr_len` samples best match what the current pointer is about to play.
    fn best_jump(&mut self, nominal: usize) -> usize {
        let newest = self.n as i64 - 1;
        let start = self.rp.floor() as i64;
        for k in 0..self.corr_len {
            self.scratch[k] = self.at(&self.mono, start + k as i64);
        }
        let lo = nominal.saturating_sub(self.search).max(self.lag_floor);
        let hi = (nominal + self.search).min(self.lag_max);
        let score = |this: &Self, lag: usize| -> f32 {
            let b0 = newest - lag as i64;
            let (mut dot, mut en) = (0.0f32, 1.0e-6f32);
            for k in 0..this.corr_len {
                let b = this.at(&this.mono, b0 + k as i64);
                dot += this.scratch[k] * b;
                en += b * b;
            }
            dot / en.sqrt()
        };
        let mut best = nominal.clamp(lo, hi);
        let mut best_score = score(self, best);
        for lag in lo..=hi {
            let s = score(self, lag);
            if s > best_score {
                best_score = s;
                best = lag;
            }
        }
        best
    }

    /// Process one stereo frame at pitch `ratio` (clamped to 0.5 ..= 2.0).
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32, ratio: f32) -> (f32, f32) {
        let ratio = ratio.clamp(0.5, 2.0) as f64;
        let w = (self.n as usize) & self.mask;
        self.l[w] = l;
        self.r[w] = r;
        self.mono[w] = 0.5 * (l + r);
        self.n += 1;

        if self.xf == 0 {
            let lag = self.lag();
            let down = lag > self.lag_max as f64;
            let up = lag < self.lag_floor as f64;
            if down || up {
                let nominal = if down {
                    self.lag_floor + self.search
                } else {
                    self.lag_max - self.search
                };
                let best = self.best_jump(nominal);
                self.rp2 = (self.n as f64 - 1.0) - best as f64 + (self.rp - self.rp.floor());
                self.xf = 1;
            }
        }

        let (mut yl, mut yr) = (self.read(&self.l, self.rp), self.read(&self.r, self.rp));
        self.rp += ratio;
        if self.xf > 0 {
            let g = 0.5 - 0.5 * (std::f32::consts::PI * self.xf as f32 / self.xf_len as f32).cos();
            let (nl, nr) = (self.read(&self.l, self.rp2), self.read(&self.r, self.rp2));
            yl += (nl - yl) * g;
            yr += (nr - yr) * g;
            self.rp2 += ratio;
            self.xf += 1;
            if self.xf > self.xf_len {
                self.xf = 0;
                self.rp = self.rp2;
            }
        }

        let dev = (ratio as f32 - 1.0).abs();
        let a = self.active.tick((dev / 0.004).min(1.0));
        let newest = self.n as i64 - 1;
        let straight = newest - self.lag_mean as i64;
        let (sl, sr) = (self.at(&self.l, straight), self.at(&self.r, straight));
        if a == 0.0 {
            (sl, sr)
        } else {
            (sl + a * (yl - sl), sr + a * (yr - sr))
        }
    }
}

/// Semitones to a frequency ratio.
#[inline]
pub fn semitones_to_ratio(st: f32) -> f32 {
    2f32.powf(st / 12.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::SpectrumAnalyzer;

    fn run(sr: f64, hz: f32, st: f32, secs: f32) -> Vec<f32> {
        let mut p = PitchShifter::new(sr);
        let n = (sr as f32 * secs) as usize;
        let ratio = semitones_to_ratio(st);
        (0..n)
            .map(|i| {
                let x = 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / sr as f32).sin();
                p.tick(x, x, ratio).0
            })
            .collect()
    }

    fn analyze(x: &[f32]) -> SpectrumAnalyzer {
        let mut an = SpectrumAnalyzer::new(16384);
        an.analyze(&x[x.len() - 16384..]);
        an
    }

    #[test]
    fn tones_land_on_the_exact_transposed_frequency() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            for hz in [110.0f32, 220.0, 440.0, 1000.0] {
                for st in [-12.0f32, -7.0, -3.0, -1.0, 2.0, 5.0, 12.0] {
                    let y = run(sr, hz, st, 1.6);
                    let want = hz * semitones_to_ratio(st);
                    let got = analyze(&y).peak_frequency(sr);
                    assert!(
                        (got / want - 1.0).abs() < 0.01,
                        "sr {sr} {hz} Hz {st} st: got {got} want {want}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_shifted_tone_is_clean() {
        // 85 % or more of the power must sit within a few bins of the peak: a SOLA splice
        // that is well aligned adds very little else.
        for st in [-7.0f32, -3.0, 4.0] {
            let y = run(48_000.0, 440.0, st, 1.6);
            let an = analyze(&y);
            let (k, _) =
                an.db.iter().enumerate().fold(
                    (0, f32::MIN),
                    |a, (i, v)| if *v > a.1 { (i, *v) } else { a },
                );
            let p = |db: f32| 10f64.powf(db as f64 / 10.0);
            let total: f64 = an.db.iter().map(|d| p(*d)).sum();
            let near: f64 = an.db[k - 4..=k + 4].iter().map(|d| p(*d)).sum();
            assert!(near / total > 0.85, "{st} st: {}", near / total);
        }
    }

    #[test]
    fn unity_is_a_pure_delay_of_exactly_the_reported_latency() {
        let mut p = PitchShifter::new(48_000.0);
        let lat = p.latency();
        let x: Vec<f32> = (0..8000)
            .map(|i| ((i * 37 % 101) as f32 - 50.0) / 60.0)
            .collect();
        let y: Vec<f32> = x.iter().map(|&v| p.tick(v, v, 1.0).0).collect();
        for i in lat..x.len() {
            assert_eq!(y[i], x[i - lat], "sample {i}");
        }
    }

    #[test]
    fn level_is_kept() {
        for st in [-12.0f32, -5.0, 7.0] {
            let y = run(48_000.0, 220.0, st, 2.0);
            let rms =
                |s: &[f32]| (s.iter().map(|v| (v * v) as f64).sum::<f64>() / s.len() as f64).sqrt();
            let r = rms(&y[48_000..]) / (0.5 / 2f64.sqrt());
            assert!((0.9..1.1).contains(&r), "{st} st: {r}");
        }
    }

    #[test]
    fn the_lag_stays_inside_its_band_and_the_output_stays_finite() {
        let mut p = PitchShifter::new(44_100.0);
        let (lo, hi) = p.lag_band();
        let mut ratio = 1.0f32;
        let mut noise = crate::util::Noise::new(5);
        for i in 0..400_000 {
            if i % 9973 == 0 {
                ratio = [0.5, 0.7, 1.0, 1.3, 2.0, 0.9, 1.01][(i / 9973) % 7];
            }
            let x = 0.6 * ((i as f32) * 0.05).sin() + 0.2 * noise.sample();
            let (a, b) = p.tick(x, -x, ratio);
            assert!(
                a.is_finite() && b.is_finite() && a.abs() < 2.0,
                "sample {i}"
            );
            let lag = p.lag();
            assert!(
                lag > lo as f64 - 400.0 && lag < hi as f64 + 400.0,
                "lag {lag} left [{lo},{hi}] at {i}"
            );
        }
    }

    #[test]
    fn it_runs_at_all_sample_rates() {
        for sr in [22_050.0, 32_000.0, 192_000.0] {
            let y = run(sr, 330.0, -4.0, 0.8);
            assert!(y.iter().all(|v| v.is_finite()));
            assert!(y[y.len() / 2..].iter().any(|v| v.abs() > 0.1));
        }
    }

    #[test]
    fn silence_in_silence_out() {
        let mut p = PitchShifter::new(48_000.0);
        for _ in 0..100_000 {
            let (a, b) = p.tick(0.0, 0.0, 0.8);
            assert_eq!((a, b), (0.0, 0.0));
        }
    }
}
