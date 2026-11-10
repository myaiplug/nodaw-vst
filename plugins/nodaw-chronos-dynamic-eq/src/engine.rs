//! The DSP engine of NoDAW Chronos Dynamic EQ: seven dynamic bands in series, each a static
//! bell or shelf plus a split-band compressor or upward expander that moves its gain with the
//! level inside the band, then output gain and a dry/wet mix.
//!
//! ```text
//! in -> input gain -+-> band 1 -> ... -> band 7 -+-> (solo audition) -> output -+-> mix -> bypass xfade
//!                   +-> dry ---------------------------------------------------^
//! ```
//!
//! A band is `x + (G - 1) * F(x)` (nodaw-dsp `DynBand`), so its gain moves every sample with no
//! zipper and an idle band is bit-transparent. Every stage is IIR: no latency. Every buffer is
//! allocated in [`Engine::new`]; [`Engine::process`] never allocates.

use nodaw_dsp::dynamics::{BandKind, DynBand, DynMode, DynSettings};
use nodaw_dsp::eq::{EqBand, Shape};
use nodaw_dsp::util::{db_to_gain, undenorm, Smooth};

/// Dynamic bands.
pub const BANDS: usize = 7;
/// Default band frequencies.
pub const DEFAULT_FREQS: [f32; BANDS] = [60.0, 150.0, 400.0, 1000.0, 2500.0, 6000.0, 12_000.0];

/// Everything one block needs, in plain units.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub bypass: bool,
    pub in_db: f32,
    pub out_db: f32,
    /// 0 ..= 1.
    pub mix: f32,
    /// Scales every band's dynamic range, 0 ..= 2.
    pub dyn_amount: f32,
    pub bands: [DynSettings; BANDS],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass: false,
            in_db: 0.0,
            out_db: 0.0,
            mix: 1.0,
            dyn_amount: 1.0,
            bands: std::array::from_fn(|i| DynSettings {
                on: true,
                kind: default_kind(i),
                freq: DEFAULT_FREQS[i],
                q: 1.0,
                gain_db: 0.0,
                mode: DynMode::Off,
                threshold_db: -24.0,
                ratio: 3.0,
                range_db: 6.0,
                attack_ms: 5.0,
                release_ms: 120.0,
                solo: false,
            }),
        }
    }
}

/// Bands 1 and 7 start as shelves, the rest as bells.
pub fn default_kind(i: usize) -> BandKind {
    match i {
        0 => BandKind::LowShelf,
        6 => BandKind::HighShelf,
        _ => BandKind::Bell,
    }
}

/// What the engine reports about a block, for the meters.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockInfo {
    /// Each band's dynamic gain at the end of the block, dB (negative: cut).
    pub dyn_db: [f32; BANDS],
    /// The deepest cut any band made in the block, as a linear gain (for the GR meter).
    pub min_gain: f32,
}

/// The processing engine.
pub struct Engine {
    sr: f64,
    bands: [DynBand; BANDS],
    audition: [EqBand; BANDS],
    was_on: [bool; BANDS],
    in_gain: Smooth,
    out_gain: Smooth,
    mix: Smooth,
    bypass: Smooth,
    solo_amt: Smooth,
    fresh: bool,
}

