//! The DSP engine of NoDAW Screw: no host types, so it is tested directly.
//!
//! Signal flow (stereo):
//!
//! ```text
//! in -> input gain -+-> pitch (SOLA) -> wobble -> low-pass -> bass shelf -> chop gate -+-> + -> width -+
//!                   |                                                                  |             |
//!                   |                                                                  +-> reverb --+     mix <- dry (delayed)
//!                   +-> dry delay (pitch + wobble latency) ------------------------------------------>+
//! mix -> output gain -> true-peak limiter -> bypass crossfade -> out
//! in -> bypass delay (total latency) ---------------------------------------------------^
//! ```
//!
//! The dry path of the mix and the bypass path are both delayed by exactly the latency of the
//! wet path they replace, so mix and bypass never comb-filter and the host's delay compensation
//! stays right whatever the settings.

use nodaw_dsp::chop::{self, ChopClock, Pattern};
use nodaw_dsp::delay::DelayLine;
use nodaw_dsp::filter::{Biquad, Coeffs};
use nodaw_dsp::limiter::Limiter;
use nodaw_dsp::pitch::{semitones_to_ratio, PitchShifter};
use nodaw_dsp::reverb::{Reverb, ReverbParams};
use nodaw_dsp::util::{db_to_gain, Smooth};
use nodaw_dsp::wobble::{self, Wobble};

/// Everything one block needs, in plain units. Built from the host parameters by the plugin.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub bypass: bool,
    pub in_db: f32,
    pub out_db: f32,
    /// 0 ..= 1.
    pub mix: f32,
    pub pitch_st: f32,
    pub wow_cents: f32,
    pub wow_rate_hz: f32,
    /// 0 ..= 1.
    pub flutter: f32,
    pub lowpass_hz: f32,
    pub bass_db: f32,
    /// 0 ..= 1.
    pub rev_mix: f32,
    pub reverb: ReverbParams,
    pub pattern: Pattern,
    pub chop_steps_per_beat: f64,
    /// 0 ..= 1.
    pub chop_depth: f32,
    /// 0 ..= 2.
    pub width: f32,
    pub limiter_on: bool,
    pub ceiling_db: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass: false,
            in_db: 0.0,
            out_db: 0.0,
            mix: 1.0,
            pitch_st: -3.0,
            wow_cents: 0.0,
            wow_rate_hz: 0.7,
            flutter: 0.0,
            lowpass_hz: 20_000.0,
            bass_db: 0.0,
            rev_mix: 0.25,
            reverb: ReverbParams::default(),
            pattern: [2; chop::STEPS],
            chop_steps_per_beat: 2.0,
            chop_depth: 1.0,
            width: 1.0,
            limiter_on: true,
            ceiling_db: -1.0,
        }
    }
}

/// What the engine reports about a block, for the meters.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockInfo {
    /// Deepest limiter gain in the block (1 = no reduction).
    pub min_gain: f32,
    /// The chop step playing at the end of the block.
    pub step: usize,
}

const SHELF_HZ: f32 = 150.0;

/// The processing engine.
pub struct Engine {
    sr: f64,
    pitch: PitchShifter,
    wobble: Wobble,
    reverb: Reverb,
    limiter: Limiter,
    dry: [DelayLine; 2],
    byp: [DelayLine; 2],
    lat_wet: usize,
    lat_total: usize,
    lp: [Biquad; 2],
    shelf: [Biquad; 2],
    lp_c: Coeffs,
    shelf_c: Coeffs,
    lp_for: f32,
    shelf_for: f32,
    in_gain: Smooth,
    out_gain: Smooth,
    mix: Smooth,
    rev: Smooth,
    width: Smooth,
    bypass: Smooth,
    chop_gain: Smooth,
    clock: ChopClock,
    /// Set by a reset: the first block jumps the smoothers to its settings (a session that
    /// was loaded after activation must sound like one loaded before it).
    fresh: bool,
}

