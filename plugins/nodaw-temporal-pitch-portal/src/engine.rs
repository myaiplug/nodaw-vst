//! The DSP engine of NoDAW Temporal Pitch Portal: a tempo-synced time warp, then an independent
//! SOLA pitch shifter (with glide and a drifting "micro pitch"), a tilt tone, a portal echo,
//! output gain and a latency-compensated dry/wet mix.
//!
//! ```text
//! in -+-> time warp -> pitch shift -> tilt -> (+ echo) -> output -+-> mix -> bypass xfade -> out
//!     +-> dry delay (total latency) ------------------------------^          ^
//!     +-> bypass delay (total latency) ---------------------------------------+
//! ```
//!
//! The time warp and the pitch shifter always run, so the latency is the same for every
//! setting: one warp grain plus the middle of the shifter's lag band. At 100 % time and 0 st
//! (no drift) both are pure delays, so the default settings null against the delayed input
//! (tested). Every buffer is allocated in [`Engine::new`]; [`Engine::process`] never allocates.

use nodaw_dsp::chop::ChopClock;
use nodaw_dsp::delay::DelayLine;
use nodaw_dsp::eq::{EqBand, Shape};
use nodaw_dsp::modfx::FeedbackDelay;
use nodaw_dsp::pitch::{semitones_to_ratio, PitchShifter};
use nodaw_dsp::timewarp::TimeWarp;
use nodaw_dsp::util::{db_to_gain, undenorm, Noise, Smooth};

/// How long one warp period is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sync {
    Quarter,
    HalfBar,
    Bar,
    TwoBars,
}

impl Sync {
    /// From the parameter's option index.
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Self::Quarter,
            1 => Self::HalfBar,
            3 => Self::TwoBars,
            _ => Self::Bar,
        }
    }
    /// The period in beats for a bar of `bar_beats` beats.
    pub fn beats(self, bar_beats: f64) -> f64 {
        match self {
            Self::Quarter => 1.0,
            Self::HalfBar => 0.5 * bar_beats,
            Self::Bar => bar_beats,
            Self::TwoBars => 2.0 * bar_beats,
        }
    }
}

/// Everything one block needs, in plain units.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub bypass: bool,
    /// Playback speed inside each warp period, 0.5 ..= 2.
    pub time: f32,
    /// Keep the pitch while warping (grains) instead of a varispeed tape.
    pub stretch: bool,
    pub sync: Sync,
    /// Pitch shift in semitones, -12 ..= 12.
    pub pitch_st: f32,
    /// Glide time of pitch changes, ms.
    pub glide_ms: f32,
    /// Depth of the drifting micro pitch, cents.
    pub drift_cents: f32,
    pub drift_hz: f32,
    /// Tilt, -1 (dark) ..= 1 (bright).
    pub tone: f32,
    pub echo_mix: f32,
    pub echo_s: f32,
    pub echo_fb: f32,
    pub out_db: f32,
    /// 0 ..= 1.
    pub mix: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass: false,
            time: 1.0,
            stretch: true,
            sync: Sync::Bar,
            pitch_st: 0.0,
            glide_ms: 30.0,
            drift_cents: 0.0,
            drift_hz: 0.7,
            tone: 0.0,
            echo_mix: 0.0,
            echo_s: 0.375,
            echo_fb: 0.35,
            out_db: 0.0,
            mix: 1.0,
        }
    }
}

/// Where the transport is, for the warp's sync.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clock {
    pub song_pos_beats: Option<f64>,
    pub playing: bool,
    pub tempo: Option<f64>,
    /// Beats per bar (from the time signature), 4 when unknown.
    pub bar_beats: f64,
}

/// What the page shows after a block.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockInfo {
    /// The pitch actually applied, semitones (glide and drift included).
    pub pitch_st: f32,
    /// The drift right now, cents.
    pub drift_cents: f32,
    /// Position inside the warp period, 0 ..= 1.
    pub phase: f32,
    /// The combined speed of the material: time (on tape) times the pitch ratio.
    pub ripple: f32,
    /// Peak of the echo return.
    pub echo_peak: f32,
}

/// Tilt hinge frequencies.
const TILT_LO: f32 = 250.0;
const TILT_HI: f32 = 2500.0;
/// Tilt at full scale, dB at each end.
const TILT_DB: f32 = 6.0;

/// The engine.
pub struct Engine {
    sr: f64,
    warp: TimeWarp,
    pitch: PitchShifter,
    lo: EqBand,
    hi: EqBand,
    echo: FeedbackDelay,
    dry: [DelayLine; 2],
    byp: [DelayLine; 2],
    lat: usize,
    clock: ChopClock,
    last_period: i64,
    st: Smooth,
    drift_phase: f64,
    drift_noise: Noise,
    drift_rand: f32,
    drift_target: f32,
    drift_count: u32,
    out_gain: Smooth,
    mix: Smooth,
    bypass: Smooth,
    echo_amt: Smooth,
    fresh: bool,
}

