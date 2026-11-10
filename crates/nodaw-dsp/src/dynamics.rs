//! Dynamics building blocks: a level detector with attack/release in the dB domain, a static
//! gain computer (compress above / expand below a threshold, with a knee and a range), and a
//! dynamic EQ band built as `x + (G - 1) * F(x)`, where `F` is a 0 dB band-pass (bell) or a
//! low/high-pass (shelves). That form lets the gain move every sample without recomputing
//! coefficients, so dynamic bands never zipper, and at `G = 1` the band is bit-transparent.

use crate::filter::{Biquad, Coeffs};
use crate::util::{db_to_gain, undenorm};

/// A peak/RMS-ish envelope follower working in dB.
#[derive(Debug, Clone)]
pub struct Envelope {
    att: f32,
    rel: f32,
    env: f32,
    sr: f64,
}

impl Envelope {
    /// A follower with times in milliseconds.
    pub fn new(sample_rate: f64, attack_ms: f32, release_ms: f32) -> Self {
        let mut e = Self {
            att: 0.0,
            rel: 0.0,
            env: 0.0,
            sr: sample_rate,
        };
        e.set_times(attack_ms, release_ms);
        e
    }
    /// Change the times.
    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32) {
        let c = |ms: f32| 1.0 - (-1.0 / (ms.max(0.05) as f64 * 0.001 * self.sr)).exp() as f32;
        self.att = c(attack_ms);
        self.rel = c(release_ms);
    }
    /// Clear.
    pub fn reset(&mut self) {
        self.env = 0.0;
    }
    /// Feed a rectified sample (linear), return the envelope (linear).
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = x.abs();
        let c = if x > self.env { self.att } else { self.rel };
        self.env = undenorm(self.env + (x - self.env) * c);
        self.env
    }
    /// Current value (linear).
    pub fn value(&self) -> f32 {
        self.env
    }
}

/// What a dynamic band does when the level crosses its threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynMode {
    /// Static EQ only.
    Off,
    /// Above the threshold, cut (or reduce a boost): a band compressor.
    Compress,
    /// Above the threshold, boost: an upward band expander ("make it pop when it's there").
    Expand,
}

impl DynMode {
    /// From a choice index.
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => DynMode::Compress,
            2 => DynMode::Expand,
            _ => DynMode::Off,
        }
    }
}

/// The gain change in dB for a detector level `level_db`. Soft knee of `knee_db`.
#[inline]
pub fn gain_computer(
    mode: DynMode,
    level_db: f32,
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    range_db: f32,
) -> f32 {
    if mode == DynMode::Off {
        return 0.0;
    }
    let over = level_db - threshold_db;
    let k = knee_db.max(0.01);
    let x = if over <= -k * 0.5 {
        0.0
    } else if over >= k * 0.5 {
        over
    } else {
        let t = over + k * 0.5;
        t * t / (2.0 * k)
    };
    let slope = 1.0 - 1.0 / ratio.max(1.0);
    let g = x * slope;
    match mode {
        DynMode::Compress => -g.min(range_db.abs()),
        DynMode::Expand => g.min(range_db.abs()),
        DynMode::Off => 0.0,
    }
}

/// The filter a dynamic band is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    /// Bell around the frequency.
    Bell,
    /// Below the frequency.
    LowShelf,
    /// Above the frequency.
    HighShelf,
}

