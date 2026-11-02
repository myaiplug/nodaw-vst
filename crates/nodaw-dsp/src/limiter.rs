//! A look-ahead true-peak brick-wall limiter. Same algorithm as the HALFSCREW web app's output
//! limiter (4x-oversampled true-peak detector, sliding minimum, moving average, release),
//! ported to Rust so a NoDAW plugin and the web apps limit identically.
//!
//! Latency is `lookahead + 16` samples. Under the ceiling the signal passes unchanged (bit
//! exact apart from the delay).

const TAPS: usize = 32;
const HALF: usize = 16;
/// Extra detector delay in samples.
const DET: usize = 17;

fn bessel0(x: f64) -> f64 {
    let (mut s, mut t) = (1.0f64, 1.0f64);
    for k in 1..30 {
        let h = x / (2.0 * k as f64);
        t *= h * h;
        s += t;
    }
    s
}

/// Interpolator coefficients for the points 1/4, 2/4, 3/4 between two samples.
fn tp_coeffs() -> [[f32; TAPS]; 3] {
    let mut out = [[0.0f32; TAPS]; 3];
    let beta = 9.0;
    let i0 = bessel0(beta);
    for (k, row) in out.iter_mut().enumerate() {
        let f = (k + 1) as f64 / 4.0;
        for (m, c) in row.iter_mut().enumerate() {
            let d = m as f64 - (HALF as f64 - 1.0) - f;
            let sinc = if d.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * d).sin() / (std::f64::consts::PI * d) };
            let r = d / HALF as f64;
            let w = if r.abs() >= 1.0 { 0.0 } else { bessel0(beta * (1.0 - r * r).sqrt()) / i0 };
            *c = (sinc * w) as f32;
        }
    }
    out
}

/// The limiter (stereo).
#[derive(Debug, Clone)]
pub struct Limiter {
    tp: [[f32; TAPS]; 3],
    ceil: f32,
    l: usize,
    w: usize,
    d: usize,
    rel: f32,
    hist: [[f32; TAPS]; 2],
    delay: [Vec<f32>; 2],
    dpos: usize,
    dq_val: Vec<f32>,
    dq_idx: Vec<u64>,
    dq_head: usize,
    dq_len: usize,
    avg: Vec<f32>,
    avg_pos: usize,
    avg_sum: f64,
    n: u64,
    gr: f32,
}

impl Limiter {
    /// A limiter with `lookahead_ms` of look-ahead and `release_ms` release.
    pub fn new(sample_rate: f64, lookahead_ms: f32, release_ms: f32) -> Self {
        let l = ((lookahead_ms as f64 / 1000.0 * sample_rate).round() as usize).max(8);
        let w = 2 * l - 1;
        let d = l + DET - 1;
        let cap = w + 2;
        Self {
            tp: tp_coeffs(),
            ceil: 1.0,
            l,
            w,
            d,
            rel: 1.0 - (-1.0 / (release_ms as f64 / 1000.0 * sample_rate)).exp() as f32,
            hist: [[0.0; TAPS]; 2],
            delay: [vec![0.0; d], vec![0.0; d]],
            dpos: 0,
            dq_val: vec![0.0; cap],
            dq_idx: vec![0; cap],
            dq_head: 0,
            dq_len: 0,
            avg: vec![1.0; l],
            avg_pos: 0,
            avg_sum: l as f64,
            n: 0,
            gr: 1.0,
        }
    }

    /// Latency in samples.
    pub fn latency(&self) -> usize {
        self.d
    }

    /// Set the ceiling in dBTP (an internal 0.3 dB margin covers the detector's estimate error).
    pub fn set_ceiling_db(&mut self, db: f32) {
        self.ceil = 10f32.powf((db - 0.3) * 0.05);
    }

    /// Effectively switch the limiter off (a huge ceiling) while keeping its latency.
    pub fn set_off(&mut self) {
        self.ceil = 1.0e4;
    }

    /// Clear state.
    pub fn reset(&mut self) {
        self.hist = [[0.0; TAPS]; 2];
        self.delay[0].fill(0.0);
        self.delay[1].fill(0.0);
        self.dpos = 0;
        self.dq_head = 0;
        self.dq_len = 0;
        self.avg.fill(1.0);
        self.avg_pos = 0;
        self.avg_sum = self.l as f64;
        self.n = 0;
        self.gr = 1.0;
    }

