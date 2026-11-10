//! Time-based effects: a stereo flanger, a feedback (optionally ping-pong) delay with a damped
//! feedback path, the stereo tools (width, Haas, pan, M/S) and a correlation meter.
//! Every buffer is allocated in `new`; `tick` never allocates.

use crate::delay::DelayLine;
use crate::filter::{Biquad, Coeffs};
use crate::util::undenorm;

/// Stereo flanger: two modulated short delays in quadrature with feedback.
#[derive(Debug, Clone)]
pub struct Flanger {
    sr: f64,
    lines: [DelayLine; 2],
    phase: f64,
    fb: [f32; 2],
}

impl Flanger {
    /// Longest sweep in seconds.
    pub const MAX_SECONDS: f64 = 0.012;

    /// A flanger for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let n = (Self::MAX_SECONDS * sample_rate) as usize + 8;
        Self {
            sr: sample_rate,
            lines: [DelayLine::new(n), DelayLine::new(n)],
            phase: 0.0,
            fb: [0.0; 2],
        }
    }
    /// Clear the state.
    pub fn reset(&mut self) {
        for l in &mut self.lines {
            l.clear();
        }
        self.phase = 0.0;
        self.fb = [0.0; 2];
    }
    /// One frame of the *wet* signal. `rate_hz` LFO rate, `depth` 0..1, `feedback` -0.95..0.95.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32, rate_hz: f32, depth: f32, feedback: f32) -> (f32, f32) {
        self.phase += rate_hz as f64 / self.sr;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let base = 0.0006 * self.sr as f32;
        let span = (0.0085 * self.sr as f32) * depth.clamp(0.0, 1.0);
        let fb = feedback.clamp(-0.95, 0.95);
        let mut out = [0.0f32; 2];
        for (ch, x) in [l, r].into_iter().enumerate() {
            let ph = self.phase + if ch == 1 { 0.25 } else { 0.0 };
            let tri = 1.0 - 2.0 * ((ph.fract()) * 2.0 - 1.0).abs() as f32; // -1..1
            let d = base + span * 0.5 * (1.0 + tri);
            let wet = self.lines[ch].tap_hermite(d.max(1.0));
            self.lines[ch].push(x + undenorm(self.fb[ch] * fb));
            self.fb[ch] = wet;
            out[ch] = wet;
        }
        (out[0], out[1])
    }
}

/// A stereo feedback delay with a low-pass and high-pass in the feedback path.
#[derive(Debug, Clone)]
pub struct FeedbackDelay {
    sr: f64,
    lines: [DelayLine; 2],
    lp: [Biquad; 2],
    hp: [Biquad; 2],
    lp_c: Coeffs,
    hp_c: Coeffs,
    tone_key: (f32, f32),
    time: crate::util::Smooth,
}

impl FeedbackDelay {
    /// Longest delay in seconds.
    pub const MAX_SECONDS: f64 = 2.0;

    /// A delay for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let n = (Self::MAX_SECONDS * sample_rate) as usize + 16;
        Self {
            sr: sample_rate,
            lines: [DelayLine::new(n), DelayLine::new(n)],
            lp: [Biquad::default(); 2],
            hp: [Biquad::default(); 2],
            lp_c: Coeffs::IDENTITY,
            hp_c: Coeffs::IDENTITY,
            tone_key: (-1.0, -1.0),
            time: crate::util::Smooth::new(0.25, sample_rate, 120.0, 120.0),
        }
    }
    /// Clear the state.
    pub fn reset(&mut self, seconds: f32) {
        for l in &mut self.lines {
            l.clear();
        }
        for b in self.lp.iter_mut().chain(self.hp.iter_mut()) {
            b.reset();
        }
        self.time.reset(seconds);
    }
    /// Set the damping of the repeats.
    pub fn set_tone(&mut self, lp_hz: f32, hp_hz: f32) {
        if (lp_hz, hp_hz) == self.tone_key {
            return;
        }
        self.tone_key = (lp_hz, hp_hz);
        self.lp_c = Coeffs::lowpass(lp_hz.min(self.sr as f32 * 0.45), 0.6, self.sr);
        self.hp_c = Coeffs::highpass(hp_hz, 0.6, self.sr);
    }
    /// The wet signal for one frame. `seconds` glides (tape-style) when it changes.
    #[inline]
    pub fn tick(
        &mut self,
        l: f32,
        r: f32,
        seconds: f32,
        feedback: f32,
        ping_pong: bool,
    ) -> (f32, f32) {
        let t = self
            .time
            .tick(seconds.clamp(0.001, Self::MAX_SECONDS as f32 - 0.001));
        let d = (t as f64 * self.sr) as f32;
        let wl = self.lines[0].tap_hermite(d);
        let wr = self.lines[1].tap_hermite(d);
        let fb = feedback.clamp(0.0, 0.97);
        let fl = self.hp[0].tick(self.lp[0].tick(wl, &self.lp_c), &self.hp_c) * fb;
        let fr = self.hp[1].tick(self.lp[1].tick(wr, &self.lp_c), &self.hp_c) * fb;
        if ping_pong {
            // mono input into the left line, the repeats cross over
            self.lines[0].push(undenorm(0.5 * (l + r) + fr));
            self.lines[1].push(undenorm(fl));
        } else {
            self.lines[0].push(undenorm(l + fl));
            self.lines[1].push(undenorm(r + fr));
        }
        (wl, wr)
    }
}