impl BandKind {
    /// From a choice index.
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => BandKind::LowShelf,
            2 => BandKind::HighShelf,
            _ => BandKind::Bell,
        }
    }
    /// The 0 dB "selection" filter.
    pub fn coeffs(self, fc: f32, q: f32, sr: f64) -> Coeffs {
        match self {
            BandKind::Bell => crate::eq::bandpass(fc, q, sr),
            BandKind::LowShelf => crate::eq::lowpass(fc, 0.6, sr),
            BandKind::HighShelf => crate::eq::highpass(fc, 0.6, sr),
        }
    }
    /// Magnitude of the band at `f` with linear gain `g` (for the UI curve and tests).
    pub fn magnitude(self, fc: f32, q: f32, g: f32, f: f32, sr: f64) -> f32 {
        let c = self.coeffs(fc, q, sr);
        // |1 + (g - 1) H| needs the complex response.
        let w = 2.0 * std::f64::consts::PI * f as f64 / sr;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let (b0, b1, b2, a1, a2) = (
            c.b0 as f64,
            c.b1 as f64,
            c.b2 as f64,
            c.a1 as f64,
            c.a2 as f64,
        );
        let (nr, ni) = (b0 + b1 * c1 + b2 * c2, -(b1 * s1 + b2 * s2));
        let (dr, di) = (1.0 + a1 * c1 + a2 * c2, -(a1 * s1 + a2 * s2));
        let den = dr * dr + di * di;
        let (hr, hi) = ((nr * dr + ni * di) / den, (ni * dr - nr * di) / den);
        let k = g as f64 - 1.0;
        (((1.0 + k * hr).powi(2) + (k * hi).powi(2)).sqrt()) as f32
    }
}

/// One stereo dynamic EQ band (the detector is linked: the louder channel drives both).
#[derive(Debug, Clone)]
pub struct DynBand {
    sel: [Biquad; 2],
    det: [Biquad; 2],
    c: Coeffs,
    key: (u8, f32, f32),
    env: Envelope,
    gain_db: f32,
    smooth: f32,
    sr: f64,
}

/// The settings of one band, in plain units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynSettings {
    /// Off bands pass the signal bit-exact.
    pub on: bool,
    /// Filter kind.
    pub kind: BandKind,
    /// Centre / corner in Hz.
    pub freq: f32,
    /// Bandwidth (bells).
    pub q: f32,
    /// Static gain in dB.
    pub gain_db: f32,
    /// Dynamic behaviour.
    pub mode: DynMode,
    /// dBFS.
    pub threshold_db: f32,
    /// 1..20.
    pub ratio: f32,
    /// Most the dynamics may add or remove, dB.
    pub range_db: f32,
    /// ms.
    pub attack_ms: f32,
    /// ms.
    pub release_ms: f32,
    /// Solo: hear only what the band selects.
    pub solo: bool,
}

impl Default for DynSettings {
    fn default() -> Self {
        Self {
            on: false,
            kind: BandKind::Bell,
            freq: 1000.0,
            q: 1.0,
            gain_db: 0.0,
            mode: DynMode::Off,
            threshold_db: -24.0,
            ratio: 3.0,
            range_db: 9.0,
            attack_ms: 5.0,
            release_ms: 120.0,
            solo: false,
        }
    }
}

