//! The DSP engine of NoDAW Harmonic Space (the Spectral Space Designer): an amp stage, a
//! 7-band EQ with band solo and mute, a sub shelf, high- and low-pass filters, then three
//! parallel space effects (feedback delay, FDN reverb, stereo flanger) over the filtered signal.
//!
//! ```text
//! in -+-> amp -> EQ (7 bands | solo audition) -> sub shelf -> HPF -> LPF -+-> (+) -> width -> output -+-> mix -> bypass xfade
//!     |                                                                   +-> delay   x delay mix ---^  |
//!     |                                                                   +-> reverb  x reverb mix --^  |
//!     |                                                                   +-> flanger x flanger mix -^  |
//!     +-> dry (the untouched input) ----------------------------------------------------------------------^
//! ```
//!
//! Everything is IIR or a plain delay line, so the plugin has no latency. Every buffer is
//! allocated in [`Engine::new`]; [`Engine::process`] never allocates.

use nodaw_dsp::eq::{EqBand, Shape};
use nodaw_dsp::modfx::{width, Correlation, FeedbackDelay, Flanger};
use nodaw_dsp::reverb::{Reverb, ReverbParams};
use nodaw_dsp::util::{db_to_gain, undenorm, Smooth};

/// EQ bands.
pub const BANDS: usize = 7;
/// The default band frequencies (the prototype's fixed bands: 60 Hz shelf ... 16 kHz shelf).
pub const DEFAULT_FREQS: [f32; BANDS] = [60.0, 200.0, 800.0, 2000.0, 4000.0, 8000.0, 16_000.0];
/// How far a muted band cuts, in dB.
pub const MUTE_DB: f32 = -36.0;
/// At or below this the high-pass is out.
pub const HPF_OFF_HZ: f32 = 20.5;
/// At or above this the low-pass is out.
pub const LPF_OFF_HZ: f32 = 21_900.0;

/// One EQ band in plain units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    pub solo: bool,
    pub mute: bool,
}

/// Everything one block needs, in plain units.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub bypass: bool,
    pub amp_db: f32,
    pub hpf_hz: f32,
    pub lpf_hz: f32,
    pub bands: [Band; BANDS],
    pub sub_db: f32,
    pub sub_hz: f32,
    /// Wet levels, 0 ..= 1.
    pub delay_mix: f32,
    pub delay_s: f32,
    /// 0 ..= 0.9.
    pub delay_fb: f32,
    pub delay_pp: bool,
    pub delay_tone_hz: f32,
    pub reverb_mix: f32,
    pub decay_s: f32,
    /// 0.5 ..= 2.
    pub size: f32,
    pub predelay_ms: f32,
    pub damp_hz: f32,
    pub flanger_mix: f32,
    pub fl_rate: f32,
    /// 0 ..= 1.
    pub fl_depth: f32,
    /// -0.9 ..= 0.9.
    pub fl_fb: f32,
    /// 0 ..= 2.
    pub width: f32,
    pub out_db: f32,
    /// 0 ..= 1.
    pub mix: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass: false,
            amp_db: 0.0,
            hpf_hz: 20.0,
            lpf_hz: 22_000.0,
            bands: std::array::from_fn(|i| Band { freq: DEFAULT_FREQS[i], gain_db: 0.0, q: if i == 0 || i == BANDS - 1 { 0.707 } else { 1.0 }, solo: false, mute: false }),
            sub_db: 0.0,
            sub_hz: 80.0,
            delay_mix: 0.0,
            delay_s: 0.3,
            delay_fb: 0.3,
            delay_pp: false,
            delay_tone_hz: 8000.0,
            reverb_mix: 0.0,
            decay_s: 2.0,
            size: 1.0,
            predelay_ms: 10.0,
            damp_hz: 7000.0,
            flanger_mix: 0.0,
            fl_rate: 0.5,
            fl_depth: 0.25,
            fl_fb: 0.0,
            width: 1.0,
            out_db: 0.0,
            mix: 1.0,
        }
    }
}

