//! A tempo-synced time warp (the "Time Shift" of Temporal Pitch Portal). A plugin cannot make
//! the host's timeline longer or shorter, so time is warped *inside* each sync period: at 50 %
//! the first half of every period is played over the whole period (half-time), at 200 % the
//! previous period is played twice as fast (double-time). Between, any speed.
//!
//! Two characters: **Tape** reads with one pointer, so pitch follows speed like a varispeed
//! deck; **Stretch** reads overlapping Hann grains at normal speed, so pitch stays put.
//!
//! Everything runs `GRAIN` samples behind the input (the plugin reports it as latency), which
//! lets a grain read forward without ever reading the future. At 100 % both characters
//! reproduce the input exactly, delayed by [`TimeWarp::latency`] (tested).

use crate::util::next_pow2;

#[derive(Debug, Clone, Copy, Default)]
struct Grain {
    start: f64,
    t: usize,
    on: bool,
}

/// The time warp (stereo).
#[derive(Debug, Clone)]
pub struct TimeWarp {
    buf: [Vec<f32>; 2],
    mask: usize,
    w: u64,
    g: usize,
    acc: f64,
    base: f64,
    max_period: usize,
    // tape: the previous pointer, faded out after a jump
    old: f64,
    xf: usize,
    xf_len: usize,
    grains: [Grain; 2],
    hop_pos: usize,
    win: Vec<f32>,
}

impl TimeWarp {
    /// Longest sync period, in seconds (slower tempos are clamped to it).
    pub const MAX_PERIOD_SECONDS: f64 = 4.0;

