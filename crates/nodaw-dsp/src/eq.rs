//! More RBJ biquad designs (peaking, shelves with a Q, band-pass, notch, tilt) and an
//! `EqBand` that glides its coefficients without zipper noise. Shared by Racks, Harmonic Space
//! and Chronos Dynamic EQ.

use crate::filter::{Biquad, Coeffs};

fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coeffs {
    Coeffs {
        b0: (b0 / a0) as f32,
        b1: (b1 / a0) as f32,
        b2: (b2 / a0) as f32,
        a1: (a1 / a0) as f32,
        a2: (a2 / a0) as f32,
    }
}

fn omega(fc: f32, sr: f64) -> (f64, f64) {
    let w = 2.0 * std::f64::consts::PI * (fc as f64).clamp(5.0, sr * 0.495) / sr;
    w.sin_cos()
}

/// The shape of one EQ band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Bell.
    Peak,
    /// Low shelf.
    LowShelf,
    /// High shelf.
    HighShelf,
    /// 12 dB/oct high-pass (gain ignored).
    HighPass,
    /// 12 dB/oct low-pass (gain ignored).
    LowPass,
    /// Notch (gain ignored).
    Notch,
    /// Band-pass, 0 dB peak (gain ignored).
    BandPass,
}

impl Shape {
    /// The shapes in the order the plugins list them.
    pub const ALL: [Shape; 7] = [
        Shape::Peak,
        Shape::LowShelf,
        Shape::HighShelf,
        Shape::HighPass,
        Shape::LowPass,
        Shape::Notch,
        Shape::BandPass,
    ];

    /// From a choice index.
    pub fn from_index(i: usize) -> Self {
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }
}

/// Peaking (bell) filter.
pub fn peak(fc: f32, q: f32, gain_db: f32, sr: f64) -> Coeffs {
    let a = 10f64.powf(gain_db as f64 / 40.0);
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    norm(
        1.0 + alpha * a,
        -2.0 * c,
        1.0 - alpha * a,
        1.0 + alpha / a,
        -2.0 * c,
        1.0 - alpha / a,
    )
}

/// Low shelf with a shelf slope set by `q` (0.707 = Butterworth-like).
pub fn low_shelf(fc: f32, q: f32, gain_db: f32, sr: f64) -> Coeffs {
    let a = 10f64.powf(gain_db as f64 / 40.0);
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    let sq = 2.0 * a.sqrt() * alpha;
    norm(
        a * ((a + 1.0) - (a - 1.0) * c + sq),
        2.0 * a * ((a - 1.0) - (a + 1.0) * c),
        a * ((a + 1.0) - (a - 1.0) * c - sq),
        (a + 1.0) + (a - 1.0) * c + sq,
        -2.0 * ((a - 1.0) + (a + 1.0) * c),
        (a + 1.0) + (a - 1.0) * c - sq,
    )
}

/// High shelf with a shelf slope set by `q`.
pub fn high_shelf(fc: f32, q: f32, gain_db: f32, sr: f64) -> Coeffs {
    let a = 10f64.powf(gain_db as f64 / 40.0);
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    let sq = 2.0 * a.sqrt() * alpha;
    norm(
        a * ((a + 1.0) + (a - 1.0) * c + sq),
        -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
        a * ((a + 1.0) + (a - 1.0) * c - sq),
        (a + 1.0) - (a - 1.0) * c + sq,
        2.0 * ((a - 1.0) - (a + 1.0) * c),
        (a + 1.0) - (a - 1.0) * c - sq,
    )
}

/// 2nd-order high-pass with resonance `q`.
pub fn highpass(fc: f32, q: f32, sr: f64) -> Coeffs {
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    norm(
        (1.0 + c) / 2.0,
        -(1.0 + c),
        (1.0 + c) / 2.0,
        1.0 + alpha,
        -2.0 * c,
        1.0 - alpha,
    )
}

/// 2nd-order low-pass with resonance `q`.
pub fn lowpass(fc: f32, q: f32, sr: f64) -> Coeffs {
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    norm(
        (1.0 - c) / 2.0,
        1.0 - c,
        (1.0 - c) / 2.0,
        1.0 + alpha,
        -2.0 * c,
        1.0 - alpha,
    )
}

