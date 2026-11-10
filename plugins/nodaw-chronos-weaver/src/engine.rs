//! The DSP engine of NoDAW Chronos Weaver: a spectral morph between the live signal (A) and a
//! second spectrum (B: a captured snapshot, or the signal's own past), as a log-magnitude blend
//! or a cross-synthesis (B's spectral envelope on A's fine structure), a spectral freeze with a
//! phase-vocoder sustain, and a granular cloud (per-bin smear, density gating and phase
//! scatter), with a tempo-synced morph LFO, output gain and a latency-compensated mix.
//!
//! ```text
//! in -+-> STFT (Hann, 75 %) -> freeze -> morph(A, B) -> cloud -> ISTFT -> output -+-> mix -> bypass -> out
//!     +-> dry delay (FFT size) -------------------------------------------------^        ^
//!     +-> bypass delay (FFT size) --------------------------------------------------------+
//! ```
//!
//! The FFT size is fixed per sample rate (2048 up to 50 kHz, 4096 above, so ~43 ms either way)
//! and is the latency; it never changes with a setting. With morph 0, freeze off and no cloud
//! the STFT reconstructs the input exactly (tested). Every buffer is allocated in
//! [`Engine::new`]; [`Engine::process`] never allocates.

use nodaw_dsp::chop::ChopClock;
use nodaw_dsp::delay::DelayLine;
use nodaw_dsp::stft::{SpectralProcessor, Stft};
use nodaw_dsp::util::{db_to_gain, undenorm, Noise, Smooth};
use std::f32::consts::PI;

/// Coarse bands of B's spectrum sent to the page.
pub const B_BANDS: usize = 32;

/// Where spectrum B comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The signal itself, `weave` ms ago.
    Past,
    /// A captured snapshot.
    Snapshot,
}

/// How A and B combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorphMode {
    /// Geometric (log-magnitude) blend of the two spectra.
    Blend,
    /// B's spectral envelope on A's fine structure (cross-synthesis).
    Cross,
}

/// LFO shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Sine,
    Triangle,
    Ramp,
}

/// Everything one block needs, in plain units.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    pub bypass: bool,
    /// 0 ..= 1.
    pub morph: f32,
    pub mode: MorphMode,
    pub source: Source,
    /// How far back the Past source looks, seconds.
    pub weave_s: f32,
    /// Rising edge: capture the snapshot.
    pub capture: bool,
    pub freeze: bool,
    /// How long a frozen spectrum lasts: 0 fades in ~1 s, 1 is infinite.
    pub sustain: f32,
    /// Cloud amount, 0 ..= 1.
    pub cloud: f32,
    /// Grain (smear) time, seconds.
    pub grain_s: f32,
    /// Fraction of bins alive at any time, 0 ..= 1.
    pub density: f32,
    /// Phase scatter, 0 ..= 1.
    pub scatter: f32,
    /// LFO depth on the morph, 0 ..= 1.
    pub lfo_depth: f32,
    /// LFO period in beats (0: off).
    pub lfo_beats: f64,
    pub lfo_shape: Shape,
    pub out_db: f32,
    pub mix: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bypass: false,
            morph: 0.0,
            mode: MorphMode::Blend,
            source: Source::Past,
            weave_s: 0.5,
            capture: false,
            freeze: false,
            sustain: 1.0,
            cloud: 0.0,
            grain_s: 0.15,
            density: 1.0,
            scatter: 0.0,
            lfo_depth: 0.0,
            lfo_beats: 0.0,
            lfo_shape: Shape::Sine,
            out_db: 0.0,
            mix: 1.0,
        }
    }
}

/// Where the transport is, for the LFO.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clock {
    pub song_pos_beats: Option<f64>,
    pub playing: bool,
    pub tempo: Option<f64>,
}

/// What the page shows after a block.
#[derive(Debug, Clone, Copy)]
pub struct BlockInfo {
    /// The morph actually applied (LFO included).
    pub morph: f32,
    /// How frozen, 0 ..= 1.
    pub freeze: f32,
    /// A snapshot is held.
    pub has_snapshot: bool,
    /// A capture finished in this block (the snapshot is new: save it with the session).
    pub captured: bool,
    /// B's spectrum in coarse log bands, dB.
    pub b_db: [f32; B_BANDS],
}

