//! Delay lines with fractional reads.

use crate::util::next_pow2;

/// A circular buffer. `push` writes the newest sample, `tap(d)` reads `d` samples back
/// (`tap(0.0)` is the newest sample).
#[derive(Debug, Clone)]
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
}

impl DelayLine {
    /// A delay line that can read up to `max_delay` samples back.
    pub fn new(max_delay: usize) -> Self {
        let n = next_pow2(max_delay + 8);
        Self {
            buf: vec![0.0; n],
            mask: n - 1,
            pos: 0,
        }
    }
    /// Longest readable delay in samples.
    pub fn capacity(&self) -> usize {
        self.buf.len() - 4
    }
    /// Silence the buffer.
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
    }
    /// Write the newest sample.
    #[inline]
    pub fn push(&mut self, x: f32) {
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) & self.mask;
    }
    #[inline]
    fn at(&self, back: usize) -> f32 {
        self.buf[(self.pos.wrapping_sub(1).wrapping_sub(back)) & self.mask]
    }
    /// The sample `d` samples before the newest, `d` a whole number.
    #[inline]
    pub fn tap_int(&self, d: usize) -> f32 {
        self.at(d)
    }
    /// Linear interpolation, `d >= 0`.
    #[inline]
    pub fn tap_linear(&self, d: f32) -> f32 {
        let i = d as usize;
        let f = d - i as f32;
        let a = self.at(i);
        let b = self.at(i + 1);
        a + (b - a) * f
    }
    /// 4-point Hermite (Catmull-Rom) interpolation; `d >= 1`. Exact at whole `d`.
    #[inline]
    pub fn tap_hermite(&self, d: f32) -> f32 {
        let i = d as usize;
        let f = d - i as f32;
        let (ym1, y0, y1, y2) = (self.at(i - 1), self.at(i), self.at(i + 1), self.at(i + 2));
        let c1 = 0.5 * (y1 - ym1);
        let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
        let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
        ((c3 * f + c2) * f + c1) * f + y0
    }
}

/// A Schroeder all-pass diffuser.
#[derive(Debug, Clone)]
pub struct Allpass {
    line: DelayLine,
    len: usize,
    g: f32,
}

impl Allpass {
    /// An all-pass of `len` samples with coefficient `g`.
    pub fn new(len: usize, g: f32) -> Self {
        Self {
            line: DelayLine::new(len + 2),
            len: len.max(1),
            g,
        }
    }
    /// Process one sample.
    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let d = self.line.tap_int(self.len - 1);
        let v = x - self.g * d;
        self.line.push(v);
        crate::util::undenorm(d + self.g * v)
    }
    /// Silence.
    pub fn clear(&mut self) {
        self.line.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_taps_are_exact() {
        let mut d = DelayLine::new(64);
        for i in 0..100 {
            d.push(i as f32);
        }
        assert_eq!(d.tap_int(0), 99.0);
        assert_eq!(d.tap_int(10), 89.0);
        assert_eq!(d.tap_hermite(10.0), 89.0);
        assert!((d.tap_linear(10.5) - 88.5).abs() < 1e-6);
        assert!((d.tap_hermite(10.5) - 88.5).abs() < 1e-5);
    }

    #[test]
    fn allpass_preserves_energy() {
        let mut a = Allpass::new(113, 0.6);
        let mut e_in = 0.0f64;
        let mut e_out = 0.0f64;
        for i in 0..20_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let y = a.tick(x);
            e_in += (x * x) as f64;
            e_out += (y * y) as f64;
        }
        assert!((e_out / e_in - 1.0).abs() < 1e-3, "{e_out}");
    }
}