impl Engine {
    /// Allocate everything for `sample_rate`. Call from `activate`, never from the audio thread.
    pub fn new(sample_rate: f64) -> Self {
        let pitch = PitchShifter::new(sample_rate);
        let wobble = Wobble::new(sample_rate);
        let limiter = Limiter::new(sample_rate, 2.0, 80.0);
        let lat_wet = pitch.latency() + wobble.latency();
        let lat_total = lat_wet + limiter.latency();
        let mut e = Self {
            sr: sample_rate,
            pitch,
            wobble,
            reverb: Reverb::new(sample_rate),
            limiter,
            dry: [DelayLine::new(lat_wet + 4), DelayLine::new(lat_wet + 4)],
            byp: [DelayLine::new(lat_total + 4), DelayLine::new(lat_total + 4)],
            lat_wet,
            lat_total,
            lp: [Biquad::default(); 2],
            shelf: [Biquad::default(); 2],
            lp_c: Coeffs::IDENTITY,
            shelf_c: Coeffs::IDENTITY,
            lp_for: -1.0,
            shelf_for: f32::NAN,
            in_gain: Smooth::new(1.0, sample_rate, 15.0, 15.0),
            out_gain: Smooth::new(1.0, sample_rate, 15.0, 15.0),
            mix: Smooth::new(1.0, sample_rate, 20.0, 20.0),
            rev: Smooth::new(0.0, sample_rate, 20.0, 20.0),
            width: Smooth::new(1.0, sample_rate, 20.0, 20.0),
            bypass: Smooth::new(0.0, sample_rate, 8.0, 8.0),
            chop_gain: Smooth::new(1.0, sample_rate, 1.5, 3.0),
            clock: ChopClock::default(),
            fresh: true,
        };
        e.reset_to(&Settings::default());
        e
    }

    /// The sample rate this engine was built for.
    pub fn sample_rate(&self) -> f64 {
        self.sr
    }

    /// Total latency in samples: pitch + wobble + limiter. Constant for the engine's life.
    pub fn latency(&self) -> usize {
        self.lat_total
    }

    /// Clear all state and jump the smoothers to `s` (so a restart does not glide in).
    pub fn reset_to(&mut self, s: &Settings) {
        self.pitch.reset();
        self.wobble.reset();
        self.reverb.reset();
        self.limiter.reset();
        for d in self.dry.iter_mut().chain(self.byp.iter_mut()) {
            d.clear();
        }
        for b in self.lp.iter_mut().chain(self.shelf.iter_mut()) {
            b.reset();
        }
        self.in_gain.reset(db_to_gain(s.in_db));
        self.out_gain.reset(db_to_gain(s.out_db));
        self.mix.reset(s.mix);
        self.rev.reset(s.rev_mix);
        self.width.reset(s.width);
        self.bypass.reset(if s.bypass { 1.0 } else { 0.0 });
        self.chop_gain.reset(1.0);
        self.clock = ChopClock::default();
        self.fresh = true;
        self.lp_for = -1.0;
        self.shelf_for = f32::NAN;
        self.update_tone(s);
        self.reverb.set(s.reverb);
    }

    /// Clear the audio state, keep the parameters' current positions.
    pub fn reset(&mut self) {
        let s = Settings {
            bypass: self.bypass.value() > 0.5,
            in_db: nodaw_dsp::gain_to_db(self.in_gain.value()),
            out_db: nodaw_dsp::gain_to_db(self.out_gain.value()),
            mix: self.mix.value(),
            rev_mix: self.rev.value(),
            width: self.width.value(),
            ..Settings::default()
        };
        self.reset_to(&s);
    }

    fn update_tone(&mut self, s: &Settings) {
        let lp = s.lowpass_hz.clamp(200.0, 20_000.0);
        if lp != self.lp_for {
            self.lp_for = lp;
            // At the top of the range the filter is out of the path altogether.
            self.lp_c = if lp >= 19_900.0 {
                Coeffs::IDENTITY
            } else {
                Coeffs::lowpass(lp.min(self.sr as f32 * 0.45), 0.707, self.sr)
            };
        }
        if s.bass_db != self.shelf_for {
            self.shelf_for = s.bass_db;
            self.shelf_c = if s.bass_db.abs() < 0.01 {
                Coeffs::IDENTITY
            } else {
                Coeffs::low_shelf(SHELF_HZ, s.bass_db, self.sr)
            };
        }
    }