/// The per-frame spectral state (both channels).
struct Weave {
    bins: usize,
    hop: usize,
    sr: f64,
    // past frames ring: [ch][frame * bins + k]
    past: [Vec<f32>; 2],
    frames: usize,
    wpos: [usize; 2],
    snap: [Vec<f32>; 2],
    snap_count: [u32; 2],
    has_snap: bool,
    frozen: [Vec<f32>; 2],
    fphase: [Vec<f32>; 2],
    fdelta: [Vec<f32>; 2],
    prev_phase: [Vec<f32>; 2],
    smear: [Vec<f32>; 2],
    gate: [Vec<f32>; 2],
    env_a: Vec<f32>,
    env_b: Vec<f32>,
    freeze_amt: [f32; 2],
    noise: Noise,
    // frames seen per channel since the engine was made (never reset: the cloud's random
    // numbers depend on the frame, not on how long the effect has been running)
    fcount: [u64; 2],
    // frames since the last reset (the first ones are only partly filled), and whether the
    // frozen spectrum holds a real frame yet (a freeze engaged in silence, or in the first
    // frames after a reset, keeps taking frames until the signal is there)
    warm: [usize; 2],
    fz_live: [bool; 2],
    // settings of the current block
    s: Settings,
    morph: f32,
    capturing: bool,
}

const EPS: f32 = 1.0e-7;
const SNAP_FRAMES: u32 = 12;

impl Weave {
    fn new(n: usize, sr: f64) -> Self {
        let bins = n / 2 + 1;
        let hop = n / 4;
        let frames = ((2.05 * sr) / hop as f64) as usize + 2;
        let v = || vec![0.0f32; bins];
        Self {
            bins,
            hop,
            sr,
            past: [vec![0.0; frames * bins], vec![0.0; frames * bins]],
            frames,
            wpos: [0, 0],
            snap: [v(), v()],
            snap_count: [0, 0],
            has_snap: false,
            frozen: [v(), v()],
            fphase: [v(), v()],
            fdelta: [v(), v()],
            prev_phase: [v(), v()],
            smear: [v(), v()],
            gate: [vec![1.0; bins], vec![1.0; bins]],
            env_a: v(),
            env_b: v(),
            freeze_amt: [0.0, 0.0],
            noise: Noise::new(0x51ce_b00c),
            fcount: [0, 0],
            warm: [0, 0],
            fz_live: [false, false],
            s: Settings::default(),
            morph: 0.0,
            capturing: false,
        }
    }

    fn reset(&mut self) {
        for c in 0..2 {
            self.past[c].fill(0.0);
            self.frozen[c].fill(0.0);
            self.fphase[c].fill(0.0);
            self.fdelta[c].fill(0.0);
            self.prev_phase[c].fill(0.0);
            self.smear[c].fill(0.0);
            self.gate[c].fill(1.0);
            self.freeze_amt[c] = 0.0;
            self.wpos[c] = 0;
            self.warm[c] = 0;
            self.fz_live[c] = false;
        }
        // the snapshot is kept: it is the user's capture
    }

    /// A smooth spectral envelope (forward-backward one-pole across bins).
    #[allow(clippy::needless_range_loop)]
    fn envelope(src: &[f32], out: &mut [f32]) {
        let a = 0.82f32;
        let mut y = src[0];
        for k in 0..src.len() {
            y = a * y + (1.0 - a) * src[k];
            out[k] = y;
        }
        let mut y = out[src.len() - 1];
        for k in (0..src.len()).rev() {
            y = a * y + (1.0 - a) * out[k];
            out[k] = y.max(EPS);
        }
    }

    fn idle(&self) -> bool {
        self.morph <= 0.0
            && !self.s.freeze
            && self.freeze_amt[0] <= 0.0
            && self.freeze_amt[1] <= 0.0
            && self.s.cloud <= 0.0
    }
}

