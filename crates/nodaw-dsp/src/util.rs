//! Small helpers: decibels, denormal flushing, a noise source and a one-pole parameter smoother.

/// Decibels to linear gain.
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db * 0.05)
}

/// Linear gain to decibels (floored at -180 dB so silence stays finite).
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.abs().max(1e-9).log10()
}

/// Flush values too small to matter (and every subnormal) to exactly zero. Used inside
/// feedback paths so a decaying tail never reaches the slow subnormal range, whatever the
/// CPU's FTZ/DAZ state is.
#[inline(always)]
pub fn undenorm(x: f32) -> f32 {
    if x.abs() < 1.0e-20 {
        0.0
    } else {
        x
    }
}

/// xorshift32 noise, white in [-1, 1). Deterministic, so tests are repeatable.
#[derive(Debug, Clone)]
pub struct Noise(u32);

impl Noise {
    /// A generator; `seed` 0 is replaced by a fixed non-zero value.
    pub fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9e37_79b9 } else { seed })
    }
    /// The next sample.
    #[inline]
    pub fn sample(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32) * (2.0 / 4_294_967_296.0) - 1.0
    }
}

/// A one-pole smoother with separate attack (rising) and release (falling) times.
#[derive(Debug, Clone)]
pub struct Smooth {
    value: f32,
    up: f32,
    down: f32,
}

impl Smooth {
    /// A smoother resting at `value`, with times in milliseconds.
    pub fn new(value: f32, sample_rate: f64, up_ms: f32, down_ms: f32) -> Self {
        let mut s = Self { value, up: 1.0, down: 1.0 };
        s.set_times(sample_rate, up_ms, down_ms);
        s
    }
    /// Change the times (milliseconds to reach ~63 %).
    pub fn set_times(&mut self, sample_rate: f64, up_ms: f32, down_ms: f32) {
        let c = |ms: f32| 1.0 - (-1.0 / (ms.max(0.01) as f64 * 0.001 * sample_rate)).exp() as f32;
        self.up = c(up_ms);
        self.down = c(down_ms);
    }
    /// Jump to `value`.
    pub fn reset(&mut self, value: f32) {
        self.value = value;
    }
    /// Move one sample towards `target` and return the new value.
    #[inline]
    pub fn tick(&mut self, target: f32) -> f32 {
        let c = if target > self.value { self.up } else { self.down };
        self.value += (target - self.value) * c;
        if (target - self.value).abs() < 3.0e-5 {
            self.value = target;
        }
        self.value
    }
    /// The current value.
    pub fn value(&self) -> f32 {
        self.value
    }
}

/// The next power of two at or above `n` (at least 2).
pub fn next_pow2(n: usize) -> usize {
    n.max(2).next_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibel_round_trip() {
        for db in [-60.0f32, -12.0, 0.0, 6.0, 24.0] {
            assert!((gain_to_db(db_to_gain(db)) - db).abs() < 1e-3);
        }
        assert!(gain_to_db(0.0).is_finite());
    }

    #[test]
    fn undenorm_flushes_subnormals_only() {
        assert_eq!(undenorm(1e-30), 0.0);
        assert_eq!(undenorm(f32::MIN_POSITIVE / 4.0), 0.0);
        assert_eq!(undenorm(0.25), 0.25);
        assert_eq!(undenorm(-0.25), -0.25);
    }

    #[test]
    fn noise_is_bounded_and_roughly_zero_mean() {
        let mut n = Noise::new(1);
        let mut sum = 0.0f64;
        for _ in 0..100_000 {
            let v = n.sample();
            assert!((-1.0..1.0).contains(&v));
            sum += v as f64;
        }
        assert!((sum / 100_000.0).abs() < 0.02);
    }

    #[test]
    fn smoother_reaches_target_and_stops() {
        let mut s = Smooth::new(0.0, 48_000.0, 5.0, 5.0);
        for _ in 0..48_000 {
            s.tick(1.0);
        }
        assert_eq!(s.value(), 1.0);
    }
}
