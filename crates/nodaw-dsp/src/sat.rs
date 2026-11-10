//! Analog-style saturation: three curves (Tape, Tube, Diode) run at 2x oversampling through a
//! linear-phase half-band FIR pair, with a static auto-gain so Drive changes colour, not level.
//!
//! Latency: exactly [`Oversampler2x::LATENCY`] samples at the base rate, whatever the settings.

/// Number of taps of each half-band filter (odd; `(TAPS-1)` divisible by 4 keeps the latency
/// a whole number of base-rate samples).
const TAPS: usize = 49;

fn kaiser_sinc_halfband() -> [f32; TAPS] {
    fn i0(x: f64) -> f64 {
        let (mut s, mut t) = (1.0f64, 1.0f64);
        for k in 1..40 {
            let h = x / (2.0 * k as f64);
            t *= h * h;
            s += t;
        }
        s
    }
    let beta = 8.0;
    let mid = (TAPS - 1) as f64 / 2.0;
    let mut h = [0.0f32; TAPS];
    let mut sum = 0.0f64;
    for (i, c) in h.iter_mut().enumerate() {
        let d = i as f64 - mid;
        let sinc = if d == 0.0 { 1.0 } else { (std::f64::consts::PI * d * 0.5).sin() / (std::f64::consts::PI * d * 0.5) };
        let r = d / mid;
        let w = i0(beta * (1.0 - r * r).max(0.0).sqrt()) / i0(beta);
        let v = 0.5 * sinc * w;
        *c = v as f32;
        sum += v;
    }
    for c in h.iter_mut() {
        *c = (*c as f64 / sum) as f32;
    }
    h
}

/// A 2x up/down sampler for one channel. No allocation after construction.
#[derive(Debug, Clone)]
pub struct Oversampler2x {
    h: [f32; TAPS],
    up: [f32; TAPS],
    up_pos: usize,
    dn: [f32; TAPS],
    dn_pos: usize,
}

impl Default for Oversampler2x {
    fn default() -> Self {
        Self::new()
    }
}

impl Oversampler2x {
    /// Base-rate latency of an up + down pass.
    pub const LATENCY: usize = (TAPS - 1) / 2;

    /// A new, silent oversampler.
    pub fn new() -> Self {
        Self { h: kaiser_sinc_halfband(), up: [0.0; TAPS], up_pos: 0, dn: [0.0; TAPS], dn_pos: 0 }
    }
    /// Clear the history.
    pub fn reset(&mut self) {
        self.up = [0.0; TAPS];
        self.dn = [0.0; TAPS];
    }
    /// One base-rate sample in, two high-rate samples out (the even one first).
    #[inline]
    pub fn up(&mut self, x: f32) -> [f32; 2] {
        // Polyphase form of "zero-stuff, then filter": both outputs read the base-rate
        // history x[n-k], the even one through the even taps, the odd one through the odd.
        self.up_pos = (self.up_pos + TAPS - 1) % TAPS;
        self.up[self.up_pos] = x;
        let (mut ye, mut yo) = (0.0f32, 0.0f32);
        for k in 0..TAPS.div_ceil(2) {
            let v = self.up[(self.up_pos + k) % TAPS];
            ye += self.h[2 * k] * v;
            if 2 * k + 1 < TAPS {
                yo += self.h[2 * k + 1] * v;
            }
        }
        [2.0 * ye, 2.0 * yo]
    }
    /// Two high-rate samples in (even, odd), one base-rate sample out.
    #[inline]
    pub fn down(&mut self, s: [f32; 2]) -> f32 {
        // The output is the filtered signal at the even position, so the total delay is a
        // whole number of base-rate samples.
        self.dn_pos = (self.dn_pos + TAPS - 1) % TAPS;
        self.dn[self.dn_pos] = s[0];
        let mut acc = 0.0f32;
        for k in 0..TAPS {
            acc += self.h[k] * self.dn[(self.dn_pos + k) % TAPS];
        }
        self.dn_pos = (self.dn_pos + TAPS - 1) % TAPS;
        self.dn[self.dn_pos] = s[1];
        acc
    }
}

/// The saturation curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
    /// Soft, symmetric, gentle top-end rounding.
    Tape,
    /// Asymmetric (even harmonics), soft knee.
    Tube,
    /// Hard-ish, bright, exponential knee.
    Diode,
}

