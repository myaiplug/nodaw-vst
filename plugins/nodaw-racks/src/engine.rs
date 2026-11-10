//! The DSP engine of NoDAW Racks: four analog-style processors in a re-orderable chain.
//!
//! ```text
//! in -> input gain -+-> [Saturator | 7-band EQ | Spatial | Reactor Limiter] in the chosen order -+-> mix -> output -> bypass xfade
//!                   +-> dry delay (total latency) --------------------------------------------->+
//! in -> bypass delay (total latency) ----------------------------------------------------------------------------^
//! ```
//!
//! Every module always runs (a module that is off is a pure delay of its own latency, or no
//! delay at all), so the latency is the same for every setting and order: the saturator's
//! oversampling filters plus the limiter's look-ahead. Switching a module glides, never clicks.

use nodaw_dsp::delay::DelayLine;
use nodaw_dsp::eq::{EqBand, Shape};
use nodaw_dsp::limiter::Limiter;
use nodaw_dsp::modfx::{balance, Correlation};
use nodaw_dsp::sat::{Curve, Saturator};
use nodaw_dsp::util::{db_to_gain, undenorm, Smooth};

pub const ORDERS: [[usize; 4]; 24] = [[0, 1, 2, 3], [0, 1, 3, 2], [0, 2, 1, 3], [0, 2, 3, 1], [0, 3, 1, 2], [0, 3, 2, 1], [1, 0, 2, 3], [1, 0, 3, 2], [1, 2, 0, 3], [1, 2, 3, 0], [1, 3, 0, 2], [1, 3, 2, 0], [2, 0, 1, 3], [2, 0, 3, 1], [2, 1, 0, 3], [2, 1, 3, 0], [2, 3, 0, 1], [2, 3, 1, 0], [3, 0, 1, 2], [3, 0, 2, 1], [3, 1, 0, 2], [3, 1, 2, 0], [3, 2, 0, 1], [3, 2, 1, 0]];
pub const ORDER_NAMES: &[&str] = &["SAT > EQ > SPACE > LIM", "SAT > EQ > LIM > SPACE", "SAT > SPACE > EQ > LIM", "SAT > SPACE > LIM > EQ", "SAT > LIM > EQ > SPACE", "SAT > LIM > SPACE > EQ", "EQ > SAT > SPACE > LIM", "EQ > SAT > LIM > SPACE", "EQ > SPACE > SAT > LIM", "EQ > SPACE > LIM > SAT", "EQ > LIM > SAT > SPACE", "EQ > LIM > SPACE > SAT", "SPACE > SAT > EQ > LIM", "SPACE > SAT > LIM > EQ", "SPACE > EQ > SAT > LIM", "SPACE > EQ > LIM > SAT", "SPACE > LIM > SAT > EQ", "SPACE > LIM > EQ > SAT", "LIM > SAT > EQ > SPACE", "LIM > SAT > SPACE > EQ", "LIM > EQ > SAT > SPACE", "LIM > EQ > SPACE > SAT", "LIM > SPACE > SAT > EQ", "LIM > SPACE > EQ > SAT"];

/// Bands of the EQ.
pub const BANDS: usize = 7;
/// The longest Haas delay, in milliseconds.
pub const MAX_HAAS_MS: f32 = 30.0;
/// The limiter's look-ahead (fixed, so the latency never changes).
pub const LOOKAHEAD_MS: f32 = 1.5;

/// One EQ band in plain units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub on: bool,
    pub shape: Shape,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
}

/// Everything one block needs, in plain units.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub bypass: bool,
    pub in_db: f32,
    pub out_db: f32,
    /// 0 ..= 1.
    pub mix: f32,
    /// Module indices in signal order: 0 saturator, 1 EQ, 2 spatial, 3 limiter.
    pub order: [usize; 4],
    pub sat_on: bool,
    pub sat_curve: Curve,
    pub sat_drive_db: f32,
    pub sat_tone_db: f32,
    pub sat_mix: f32,
    pub sat_auto: bool,
    pub eq_on: bool,
    pub bands: [Band; BANDS],
    pub sp_on: bool,
    /// 0 ..= 2.
    pub width: f32,
    /// Negative delays the left channel, positive the right.
    pub haas_ms: f32,
    pub mid_db: f32,
    pub side_db: f32,
    /// -1 ..= 1.
    pub pan: f32,
    /// Side high-pass (mono bass) in Hz; at or below 20 Hz it is off.
    pub mono_hz: f32,
    pub lim_on: bool,
    pub lim_gain_db: f32,
    pub ceiling_db: f32,
    pub release_ms: f32,
    pub adaptive: bool,
}