impl DynBand {
    /// A band for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sel: [Biquad::default(); 2],
            det: [Biquad::default(); 2],
            c: Coeffs::IDENTITY,
            key: (255, 0.0, 0.0),
            env: Envelope::new(sample_rate, 5.0, 120.0),
            gain_db: 0.0,
            smooth: 0.0,
            sr: sample_rate,
        }
    }
    /// Clear the state.
    pub fn reset(&mut self) {
        for b in self.sel.iter_mut().chain(self.det.iter_mut()) {
            b.reset();
        }
        self.env.reset();
        self.gain_db = 0.0;
    }
    /// Prepare a block (coefficients and times). Cheap when nothing changed.
    pub fn prepare(&mut self, s: &DynSettings) {
        let k = (s.kind as u8, s.freq, s.q);
        if k != self.key {
            self.key = k;
            self.c = s.kind.coeffs(s.freq, s.q, self.sr);
        }
        self.env.set_times(s.attack_ms, s.release_ms);
        self.smooth = 1.0 - (-1.0 / (0.002 * self.sr)).exp() as f32;
    }
    /// The dynamic gain (dB) now, for the meters.
    pub fn dyn_db(&self) -> f32 {
        self.gain_db
    }
    /// One frame. Returns the processed frame; with `solo` the band's selection.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32, s: &DynSettings) -> (f32, f32) {
        let fl = self.sel[0].tick(l, &self.c);
        let fr = self.sel[1].tick(r, &self.c);
        let target = if s.mode == DynMode::Off {
            0.0
        } else {
            // detector: the selected band itself (a split-band detector)
            let dl = self.det[0].tick(l, &self.c);
            let dr = self.det[1].tick(r, &self.c);
            let e = self.env.tick(dl.abs().max(dr.abs()));
            let lvl = 20.0 * e.max(1e-6).log10();
            gain_computer(s.mode, lvl, s.threshold_db, s.ratio, 6.0, s.range_db)
        };
        self.gain_db += (target - self.gain_db) * self.smooth;
        self.gain_db = undenorm(self.gain_db);
        if s.solo {
            return (fl, fr);
        }
        let total = s.gain_db + self.gain_db;
        if total.abs() < 1e-4 {
            return (l, r);
        }
        let k = db_to_gain(total) - 1.0;
        (l + k * fl, r + k * fr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gain_computer_has_a_knee_ratio_and_range() {
        assert_eq!(
            gain_computer(DynMode::Compress, -40.0, -20.0, 4.0, 6.0, 12.0),
            0.0
        );
        let g = gain_computer(DynMode::Compress, 0.0, -20.0, 4.0, 6.0, 30.0);
        assert!((g + 15.0).abs() < 1e-4, "{g}");
        assert_eq!(
            gain_computer(DynMode::Compress, 0.0, -20.0, 20.0, 6.0, 6.0),
            -6.0
        );
        assert!(gain_computer(DynMode::Expand, 0.0, -20.0, 2.0, 0.0, 30.0) > 9.9);
        let k = gain_computer(DynMode::Compress, -20.0, -20.0, 4.0, 6.0, 30.0);
        assert!(k < 0.0 && k > -1.0, "inside the knee: {k}");
    }

    #[test]
    fn a_bell_band_hits_its_static_gain_at_the_centre() {
        let sr = 48_000.0;
        for g in [-12.0f32, -3.0, 6.0, 12.0] {
            let m = BandKind::Bell.magnitude(1000.0, 1.5, db_to_gain(g), 1000.0, sr);
            assert!(
                (20.0 * m.log10() - g).abs() < 0.05,
                "{g}: {}",
                20.0 * m.log10()
            );
            let far = BandKind::Bell.magnitude(1000.0, 1.5, db_to_gain(g), 20.0, sr);
            assert!((20.0 * far.log10()).abs() < 0.5);
        }
    }

    #[test]
    fn a_loud_band_is_compressed_and_a_quiet_one_is_left_alone() {
        let sr = 48_000.0;
        let s = DynSettings {
            on: true,
            freq: 1000.0,
            q: 1.0,
            mode: DynMode::Compress,
            threshold_db: -30.0,
            ratio: 8.0,
            range_db: 12.0,
            ..Default::default()
        };
        let run = |amp: f32| {
            let mut b = DynBand::new(sr);
            b.prepare(&s);
            let mut peak = 0.0f32;
            for i in 0..48_000 {
                let x = amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / sr as f32).sin();
                let y = b.tick(x, x, &s).0;
                if i > 24_000 {
                    peak = peak.max(y.abs());
                }
            }
            (peak, b.dyn_db())
        };
        let (loud, g) = run(0.5);
        assert!(g < -8.0, "{g}");
        assert!(loud < 0.25, "{loud}");
        let (quiet, g2) = run(0.005);
        assert!(
            g2.abs() < 0.1 && (quiet - 0.005).abs() < 5e-4,
            "{quiet} {g2}"
        );
    }

    #[test]
    fn an_idle_band_is_bit_transparent() {
        let mut b = DynBand::new(44_100.0);
        let s = DynSettings::default();
        b.prepare(&s);
        for i in 0..1000 {
            let x = (i as f32 * 0.1).sin();
            assert_eq!(b.tick(x, -x, &s), (x, -x));
        }
    }
}