    /// A warp for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let g = (((0.043 * sample_rate) as usize) / 2 * 2).max(64);
        let max_period = (Self::MAX_PERIOD_SECONDS * sample_rate) as usize;
        let size = next_pow2(2 * max_period + 4 * g + 64);
        let win = (0..g)
            .map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / g as f64).cos()) as f32)
            .collect();
        let mut s = Self {
            buf: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            w: 0,
            g,
            acc: 0.0,
            base: 0.0,
            max_period,
            old: 0.0,
            xf: 0,
            xf_len: ((0.006 * sample_rate) as usize).max(8),
            grains: [Grain::default(); 2],
            hop_pos: 0,
            win,
        };
        s.reset();
        s
    }
    /// Latency in samples (one grain).
    pub fn latency(&self) -> usize {
        self.g
    }
    /// The longest period in samples.
    pub fn max_period(&self) -> usize {
        self.max_period
    }
    /// Clear.
    pub fn reset(&mut self) {
        for b in &mut self.buf {
            b.fill(0.0);
        }
        // pretend a long silence was written, so every read is of the past
        self.w = (2 * self.max_period + 2 * self.g) as u64;
        self.base = self.now();
        self.acc = 0.0;
        self.old = self.base;
        self.xf = 0;
        self.grains = [Grain::default(); 2];
        self.hop_pos = 0;
    }
    /// The "virtual now": the newest sample minus the latency.
    #[inline]
    fn now(&self) -> f64 {
        (self.w - 1) as f64 - self.g as f64
    }
    #[inline]
    fn at(&self, ch: usize, i: i64) -> f32 {
        self.buf[ch][(i as usize) & self.mask]
    }
    #[inline]
    fn read(&self, ch: usize, pos: f64) -> f32 {
        let i = pos.floor() as i64;
        let f = (pos - i as f64) as f32;
        if f == 0.0 {
            return self.at(ch, i);
        }
        let (ym1, y0, y1, y2) = (
            self.at(ch, i - 1),
            self.at(ch, i),
            self.at(ch, i + 1),
            self.at(ch, i + 2),
        );
        let c1 = 0.5 * (y1 - ym1);
        let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
        let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
        ((c3 * f + c2) * f + c1) * f + y0
    }

    /// The grain start near `nominal` whose next samples best match what the playing grain
    /// would read next (`cont`). Never later than the newest sample allows for a whole grain.
    fn align(&self, nominal: f64, cont: f64) -> f64 {
        if (cont - nominal).abs() < 0.5 {
            return nominal;
        }
        let latest = self.w as f64 - 1.0 - self.g as f64;
        let search = (self.g / 4) as i64;
        let len = (self.g / 4) as i64;
        let c0 = cont.round() as i64;
        let n0 = nominal.round() as i64;
        let mono = |i: i64| self.at(0, i) + self.at(1, i);
        let score = |start: i64| {
            let (mut dot, mut en) = (0.0f32, 1.0e-9f32);
            let mut k = 0;
            while k < len {
                let b = mono(start + k);
                dot += mono(c0 + k) * b;
                en += b * b;
                k += 2;
            }
            dot / en.sqrt()
        };
        let mut best = 0i64;
        let mut best_s = score(n0);
        let mut d = -search;
        while d <= search {
            if d != 0 && ((n0 + d) as f64) <= latest {
                let sc = score(n0 + d);
                if sc > best_s {
                    best_s = sc;
                    best = d;
                }
            }
            d += 1;
        }
        nominal + best as f64
    }

    /// One stereo frame. `rate` 0.5 ..= 2; `new_period`: this sample starts a sync period of
    /// `period` samples; `stretch`: keep the pitch.
    #[inline]
    pub fn tick(
        &mut self,
        l: f32,
        r: f32,
        rate: f32,
        stretch: bool,
        new_period: bool,
        period: usize,
    ) -> (f32, f32) {
        let wi = (self.w as usize) & self.mask;
        self.buf[0][wi] = if l.is_finite() { l } else { 0.0 };
        self.buf[1][wi] = if r.is_finite() { r } else { 0.0 };
        self.w += 1;
        let rate = rate.clamp(0.5, 2.0) as f64;
        let period = period.clamp(16, self.max_period) as f64;
        let now = self.now();
        if new_period {
            let prev = self.base + self.acc;
            self.base = if rate <= 1.0 { now } else { now - period };
            self.acc = 0.0;
            if (prev - self.base).abs() > 0.5 {
                self.old = prev;
                self.xf = 1;
            }
        }
        let oldest = self.w as f64 - (self.mask as f64) + 8.0;
        let pos = (self.base + self.acc).min(now).max(oldest);
        self.acc += rate;

        if !stretch {
            let (mut a, mut b) = (self.read(0, pos), self.read(1, pos));
            if self.xf > 0 {
                let t = self.xf as f32 / self.xf_len as f32;
                let g = 0.5 + 0.5 * (std::f32::consts::PI * t).cos(); // old fades 1 -> 0
                let op = self.old.min(now);
                a = a + (self.read(0, op) - a) * g;
                b = b + (self.read(1, op) - b) * g;
                self.old += rate;
                self.xf += 1;
                if self.xf > self.xf_len {
                    self.xf = 0;
                }
            }
            return (a, b);
        }
        // Stretch: a grain every half grain, each read at normal speed from where the warp
        // pointer was when it started. Hann at 50 % overlap sums to exactly 1.
        if self.hop_pos == 0 {
            let slot = if self.grains[0].on && self.grains[0].t < self.g / 2 {
                1
            } else if !self.grains[0].on || self.grains[0].t >= self.g {
                0
            } else {
                1
            };
            // WSOLA: start the new grain where the waveform best continues the one already
            // playing (searched around the warp pointer), so the overlap adds in phase.
            let other = self.grains[1 - slot];
            let start = if other.on {
                self.align(pos, other.start + other.t as f64)
            } else {
                pos
            };
            self.grains[slot] = Grain {
                start,
                t: 0,
                on: true,
            };
        }
        self.hop_pos = (self.hop_pos + 1) % (self.g / 2);
        let (mut a, mut b) = (0.0f32, 0.0f32);
        for k in 0..2 {
            let gr = self.grains[k];
            if !gr.on {
                continue;
            }
            let p = (gr.start + gr.t as f64).min(self.w as f64 - 1.0);
            let wv = self.win[gr.t];
            a += wv * self.read(0, p);
            b += wv * self.read(1, p);
            let t = gr.t + 1;
            self.grains[k].t = t;
            if t >= self.g {
                self.grains[k].on = false;
            }
        }
        (a, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(i: usize) -> f32 {
        0.4 * (i as f32 * 0.021).sin() + 0.2 * (i as f32 * 0.17).sin()
    }

    #[test]
    fn at_unity_both_characters_are_a_pure_delay() {
        for stretch in [false, true] {
            let mut t = TimeWarp::new(48_000.0);
            let lat = t.latency();
            let n = 30_000;
            let y: Vec<f32> = (0..n)
                .map(|i| t.tick(sig(i), sig(i), 1.0, stretch, i % 6000 == 0, 6000).0)
                .collect();
            let worst = (lat + t.g + 10..n)
                .map(|i| (y[i] - sig(i - lat)).abs())
                .fold(0.0f32, f32::max);
            assert!(worst < 1e-4, "stretch {stretch}: {worst}");
        }
    }

    #[test]
    fn half_time_repeats_the_first_half_of_each_period_slowly() {
        // Tape at 50 %: the period's output is its first half at half speed, so a ramp input
        // gives a ramp of half the slope that restarts every period.
        let mut t = TimeWarp::new(48_000.0);
        let lat = t.latency();
        let p = 4800;
        let n = 6 * p;
        let y: Vec<f32> = (0..n)
            .map(|i| t.tick(i as f32, i as f32, 0.5, false, i % p == 0, p).0)
            .collect();
        // inside the 4th period, away from the crossfade
        let a = y[3 * p + 1000];
        let b = y[3 * p + 3000];
        assert!(((b - a) - 1000.0).abs() < 2.0, "slope {}", (b - a) / 2000.0);
        // it starts where the (delayed) period started
        assert!(
            (y[3 * p + 1000] - ((3 * p) as f32 - lat as f32 - 1.0 + 500.0)).abs() < 3.0,
            "{}",
            y[3 * p + 1000]
        );
    }

    #[test]
    fn double_time_never_reads_the_future_and_everything_stays_finite() {
        for stretch in [false, true] {
            for rate in [0.5f32, 0.73, 1.37, 2.0] {
                let mut t = TimeWarp::new(44_100.0);
                let p = 5000;
                for i in 0..40_000 {
                    let (a, b) = t.tick(sig(i), -sig(i), rate, stretch, i % p == 0, p);
                    assert!(
                        a.is_finite() && b.is_finite() && a.abs() < 1.5,
                        "{stretch} {rate} {i}"
                    );
                }
            }
        }
        // a ramp at 2x in tape mode: output never exceeds the delayed input (no future reads)
        let mut t = TimeWarp::new(48_000.0);
        let lat = t.latency() as f32;
        for i in 0..30_000usize {
            let y = t.tick(i as f32, 0.0, 2.0, false, i % 3000 == 0, 3000).0;
            assert!(y <= (i as f32 - lat + 1.0).max(0.0), "{i}: {y}");
        }
    }
}
