//! Lock-free hand-over of levels and audio snapshots from the audio thread to the UI thread.
//! Everything here is plain atomics: the audio side never waits.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Peak and block-RMS of a stereo pair since the UI last asked.
#[derive(Debug, Default)]
pub struct LevelMeter {
    // For non-negative floats the bit patterns order like the floats, so `fetch_max` works.
    peak: [AtomicU32; 2],
    rms: [AtomicU32; 2],
}

impl LevelMeter {
    /// Audio thread: note one block.
    pub fn record(&self, l: &[f32], r: &[f32]) {
        for (c, x) in [l, r].into_iter().enumerate() {
            let mut peak = 0.0f32;
            let mut sum = 0.0f32;
            for &v in x {
                let a = v.abs();
                if a > peak {
                    peak = a;
                }
                sum += v * v;
            }
            if !peak.is_finite() {
                peak = 0.0;
            }
            let rms = if x.is_empty() { 0.0 } else { (sum / x.len() as f32).sqrt() };
            self.peak[c].fetch_max(peak.to_bits(), Ordering::Relaxed);
            self.rms[c].fetch_max(if rms.is_finite() { rms } else { 0.0 }.to_bits(), Ordering::Relaxed);
        }
    }
    /// UI thread: `[peak_l, peak_r, rms_l, rms_r]` since the last call; starts a new interval.
    pub fn take(&self) -> [f32; 4] {
        [
            f32::from_bits(self.peak[0].swap(0, Ordering::Relaxed)),
            f32::from_bits(self.peak[1].swap(0, Ordering::Relaxed)),
            f32::from_bits(self.rms[0].swap(0, Ordering::Relaxed)),
            f32::from_bits(self.rms[1].swap(0, Ordering::Relaxed)),
        ]
    }
}

/// The deepest gain reduction (as a linear gain, 1 = none) since the UI last asked.
#[derive(Debug)]
pub struct GainReduction(AtomicU32);

impl Default for GainReduction {
    fn default() -> Self {
        Self(AtomicU32::new(1.0f32.to_bits()))
    }
}

impl GainReduction {
    /// Audio thread.
    pub fn record(&self, gain: f32) {
        if gain.is_finite() && gain > 0.0 {
            self.0.fetch_min(gain.to_bits(), Ordering::Relaxed);
        }
    }
    /// UI thread.
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(1.0f32.to_bits(), Ordering::Relaxed))
    }
}

/// A ring of the most recent mono samples. The audio thread writes, the UI thread copies the
/// last N out for the analyser. A copy racing a write can see a few samples of the next lap;
/// for a display that is harmless, and no lock or `unsafe` is needed.
#[derive(Debug)]
pub struct ScopeRing {
    buf: Box<[AtomicU32]>,
    mask: usize,
    head: AtomicUsize,
}

impl ScopeRing {
    /// A ring of `capacity` samples (rounded up to a power of two).
    pub fn new(capacity: usize) -> Self {
        let n = capacity.max(2).next_power_of_two();
        Self { buf: (0..n).map(|_| AtomicU32::new(0)).collect(), mask: n - 1, head: AtomicUsize::new(0) }
    }
    /// Audio thread: append the mono sum of a stereo block.
    pub fn push_stereo(&self, l: &[f32], r: &[f32]) {
        let mut h = self.head.load(Ordering::Relaxed);
        for (a, b) in l.iter().zip(r) {
            let m = 0.5 * (a + b);
            self.buf[h & self.mask].store((if m.is_finite() { m } else { 0.0 }).to_bits(), Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
    /// UI thread: copy the most recent `out.len()` samples (oldest first).
    pub fn snapshot(&self, out: &mut [f32]) {
        let h = self.head.load(Ordering::Acquire);
        let n = out.len().min(self.buf.len());
        let start = h.wrapping_sub(n);
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = f32::from_bits(self.buf[start.wrapping_add(i) & self.mask].load(Ordering::Relaxed));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_meter_keeps_the_loudest_and_resets() {
        let m = LevelMeter::default();
        m.record(&[0.1, -0.7, 0.2], &[0.0; 3]);
        m.record(&[0.3, 0.3, 0.3], &[0.5, 0.5, 0.5]);
        let t = m.take();
        assert_eq!(t[0], 0.7);
        assert_eq!(t[1], 0.5);
        assert!((t[3] - 0.5).abs() < 1e-6);
        assert_eq!(m.take(), [0.0; 4]);
    }

    #[test]
    fn nan_never_reaches_the_meter() {
        let m = LevelMeter::default();
        m.record(&[f32::NAN, 0.2], &[f32::INFINITY, 0.1]);
        let t = m.take();
        assert!(t.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn gain_reduction_keeps_the_deepest() {
        let g = GainReduction::default();
        g.record(0.9);
        g.record(0.5);
        g.record(0.8);
        assert_eq!(g.take(), 0.5);
        assert_eq!(g.take(), 1.0);
    }

    #[test]
    fn scope_ring_returns_the_latest_samples_in_order() {
        let r = ScopeRing::new(8);
        let a: Vec<f32> = (0..20).map(|i| i as f32).collect();
        r.push_stereo(&a, &a);
        let mut out = [0.0; 5];
        r.snapshot(&mut out);
        assert_eq!(out, [15.0, 16.0, 17.0, 18.0, 19.0]);
        let mut big = [0.0; 100];
        r.snapshot(&mut big);
        assert_eq!(big[7], 19.0, "asking for more than the ring holds gives what it has");
    }
}