/// The shape of band `i` (shelves at the ends, bells between).
pub fn band_shape(i: usize) -> Shape {
    match i {
        0 => Shape::LowShelf,
        6 => Shape::HighShelf,
        _ => Shape::Peak,
    }
}

/// What the engine reports about a block, for the meters.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockInfo {
    /// Peak of the delay, reverb and flanger returns (after their mix), linear.
    pub fx_peak: [f32; 3],
    /// Output correlation.
    pub correlation: f32,
}

/// The processing engine.
pub struct Engine {
    sr: f64,
    eq: [EqBand; BANDS],
    solo: [EqBand; BANDS],
    sub: EqBand,
    hpf: EqBand,
    lpf: EqBand,
    delay: FeedbackDelay,
    reverb: Reverb,
    flanger: Flanger,
    rev_set: ReverbParams,
    fx_live: [bool; 3],
    corr: Correlation,
    amp: Smooth,
    out: Smooth,
    mix: Smooth,
    bypass: Smooth,
    solo_amt: Smooth,
    dmix: Smooth,
    rmix: Smooth,
    fmix: Smooth,
    dfb: Smooth,
    width: Smooth,
    fresh: bool,
}

fn ramp(sr: f64) -> u32 {
    (sr * 0.02) as u32
}

impl Engine {
    /// Allocate everything for `sample_rate`. Call from `activate`.
    pub fn new(sample_rate: f64) -> Self {
        let sr = sample_rate;
        let sm = |v: f32, ms: f32| Smooth::new(v, sr, ms, ms);
        let mut e = Self {
            sr,
            eq: std::array::from_fn(|_| EqBand::new(ramp(sr))),
            solo: std::array::from_fn(|_| EqBand::new(ramp(sr))),
            sub: EqBand::new(ramp(sr)),
            hpf: EqBand::new(ramp(sr)),
            lpf: EqBand::new(ramp(sr)),
            delay: FeedbackDelay::new(sr),
            reverb: Reverb::new(sr),
            flanger: Flanger::new(sr),
            rev_set: ReverbParams::default(),
            fx_live: [false; 3],
            corr: Correlation::new(sr),
            amp: sm(1.0, 15.0),
            out: sm(1.0, 15.0),
            mix: sm(1.0, 20.0),
            bypass: sm(0.0, 8.0),
            solo_amt: sm(0.0, 10.0),
            dmix: sm(0.0, 25.0),
            rmix: sm(0.0, 25.0),
            fmix: sm(0.0, 25.0),
            dfb: sm(0.3, 30.0),
            width: sm(1.0, 20.0),
            fresh: true,
        };
        e.reset_to(&Settings::default());
        e
    }

    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    /// No latency: every stage is IIR or a delay line.
    pub fn latency(&self) -> usize {
        0
    }

    /// How long the output rings on after the input stops at these settings, in seconds.
    pub fn tail_seconds(s: &Settings) -> f32 {
        let mut t = 0.1f32;
        if s.reverb_mix > 0.0 {
            t = t.max(s.decay_s + s.predelay_ms * 0.001);
        }
        if s.delay_mix > 0.0 {
            let fb = s.delay_fb.clamp(0.01, 0.9);
            // repeats until -60 dB
            t = t.max(s.delay_s * (1.0 + (-3.0 / fb.log10()).min(80.0)));
        }
        t.min(60.0)
    }

    /// Clear all state; the next block jumps the smoothers and filters to its settings.
    pub fn reset_to(&mut self, s: &Settings) {
        self.reset();
        self.delay.reset(s.delay_s);
        self.fresh = true;
        self.update(s);
        for b in self.all_filters() {
            b.reset();
        }
    }

    /// Clear the audio state.
    pub fn reset(&mut self) {
        for b in self.all_filters() {
            b.reset();
        }
        self.delay.reset(0.3);
        self.reverb.reset();
        self.flanger.reset();
        self.corr = Correlation::new(self.sr);
        self.fx_live = [false; 3];
    }

