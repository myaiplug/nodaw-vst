//! A stereo feedback-delay-network reverb: 8 modulated delay lines, a Householder feedback
//! matrix, per-line damping filters tuned for an exact RT60, input diffusion and pre-delay.

use crate::delay::{Allpass, DelayLine};
use crate::util::undenorm;

const N: usize = 8;
/// Line lengths in milliseconds at size 1.0 (mutually inharmonic).
const LINE_MS: [f32; N] = [29.7, 37.1, 41.1, 43.7, 53.3, 59.9, 67.7, 79.1];
/// Pre-delay upper limit in seconds.
pub const MAX_PREDELAY_SECONDS: f32 = 0.25;
/// Largest `size` factor accepted.
pub const MAX_SIZE: f32 = 2.0;
/// Longest RT60 accepted, in seconds.
pub const MAX_DECAY_SECONDS: f32 = 20.0;

#[derive(Debug, Clone)]
struct Line {
    buf: Vec<f32>,
    mask: usize,
    pos: usize,
    len: f32,
    lp: f32,
    lfo: f32,
    lfo_inc: f32,
}

/// Parameters of the reverb (all plain units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReverbParams {
    /// RT60 in seconds.
    pub decay: f32,
    /// Room size factor (0.5 ..= 2.0).
    pub size: f32,
    /// Damping low-pass in Hz.
    pub damp_hz: f32,
    /// Pre-delay in milliseconds.
    pub predelay_ms: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            decay: 3.0,
            size: 1.0,
            damp_hz: 6000.0,
            predelay_ms: 20.0,
        }
    }
}

/// The reverb.
#[derive(Debug, Clone)]
pub struct Reverb {
    sr: f32,
    lines: [Line; N],
    target_len: [f32; N],
    gain: [f32; N],
    damp: f32,
    diff: [[Allpass; 2]; 2],
    pre: [DelayLine; 2],
    pre_len: f32,
    pre_target: f32,
    tail_seconds: f32,
}