impl Engine {
    /// An engine for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let warp = TimeWarp::new(sample_rate);
        let pitch = PitchShifter::new(sample_rate);
        let lat = warp.latency() + pitch.latency();
        Self {
            sr: sample_rate,
            warp,
            pitch,
            lo: EqBand::new(64),
            hi: EqBand::new(64),
            echo: FeedbackDelay::new(sample_rate),
            dry: [DelayLine::new(lat + 4), DelayLine::new(lat + 4)],
            byp: [DelayLine::new(lat + 4), DelayLine::new(lat + 4)],
            lat,
            clock: ChopClock::default(),
            last_period: i64::MIN,
            st: Smooth::new(0.0, sample_rate, 30.0, 30.0),
            drift_phase: 0.0,
            drift_noise: Noise::new(0x7e3a_11d5),
            drift_rand: 0.0,
            drift_target: 0.0,
            drift_count: 0,
            out_gain: Smooth::new(1.0, sample_rate, 20.0, 20.0),
            mix: Smooth::new(1.0, sample_rate, 20.0, 20.0),
            bypass: Smooth::new(0.0, sample_rate, 10.0, 10.0),
            echo_amt: Smooth::new(0.0, sample_rate, 20.0, 20.0),
            fresh: true,
        }
    }

    /// The sample rate this engine was built for.
    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    /// Total latency in samples (constant for the engine's life).
    pub fn latency(&self) -> usize {
        self.lat
    }

    /// Clear every buffer; the next block starts from its settings.
    pub fn reset(&mut self) {
        self.warp.reset();
        self.pitch.reset();
        self.lo.reset();
        self.hi.reset();
        self.echo.reset(0.375);
        for d in self.dry.iter_mut().chain(self.byp.iter_mut()) {
            d.clear();
        }
        self.clock = ChopClock::default();
        self.last_period = i64::MIN;
        self.drift_phase = 0.0;
        self.drift_rand = 0.0;
        self.drift_target = 0.0;
        self.drift_count = 0;
        self.fresh = true;
    }

    /// Reset and snap every smoother to `s`.
    pub fn reset_to(&mut self, s: &Settings) {
        self.reset();
        self.echo.reset(s.echo_s);
    }

    /// How long the plugin rings after the input stops, seconds.
    pub fn tail_seconds(s: &Settings) -> f32 {
        let echo = if s.echo_mix > 0.0 {
            let fb = s.echo_fb.clamp(0.0, 0.97).max(1e-3);
            // repeats until -60 dB
            (s.echo_s * (-3.0 / fb.log10()).min(60.0)).min(30.0)
        } else {
            0.0
        };
        // a warp period can replay up to 4 s of the past
        echo + if (s.time - 1.0).abs() > 1e-4 {
            TimeWarp::MAX_PERIOD_SECONDS as f32
        } else {
            0.1
        }
    }

    /// Process one block in place.
    pub fn process(
        &mut self,
        l: &mut [f32],
        r: &mut [f32],
        s: &Settings,
        clock: &Clock,
    ) -> BlockInfo {
        let n = l.len().min(r.len());
        let sr = self.sr;
        let t = s.tone.clamp(-1.0, 1.0);
        self.lo.set(Shape::LowShelf, TILT_LO, 0.7, -t * TILT_DB, sr);
        self.hi.set(Shape::HighShelf, TILT_HI, 0.7, t * TILT_DB, sr);
        let nyq = (sr * 0.45) as f32;
        self.echo.set_tone(9000.0f32.min(nyq), 90.0);
        let glide = s.glide_ms.max(1.0);
        self.st.set_times(sr, glide, glide);
        let out_t = db_to_gain(s.out_db);
        let byp_t = if s.bypass { 1.0 } else { 0.0 };
        let echo_t = s.echo_mix.clamp(0.0, 1.0);
        if self.fresh {
            self.fresh = false;
            self.st.reset(s.pitch_st);
            self.out_gain.reset(out_t);
            self.mix.reset(s.mix);
            self.bypass.reset(byp_t);
            self.echo_amt.reset(echo_t);
            self.echo.reset(s.echo_s);
        }

        let (start, inc) =
            self.clock
                .begin_block(clock.song_pos_beats, clock.playing, clock.tempo, sr);
        let bar = if clock.bar_beats.is_finite() && clock.bar_beats > 0.0 {
            clock.bar_beats
        } else {
            4.0
        };
        let per_beats = s.sync.beats(bar);
        let period = ((per_beats / inc).round() as usize).clamp(16, self.warp.max_period());
        let drift_inc = (s.drift_hz.clamp(0.01, 20.0) as f64) / sr;
        let drift_hold = ((sr / (s.drift_hz.max(0.01) as f64 * 1.7)) as u32).max(1);
        let drift_on = s.drift_cents > 0.0;
        let rate = s.time.clamp(0.5, 2.0);

        let mut info = BlockInfo::default();
        let mut ratio = 1.0f32;
        let mut drift_c = 0.0f32;
        for i in 0..n {
            let (xl, xr) = (l[i], r[i]);
            let (xl, xr) = (
                if xl.is_finite() { xl } else { 0.0 },
                if xr.is_finite() { xr } else { 0.0 },
            );

            // the warp's sync period
            let beat = start + inc * i as f64;
            let idx = (beat / per_beats).floor() as i64;
            let new_period = idx != self.last_period;
            self.last_period = idx;
            let (wl, wr) = self.warp.tick(xl, xr, rate, s.stretch, new_period, period);

            // pitch: glide plus drift (a slow sine and a smoothed random walk)
            let st = self.st.tick(s.pitch_st);
            if drift_on {
                self.drift_phase += drift_inc;
                if self.drift_phase >= 1.0 {
                    self.drift_phase -= 1.0;
                }
                if self.drift_count == 0 {
                    self.drift_target = self.drift_noise.sample();
                    self.drift_count = drift_hold;
                }
                self.drift_count -= 1;
                self.drift_rand +=
                    (self.drift_target - self.drift_rand) * (8.0 / drift_hold as f32).min(1.0);
                let lfo = 0.65 * (2.0 * std::f64::consts::PI * self.drift_phase).sin() as f32
                    + 0.35 * self.drift_rand;
                drift_c = s.drift_cents * lfo;
            } else {
                drift_c = 0.0;
            }
            ratio = if st == 0.0 && drift_c == 0.0 {
                1.0
            } else {
                semitones_to_ratio(st + drift_c * 0.01)
            };
            let (mut a, mut b) = self.pitch.tick(wl, wr, ratio);

            // tilt
            let (ta, tb) = self.hi.tick(a, b);
            let (ta, tb) = self.lo.tick(ta, tb);
            a = ta;
            b = tb;

            // echo (a return on top of the shifted signal)
            let ea = self.echo_amt.tick(echo_t);
            let (el, er) = if ea > 0.0 || echo_t > 0.0 {
                self.echo.tick(a, b, s.echo_s, s.echo_fb, false)
            } else {
                (0.0, 0.0)
            };
            info.echo_peak = info.echo_peak.max(el.abs()).max(er.abs());
            a += ea * el;
            b += ea * er;

            let g = self.out_gain.tick(out_t);
            a *= g;
            b *= g;

            // dry/wet with the dry delayed by the latency
            self.dry[0].push(xl);
            self.dry[1].push(xr);
            let (dl, dr) = (self.dry[0].tap_int(self.lat), self.dry[1].tap_int(self.lat));
            let m = self.mix.tick(s.mix);
            let (ol, or) = if m >= 1.0 {
                (a, b)
            } else {
                (dl + (a - dl) * m, dr + (b - dr) * m)
            };

            // bypass: a crossfade to the input delayed by the latency
            self.byp[0].push(xl);
            self.byp[1].push(xr);
            let bp = self.bypass.tick(byp_t);
            let (yl, yr) = if bp <= 0.0 {
                (ol, or)
            } else {
                let (bl, br) = (self.byp[0].tap_int(self.lat), self.byp[1].tap_int(self.lat));
                if bp >= 1.0 {
                    (bl, br)
                } else {
                    (ol + (bl - ol) * bp, or + (br - or) * bp)
                }
            };
            l[i] = undenorm(yl);
            r[i] = undenorm(yr);
        }
        if echo_t <= 0.0 && self.echo_amt.value() <= 0.0 {
            // keep the line clean for the next time the echo comes up
            self.echo.reset(s.echo_s);
        }
        self.clock.end_block(start, inc, n);
        let beat_end = start + inc * n as f64;
        info.phase = ((beat_end / per_beats).fract()) as f32;
        info.pitch_st = self.st.value() + drift_c * 0.01;
        info.drift_cents = drift_c;
        info.ripple = ratio * if s.stretch { 1.0 } else { rate };
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodaw_dsp::fft::SpectrumAnalyzer;

    fn sine(n: usize, hz: f32, sr: f64) -> Vec<f32> {
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / sr as f32).sin())
            .collect()
    }

    fn run(e: &mut Engine, s: &Settings, x: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        let clock = Clock {
            song_pos_beats: None,
            playing: false,
            tempo: Some(120.0),
            bar_beats: 4.0,
        };
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            e.process(a, b, s, &clock);
        }
        (l, r)
    }

    #[test]
    fn defaults_are_a_pure_delay_of_the_latency() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            let mut e = Engine::new(sr);
            let lat = e.latency();
            let x: Vec<f32> = (0..20_000)
                .map(|i| ((i * 37 % 101) as f32 - 50.0) / 60.0)
                .collect();
            let (y, _) = run(&mut e, &Settings::default(), &x, 333);
            for i in lat..x.len() {
                assert!(
                    (y[i] - x[i - lat]).abs() < 1e-6,
                    "{sr}: sample {i}: {} vs {}",
                    y[i],
                    x[i - lat]
                );
            }
        }
    }

    #[test]
    fn pitch_moves_a_tone_by_the_set_interval() {
        let sr = 48_000.0;
        for st in [-5.0f32, 7.0] {
            let mut e = Engine::new(sr);
            let s = Settings {
                pitch_st: st,
                ..Settings::default()
            };
            let (y, _) = run(&mut e, &s, &sine(96_000, 440.0, sr), 512);
            let mut an = SpectrumAnalyzer::new(16384);
            an.analyze(&y[y.len() - 16384..]);
            let want = 440.0 * semitones_to_ratio(st);
            let got = an.peak_frequency(sr);
            assert!((got / want - 1.0).abs() < 0.01, "{st}: {got} vs {want}");
        }
    }

    #[test]
    fn stretched_half_time_keeps_the_pitch() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let s = Settings {
            time: 0.5,
            stretch: true,
            sync: Sync::Quarter,
            ..Settings::default()
        };
        let (y, _) = run(&mut e, &s, &sine(96_000, 330.0, sr), 256);
        let mut an = SpectrumAnalyzer::new(16384);
        an.analyze(&y[y.len() - 16384..]);
        let got = an.peak_frequency(sr);
        assert!((got / 330.0 - 1.0).abs() < 0.02, "{got}");
    }

    #[test]
    fn tape_half_time_drops_an_octave() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let s = Settings {
            time: 0.5,
            stretch: false,
            sync: Sync::TwoBars,
            ..Settings::default()
        };
        // 2 bars at 120 BPM = 4 s: the first half of the period plays at half speed
        let (y, _) = run(&mut e, &s, &sine(4 * 48_000, 660.0, sr), 512);
        let mut an = SpectrumAnalyzer::new(16384);
        an.analyze(&y[60_000..60_000 + 16384]);
        let got = an.peak_frequency(sr);
        assert!((got / 330.0 - 1.0).abs() < 0.02, "{got}");
    }

    #[test]
    fn drift_wobbles_around_the_set_pitch_and_stays_finite() {
        let sr = 44_100.0;
        let mut e = Engine::new(sr);
        let s = Settings {
            pitch_st: -3.0,
            drift_cents: 40.0,
            drift_hz: 2.0,
            echo_mix: 0.5,
            echo_fb: 0.8,
            ..Settings::default()
        };
        let x = sine(88_200, 220.0, sr);
        let (mut l, mut r) = (x.clone(), x.clone());
        let clock = Clock {
            tempo: Some(97.0),
            bar_beats: 3.0,
            ..Clock::default()
        };
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for (a, b) in l.chunks_mut(100).zip(r.chunks_mut(100)) {
            let info = e.process(a, b, &s, &clock);
            lo = lo.min(info.drift_cents);
            hi = hi.max(info.drift_cents);
            assert!((info.pitch_st + 3.0).abs() <= 0.41);
        }
        assert!(hi > 10.0 && lo < -10.0, "{lo}..{hi}");
        assert!(l.iter().chain(&r).all(|v| v.is_finite() && v.abs() < 8.0));
    }

    #[test]
    fn mix_zero_and_bypass_are_the_delayed_input() {
        let sr = 48_000.0;
        let x = sine(30_000, 300.0, sr);
        for s in [
            Settings {
                mix: 0.0,
                pitch_st: 5.0,
                time: 0.7,
                tone: -1.0,
                ..Settings::default()
            },
            Settings {
                bypass: true,
                pitch_st: -7.0,
                echo_mix: 1.0,
                ..Settings::default()
            },
        ] {
            let mut e = Engine::new(sr);
            let lat = e.latency();
            let (y, _) = run(&mut e, &s, &x, 128);
            for i in lat..x.len() {
                assert_eq!(y[i], x[i - lat], "sample {i}");
            }
        }
    }
}