    fn all_filters(&mut self) -> impl Iterator<Item = &mut EqBand> {
        self.eq.iter_mut().chain(self.solo.iter_mut()).chain([&mut self.sub, &mut self.hpf, &mut self.lpf])
    }

    fn update(&mut self, s: &Settings) {
        let sr = self.sr;
        let nyq = (sr * 0.45) as f32;
        for (i, (b, set)) in self.eq.iter_mut().zip(s.bands.iter()).enumerate() {
            let g = if set.mute { MUTE_DB } else { set.gain_db };
            b.set(band_shape(i), set.freq.clamp(10.0, nyq), set.q, g, sr);
        }
        for (i, (b, set)) in self.solo.iter_mut().zip(s.bands.iter()).enumerate() {
            // audition: what the band works on (its shelf region or its bell's band)
            let shape = match i {
                0 => Shape::LowPass,
                6 => Shape::HighPass,
                _ => Shape::BandPass,
            };
            b.set(shape, set.freq.clamp(10.0, nyq), set.q.max(0.5), 0.0, sr);
        }
        self.sub.set(Shape::LowShelf, s.sub_hz.clamp(20.0, 400.0), 0.8, s.sub_db, sr);
        if s.hpf_hz > HPF_OFF_HZ {
            self.hpf.set(Shape::HighPass, s.hpf_hz.min(nyq), 0.707, 0.0, sr);
        } else {
            self.hpf.set(Shape::Peak, 1000.0, 1.0, 0.0, sr);
        }
        if s.lpf_hz < LPF_OFF_HZ && s.lpf_hz < nyq {
            self.lpf.set(Shape::LowPass, s.lpf_hz, 0.707, 0.0, sr);
        } else {
            self.lpf.set(Shape::Peak, 1000.0, 1.0, 0.0, sr);
        }
        let rp = ReverbParams { decay: s.decay_s, size: s.size, damp_hz: s.damp_hz, predelay_ms: s.predelay_ms };
        if rp != self.rev_set {
            self.rev_set = rp;
            self.reverb.set(rp);
        }
        self.delay.set_tone(s.delay_tone_hz.min(nyq), 60.0);
    }

