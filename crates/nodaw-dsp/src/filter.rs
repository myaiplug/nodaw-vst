//! RBJ biquads.

use crate::util::undenorm;

/// Biquad coefficients (a0 normalised to 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    /// Feed-forward 0.
    pub b0: f32,
    /// Feed-forward 1.
    pub b1: f32,
    /// Feed-forward 2.
    pub b2: f32,
    /// Feedback 1.
    pub a1: f32,
    /// Feedback 2.
    pub a2: f32,
}

impl Coeffs {
    /// Passes everything.
    pub const IDENTITY: Coeffs = Coeffs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b2 / a0) as f32,
            a1: (a1 / a0) as f32,
            a2: (a2 / a0) as f32,
        }
    }

    /// 2nd-order low-pass.
    pub fn lowpass(fc: f32, q: f32, sr: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * (fc as f64).clamp(10.0, sr * 0.49) / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q as f64);
        Self::norm(
            (1.0 - c) / 2.0,
            1.0 - c,
            (1.0 - c) / 2.0,
            1.0 + alpha,
            -2.0 * c,
            1.0 - alpha,
        )
    }

    /// Low shelf (slope 1) with `gain_db` below `fc`.
    pub fn low_shelf(fc: f32, gain_db: f32, sr: f64) -> Self {
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let w = 2.0 * std::f64::consts::PI * (fc as f64).clamp(10.0, sr * 0.49) / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / 2.0 * 2f64.sqrt();
        let sq = 2.0 * a.sqrt() * alpha;
        Self::norm(
            a * ((a + 1.0) - (a - 1.0) * c + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - sq),
            (a + 1.0) + (a - 1.0) * c + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - sq,
        )
    }

    /// 2nd-order high-pass.
    pub fn highpass(fc: f32, q: f32, sr: f64) -> Self {
        let w = 2.0 * std::f64::consts::PI * (fc as f64).clamp(5.0, sr * 0.49) / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * q as f64);
        Self::norm(
            (1.0 + c) / 2.0,
            -(1.0 + c),
            (1.0 + c) / 2.0,
            1.0 + alpha,
            -2.0 * c,
            1.0 - alpha,
        )
    }

    /// Magnitude response at `f` Hz, for tests and drawing.
    pub fn magnitude(&self, f: f32, sr: f64) -> f32 {
        let w = 2.0 * std::f64::consts::PI * f as f64 / sr;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let (b0, b1, b2, a1, a2) = (
            self.b0 as f64,
            self.b1 as f64,
            self.b2 as f64,
            self.a1 as f64,
            self.a2 as f64,
        );
        let nr = b0 + b1 * c1 + b2 * c2;
        let ni = -(b1 * s1 + b2 * s2);
        let dr = 1.0 + a1 * c1 + a2 * c2;
        let di = -(a1 * s1 + a2 * s2);
        (((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()) as f32
    }
}

/// A transposed direct-form II biquad state (one channel).
#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Process one sample.
    #[inline]
    pub fn tick(&mut self, x: f32, c: &Coeffs) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = undenorm(c.b1 * x - c.a1 * y + self.z2);
        self.z2 = undenorm(c.b2 * x - c.a2 * y);
        y
    }
    /// Clear the state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowpass_is_minus_3db_at_cutoff_for_butterworth_q() {
        let c = Coeffs::lowpass(1000.0, std::f32::consts::FRAC_1_SQRT_2, 48_000.0);
        let m = c.magnitude(1000.0, 48_000.0);
        assert!((m - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01, "{m}");
        assert!(c.magnitude(100.0, 48_000.0) > 0.99);
        assert!(c.magnitude(10_000.0, 48_000.0) < 0.02);
    }

    #[test]
    fn low_shelf_gain_is_as_asked() {
        let c = Coeffs::low_shelf(200.0, 6.0, 48_000.0);
        let lo = crate::gain_to_db(c.magnitude(20.0, 48_000.0));
        let hi = crate::gain_to_db(c.magnitude(10_000.0, 48_000.0));
        assert!((lo - 6.0).abs() < 0.3, "{lo}");
        assert!(hi.abs() < 0.3, "{hi}");
    }

    #[test]
    fn filters_never_leave_subnormals_behind() {
        let c = Coeffs::lowpass(300.0, 0.7, 48_000.0);
        let mut f = Biquad::default();
        f.tick(1.0, &c);
        let mut worst = 1.0f32;
        for _ in 0..200_000 {
            let y = f.tick(0.0, &c);
            assert!(!y.is_subnormal());
            worst = worst.min(y.abs());
        }
        assert!(worst == 0.0 || worst >= f32::MIN_POSITIVE);
    }
}