impl SpectralProcessor for Weave {
    // the bin loops index several per-bin arrays at once
    #[allow(clippy::needless_range_loop)]
    fn process(&mut self, c: usize, mag: &mut [f32], phase: &mut [f32]) {
        let bins = self.bins;
        let hop_s = self.hop as f32 / self.sr as f32;
        // remember the live spectrum (for Past and the snapshot)
        let fno = self.fcount[c];
        self.fcount[c] = fno.wrapping_add(1);
        let w = self.wpos[c];
        self.past[c][w * bins..(w + 1) * bins].copy_from_slice(mag);
        self.wpos[c] = (w + 1) % self.frames;
        if self.capturing {
            let n = self.snap_count[c];
            if n < SNAP_FRAMES {
                let t = 1.0 / (n + 1) as f32;
                for k in 0..bins {
                    self.snap[c][k] += (mag[k] - self.snap[c][k]) * t;
                }
                self.snap_count[c] = n + 1;
            }
        }

        // freeze: capture on the way in, then a phase-vocoder sustain
        let target = if self.s.freeze { 1.0 } else { 0.0 };
        let fa = self.freeze_amt[c];
        let step = (hop_s / 0.08).min(1.0);
        let nfa = if target > fa {
            (fa + step).min(1.0)
        } else {
            (fa - step).max(0.0)
        };
        let warm = self.warm[c] >= 4;
        if !warm {
            self.warm[c] += 1;
        }
        if fa <= 0.0 && nfa > 0.0 {
            self.fz_live[c] = false;
        }
        if nfa > 0.0 && !self.fz_live[c] {
            // a full frame with signal in it (-80 dBFS): from then on the freeze holds it
            let full = 0.25 * (2 * (bins - 1)) as f32;
            let peak = mag[..bins].iter().fold(0.0f32, |m, v| m.max(*v));
            self.fz_live[c] = warm && peak > full * 1.0e-4;
            for k in 0..bins {
                self.frozen[c][k] = mag[k];
                self.fphase[c][k] = phase[k];
                let mut d = phase[k] - self.prev_phase[c][k];
                d -= (2.0 * PI) * (d / (2.0 * PI)).round();
                self.fdelta[c][k] = d;
            }
        }
        self.freeze_amt[c] = nfa;
        self.prev_phase[c][..bins].copy_from_slice(&phase[..bins]);
        if self.idle() {
            return;
        }
        if nfa > 0.0 {
            // sustain 1 holds forever; 0 fades to -60 dB in ~1 s
            let rt = 0.6 + 60.0 * self.s.sustain * self.s.sustain;
            let decay = if self.s.sustain >= 0.999 {
                1.0
            } else {
                db_to_gain(-60.0 * hop_s / rt)
            };
            for k in 0..bins {
                self.frozen[c][k] *= decay;
                self.fphase[c][k] += self.fdelta[c][k];
                if self.fphase[c][k] > PI {
                    self.fphase[c][k] -= 2.0 * PI * ((self.fphase[c][k] + PI) / (2.0 * PI)).floor();
                }
                let fm = self.frozen[c][k];
                mag[k] += (fm - mag[k]) * nfa;
                if nfa >= 1.0 {
                    phase[k] = self.fphase[c][k];
                } else {
                    // blend on the complex plane
                    let (s1, c1) = phase[k].sin_cos();
                    let (s2, c2) = self.fphase[c][k].sin_cos();
                    phase[k] = (s1 * (1.0 - nfa) + s2 * nfa).atan2(c1 * (1.0 - nfa) + c2 * nfa);
                }
            }
        }

        // morph with B
        let t = self.morph;
        // a Snapshot source with nothing captured yet is A itself: nothing to morph
        let no_b = self.s.source == Source::Snapshot && !self.has_snap;
        if t > 0.0 && !no_b {
            let use_snap = self.s.source == Source::Snapshot && self.has_snap;
            let back = ((self.s.weave_s as f64 * self.sr) / self.hop as f64).round() as usize;
            let back = back.clamp(1, self.frames - 1);
            let bf = (self.wpos[c] + self.frames - 1 - back) % self.frames;
            let (b_src, b_off): (&[f32], usize) = if use_snap {
                (&self.snap[c], 0)
            } else {
                (&self.past[c], bf * bins)
            };
            let b = &b_src[b_off..b_off + bins];
            match self.s.mode {
                MorphMode::Blend => {
                    for k in 0..bins {
                        let la = (mag[k] + EPS).ln();
                        let lb = (b[k] + EPS).ln();
                        mag[k] = ((la + (lb - la) * t).exp() - EPS).max(0.0);
                    }
                }
                MorphMode::Cross => {
                    let mut ea = std::mem::take(&mut self.env_a);
                    let mut eb = std::mem::take(&mut self.env_b);
                    Self::envelope(mag, &mut ea);
                    Self::envelope(b, &mut eb);
                    for k in 0..bins {
                        let g = (eb[k] / ea[k]).clamp(1.0e-4, 1.0e3);
                        mag[k] *= g.powf(t);
                    }
                    self.env_a = ea;
                    self.env_b = eb;
                }
            }
        }

        // the cloud: smear in time, gate by density, scatter the phases
        let cl = self.s.cloud;
        if cl > 0.0 {
            let a = 1.0 - (-hop_s / self.s.grain_s.max(0.005)).exp();
            let dens = self.s.density.clamp(0.02, 1.0);
            let gate_a = (hop_s / (0.25 * self.s.grain_s.max(0.005))).min(1.0);
            let sc = self.s.scatter * PI;
            // density gating keeps the energy: the live bins are boosted by 1/sqrt(density)
            let comp = 1.0 / dens.sqrt();
            // this frame's random numbers: a function of the frame and the channel alone
            let h = (fno ^ ((c as u64) << 62)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            self.noise = Noise::new((h >> 32) as u32 ^ h as u32);
            for k in 0..bins {
                let sm = &mut self.smear[c][k];
                *sm += (mag[k] - *sm) * a;
                let alive = if dens >= 1.0 || (self.noise.sample() * 0.5 + 0.5) < dens {
                    1.0
                } else {
                    0.0
                };
                let g = &mut self.gate[c][k];
                *g += (alive - *g) * gate_a;
                let cloud_mag = *sm * *g * comp;
                mag[k] += (cloud_mag - mag[k]) * cl;
                if sc > 0.0 {
                    phase[k] += sc * cl * self.noise.sample();
                }
            }
        }
    }
}

/// The engine.
pub struct Engine {
    sr: f64,
    stft: Stft,
    weave: Weave,
    dry: [DelayLine; 2],
    byp: [DelayLine; 2],
    lat: usize,
    clock: ChopClock,
    last_capture: bool,
    out_gain: Smooth,
    mix: Smooth,
    bypass: Smooth,
    fresh: bool,
}

impl Engine {
    /// The FFT size at `sample_rate`.
    pub fn fft_size(sample_rate: f64) -> usize {
        if sample_rate > 50_000.0 {
            4096
        } else {
            2048
        }
    }