/// Notch.
pub fn notch(fc: f32, q: f32, sr: f64) -> Coeffs {
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    norm(1.0, -2.0 * c, 1.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
}

/// Band-pass with 0 dB at the centre.
pub fn bandpass(fc: f32, q: f32, sr: f64) -> Coeffs {
    let (s, c) = omega(fc, sr);
    let alpha = s / (2.0 * (q as f64).max(0.05));
    norm(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
}

/// Any shape.
pub fn design(shape: Shape, fc: f32, q: f32, gain_db: f32, sr: f64) -> Coeffs {
    match shape {
        Shape::Peak => {
            if gain_db.abs() < 1e-3 {
                Coeffs::IDENTITY
            } else {
                peak(fc, q, gain_db, sr)
            }
        }
        Shape::LowShelf => {
            if gain_db.abs() < 1e-3 {
                Coeffs::IDENTITY
            } else {
                low_shelf(fc, q, gain_db, sr)
            }
        }
        Shape::HighShelf => {
            if gain_db.abs() < 1e-3 {
                Coeffs::IDENTITY
            } else {
                high_shelf(fc, q, gain_db, sr)
            }
        }
        Shape::HighPass => highpass(fc, q, sr),
        Shape::LowPass => lowpass(fc, q, sr),
        Shape::Notch => notch(fc, q, sr),
        Shape::BandPass => bandpass(fc, q, sr),
    }
}

/// Linear interpolation between two coefficient sets.
#[inline]
pub fn lerp(a: &Coeffs, b: &Coeffs, t: f32) -> Coeffs {
    Coeffs {
        b0: a.b0 + (b.b0 - a.b0) * t,
        b1: a.b1 + (b.b1 - a.b1) * t,
        b2: a.b2 + (b.b2 - a.b2) * t,
        a1: a.a1 + (b.a1 - a.a1) * t,
        a2: a.a2 + (b.a2 - a.a2) * t,
    }
}

/// One stereo EQ band whose coefficients glide to their target over a short ramp each time
/// the settings change, so automation never clicks. No allocation.
#[derive(Debug, Clone)]
pub struct EqBand {
    st: [Biquad; 2],
    cur: Coeffs,
    from: Coeffs,
    to: Coeffs,
    ramp: u32,
    pos: u32,
    key: [f32; 4],
}

impl EqBand {
    /// A band that is flat, with `ramp` samples of glide per change.
    pub fn new(ramp: u32) -> Self {
        Self {
            st: [Biquad::default(); 2],
            cur: Coeffs::IDENTITY,
            from: Coeffs::IDENTITY,
            to: Coeffs::IDENTITY,
            ramp: ramp.max(1),
            pos: 0,
            key: [f32::NAN; 4],
        }
    }
    /// Set the target. Cheap when nothing changed.
    pub fn set(&mut self, shape: Shape, fc: f32, q: f32, gain_db: f32, sr: f64) {
        let k = [shape as u8 as f32, fc, q, gain_db];
        if k == self.key {
            return;
        }
        let first = self.key[0].is_nan();
        self.key = k;
        self.to = design(shape, fc, q, gain_db, sr);
        if first {
            self.cur = self.to;
            self.from = self.to;
            self.pos = self.ramp;
        } else {
            self.from = self.cur;
            self.pos = 0;
        }
    }
    /// Jump to the target and clear the state.
    pub fn reset(&mut self) {
        self.cur = self.to;
        self.from = self.to;
        self.pos = self.ramp;
        for b in &mut self.st {
            b.reset();
        }
    }
    /// The current (target) coefficients.
    pub fn coeffs(&self) -> Coeffs {
        self.to
    }
    /// True while the band is flat and settled (it can be skipped).
    pub fn is_identity(&self) -> bool {
        self.pos >= self.ramp && self.to == Coeffs::IDENTITY
    }
    /// One stereo frame.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.pos < self.ramp {
            self.pos += 1;
            // Update every 16 samples (and at the end): cheap and still smooth.
            if self.pos.is_multiple_of(16) || self.pos == self.ramp {
                self.cur = lerp(&self.from, &self.to, self.pos as f32 / self.ramp as f32);
            }
        }
        let c = self.cur;
        (self.st[0].tick(l, &c), self.st[1].tick(r, &c))
    }
    /// One mono sample on channel `ch`.
    #[inline]
    pub fn tick_ch(&mut self, ch: usize, x: f32) -> f32 {
        let c = self.cur;
        self.st[ch & 1].tick(x, &c)
    }
    /// Advance the glide by one sample without filtering (pair with [`Self::tick_ch`]).
    #[inline]
    pub fn advance(&mut self) {
        if self.pos < self.ramp {
            self.pos += 1;
            if self.pos.is_multiple_of(16) || self.pos == self.ramp {
                self.cur = lerp(&self.from, &self.to, self.pos as f32 / self.ramp as f32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bells_and_shelves_hit_their_gain() {
        let sr = 48_000.0;
        let p = peak(1000.0, 1.0, 6.0, sr);
        assert!((20.0 * p.magnitude(1000.0, sr).log10() - 6.0).abs() < 0.05);
        assert!((20.0 * p.magnitude(20.0, sr).log10()).abs() < 0.1);
        let ls = low_shelf(100.0, 0.707, -9.0, sr);
        assert!((20.0 * ls.magnitude(20.0, sr).log10() + 9.0).abs() < 0.5);
        let hs = high_shelf(8000.0, 0.707, 4.0, sr);
        assert!((20.0 * hs.magnitude(20_000.0, sr).log10() - 4.0).abs() < 0.5);
        assert!(notch(1000.0, 4.0, sr).magnitude(1000.0, sr) < 1e-3);
        assert!((bandpass(1000.0, 2.0, sr).magnitude(1000.0, sr) - 1.0).abs() < 1e-3);
        assert!(highpass(1000.0, 0.707, sr).magnitude(50.0, sr) < 0.01);
        assert!(lowpass(1000.0, 0.707, sr).magnitude(15_000.0, sr) < 0.01);
    }

    #[test]
    fn a_band_glides_and_stays_finite() {
        let sr = 44_100.0;
        let mut b = EqBand::new(256);
        b.set(Shape::Peak, 500.0, 2.0, 12.0, sr);
        let mut y = 0.0;
        for i in 0..20_000 {
            if i == 5000 {
                b.set(Shape::Peak, 5000.0, 8.0, -18.0, sr);
            }
            let x = ((i as f32) * 0.07).sin();
            y = b.tick(x, x).0;
            assert!(y.is_finite());
        }
        assert!(y.abs() < 5.0);
        let mut flat = EqBand::new(64);
        flat.set(Shape::Peak, 1000.0, 1.0, 0.0, sr);
        assert!(flat.is_identity());
    }
}