impl Reverb {
    /// Allocate everything for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate as f32;
        let max_len = (LINE_MS[N - 1] * 0.001 * sr * MAX_SIZE * 1.15) as usize + 16;
        let n = crate::util::next_pow2(max_len);
        let mk = |i: usize| Line {
            buf: vec![0.0; n],
            mask: n - 1,
            pos: 0,
            len: LINE_MS[i] * 0.001 * sr,
            lp: 0.0,
            lfo: i as f32 * 0.125,
            lfo_inc: (0.11 + 0.043 * i as f32) / sr,
        };
        let scale = sr / 44_100.0;
        let ap = |l: f32| Allpass::new((l * scale) as usize, 0.6);
        let mut r = Self {
            sr,
            lines: std::array::from_fn(mk),
            target_len: [0.0; N],
            gain: [0.0; N],
            damp: 0.0,
            diff: [[ap(142.0), ap(107.0)], [ap(151.0), ap(113.0)]],
            pre: [
                DelayLine::new((MAX_PREDELAY_SECONDS * sr) as usize + 4),
                DelayLine::new((MAX_PREDELAY_SECONDS * sr) as usize + 4),
            ],
            pre_len: 0.0,
            pre_target: 0.0,
            tail_seconds: 0.0,
        };
        r.set(ReverbParams::default());
        for i in 0..N {
            r.lines[i].len = r.target_len[i];
        }
        r.pre_len = r.pre_target;
        r
    }

    /// Update the parameters (cheap; call once per block).
    pub fn set(&mut self, p: ReverbParams) {
        let size = p.size.clamp(0.5, MAX_SIZE);
        let decay = p.decay.clamp(0.1, MAX_DECAY_SECONDS);
        for i in 0..N {
            let len = LINE_MS[i] * 0.001 * self.sr * size;
            self.target_len[i] = len;
            // -60 dB after `decay` seconds: gain per pass = 10^(-3 * (len/sr) / decay)
            self.gain[i] = 10f32.powf(-3.0 * (len / self.sr) / decay);
        }
        self.damp =
            (-2.0 * std::f32::consts::PI * p.damp_hz.clamp(500.0, 20_000.0) / self.sr).exp();
        self.pre_target =
            (p.predelay_ms.clamp(0.0, MAX_PREDELAY_SECONDS * 1000.0) * 0.001 * self.sr).max(0.0);
        self.tail_seconds = decay + p.predelay_ms * 0.001;
    }

    /// Longest time (seconds) a signal takes to die away at the current settings.
    pub fn tail_seconds(&self) -> f32 {
        self.tail_seconds
    }

    /// The worst-case tail for the parameter ranges (for the host's tail report), in seconds.
    pub const MAX_TAIL_SECONDS: f32 = MAX_DECAY_SECONDS + MAX_PREDELAY_SECONDS;

    /// Jump the delay lengths and the pre-delay to the last [`Reverb::set`] (no glide), for a
    /// fresh start at new settings.
    pub fn snap(&mut self) {
        for i in 0..N {
            self.lines[i].len = self.target_len[i];
        }
        self.pre_len = self.pre_target;
    }

    /// Clear all state.
    pub fn reset(&mut self) {
        for l in &mut self.lines {
            l.buf.fill(0.0);
            l.lp = 0.0;
        }
        for d in self.diff.iter_mut().flatten() {
            d.clear();
        }
        self.pre[0].clear();
        self.pre[1].clear();
    }

    /// One stereo frame in, one wet stereo frame out (100 % wet).
    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        // pre-delay with a slewed length (no clicks when it moves)
        self.pre_len = crate::util::glide(self.pre_len, self.pre_target, 0.0008);
        self.pre[0].push(in_l);
        self.pre[1].push(in_r);
        let (pl, pr) = if self.pre_len < 1.0 {
            (in_l, in_r)
        } else {
            (
                self.pre[0].tap_linear(self.pre_len),
                self.pre[1].tap_linear(self.pre_len),
            )
        };
        let mut dl = pl;
        let mut dr = pr;
        for a in &mut self.diff[0] {
            dl = a.tick(dl);
        }
        for a in &mut self.diff[1] {
            dr = a.tick(dr);
        }

        let mut out = [0.0f32; N];
        for i in 0..N {
            let l = &mut self.lines[i];
            l.len = crate::util::glide(l.len, self.target_len[i], 0.0004);
            l.lfo += l.lfo_inc;
            l.lfo -= l.lfo.floor();
            let m = (2.0 * std::f32::consts::PI * l.lfo).sin() * (0.00028 * self.sr);
            let d = (l.len + m).max(2.0);
            let di = d as usize;
            let f = d - di as f32;
            let a = l.buf[(l.pos.wrapping_sub(1).wrapping_sub(di)) & l.mask];
            let b = l.buf[(l.pos.wrapping_sub(2).wrapping_sub(di)) & l.mask];
            let x = a + (b - a) * f;
            l.lp = undenorm((1.0 - self.damp) * x + self.damp * l.lp);
            out[i] = l.lp * self.gain[i];
        }
        let sum: f32 = out.iter().sum();
        let k = sum * (2.0 / N as f32);
        let (mut wl, mut wr) = (0.0f32, 0.0f32);
        for i in 0..N {
            let fb = out[i] - k;
            let inj = if i % 2 == 0 { dl } else { dr };
            let sign = if (i / 2) % 2 == 0 { 1.0 } else { -1.0 };
            let l = &mut self.lines[i];
            l.buf[l.pos & l.mask] = undenorm(fb + inj * 0.35);
            l.pos = l.pos.wrapping_add(1);
            if i % 2 == 0 {
                wl += out[i] * sign;
            } else {
                wr += out[i] * sign;
            }
        }
        (wl * 0.5, wr * 0.5)
    }

    /// True if any internal buffer holds a subnormal number (a test hook).
    pub fn holds_subnormals(&self) -> bool {
        self.lines
            .iter()
            .any(|l| l.lp.is_subnormal() || l.buf.iter().any(|v| v.is_subnormal()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_response(sr: f64, p: ReverbParams, secs: f32) -> Vec<f32> {
        let mut r = Reverb::new(sr);
        r.set(p);
        // let the slewed lengths settle first
        for _ in 0..(sr as usize * 2) {
            r.tick(0.0, 0.0);
        }
        r.reset();
        let n = (sr as f32 * secs) as usize;
        (0..n)
            .map(|i| {
                r.tick(
                    if i == 0 { 1.0 } else { 0.0 },
                    if i == 0 { 1.0 } else { 0.0 },
                )
                .0
            })
            .collect()
    }

    fn energy_db(x: &[f32]) -> f32 {
        let e: f64 = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64;
        10.0 * (e.max(1e-30)).log10() as f32
    }

    #[test]
    fn decay_time_is_close_to_the_requested_rt60() {
        let sr = 48_000.0;
        for rt in [1.0f32, 3.0, 6.0] {
            let ir = impulse_response(
                sr,
                ReverbParams {
                    decay: rt,
                    size: 1.0,
                    damp_hz: 20_000.0,
                    predelay_ms: 0.0,
                },
                rt * 1.6,
            );
            // energy in 100 ms windows; the slope between 0.15 s and rt*0.75 s extrapolated to -60 dB
            let w = (sr * 0.1) as usize;
            let level =
                |t: f32| energy_db(&ir[(t * sr as f32) as usize..(t * sr as f32) as usize + w]);
            let (t0, t1) = (0.15, rt * 0.7);
            let slope = (level(t1) - level(t0)) / (t1 - t0); // dB per second, negative
            let measured = -60.0 / slope;
            assert!(
                (measured / rt - 1.0).abs() < 0.3,
                "rt {rt}: measured {measured}"
            );
        }
    }

    #[test]
    fn it_is_stable_and_bounded_under_loud_noise() {
        let mut r = Reverb::new(44_100.0);
        r.set(ReverbParams {
            decay: 20.0,
            size: 2.0,
            damp_hz: 20_000.0,
            predelay_ms: 250.0,
        });
        let mut n = crate::util::Noise::new(7);
        let mut peak = 0.0f32;
        for _ in 0..44_100 * 20 {
            let (a, b) = r.tick(n.sample(), n.sample());
            assert!(a.is_finite() && b.is_finite());
            peak = peak.max(a.abs()).max(b.abs());
        }
        assert!(peak < 40.0, "{peak}");
    }

    #[test]
    fn a_decaying_tail_never_enters_the_subnormal_range() {
        let mut r = Reverb::new(48_000.0);
        r.set(ReverbParams {
            decay: 0.3,
            size: 0.5,
            damp_hz: 800.0,
            predelay_ms: 0.0,
        });
        r.tick(1.0, 1.0);
        for i in 0..48_000 * 20 {
            let (a, b) = r.tick(0.0, 0.0);
            assert!(!a.is_subnormal() && !b.is_subnormal());
            if i % 4096 == 0 {
                assert!(!r.holds_subnormals(), "subnormal at {i}");
            }
        }
        assert!(!r.holds_subnormals());
    }

    #[test]
    fn works_at_every_common_sample_rate() {
        for sr in [22_050.0, 44_100.0, 48_000.0, 88_200.0, 96_000.0, 192_000.0] {
            let ir = impulse_response(sr, ReverbParams::default(), 0.5);
            assert!(ir.iter().all(|v| v.is_finite()));
            assert!(ir.iter().any(|v| v.abs() > 1e-4), "{sr} produced nothing");
        }
    }

    #[test]
    fn left_and_right_outputs_differ_so_it_is_a_stereo_reverb() {
        let mut r = Reverb::new(48_000.0);
        let mut diff = 0.0f32;
        for i in 0..20_000 {
            let (a, b) = r.tick(
                if i == 0 { 1.0 } else { 0.0 },
                if i == 0 { 1.0 } else { 0.0 },
            );
            diff += (a - b).abs();
        }
        assert!(diff > 0.01);
    }
}