impl Engine {
    /// Allocate everything for `sample_rate`. Call from `activate`.
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate;
        let sm = |v: f32, ms: f32| Smooth::new(v, sr, ms, ms);
        Self {
            sr,
            bands: std::array::from_fn(|_| DynBand::new(sr)),
            audition: std::array::from_fn(|_| EqBand::new((sr * 0.02) as u32)),
            was_on: [true; BANDS],
            in_gain: sm(1.0, 15.0),
            out_gain: sm(1.0, 15.0),
            mix: sm(1.0, 20.0),
            bypass: sm(0.0, 8.0),
            solo_amt: sm(0.0, 10.0),
            fresh: true,
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    /// No latency.
    pub fn latency(&self) -> usize {
        0
    }

    /// Clear all state; the next block jumps the smoothers to its settings.
    pub fn reset(&mut self) {
        for b in &mut self.bands {
            b.reset();
        }
        for a in &mut self.audition {
            a.reset();
        }
        self.fresh = true;
    }

    /// One band's settings as the DSP runs them (the dynamic range scaled by the amount).
    fn band_settings(s: &Settings, i: usize) -> DynSettings {
        let mut b = s.bands[i];
        b.range_db = (b.range_db * s.dyn_amount.clamp(0.0, 2.0)).min(24.0);
        b.solo = false;
        b.freq = b.freq.clamp(10.0, 24_000.0);
        b
    }

    /// Process one block in place. (The band loops index several per-band arrays at once.)
    #[allow(clippy::needless_range_loop)]
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], s: &Settings) -> BlockInfo {
        let n = l.len().min(r.len());
        let nyq = (self.sr * 0.45) as f32;
        let mut set = [DynSettings::default(); BANDS];
        for i in 0..BANDS {
            set[i] = Self::band_settings(s, i);
            set[i].freq = set[i].freq.min(nyq);
            if set[i].on && !self.was_on[i] {
                self.bands[i].reset();
            }
            self.was_on[i] = set[i].on;
            self.bands[i].prepare(&set[i]);
            let shape = match set[i].kind {
                BandKind::Bell => Shape::BandPass,
                BandKind::LowShelf => Shape::LowPass,
                BandKind::HighShelf => Shape::HighPass,
            };
            self.audition[i].set(shape, set[i].freq, set[i].q.max(0.5), 0.0, self.sr);
        }
        let any_solo = s.bands.iter().any(|b| b.solo && b.on);
        let in_t = db_to_gain(s.in_db);
        let out_t = db_to_gain(s.out_db);
        let byp_t = if s.bypass { 1.0 } else { 0.0 };
        let solo_t = if any_solo { 1.0 } else { 0.0 };
        if self.fresh {
            self.fresh = false;
            self.in_gain.reset(in_t);
            self.out_gain.reset(out_t);
            self.mix.reset(s.mix);
            self.bypass.reset(byp_t);
            self.solo_amt.reset(solo_t);
            for a in &mut self.audition {
                a.reset();
            }
        }
        let solo_active = any_solo || self.solo_amt.value() > 0.0;
        let mut min_gain = 1.0f32;
        for i in 0..n {
            let (xl, xr) = (l[i], r[i]);
            let (xl, xr) = (
                if xl.is_finite() { xl } else { 0.0 },
                if xr.is_finite() { xr } else { 0.0 },
            );
            let g = self.in_gain.tick(in_t);
            let (il, ir) = (xl * g, xr * g);
            let (mut a, mut b) = (il, ir);
            let (mut sl, mut sr) = (0.0f32, 0.0f32);
            for k in 0..BANDS {
                if !set[k].on {
                    self.audition[k].advance();
                    continue;
                }
                if solo_active {
                    if s.bands[k].solo {
                        let (y0, y1) = self.audition[k].tick(a, b);
                        sl += y0;
                        sr += y1;
                    } else {
                        self.audition[k].advance();
                    }
                }
                (a, b) = self.bands[k].tick(a, b, &set[k]);
            }
            if solo_active {
                let sa = self.solo_amt.tick(solo_t);
                a += (sl - a) * sa;
                b += (sr - b) * sa;
            } else {
                for au in &mut self.audition {
                    au.advance();
                }
            }
            let og = self.out_gain.tick(out_t);
            let mx = self.mix.tick(s.mix);
            let (ol, or) = (xl + (a * og - xl) * mx, xr + (b * og - xr) * mx);
            let bb = self.bypass.tick(byp_t);
            // APF runs process() with flush-to-zero on; this keeps the engine clean without it.
            l[i] = undenorm(ol + (xl - ol) * bb);
            r[i] = undenorm(or + (xr - or) * bb);
        }
        let mut dyn_db = [0.0f32; BANDS];
        for k in 0..BANDS {
            dyn_db[k] = if set[k].on {
                self.bands[k].dyn_db()
            } else {
                0.0
            };
            if dyn_db[k] < 0.0 {
                min_gain = min_gain.min(db_to_gain(dyn_db[k]));
            }
        }
        BlockInfo { dyn_db, min_gain }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, hz: f64, sr: f64, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (amp as f64 * (2.0 * std::f64::consts::PI * hz * i as f64 / sr).sin()) as f32)
            .collect()
    }
    fn run(
        e: &mut Engine,
        s: &Settings,
        x: &[f32],
        block: usize,
    ) -> (Vec<f32>, Vec<f32>, BlockInfo) {
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        let mut info = BlockInfo::default();
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            info = e.process(a, b, s);
        }
        (l, r, info)
    }
    fn peak(v: &[f32]) -> f32 {
        v.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn defaults_are_bit_transparent() {
        let x = sine(9600, 440.0, 48_000.0, 0.5);
        let (l, r, _) = run(&mut Engine::new(48_000.0), &Settings::default(), &x, 333);
        assert_eq!(l, x);
        assert_eq!(r, x);
    }

    #[test]
    fn a_compressing_band_cuts_only_when_the_band_is_loud() {
        let sr = 48_000.0;
        let mut s = Settings::default();
        s.bands[3] = DynSettings {
            mode: DynMode::Compress,
            threshold_db: -30.0,
            ratio: 6.0,
            range_db: 12.0,
            ..s.bands[3]
        };
        let (l, _, info) = run(
            &mut Engine::new(sr),
            &s,
            &sine(48_000, 1000.0, sr, 0.5),
            256,
        );
        assert!(info.dyn_db[3] < -6.0, "{:?}", info.dyn_db);
        assert!(peak(&l[24_000..]) < 0.5 * db_to_gain(-6.0));
        let (l, _, info) = run(
            &mut Engine::new(sr),
            &s,
            &sine(48_000, 1000.0, sr, 0.005),
            256,
        );
        assert!(info.dyn_db[3].abs() < 0.1);
        assert!((peak(&l[24_000..]) - 0.005).abs() < 5e-4);
        // another band's frequency is left alone
        let (l, _, _) = run(&mut Engine::new(sr), &s, &sine(48_000, 60.0, sr, 0.5), 256);
        assert!(peak(&l[24_000..]) > 0.45);
    }

    #[test]
    fn an_expanding_band_lifts_when_the_band_is_loud() {
        let sr = 48_000.0;
        let mut s = Settings::default();
        s.bands[5] = DynSettings {
            mode: DynMode::Expand,
            threshold_db: -30.0,
            ratio: 4.0,
            range_db: 6.0,
            ..s.bands[5]
        };
        let (l, _, info) = run(
            &mut Engine::new(sr),
            &s,
            &sine(48_000, 6000.0, sr, 0.3),
            256,
        );
        assert!(info.dyn_db[5] > 5.0, "{:?}", info.dyn_db);
        assert!(peak(&l[24_000..]) > 0.3 * db_to_gain(5.0));
    }

    #[test]
    fn the_amount_scales_the_dynamics_and_off_bands_do_nothing() {
        let sr = 48_000.0;
        let mut s = Settings {
            dyn_amount: 0.0,
            ..Settings::default()
        };
        s.bands[3] = DynSettings {
            mode: DynMode::Compress,
            threshold_db: -40.0,
            ratio: 10.0,
            range_db: 12.0,
            gain_db: 0.0,
            ..s.bands[3]
        };
        let x = sine(24_000, 1000.0, sr, 0.5);
        let (l, _, _) = run(&mut Engine::new(sr), &s, &x, 256);
        assert!(l.iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-5));
        let mut s2 = Settings::default();
        s2.bands[3] = DynSettings {
            on: false,
            gain_db: 12.0,
            ..s2.bands[3]
        };
        let (l, _, _) = run(&mut Engine::new(sr), &s2, &x, 256);
        assert_eq!(l, x);
    }

    #[test]
    fn solo_auditions_the_band() {
        let sr = 48_000.0;
        let mut s = Settings::default();
        s.bands[1].solo = true; // 150 Hz bell
        let (l, _, _) = run(
            &mut Engine::new(sr),
            &s,
            &sine(24_000, 8000.0, sr, 0.2),
            256,
        );
        assert!(peak(&l[12_000..]) < 0.01);
        let (l, _, _) = run(&mut Engine::new(sr), &s, &sine(24_000, 150.0, sr, 0.2), 256);
        assert!(peak(&l[12_000..]) > 0.18);
    }
}