    /// An engine for `sample_rate`.
    pub fn new(sample_rate: f64) -> Self {
        let n = Self::fft_size(sample_rate);
        let stft = Stft::new(n);
        let lat = stft.latency();
        Self {
            sr: sample_rate,
            stft,
            weave: Weave::new(n, sample_rate),
            dry: [DelayLine::new(lat + 4), DelayLine::new(lat + 4)],
            byp: [DelayLine::new(lat + 4), DelayLine::new(lat + 4)],
            lat,
            clock: ChopClock::default(),
            last_capture: false,
            out_gain: Smooth::new(1.0, sample_rate, 20.0, 20.0),
            mix: Smooth::new(1.0, sample_rate, 20.0, 20.0),
            bypass: Smooth::new(0.0, sample_rate, 10.0, 10.0),
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

    /// Clear every buffer (the captured snapshot is kept).
    pub fn reset(&mut self) {
        self.stft.reset();
        self.weave.reset();
        for d in self.dry.iter_mut().chain(self.byp.iter_mut()) {
            d.clear();
        }
        self.clock = ChopClock::default();
        self.fresh = true;
    }

    /// Whether a snapshot is held.
    pub fn has_snapshot(&self) -> bool {
        self.weave.has_snap
    }

    /// Bins per channel of the snapshot at this engine's FFT size.
    pub fn snapshot_bins(&self) -> usize {
        self.weave.bins
    }

    /// The held snapshot's magnitudes (left, right), if there is one.
    pub fn snapshot(&self) -> Option<(&[f32], &[f32])> {
        let w = &self.weave;
        w.has_snap.then(|| (&w.snap[0][..], &w.snap[1][..]))
    }

    /// Hold a snapshot restored with the session: `get(ch, k)` gives the magnitude of bin `k`
    /// (0 ..= `bins - 1`) of a snapshot taken at `sample_rate` with `bins` bins per channel; it
    /// is resampled to this engine's bins by frequency (and scaled for the FFT size) when they
    /// differ. A capture button held on in the restored values does not capture again.
    pub fn restore_snapshot(
        &mut self,
        sample_rate: f64,
        bins: usize,
        capture_on: bool,
        get: impl Fn(usize, usize) -> f32,
    ) {
        let w = &mut self.weave;
        let nb = w.bins;
        let same = bins == nb && (sample_rate - w.sr).abs() < 0.5;
        // bin spacing ratio and magnitude scale (an FFT's magnitudes grow with its size)
        let ratio = (w.sr / ((nb - 1) * 2) as f64) / (sample_rate / ((bins - 1) * 2) as f64);
        let scale = (nb - 1) as f32 / (bins - 1) as f32;
        for c in 0..2 {
            for k in 0..nb {
                w.snap[c][k] = if same {
                    get(c, k)
                } else {
                    let x = k as f64 * ratio;
                    let i = x.floor() as usize;
                    if i + 1 >= bins {
                        get(c, bins - 1) * scale
                    } else {
                        let f = (x - i as f64) as f32;
                        (get(c, i) + (get(c, i + 1) - get(c, i)) * f) * scale
                    }
                };
            }
            w.snap_count[c] = SNAP_FRAMES;
        }
        w.has_snap = true;
        w.capturing = false;
        self.last_capture = capture_on;
    }

    /// Forget the snapshot (a restored session without one). A capture button held on in the
    /// restored values does not capture again.
    pub fn forget_snapshot(&mut self, capture_on: bool) {
        self.clear_snapshot();
        self.weave.capturing = false;
        self.last_capture = capture_on;
    }

    /// Forget the snapshot.
    pub fn clear_snapshot(&mut self) {
        self.weave.has_snap = false;
        for c in 0..2 {
            self.weave.snap[c].fill(0.0);
            self.weave.snap_count[c] = 0;
        }
    }

    /// How long the plugin rings after the input stops, seconds.
    pub fn tail_seconds(s: &Settings) -> f32 {
        if s.freeze {
            return if s.sustain >= 0.999 {
                30.0
            } else {
                0.6 + 60.0 * s.sustain * s.sustain
            };
        }
        let mut t = 0.05;
        if s.cloud > 0.0 {
            t += 7.0 * s.grain_s;
        }
        if s.morph > 0.0 || s.lfo_depth > 0.0 {
            t += s.weave_s;
        }
        t
    }

    fn lfo(shape: Shape, ph: f64) -> f32 {
        let p = ph.rem_euclid(1.0) as f32;
        match shape {
            Shape::Sine => (2.0 * PI * p).sin(),
            Shape::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
            Shape::Ramp => 2.0 * p - 1.0,
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
        let (start, inc) =
            self.clock
                .begin_block(clock.song_pos_beats, clock.playing, clock.tempo, self.sr);
        let mut morph = s.morph;
        if s.lfo_depth > 0.0 && s.lfo_beats > 0.0 {
            // the LFO swings the morph around its knob, clamped to 0 ..= 1
            morph += 0.5 * s.lfo_depth * Self::lfo(s.lfo_shape, start / s.lfo_beats);
        }
        let morph = morph.clamp(0.0, 1.0);
        if s.capture && !self.last_capture {
            self.weave.snap_count = [0, 0];
            for c in 0..2 {
                self.weave.snap[c].fill(0.0);
            }
            self.weave.capturing = true;
        }
        self.last_capture = s.capture;
        let mut captured = false;
        if self.weave.capturing && self.weave.snap_count.iter().all(|c| *c >= SNAP_FRAMES) {
            self.weave.capturing = false;
            self.weave.has_snap = true;
            captured = true;
        }
        self.weave.s = *s;
        self.weave.morph = morph;

        let out_t = db_to_gain(s.out_db);
        let byp_t = if s.bypass { 1.0 } else { 0.0 };
        if self.fresh {
            self.fresh = false;
            self.out_gain.reset(out_t);
            self.mix.reset(s.mix);
            self.bypass.reset(byp_t);
        }
        for i in 0..n {
            let (xl, xr) = (l[i], r[i]);
            let (xl, xr) = (
                if xl.is_finite() { xl } else { 0.0 },
                if xr.is_finite() { xr } else { 0.0 },
            );
            let (a, b) = self.stft.tick(xl, xr, &mut self.weave);
            let g = self.out_gain.tick(out_t);
            let (a, b) = (a * g, b * g);
            self.dry[0].push(xl);
            self.dry[1].push(xr);
            let (dl, dr) = (self.dry[0].tap_int(self.lat), self.dry[1].tap_int(self.lat));
            let m = self.mix.tick(s.mix);
            let (ol, or) = if m >= 1.0 {
                (a, b)
            } else {
                (dl + (a - dl) * m, dr + (b - dr) * m)
            };
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
        self.clock.end_block(start, inc, n);

        // B in coarse bands for the page
        let mut b_db = [-120.0f32; B_BANDS];
        let w = &self.weave;
        let bins = w.bins;
        let norm = 2.0 / (0.5 * Self::fft_size(self.sr) as f32);
        let back = ((s.weave_s as f64 * self.sr) / w.hop as f64).round() as usize;
        let bf = (w.wpos[0] + w.frames - 1 - back.clamp(1, w.frames - 1)) % w.frames;
        let use_snap = s.source == Source::Snapshot && w.has_snap;
        if s.source == Source::Past || use_snap {
            for (j, out) in b_db.iter_mut().enumerate() {
                let f0 = 20.0 * 1000f32.powf(j as f32 / B_BANDS as f32);
                let f1 = 20.0 * 1000f32.powf((j + 1) as f32 / B_BANDS as f32);
                let k0 = ((f0 / self.sr as f32) * 2.0 * (bins - 1) as f32) as usize;
                let k1 = (((f1 / self.sr as f32) * 2.0 * (bins - 1) as f32) as usize)
                    .max(k0 + 1)
                    .min(bins);
                let mut m = 0.0f32;
                for k in k0.min(bins - 1)..k1 {
                    let v = if use_snap {
                        w.snap[0][k]
                    } else {
                        w.past[0][bf * bins + k]
                    };
                    m = m.max(v);
                }
                *out = 20.0 * (m * norm + 1.0e-9).log10();
            }
        }
        BlockInfo {
            morph,
            freeze: 0.5 * (w.freeze_amt[0] + w.freeze_amt[1]),
            has_snapshot: w.has_snap,
            captured,
            b_db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodaw_dsp::fft::SpectrumAnalyzer;

    fn sine(n: usize, hz: f32, sr: f64, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * PI * hz * i as f32 / sr as f32).sin())
            .collect()
    }

    fn run(e: &mut Engine, s: &Settings, x: &[f32], block: usize) -> Vec<f32> {
        let (mut l, mut r) = (x.to_vec(), x.to_vec());
        let clock = Clock {
            tempo: Some(120.0),
            ..Clock::default()
        };
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            e.process(a, b, s, &clock);
        }
        l
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn defaults_reconstruct_the_input_delayed_by_the_fft_size() {
        for sr in [44_100.0, 48_000.0, 96_000.0] {
            let mut e = Engine::new(sr);
            let lat = e.latency();
            assert_eq!(lat, Engine::fft_size(sr));
            let x: Vec<f32> = (0..30_000)
                .map(|i| ((i * 37 % 101) as f32 - 50.0) / 60.0)
                .collect();
            let y = run(&mut e, &Settings::default(), &x, 300);
            let worst = (lat + 10..x.len())
                .map(|i| (y[i] - x[i - lat]).abs())
                .fold(0.0f32, f32::max);
            assert!(worst < 1e-4, "{sr}: {worst}");
        }
    }

    #[test]
    fn freeze_sustains_a_tone_after_the_input_stops() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let mut x = sine(48_000, 440.0, sr, 0.5);
        x.extend(std::iter::repeat_n(0.0, 96_000));
        let (mut l, mut r) = (x.clone(), x.clone());
        let clock = Clock::default();
        for (i, (a, b)) in l.chunks_mut(512).zip(r.chunks_mut(512)).enumerate() {
            let s = Settings {
                freeze: i * 512 >= 30_000,
                ..Settings::default()
            };
            e.process(a, b, &s, &clock);
        }
        let tail = &l[100_000..140_000];
        assert!(rms(tail) > 0.15, "frozen level {}", rms(tail));
        let mut an = SpectrumAnalyzer::new(16384);
        an.analyze(&l[110_000..110_000 + 16384]);
        let f = an.peak_frequency(sr);
        assert!((f / 440.0 - 1.0).abs() < 0.02, "{f}");
    }

    #[test]
    fn morph_to_the_past_brings_in_what_was_playing() {
        // 0.5 s of 300 Hz then 1000 Hz: at full morph to the past (0.5 s), 300 Hz is heard
        // while 1000 Hz plays.
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let mut x = sine(24_000, 300.0, sr, 0.5);
        x.extend(sine(30_000, 1000.0, sr, 0.5));
        let s = Settings {
            morph: 1.0,
            source: Source::Past,
            weave_s: 0.5,
            ..Settings::default()
        };
        let y = run(&mut e, &s, &x, 256);
        let mut an = SpectrumAnalyzer::new(8192);
        an.analyze(&y[24_000 + 2048 + 1000..24_000 + 2048 + 1000 + 8192]);
        let f = an.peak_frequency(sr);
        assert!(f < 600.0, "{f}");
    }

    #[test]
    fn snapshot_cross_synthesis_imposes_b_and_needs_a_capture() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        // capture a bright noise, then process a dark tone with Cross
        let mut nz = Noise::new(3);
        let noise: Vec<f32> = (0..24_000).map(|_| 0.3 * nz.sample()).collect();
        let s_cap = Settings {
            capture: true,
            ..Settings::default()
        };
        run(&mut e, &s_cap, &noise, 512);
        run(&mut e, &Settings::default(), &noise[..4096], 512);
        assert!(e.has_snapshot());
        let s = Settings {
            morph: 1.0,
            source: Source::Snapshot,
            mode: MorphMode::Cross,
            cloud: 0.6,
            density: 0.5,
            scatter: 0.5,
            ..Settings::default()
        };
        let y = run(&mut e, &s, &sine(48_000, 220.0, sr, 0.5), 128);
        assert!(y.iter().all(|v| v.is_finite() && v.abs() < 8.0));
        assert!(rms(&y[10_000..]) > 0.01);
        e.clear_snapshot();
        assert!(!e.has_snapshot());
    }

    #[test]
    fn mix_zero_and_bypass_are_the_delayed_input() {
        let sr = 48_000.0;
        let x = sine(20_000, 300.0, sr, 0.4);
        for s in [
            Settings {
                mix: 0.0,
                morph: 1.0,
                freeze: true,
                cloud: 1.0,
                ..Settings::default()
            },
            Settings {
                bypass: true,
                morph: 1.0,
                ..Settings::default()
            },
        ] {
            let mut e = Engine::new(sr);
            let lat = e.latency();
            let y = run(&mut e, &s, &x, 100);
            for i in lat..x.len() {
                assert_eq!(y[i], x[i - lat], "sample {i}");
            }
        }
    }

    #[test]
    fn the_lfo_moves_the_morph() {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        let s = Settings {
            morph: 0.5,
            lfo_depth: 1.0,
            lfo_beats: 1.0,
            ..Settings::default()
        };
        let mut x = vec![0.1f32; 512];
        let mut y = x.clone();
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        for _ in 0..200 {
            let info = e.process(
                &mut x,
                &mut y,
                &s,
                &Clock {
                    tempo: Some(120.0),
                    ..Clock::default()
                },
            );
            lo = lo.min(info.morph);
            hi = hi.max(info.morph);
        }
        assert!(lo < 0.1 && hi > 0.9, "{lo}..{hi}");
    }
}