    /// Process one block in place. `beats` is the host's song position at the first sample of
    /// the block and `playing` its transport; `tempo` in BPM when known.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        l: &mut [f32],
        r: &mut [f32],
        s: &Settings,
        beats: Option<f64>,
        playing: bool,
        tempo: Option<f64>,
    ) -> BlockInfo {
        let n = l.len().min(r.len());
        self.update_tone(s);
        self.reverb.set(s.reverb);
        if s.limiter_on {
            self.limiter.set_ceiling_db(s.ceiling_db);
        } else {
            self.limiter.set_off();
        }
        let ratio = semitones_to_ratio(s.pitch_st);
        let rate = s.wow_rate_hz.clamp(0.05, 20.0);
        let exc_ms = if s.wow_cents > 0.0 {
            wobble::excursion_ms(s.wow_cents, rate)
        } else {
            0.0
        };
        let (start, inc) = self.clock.begin_block(beats, playing, tempo, self.sr);
        let chop_on = s.pattern.iter().any(|&v| v != 2) && s.chop_depth > 0.0;

        let in_t = db_to_gain(s.in_db);
        let out_t = db_to_gain(s.out_db);
        let byp_t = if s.bypass { 1.0 } else { 0.0 };
        if self.fresh {
            self.fresh = false;
            self.in_gain.reset(in_t);
            self.out_gain.reset(out_t);
            self.mix.reset(s.mix);
            self.rev.reset(s.rev_mix);
            self.width.reset(s.width);
            self.bypass.reset(byp_t);
        }
        let mut min_gain = 1.0f32;
        let mut step = 0usize;
        let (lat_wet, lat_total) = (self.lat_wet, self.lat_total);
        for i in 0..n {
            let (xl, xr) = (l[i], r[i]);
            // A non-finite sample must not poison the feedback paths.
            let (xl, xr) = (
                if xl.is_finite() { xl } else { 0.0 },
                if xr.is_finite() { xr } else { 0.0 },
            );
            self.byp[0].push(xl);
            self.byp[1].push(xr);
            let (bl, br) = (
                self.byp[0].tap_int(lat_total),
                self.byp[1].tap_int(lat_total),
            );

            let g = self.in_gain.tick(in_t);
            let (il, ir) = (xl * g, xr * g);
            self.dry[0].push(il);
            self.dry[1].push(ir);
            let (dl, dr) = (self.dry[0].tap_int(lat_wet), self.dry[1].tap_int(lat_wet));

            let (pl, pr) = self.pitch.tick(il, ir, ratio);
            let (wl, wr) = self.wobble.tick(pl, pr, exc_ms, rate, s.flutter);
            let (tl, tr) = if self.lp_for >= 19_900.0 && s.bass_db.abs() < 0.01 {
                (wl, wr)
            } else {
                let a = self.shelf[0].tick(self.lp[0].tick(wl, &self.lp_c), &self.shelf_c);
                let b = self.shelf[1].tick(self.lp[1].tick(wr, &self.lp_c), &self.shelf_c);
                (a, b)
            };

            // The gate is evaluated where this sample sits on the host's timeline: it left the
            // input `lat_wet` samples ago.
            let pos = start + inc * (i as f64 - lat_wet as f64);
            step = chop::step_at(pos, s.chop_steps_per_beat);
            let target = if chop_on {
                chop::level_gain(s.pattern[step], s.chop_depth)
            } else {
                1.0
            };
            let cg = self.chop_gain.tick(target);
            let (cl, cr) = (tl * cg, tr * cg);

            let (rl, rr) = self.reverb.tick(cl, cr);
            let rm = self.rev.tick(s.rev_mix);
            let (mut wl, mut wr) = (cl + rm * rl, cr + rm * rr);

            let w = self.width.tick(s.width);
            let (m, side) = (0.5 * (wl + wr), 0.5 * (wl - wr) * w);
            wl = m + side;
            wr = m - side;

            let mx = self.mix.tick(s.mix);
            let og = self.out_gain.tick(out_t);
            let (ol, or) = ((dl + (wl - dl) * mx) * og, (dr + (wr - dr) * mx) * og);

            let (ll, lr, gr) = self.limiter.tick(ol, or);
            if gr < min_gain {
                min_gain = gr;
            }
            let b = self.bypass.tick(byp_t);
            l[i] = ll + (bl - ll) * b;
            r[i] = lr + (br - lr) * b;
        }
        self.clock.end_block(start, inc, n);
        BlockInfo { min_gain, step }
    }

    /// The longest tail the reverb can ring for at the current settings, in seconds.
    pub fn tail_seconds(&self) -> f32 {
        self.reverb.tail_seconds()
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
        l: &[f32],
        r: &[f32],
        block: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let (mut ol, mut or) = (l.to_vec(), r.to_vec());
        let mut beats = 0.0;
        for (cl, cr) in ol.chunks_mut(block).zip(or.chunks_mut(block)) {
            let n = cl.len();
            e.process(cl, cr, s, Some(beats), true, Some(120.0));
            beats += n as f64 / e.sample_rate() * 2.0;
        }
        (ol, or)
    }

    #[test]
    fn latency_is_the_sum_of_the_stages_and_the_same_at_every_rate() {
        for sr in [22_050.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            let e = Engine::new(sr);
            let ms = e.latency() as f64 / sr * 1000.0;
            assert!(ms > 5.0 && ms < 80.0, "{sr}: {ms} ms");
        }
    }

    #[test]
    fn bypass_is_a_pure_delay_by_the_reported_latency() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let x = sine(48_000, 330.0, sr, 0.5);
        let s = Settings {
            bypass: true,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, _) = run(&mut e, &s, &x, &x, 480);
        let lat = e.latency();
        for i in lat..x.len() {
            assert!((l[i] - x[i - lat]).abs() < 1e-6, "sample {i}");
        }
    }

    #[test]
    fn mix_zero_is_the_delayed_dry_signal() {
        let sr = 44_100.0;
        let mut e = Engine::new(sr);
        let x = sine(44_100, 220.0, sr, 0.4);
        let s = Settings {
            mix: 0.0,
            rev_mix: 1.0,
            pitch_st: -7.0,
            limiter_on: false,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, _) = run(&mut e, &s, &x, &x, 441);
        let lat_wet = e.latency() - Limiter::new(sr, 2.0, 80.0).latency();
        let lim = e.latency() - lat_wet;
        // With the limiter off the output is the dry signal delayed by `lat_wet` plus whatever
        // the limiter's own (inactive) delay line adds.
        for i in (lat_wet + lim)..x.len() {
            assert!((l[i] - x[i - lat_wet - lim]).abs() < 1e-5, "sample {i}");
        }
    }

    #[test]
    fn the_wet_path_has_the_pitch_and_the_reported_latency() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let x = sine(96_000, 440.0, sr, 0.5);
        let s = Settings {
            pitch_st: -12.0,
            rev_mix: 0.0,
            limiter_on: false,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, _) = run(&mut e, &s, &x, &x, 512);
        let tail = &l[48_000..];
        let crossings = tail
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count() as f64;
        let hz = crossings / (tail.len() as f64 / sr);
        assert!((hz - 220.0).abs() < 4.0, "got {hz} Hz");
        let peak = tail.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.4 && peak < 0.6, "level {peak}");
    }

    #[test]
    fn dual_mono_in_gives_dual_mono_out_when_nothing_is_stereo() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let x = sine(48_000, 500.0, sr, 0.3);
        // Wow and flutter are deliberately a little different left to right (a 0.19 cycle offset
        // and independent flutter) and the reverb is stereo by design, so the check holds for the rest.
        let s = Settings {
            wow_cents: 0.0,
            flutter: 0.0,
            rev_mix: 0.0,
            pitch_st: -2.0,
            width: 1.4,
            chop_depth: 0.7,
            pattern: chop::PATTERNS[3].1,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, r) = run(&mut e, &s, &x, &x, 256);
        let worst = l
            .iter()
            .zip(&r)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-3, "mono sources lost mono: {worst}");
    }

    #[test]
    fn width_zero_collapses_to_mono() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let l = sine(24_000, 300.0, sr, 0.3);
        let r = sine(24_000, 410.0, sr, 0.3);
        let s = Settings {
            width: 0.0,
            rev_mix: 0.5,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (ol, or) = run(&mut e, &s, &l, &r, 480);
        // Wet side is collapsed; the dry side still carries the stereo image, but mix is 100 %.
        let worst = ol[12_000..]
            .iter()
            .zip(&or[12_000..])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-3, "{worst}");
    }

    #[test]
    fn the_result_does_not_depend_on_the_buffer_size() {
        let sr = 48_000.0;
        let x = sine(24_000, 220.0, sr, 0.4);
        let y = sine(24_000, 331.0, sr, 0.3);
        let s = Settings {
            pitch_st: -4.0,
            wow_cents: 12.0,
            flutter: 0.2,
            rev_mix: 0.3,
            lowpass_hz: 9000.0,
            bass_db: 3.0,
            chop_steps_per_beat: 4.0,
            ..Settings::default()
        };
        // Without the chop gate the result is identical for every block size. With it, a gate
        // edge may land one sample apart (the beat position is a float summed per block), which
        // is a difference of a few thousandths for an instant.
        for (pattern, tol) in [(chop::PATTERNS[0].1, 1e-5f32), (chop::PATTERNS[5].1, 6e-3)] {
            let s = Settings { pattern, ..s };
            let mut reference: Option<(Vec<f32>, Vec<f32>)> = None;
            for block in [1usize, 7, 64, 480, 1024, 4096] {
                let mut e = Engine::new(sr);
                e.reset_to(&s);
                let out = run(&mut e, &s, &x, &y, block);
                match &reference {
                    None => reference = Some(out),
                    Some((rl, rr)) => {
                        let worst = out
                            .0
                            .iter()
                            .zip(rl)
                            .chain(out.1.iter().zip(rr))
                            .map(|(a, b)| (a - b).abs())
                            .fold(0.0f32, f32::max);
                        assert!(worst < tol, "block {block}: differs by {worst}");
                    }
                }
            }
        }
    }

    #[test]
    fn hostile_input_never_produces_non_finite_output() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let mut x = sine(20_000, 200.0, sr, 0.5);
        x[100] = f32::NAN;
        x[2000] = f32::INFINITY;
        x[3000] = f32::NEG_INFINITY;
        x[4000] = 1.0e30;
        x[5000] = 1.0e-42;
        let s = Settings {
            rev_mix: 0.5,
            wow_cents: 50.0,
            flutter: 1.0,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, r) = run(&mut e, &s, &x, &x, 333);
        assert!(
            l.iter().chain(&r).all(|v| v.is_finite()),
            "non-finite sample"
        );
    }

    #[test]
    fn extreme_parameters_stay_finite_and_bounded() {
        let sr = 44_100.0;
        let x = sine(30_000, 90.0, sr, 0.9);
        for pitch in [-12.0, 12.0] {
            for decay in [0.2, 20.0] {
                let s = Settings {
                    pitch_st: pitch,
                    reverb: ReverbParams {
                        decay,
                        size: 2.0,
                        damp_hz: 20_000.0,
                        predelay_ms: 250.0,
                    },
                    rev_mix: 1.0,
                    wow_cents: 100.0,
                    wow_rate_hz: 10.0,
                    flutter: 1.0,
                    bass_db: 12.0,
                    lowpass_hz: 800.0,
                    in_db: 24.0,
                    out_db: 24.0,
                    ceiling_db: -12.0,
                    ..Settings::default()
                };
                let mut e = Engine::new(sr);
                e.reset_to(&s);
                let (l, r) = run(&mut e, &s, &x, &x, 512);
                let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(
                    peak.is_finite() && peak <= 10f32.powf(-11.0 / 20.0) * 1.05,
                    "pitch {pitch} decay {decay}: peak {peak}"
                );
            }
        }
    }

    #[test]
    fn the_limiter_holds_the_ceiling() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let x = sine(48_000, 1000.0, sr, 1.0);
        let s = Settings {
            pitch_st: 0.0,
            rev_mix: 0.0,
            in_db: 12.0,
            ceiling_db: -1.0,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, _) = run(&mut e, &s, &x, &x, 480);
        let peak = l[4800..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak <= 10f32.powf(-1.0 / 20.0) + 1e-3, "peak {peak}");
        assert!(peak > 0.85, "limiter over-reduced: {peak}");
    }

    #[test]
    fn silence_decays_to_exact_zero_with_no_subnormals_left() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let s = Settings {
            rev_mix: 1.0,
            pitch_st: -5.0,
            reverb: ReverbParams {
                decay: 20.0,
                ..ReverbParams::default()
            },
            ..Settings::default()
        };
        e.reset_to(&s);
        let burst = sine(4800, 800.0, sr, 0.5);
        run(&mut e, &s, &burst, &burst, 512);
        let silence = vec![0.0f32; (sr as usize) * 120];
        let (l, _) = run(&mut e, &s, &silence, &silence, 1024);
        let last = &l[l.len() - 4800..];
        assert!(
            last.iter().all(|v| *v == 0.0 || v.is_normal()),
            "a subnormal reached the output"
        );
        assert!(last.iter().all(|v| v.abs() < 1e-6), "the tail never died");
        assert!(!e.reverb.holds_subnormals());
    }

    #[test]
    fn the_chop_gate_follows_the_host_timeline() {
        // Pulse: open on even sixteenths. A noise burst train is gated on the host grid
        // after the delay compensation, so the output shows gaps at the host's odd steps.
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let n = (sr * 2.0) as usize;
        let x = vec![0.5f32; n];
        let s = Settings {
            pitch_st: 0.0,
            rev_mix: 0.0,
            limiter_on: false,
            pattern: chop::PATTERNS[1].1,
            chop_steps_per_beat: 4.0,
            ..Settings::default()
        };
        e.reset_to(&s);
        let (l, _) = run(&mut e, &s, &x, &x, 480);
        let lat = e.latency();
        // 120 bpm, 4 steps per beat: a step lasts 0.125 s = 6000 samples. Step 1 (closed) is
        // host time 6000..12000; the plugin's output is delayed by `lat`.
        let host = |t: usize| l[t + lat];
        assert!(host(3000) > 0.45, "step 0 open: {}", host(3000));
        assert!(host(9000).abs() < 0.05, "step 1 closed: {}", host(9000));
        assert!(host(15_000) > 0.45, "step 2 open: {}", host(15_000));
        // The edges sit on the grid within a few milliseconds.
        let first_closed = (2000..n - lat).find(|&t| host(t).abs() < 0.25).unwrap();
        assert!(
            (first_closed as i64 - 6000).abs() < (0.004 * sr) as i64,
            "edge at {first_closed}"
        );
    }

    #[test]
    fn sample_rate_change_rebuilds_cleanly() {
        for sr in [44_100.0, 48_000.0, 88_200.0, 96_000.0, 176_400.0, 192_000.0] {
            let mut e = Engine::new(sr);
            let x = sine(sr as usize / 2, 440.0, sr, 0.5);
            let s = Settings::default();
            e.reset_to(&s);
            let (l, r) = run(&mut e, &s, &x, &x, 256);
            assert!(
                l.iter().chain(&r).all(|v| v.is_finite() && v.abs() <= 1.0),
                "{sr}"
            );
            assert!(l.iter().any(|v| v.abs() > 0.1), "{sr}: silent");
        }
    }
}