    /// Process one block in place.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], s: &Settings) -> BlockInfo {
        let n = l.len().min(r.len());
        self.update(s);
        let amp_t = db_to_gain(s.amp_db);
        let out_t = db_to_gain(s.out_db);
        let byp_t = if s.bypass { 1.0 } else { 0.0 };
        let any_solo = s.bands.iter().any(|b| b.solo);
        let solo_t = if any_solo { 1.0 } else { 0.0 };
        let fb_t = s.delay_fb.clamp(0.0, 0.9);
        if self.fresh {
            self.fresh = false;
            self.amp.reset(amp_t);
            self.out.reset(out_t);
            self.mix.reset(s.mix);
            self.bypass.reset(byp_t);
            self.solo_amt.reset(solo_t);
            self.dmix.reset(s.delay_mix);
            self.rmix.reset(s.reverb_mix);
            self.fmix.reset(s.flanger_mix);
            self.dfb.reset(fb_t);
            self.width.reset(s.width);
        }
        // an effect whose return has faded to nothing stops running (and starts clean)
        let targets = [s.delay_mix, s.reverb_mix, s.flanger_mix];
        let shown = [self.dmix.value(), self.rmix.value(), self.fmix.value()];
        for k in 0..3 {
            let live = targets[k] > 0.0 || shown[k] > 0.0;
            if !live && self.fx_live[k] {
                match k {
                    0 => self.delay.reset(s.delay_s),
                    1 => self.reverb.reset(),
                    _ => self.flanger.reset(),
                }
            }
            self.fx_live[k] = live;
        }
        let eq_active = !self.eq.iter().all(|b| b.is_identity());
        let solo_active = any_solo || self.solo_amt.value() > 0.0;
        let tone_active = !(self.sub.is_identity() && self.hpf.is_identity() && self.lpf.is_identity());
        let mut fx_peak = [0.0f32; 3];
        for i in 0..n {
            let (xl, xr) = (l[i], r[i]);
            let (xl, xr) = (if xl.is_finite() { xl } else { 0.0 }, if xr.is_finite() { xr } else { 0.0 });
            let g = self.amp.tick(amp_t);
            let (il, ir) = (xl * g, xr * g);

            // EQ, or the soloed bands' audition
            let (mut a, mut b) = (il, ir);
            if eq_active {
                for band in self.eq.iter_mut() {
                    if band.is_identity() {
                        band.advance();
                    } else {
                        (a, b) = band.tick(a, b);
                    }
                }
            } else {
                for band in self.eq.iter_mut() {
                    band.advance();
                }
            }
            let sa = self.solo_amt.tick(solo_t);
            if solo_active {
                let (mut ol, mut or) = (0.0, 0.0);
                for (k, band) in self.solo.iter_mut().enumerate() {
                    if s.bands[k].solo {
                        let (y0, y1) = band.tick(a, b);
                        ol += y0;
                        or += y1;
                    } else {
                        band.advance();
                    }
                }
                a += (ol - a) * sa;
                b += (or - b) * sa;
            } else {
                for band in self.solo.iter_mut() {
                    band.advance();
                }
            }
            if tone_active {
                (a, b) = self.sub.tick(a, b);
                (a, b) = self.hpf.tick(a, b);
                (a, b) = self.lpf.tick(a, b);
            } else {
                self.sub.advance();
                self.hpf.advance();
                self.lpf.advance();
            }

            // the space: three parallel returns over the filtered signal
            let (mut wl, mut wr) = (0.0f32, 0.0f32);
            let dm = self.dmix.tick(s.delay_mix);
            let fbk = self.dfb.tick(fb_t);
            if self.fx_live[0] {
                let (y0, y1) = self.delay.tick(a, b, s.delay_s, fbk, s.delay_pp);
                let (y0, y1) = (y0 * dm, y1 * dm);
                fx_peak[0] = fx_peak[0].max(y0.abs()).max(y1.abs());
                wl += y0;
                wr += y1;
            }
            let rm = self.rmix.tick(s.reverb_mix);
            if self.fx_live[1] {
                let (y0, y1) = self.reverb.tick(a, b);
                let (y0, y1) = (y0 * rm, y1 * rm);
                fx_peak[1] = fx_peak[1].max(y0.abs()).max(y1.abs());
                wl += y0;
                wr += y1;
            }
            let fm = self.fmix.tick(s.flanger_mix);
            if self.fx_live[2] {
                let (y0, y1) = self.flanger.tick(a, b, s.fl_rate, s.fl_depth, s.fl_fb);
                let (y0, y1) = (y0 * fm, y1 * fm);
                fx_peak[2] = fx_peak[2].max(y0.abs()).max(y1.abs());
                wl += y0;
                wr += y1;
            }
            let w = self.width.tick(s.width);
            let (yl, yr) = width(a + wl, b + wr, w);
            let og = self.out.tick(out_t);
            let mx = self.mix.tick(s.mix);
            let (ol, or) = (xl + (yl * og - xl) * mx, xr + (yr * og - xr) * mx);
            self.corr.push(ol, or);
            let bb = self.bypass.tick(byp_t);
            // APF runs process() with flush-to-zero on; this keeps the engine clean without it.
            l[i] = undenorm(ol + (xl - ol) * bb);
            r[i] = undenorm(or + (xr - or) * bb);
        }
        BlockInfo { fx_peak, correlation: self.corr.value() }
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
    fn peak(v: &[f32]) -> f32 {
        v.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn defaults_are_transparent() {
        let sr = 48_000.0;
        let x = sine(9600, 440.0, sr, 0.5);
        let mut e = Engine::new(sr);
        let (l, r) = run(&mut e, &Settings::default(), &x, 256);
        for i in 0..x.len() {
            assert!((l[i] - x[i]).abs() < 1e-4 && (r[i] - x[i]).abs() < 1e-4, "@{i}");
        }
    }

    #[test]
    fn eq_bands_boost_and_mute_cuts() {
        let sr = 48_000.0;
        let x = sine(24_000, 800.0, sr, 0.1);
        let mut s = Settings::default();
        s.bands[2].gain_db = 12.0;
        let (l, _) = run(&mut Engine::new(sr), &s, &x, 128);
        assert!((peak(&l[12_000..]) / 0.1 - db_to_gain(12.0)).abs() < 0.2);
        s.bands[2].mute = true;
        let (l, _) = run(&mut Engine::new(sr), &s, &x, 128);
        assert!(peak(&l[12_000..]) < 0.1 * db_to_gain(-24.0), "{}", peak(&l[12_000..]));
    }

    #[test]
    fn solo_auditions_only_the_soloed_band() {
        let sr = 48_000.0;
        let lo = sine(24_000, 200.0, sr, 0.1);
        let hi = sine(24_000, 8000.0, sr, 0.1);
        let mut s = Settings::default();
        s.bands[1].solo = true; // 200 Hz bell
        let (l, _) = run(&mut Engine::new(sr), &s, &lo, 256);
        assert!(peak(&l[12_000..]) > 0.08, "the soloed band passes");
        let (l, _) = run(&mut Engine::new(sr), &s, &hi, 256);
        assert!(peak(&l[12_000..]) < 0.01, "everything else is out: {}", peak(&l[12_000..]));
    }

    #[test]
    fn filters_and_sub_shape_the_extremes() {
        let sr = 48_000.0;
        let s = Settings { hpf_hz: 300.0, ..Settings::default() };
        let (l, _) = run(&mut Engine::new(sr), &s, &sine(24_000, 50.0, sr, 0.1), 256);
        assert!(peak(&l[12_000..]) < 0.01);
        let s = Settings { lpf_hz: 2000.0, ..Settings::default() };
        let (l, _) = run(&mut Engine::new(sr), &s, &sine(24_000, 12_000.0, sr, 0.1), 256);
        assert!(peak(&l[12_000..]) < 0.01);
        let s = Settings { sub_db: 10.0, ..Settings::default() };
        let (l, _) = run(&mut Engine::new(sr), &s, &sine(48_000, 40.0, sr, 0.1), 256);
        assert!(peak(&l[24_000..]) > 0.1 * db_to_gain(7.0));
    }

    #[test]
    fn the_space_effects_ring_on_and_die_away() {
        let sr = 48_000.0;
        let mut x = vec![0.0f32; 48_000 * 4];
        x[100] = 1.0;
        for (name, s) in [
            ("delay", Settings { delay_mix: 0.8, delay_s: 0.25, delay_fb: 0.5, ..Settings::default() }),
            ("reverb", Settings { reverb_mix: 0.6, decay_s: 1.0, ..Settings::default() }),
            ("flanger", Settings { flanger_mix: 0.8, fl_fb: 0.6, ..Settings::default() }),
        ] {
            let (l, r) = run(&mut Engine::new(sr), &s, &x, 512);
            let after = peak(&l[2000..24_000]).max(peak(&r[2000..24_000]));
            if name != "flanger" {
                assert!(after > 0.01, "{name}: no return ({after})");
            }
            let end = peak(&l[180_000..]).max(peak(&r[180_000..]));
            assert!(end < 1e-3, "{name}: still ringing at the end ({end})");
            assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 4.0));
        }
    }

    #[test]
    fn mix_zero_and_bypass_are_the_dry_input() {
        let sr = 48_000.0;
        let x = sine(9600, 300.0, sr, 0.4);
        let mut s = Settings { amp_db: 9.0, reverb_mix: 0.5, delay_mix: 0.5, hpf_hz: 200.0, mix: 0.0, ..Settings::default() };
        s.bands[3].gain_db = 12.0;
        let (l, _) = run(&mut Engine::new(sr), &s, &x, 256);
        assert!(l.iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-5));
        let s = Settings { mix: 1.0, bypass: true, ..s };
        let (l, _) = run(&mut Engine::new(sr), &s, &x, 256);
        assert!(l.iter().zip(&x).all(|(a, b)| (a - b).abs() < 1e-5));
    }
}