/// The default bands: low shelf, five bells, high shelf.
pub const DEFAULT_FREQS: [f32; BANDS] = [80.0, 200.0, 500.0, 1200.0, 3000.0, 7000.0, 12_000.0];

impl Default for Settings {
    fn default() -> Self {
        let bands = std::array::from_fn(|i| Band {
            on: true,
            shape: if i == 0 {
                Shape::LowShelf
            } else if i == BANDS - 1 {
                Shape::HighShelf
            } else {
                Shape::Peak
            },
            freq: DEFAULT_FREQS[i],
            gain_db: 0.0,
            q: if i == 0 || i == BANDS - 1 { 0.707 } else { 1.0 },
        });
        Self {
            bypass: false,
            in_db: 0.0,
            out_db: 0.0,
            mix: 1.0,
            order: ORDERS[0],
            sat_on: true,
            sat_curve: Curve::Tape,
            sat_drive_db: 6.0,
            sat_tone_db: 0.0,
            sat_mix: 1.0,
            sat_auto: true,
            eq_on: true,
            bands,
            sp_on: true,
            width: 1.0,
            haas_ms: 0.0,
            mid_db: 0.0,
            side_db: 0.0,
            pan: 0.0,
            mono_hz: 20.0,
            lim_on: true,
            lim_gain_db: 0.0,
            ceiling_db: -1.0,
            release_ms: 120.0,
            adaptive: true,
        }
    }
}

/// What the engine reports about a block, for the meters.
#[derive(Debug, Clone, Copy)]
pub struct BlockInfo {
    /// Deepest limiter gain (1 = none).
    pub min_gain: f32,
    /// Peak level after each module (by module index), linear.
    pub module_peak: [f32; 4],
    /// Output correlation, -1 ..= 1.
    pub correlation: f32,
}

/// The processing engine.
pub struct Engine {
    sr: f64,
    sat: Saturator,
    tone: [EqBand; 2],
    eq: [EqBand; BANDS],
    mono: EqBand,
    haas: [DelayLine; 2],
    limiter: Limiter,
    dry: [DelayLine; 2],
    byp: [DelayLine; 2],
    lat: usize,
    corr: Correlation,
    in_gain: Smooth,
    out_gain: Smooth,
    mix: Smooth,
    bypass: Smooth,
    drive: Smooth,
    sat_mix: Smooth,
    sp_amt: Smooth,
    width: Smooth,
    mid: Smooth,
    side: Smooth,
    pan: Smooth,
    haas_d: Smooth,
    lim_gain: Smooth,
    rel_for: f32,
    fresh: bool,
}

fn ramp(sr: f64) -> u32 {
    (sr * 0.02) as u32
}