/// Stereo width by mid/side: 0 = mono, 1 = unchanged, 2 = twice the side.
#[inline]
pub fn width(l: f32, r: f32, w: f32) -> (f32, f32) {
    let m = 0.5 * (l + r);
    let s = 0.5 * (l - r) * w;
    (m + s, m - s)
}

/// Constant-power balance, `p` -1 (left) ..= 1 (right).
#[inline]
pub fn balance(l: f32, r: f32, p: f32) -> (f32, f32) {
    let p = p.clamp(-1.0, 1.0);
    let a = (p + 1.0) * std::f32::consts::FRAC_PI_4;
    let (gl, gr) = (
        a.cos() * std::f32::consts::SQRT_2,
        a.sin() * std::f32::consts::SQRT_2,
    );
    (l * gl.min(1.0), r * gr.min(1.0))
}

/// Running stereo correlation (-1 .. 1) with a ~300 ms window.
#[derive(Debug, Clone)]
pub struct Correlation {
    a: f32,
    lr: f32,
    ll: f32,
    rr: f32,
}

impl Correlation {
    /// A meter for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        Self {
            a: (1.0 - (-1.0 / (0.3 * sample_rate)).exp()) as f32,
            lr: 0.0,
            ll: 0.0,
            rr: 0.0,
        }
    }
    /// Feed a frame.
    #[inline]
    pub fn push(&mut self, l: f32, r: f32) {
        self.lr = undenorm(self.lr + (l * r - self.lr) * self.a);
        self.ll = undenorm(self.ll + (l * l - self.ll) * self.a);
        self.rr = undenorm(self.rr + (r * r - self.rr) * self.a);
    }
    /// The current value (1 for silence).
    pub fn value(&self) -> f32 {
        let d = (self.ll * self.rr).sqrt();
        if d < 1e-9 {
            1.0
        } else {
            (self.lr / d).clamp(-1.0, 1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_delay_repeats_at_its_time_and_decays() {
        let sr = 48_000.0;
        let mut d = FeedbackDelay::new(sr);
        d.set_tone(20_000.0, 20.0);
        d.reset(0.1);
        let mut out = Vec::new();
        for i in 0..20_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            out.push(d.tick(x, x, 0.1, 0.5, false).0);
        }
        let (idx, _) =
            out.iter().enumerate().fold(
                (0, 0.0f32),
                |m, (i, v)| if v.abs() > m.1 { (i, v.abs()) } else { m },
            );
        assert!((idx as i64 - 4800).abs() <= 2, "{idx}");
        let second = out[9590..9610].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(second > 0.3 && second < 0.6, "{second}");
    }

    #[test]
    fn flanger_width_balance_and_correlation_behave() {
        let sr = 44_100.0;
        let mut f = Flanger::new(sr);
        for i in 0..50_000 {
            let x = ((i as f32) * 0.03).sin();
            let (a, b) = f.tick(x, x, 0.5, 1.0, 0.9);
            assert!(a.is_finite() && b.is_finite() && a.abs() < 20.0);
        }
        let (l, r) = width(0.5, 0.1, 0.0);
        assert!((l - r).abs() < 1e-7);
        let (l, r) = balance(1.0, 1.0, 0.0);
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let (l, r) = balance(1.0, 1.0, 1.0);
        assert!(l.abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let mut c = Correlation::new(sr);
        for i in 0..44_100 {
            let x = ((i as f32) * 0.01).sin();
            c.push(x, -x);
        }
        assert!(c.value() < -0.99);
    }
}