    /// One stereo frame; returns the delayed, limited frame and the gain applied (1 = none).
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32, f32) {
        let ins = [l, r];
        let mut peak = 0.0f32;
        let th = self.ceil * 0.3;
        for c in 0..2 {
            let h = &mut self.hist[c];
            h.copy_within(1.., 0);
            h[TAPS - 1] = ins[c];
            let a = h[HALF - 1].abs();
            if a > peak {
                peak = a;
            }
            let loud = h[HALF - 4..HALF + 4].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            if loud > th {
                for k in 0..3 {
                    let y: f32 = self.tp[k].iter().zip(h.iter()).map(|(a, b)| a * b).sum();
                    if y.abs() > peak {
                        peak = y.abs();
                    }
                }
            }
        }
        let t = if peak > self.ceil { self.ceil / peak } else { 1.0 };
        let cap = self.dq_val.len();
        let s = self.n;
        self.n += 1;
        while self.dq_len > 0 {
            let last = (self.dq_head + self.dq_len - 1) % cap;
            if self.dq_val[last] >= t {
                self.dq_len -= 1;
            } else {
                break;
            }
        }
        let tail = (self.dq_head + self.dq_len) % cap;
        self.dq_val[tail] = t;
        self.dq_idx[tail] = s;
        self.dq_len += 1;
        while self.dq_idx[self.dq_head] + self.w as u64 <= s {
            self.dq_head = (self.dq_head + 1) % cap;
            self.dq_len -= 1;
        }
        let m = self.dq_val[self.dq_head];
        self.avg_sum += (m - self.avg[self.avg_pos]) as f64;
        self.avg[self.avg_pos] = m;
        self.avg_pos = (self.avg_pos + 1) % self.l;
        let mut g = (self.avg_sum / self.l as f64) as f32;
        if g > 1.0 {
            g = 1.0;
        }
        let rec = self.gr + (1.0 - self.gr) * self.rel;
        self.gr = if g < rec { g } else { rec };
        if self.gr > 0.9999 {
            self.gr = 1.0;
        }
        let dp = self.dpos;
        let (xl, xr) = (self.delay[0][dp], self.delay[1][dp]);
        self.delay[0][dp] = l;
        self.delay[1][dp] = r;
        self.dpos = if dp + 1 == self.d { 0 } else { dp + 1 };
        if self.gr == 1.0 {
            (xl, xr, 1.0)
        } else {
            (xl * self.gr, xr * self.gr, self.gr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn under_the_ceiling_the_signal_is_only_delayed() {
        let mut lim = Limiter::new(48_000.0, 2.0, 90.0);
        lim.set_ceiling_db(-1.0);
        let lat = lim.latency();
        let x: Vec<f32> = (0..5000).map(|i| 0.5 * (i as f32 * 0.07).sin()).collect();
        let y: Vec<f32> = x.iter().map(|&v| lim.tick(v, -v).0).collect();
        for i in lat..x.len() {
            assert_eq!(y[i], x[i - lat]);
        }
    }

    #[test]
    fn a_hot_signal_never_exceeds_the_ceiling() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            let mut lim = Limiter::new(sr, 2.0, 90.0);
            lim.set_ceiling_db(-1.0);
            let ceil = 10f32.powf(-1.0 / 20.0);
            let mut peak = 0.0f32;
            let mut n = crate::util::Noise::new(3);
            for i in 0..sr as usize * 2 {
                let x = 4.0 * (i as f32 * 0.11).sin() + 0.5 * n.sample();
                let (a, b, _) = lim.tick(x, -x * 0.7);
                peak = peak.max(a.abs()).max(b.abs());
            }
            assert!(peak <= ceil * 1.002, "sr {sr}: peak {peak} vs ceiling {ceil}");
            assert!(peak > ceil * 0.8, "the limiter should not over-attenuate: {peak}");
        }
    }

    #[test]
    fn an_impulse_comes_out_exactly_latency_samples_later() {
        let mut lim = Limiter::new(48_000.0, 2.0, 90.0);
        lim.set_ceiling_db(0.0);
        let lat = lim.latency();
        let mut out = vec![];
        for i in 0..lat + 50 {
            out.push(lim.tick(if i == 10 { 0.25 } else { 0.0 }, 0.0).0);
        }
        assert_eq!(out[10 + lat], 0.25);
        assert_eq!(out.iter().filter(|v| **v != 0.0).count(), 1);
    }

    #[test]
    fn off_keeps_the_latency_and_lets_peaks_through() {
        let mut lim = Limiter::new(48_000.0, 2.0, 90.0);
        lim.set_off();
        let lat = lim.latency();
        let mut out = vec![];
        for i in 0..lat + 20 {
            out.push(lim.tick(if i == 3 { 3.0 } else { 0.0 }, 0.0).0);
        }
        assert_eq!(out[3 + lat], 3.0);
    }

    #[test]
    fn the_gain_recovers_after_a_burst() {
        let mut lim = Limiter::new(48_000.0, 2.0, 50.0);
        lim.set_ceiling_db(-1.0);
        for i in 0..4800 {
            lim.tick(3.0 * (i as f32 * 0.2).sin(), 0.0);
        }
        let mut last = 0.0;
        for _ in 0..48_000 {
            last = lim.tick(0.01, 0.0).2;
        }
        assert_eq!(last, 1.0);
    }
}