impl Curve {
    /// From a choice index.
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Curve::Tube,
            2 => Curve::Diode,
            _ => Curve::Tape,
        }
    }
    /// The static curve. Bounded for any finite input.
    #[inline]
    pub fn shape(self, x: f32) -> f32 {
        match self {
            Curve::Tape => {
                // tanh-like rational approximation, smooth and cheap
                let x = x.clamp(-4.5, 4.5);
                let x2 = x * x;
                x * (27.0 + x2) / (27.0 + 9.0 * x2)
            }
            Curve::Tube => {
                // Asymmetric: the positive half compresses later than the negative one.
                let y = if x >= 0.0 { 1.0 - (-x * 1.15).exp() } else { -(1.0 - (x * 0.85).exp()) / 0.85 * 0.85 };
                y.clamp(-1.25, 1.0)
            }
            Curve::Diode => {
                let a = x.abs();
                let y = if a < 0.6 { a } else { 0.6 + 0.4 * (1.0 - (-(a - 0.6) * 2.5).exp()) };
                y.copysign(x)
            }
        }
    }
    /// The small-signal gain the auto-gain divides out at a given drive.
    pub fn makeup(self, drive: f32) -> f32 {
        // Level of a -12 dBFS sine's peak through the curve, relative to its input.
        let x = 0.25f32;
        let y = self.shape(x * drive).abs().max(1e-6);
        x / y
    }
}

/// A stereo saturator: drive, curve, tone tilt, auto-gain, mix. 2x oversampled.
#[derive(Debug, Clone)]
pub struct Saturator {
    os: [Oversampler2x; 2],
    dry: [crate::delay::DelayLine; 2],
    dc: [f32; 2],
    dc_x: [f32; 2],
}

impl Default for Saturator {
    fn default() -> Self {
        Self::new()
    }
}

impl Saturator {
    /// A new saturator.
    pub fn new() -> Self {
        let lat = Oversampler2x::LATENCY;
        Self { os: [Oversampler2x::new(), Oversampler2x::new()], dry: [crate::delay::DelayLine::new(lat + 4), crate::delay::DelayLine::new(lat + 4)], dc: [0.0; 2], dc_x: [0.0; 2] }
    }
    /// Latency in samples (constant).
    pub fn latency(&self) -> usize {
        Oversampler2x::LATENCY
    }
    /// Clear the state.
    pub fn reset(&mut self) {
        for o in &mut self.os {
            o.reset();
        }
        for d in &mut self.dry {
            d.clear();
        }
        self.dc = [0.0; 2];
        self.dc_x = [0.0; 2];
    }
    /// One stereo frame. `drive` is linear (1 = 0 dB), `makeup` linear, `mix` 0..1.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32, curve: Curve, drive: f32, makeup: f32, mix: f32) -> (f32, f32) {
        let mut out = [0.0f32; 2];
        for (ch, x) in [l, r].into_iter().enumerate() {
            self.dry[ch].push(x);
            let d = self.dry[ch].tap_int(Oversampler2x::LATENCY);
            let u = self.os[ch].up(x * drive);
            let s = [curve.shape(u[0]), curve.shape(u[1])];
            let y = self.os[ch].down(s) * makeup;
            // DC blocker (the tube curve is asymmetric): one pole at ~8 Hz.
            let hp = y - self.dc_x[ch] + 0.999 * self.dc[ch];
            self.dc_x[ch] = y;
            self.dc[ch] = crate::util::undenorm(hp);
            out[ch] = d + (hp - d) * mix;
        }
        (out[0], out[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oversampler_is_a_pure_delay_of_its_latency() {
        let mut o = Oversampler2x::new();
        let n = 400;
        let x: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let y: Vec<f32> = x.iter().map(|&v| { let u = o.up(v); o.down(u) }).collect();
        let lat = Oversampler2x::LATENCY;
        let worst = (lat + 50..n).map(|i| (y[i] - x[i - lat]).abs()).fold(0.0f32, f32::max);
        assert!(worst < 2e-3, "{worst}");
    }

    #[test]
    fn curves_are_bounded_odd_ish_and_monotonic() {
        for c in [Curve::Tape, Curve::Tube, Curve::Diode] {
            let mut prev = f32::MIN;
            for i in -2000..=2000 {
                let x = i as f32 / 100.0;
                let y = c.shape(x);
                assert!(y.is_finite() && y.abs() <= 1.3, "{c:?} {x} {y}");
                assert!(y >= prev - 1e-6, "{c:?} not monotonic at {x}");
                prev = y;
            }
            assert!((c.shape(0.01) / 0.01 - 1.0).abs() < 0.2, "{c:?} small-signal gain");
            assert!(c.makeup(8.0) < 1.0);
        }
    }

    #[test]
    fn mix_zero_is_the_delayed_input_and_drive_adds_harmonics() {
        let sr = 48_000.0;
        let mut s = Saturator::new();
        let n = 4800;
        let x: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin() * 0.5).collect();
        let y: Vec<f32> = x.iter().map(|&v| s.tick(v, v, Curve::Tape, 4.0, 1.0, 0.0).0).collect();
        let lat = s.latency();
        for i in lat..n {
            assert!((y[i] - x[i - lat]).abs() < 1e-6);
        }
        let mut s2 = Saturator::new();
        let y2: Vec<f32> = x.iter().map(|&v| s2.tick(v, v, Curve::Diode, 8.0, Curve::Diode.makeup(8.0), 1.0).0).collect();
        assert!(y2.iter().all(|v| v.is_finite() && v.abs() < 2.0));
        let peak = y2[2000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.2, "{peak}");
    }
}