impl Engine {
    /// Allocate everything for `sample_rate`. Call from `activate`.
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate;
        let limiter = Limiter::new(sr, LOOKAHEAD_MS, 120.0);
        let sat = Saturator::new();
        let lat = sat.latency() + limiter.latency();
        let haas_len = (MAX_HAAS_MS as f64 * 0.001 * sr) as usize + 8;
        let sm = |v: f32, ms: f32| Smooth::new(v, sr, ms, ms);
        let mut e = Self {
            sr,
            sat,
            tone: [EqBand::new(ramp(sr)), EqBand::new(ramp(sr))],
            eq: std::array::from_fn(|_| EqBand::new(ramp(sr))),
            mono: EqBand::new(ramp(sr)),
            haas: [DelayLine::new(haas_len), DelayLine::new(haas_len)],
            limiter,
            dry: [DelayLine::new(lat + 4), DelayLine::new(lat + 4)],
            byp: [DelayLine::new(lat + 4), DelayLine::new(lat + 4)],
            lat,
            corr: Correlation::new(sr),
            in_gain: sm(1.0, 15.0),
            out_gain: sm(1.0, 15.0),
            mix: sm(1.0, 20.0),
            bypass: sm(0.0, 8.0),
            drive: sm(1.0, 20.0),
            sat_mix: sm(1.0, 20.0),
            sp_amt: sm(1.0, 20.0),
            width: sm(1.0, 20.0),
            mid: sm(1.0, 20.0),
            side: sm(1.0, 20.0),
            pan: sm(0.0, 20.0),
            haas_d: sm(0.0, 60.0),
            lim_gain: sm(1.0, 20.0),
            rel_for: -1.0,
            fresh: true,
        };
        e.reset_to(&Settings::default());
        e
    }

    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    /// Total latency in samples (constant for the engine's life).
    pub fn latency(&self) -> usize {
        self.lat
    }

    /// Clear all state; the next block jumps the smoothers and filters to its settings.
    pub fn reset_to(&mut self, s: &Settings) {
        self.sat.reset();
        self.limiter.reset();
        for d in self.dry.iter_mut().chain(self.byp.iter_mut()).chain(self.haas.iter_mut()) {
            d.clear();
        }
        self.corr = Correlation::new(self.sr);
        self.fresh = true;
        self.rel_for = -1.0;
        self.update_filters(s);
        for b in self.eq.iter_mut().chain(self.tone.iter_mut()) {
            b.reset();
        }
        self.mono.reset();
    }

    /// Clear the audio state.
    pub fn reset(&mut self) {
        self.sat.reset();
        self.limiter.reset();
        for d in self.dry.iter_mut().chain(self.byp.iter_mut()).chain(self.haas.iter_mut()) {
            d.clear();
        }
        for b in self.eq.iter_mut().chain(self.tone.iter_mut()) {
            b.reset();
        }
        self.mono.reset();
    }

    fn update_filters(&mut self, s: &Settings) {
        let sr = self.sr;
        let nyq = (sr * 0.45) as f32;
        for (b, set) in self.eq.iter_mut().zip(s.bands.iter()) {
            if s.eq_on && set.on {
                b.set(set.shape, set.freq.clamp(10.0, nyq), set.q, set.gain_db, sr);
            } else {
                b.set(Shape::Peak, 1000.0, 1.0, 0.0, sr);
            }
        }
        let t = if s.sat_on { s.sat_tone_db } else { 0.0 };
        self.tone[0].set(Shape::LowShelf, 700.0, 0.6, -0.5 * t, sr);
        self.tone[1].set(Shape::HighShelf, 1400.0, 0.6, 0.5 * t, sr);
        if s.sp_on && s.mono_hz > 20.5 {
            self.mono.set(Shape::HighPass, s.mono_hz.min(nyq), 0.707, 0.0, sr);
        } else {
            self.mono.set(Shape::Peak, 1000.0, 1.0, 0.0, sr);
        }
    }

    /// Process one block in place.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], s: &Settings) -> BlockInfo {
        let n = l.len().min(r.len());
        let sr = self.sr;
        self.update_filters(s);
        if s.lim_on {
            self.limiter.set_ceiling_db(s.ceiling_db);
        } else {
            self.limiter.set_off();
        }
        let in_t = db_to_gain(s.in_db);
        let out_t = db_to_gain(s.out_db);
        let byp_t = if s.bypass { 1.0 } else { 0.0 };
        let drive_t = db_to_gain(s.sat_drive_db);
        let makeup = if s.sat_auto { s.sat_curve.makeup(drive_t) } else { 1.0 };
        let sat_mix_t = if s.sat_on { s.sat_mix } else { 0.0 };
        let sp_t = if s.sp_on { 1.0 } else { 0.0 };
        let mid_t = db_to_gain(s.mid_db);
        let side_t = db_to_gain(s.side_db);
        let haas_t = (s.haas_ms.clamp(-MAX_HAAS_MS, MAX_HAAS_MS) as f64 * 0.001 * sr) as f32;
        let lim_g_t = if s.lim_on { db_to_gain(s.lim_gain_db) } else { 1.0 };
        if self.fresh {
            self.fresh = false;
            self.in_gain.reset(in_t);
            self.out_gain.reset(out_t);
            self.mix.reset(s.mix);
            self.bypass.reset(byp_t);
            self.drive.reset(drive_t);
            self.sat_mix.reset(sat_mix_t);
            self.sp_amt.reset(sp_t);
            self.width.reset(s.width);
            self.mid.reset(mid_t);
            self.side.reset(side_t);
            self.pan.reset(s.pan);
            self.haas_d.reset(haas_t);
            self.lim_gain.reset(lim_g_t);
            for b in self.eq.iter_mut().chain(self.tone.iter_mut()) {
                b.reset();
            }
            self.mono.reset();
        }
        let tone_active = !(self.tone[0].is_identity() && self.tone[1].is_identity());
        let eq_active = !self.eq.iter().all(|b| b.is_identity());
        let mut min_gain = 1.0f32;
        let mut peaks = [0.0f32; 4];
        let lat = self.lat;
        for i in 0..n {
            let (xl, xr) = (l[i], r[i]);
            let (xl, xr) = (if xl.is_finite() { xl } else { 0.0 }, if xr.is_finite() { xr } else { 0.0 });
            self.byp[0].push(xl);
            self.byp[1].push(xr);
            let (bl, br) = (self.byp[0].tap_int(lat), self.byp[1].tap_int(lat));
            let g = self.in_gain.tick(in_t);
            let (il, ir) = (xl * g, xr * g);
            self.dry[0].push(il);
            self.dry[1].push(ir);
            let (dl, dr) = (self.dry[0].tap_int(lat), self.dry[1].tap_int(lat));

            // per-sample smoothed module controls
            let drive = self.drive.tick(drive_t);
            let smix = self.sat_mix.tick(sat_mix_t);
            let amt = self.sp_amt.tick(sp_t);
            let w = self.width.tick(s.width);
            let mg = self.mid.tick(mid_t);
            let sg = self.side.tick(side_t);
            let pan = self.pan.tick(s.pan);
            let hd = self.haas_d.tick(haas_t);
            let lg = self.lim_gain.tick(lim_g_t);

            let (mut a, mut b) = (il, ir);
            for &m in &s.order {
                match m {
                    0 => {
                        let (y0, y1) = self.sat.tick(a, b, s.sat_curve, drive, makeup, smix);
                        let (y0, y1) = if tone_active {
                            let (t0, t1) = self.tone[0].tick(y0, y1);
                            self.tone[1].tick(t0, t1)
                        } else {
                            (y0, y1)
                        };
                        a = y0;
                        b = y1;
                    }
                    1 => {
                        if eq_active {
                            for band in self.eq.iter_mut() {
                                if !band.is_identity() {
                                    let (y0, y1) = band.tick(a, b);
                                    a = y0;
                                    b = y1;
                                }
                            }
                        }
                    }
                    2 => {
                        let (m0, s0) = (0.5 * (a + b), 0.5 * (a - b));
                        let s1 = self.mono.tick_ch(0, s0);
                        self.mono.advance();
                        let (m1, s2) = (m0 * mg, s1 * sg * w);
                        let (mut y0, mut y1) = (m1 + s2, m1 - s2);
                        self.haas[0].push(y0);
                        self.haas[1].push(y1);
                        if hd < -0.01 {
                            y0 = self.haas[0].tap_linear(-hd);
                        } else if hd > 0.01 {
                            y1 = self.haas[1].tap_linear(hd);
                        }
                        let (y0, y1) = balance(y0, y1, pan);
                        a += (y0 - a) * amt;
                        b += (y1 - b) * amt;
                    }
                    _ => {
                        let (y0, y1, gr) = self.limiter.tick(a * lg, b * lg);
                        if gr < min_gain {
                            min_gain = gr;
                        }
                        a = y0;
                        b = y1;
                    }
                }
                let p = a.abs().max(b.abs());
                if p > peaks[m] {
                    peaks[m] = p;
                }
            }
            if !eq_active {
                for band in self.eq.iter_mut() {
                    band.advance();
                }
            }
            if !tone_active {
                self.tone[0].advance();
                self.tone[1].advance();
            }
            let mx = self.mix.tick(s.mix);
            let og = self.out_gain.tick(out_t);
            let (ol, or) = ((dl + (a - dl) * mx) * og, (dr + (b - dr) * mx) * og);
            self.corr.push(ol, or);
            let bb = self.bypass.tick(byp_t);
            // APF runs process() with flush-to-zero on; this keeps the engine clean without it.
            l[i] = undenorm(ol + (bl - ol) * bb);
            r[i] = undenorm(or + (br - or) * bb);
        }
        // Adaptive release: the deeper the limiter works, the slower it lets go (program
        // dependent, less pumping on dense material).
        let rel = if s.adaptive { s.release_ms * (1.0 + 3.0 * (1.0 - min_gain).clamp(0.0, 0.5) * 2.0) } else { s.release_ms };
        if (rel - self.rel_for).abs() > 0.5 {
            self.rel_for = rel;
            self.limiter.set_release_ms(sr, rel);
        }
        BlockInfo { min_gain, module_peak: peaks, correlation: self.corr.value() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(n: usize, hz: f64, sr: f64, amp: f32) -> Vec<f32> {
        (0..n).map(|i| (amp as f64 * (2.0 * std::f64::consts::PI * hz * i as f64 / sr).sin()) as f32).collect()
    }

    fn run(e: &mut Engine, s: &Settings, x: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            e.process(a, b, s);
        }
        (l, r)
    }

    fn neutral() -> Settings {
        Settings { sat_on: false, lim_on: false, ..Settings::default() }
    }

    #[test]
    fn neutral_settings_are_a_pure_delay_in_every_order() {
        let sr = 48_000.0;
        let x = sine(9600, 440.0, sr, 0.5);
        for (k, order) in ORDERS.iter().enumerate().step_by(5) {
            let mut e = Engine::new(sr);
            let s = Settings { order: *order, ..neutral() };
            let (l, r) = run(&mut e, &s, &x, 256);
            let lat = e.latency();
            for i in lat + 2000..x.len() {
                assert!((l[i] - x[i - lat]).abs() < 2e-3, "order {k} @{i}: {} vs {}", l[i], x[i - lat]);
                assert!((r[i] - x[i - lat]).abs() < 2e-3);
            }
        }
    }

    #[test]
    fn the_limiter_holds_the_ceiling_and_reports_gain_reduction() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let s = Settings { sat_on: false, lim_gain_db: 12.0, ceiling_db: -1.0, ..Settings::default() };
        let x = sine(48_000, 100.0, sr, 0.9);
        let (mut l, mut r) = (x.clone(), x.clone());
        let mut min_g = 1.0f32;
        for (a, b) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            min_g = min_g.min(e.process(a, b, &s).min_gain);
        }
        assert!(min_g < 0.4, "{min_g}");
        let ceil = db_to_gain(-1.0) * 1.01;
        assert!(l[4800..].iter().chain(&r[4800..]).all(|v| v.abs() <= ceil), "over the ceiling");
    }

    #[test]
    fn saturation_adds_harmonics_and_auto_gain_keeps_the_level() {
        let sr = 48_000.0;
        let x = sine(48_000, 200.0, sr, 0.25);
        for curve in [Curve::Tape, Curve::Tube, Curve::Diode] {
            let mut e = Engine::new(sr);
            let s = Settings { sat_curve: curve, sat_drive_db: 18.0, lim_on: false, ..Settings::default() };
            let (l, _) = run(&mut e, &s, &x, 512);
            let rms = |v: &[f32]| (v.iter().map(|a| a * a).sum::<f32>() / v.len() as f32).sqrt();
            let (ri, ro) = (rms(&x[8000..]), rms(&l[8000..]));
            let ratio_db = 20.0 * (ro / ri).log10();
            assert!(ratio_db.abs() < 4.0, "{curve:?}: {ratio_db} dB");
            assert!(l.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn the_eq_boosts_where_asked() {
        let sr = 48_000.0;
        let mut s = neutral();
        s.bands[3].gain_db = 12.0;
        s.bands[3].freq = 1000.0;
        let mut e = Engine::new(sr);
        let x = sine(24_000, 1000.0, sr, 0.1);
        let (l, _) = run(&mut e, &s, &x, 128);
        let pk = l[12_000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((pk / 0.1 - db_to_gain(12.0)).abs() < 0.2, "{pk}");
    }

    #[test]
    fn width_zero_is_mono_and_mono_bass_folds_the_lows() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let s = Settings { width: 0.0, ..neutral() };
        let xl = sine(9600, 300.0, sr, 0.3);
        let xr: Vec<f32> = xl.iter().map(|v| -v * 0.5).collect();
        let (mut l, mut r) = (xl.clone(), xr.clone());
        e.process(&mut l, &mut r, &s);
        for i in 4000..9600 {
            assert!((l[i] - r[i]).abs() < 1e-3);
        }
        let mut e = Engine::new(sr);
        let s = Settings { mono_hz: 300.0, ..neutral() };
        let xl = sine(19200, 40.0, sr, 0.3);
        let xr: Vec<f32> = xl.iter().map(|v| -v).collect();
        let (mut l, mut r) = (xl.clone(), xr.clone());
        e.process(&mut l, &mut r, &s);
        let side = l[12_000..].iter().zip(&r[12_000..]).fold(0.0f32, |m, (a, b)| m.max((a - b).abs() * 0.5));
        assert!(side < 0.03, "side at 40 Hz {side}");
    }

    #[test]
    fn stays_finite_and_denormal_free_at_every_rate_and_block() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            for block in [1usize, 17, 64, 4096] {
                let mut e = Engine::new(sr);
                let mut s = Settings { sat_drive_db: 24.0, haas_ms: 12.0, width: 2.0, lim_gain_db: 18.0, mono_hz: 120.0, ..Settings::default() };
                s.bands[0].gain_db = 15.0;
                s.bands[6].shape = Shape::LowPass;
                s.bands[6].freq = 5000.0;
                let mut x = sine(8192, 90.0, sr, 0.8);
                x.extend(std::iter::repeat_n(0.0, sr as usize));
                let (l, r) = run(&mut e, &s, &x, block);
                for v in l.iter().chain(&r) {
                    assert!(v.is_finite(), "{sr} {block}");
                    assert!(*v == 0.0 || v.abs() >= f32::MIN_POSITIVE, "denormal at {sr} {block}: {v:e}");
                }
            }
        }
    }
}
