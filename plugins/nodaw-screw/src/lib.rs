//! NoDAW Screw: a real-time chopped-and-screwed effect for any DAW (CLAP and VST3).
//!
//! A pitch drop, tape wow and flutter, tone, a tempo-synced chop gate, a reverb and a
//! true-peak limiter. Every control in the window is a host parameter (automatable, saved in
//! the session); the only extra session state is the Custom chop pattern and the A/B slots,
//! kept in the [`ScrewController`]. See `docs/DESIGN.md`.

pub mod engine;
pub mod license;
pub mod shared;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use apf::prelude::*;
use nodaw_dsp::chop::{self, STEPS};
use nodaw_dsp::meter::GainReduction;
use nodaw_dsp::reverb::{Reverb, ReverbParams};

use engine::{Engine, Settings};
use shared::{ScrewController, Shared};

#[cfg(feature = "ui")]
use apf_webview::{assets, Asset, WebView};

/// Chop pattern choices: the factory patterns, then the one the user draws.
const PATTERN_NAMES: &[&str] = &["Off", "Pulse", "Offbeat", "Skip", "Stutter", "Trap Roll", "Half-time", "Gallop", "Custom"];
const CUSTOM_PATTERN: usize = 8;
const RATE_NAMES: &[&str] = &["1/4", "1/8", "1/16", "1/32", "1/8T", "1/16T"];

/// The parameters. Append only: a host stores them by the hash of the key, so renaming a key
/// or changing a range would break saved sessions.
pub const PARAMS: &[ParamDef] = &[
    ParamDef::bypass("bypass"),
    ParamDef::float("in_gain", "Input", -24.0, 24.0, 0.0).unit("dB").decimals(1).module("Global"),
    ParamDef::float("out_gain", "Output", -24.0, 24.0, 0.0).unit("dB").decimals(1).module("Global"),
    ParamDef::float("mix", "Mix", 0.0, 100.0, 100.0).unit("%").decimals(0).module("Global"),
    ParamDef::float("pitch", "Pitch", -12.0, 12.0, -3.0).unit("st").decimals(2).module("Screw"),
    ParamDef::float("wow", "Wow", 0.0, 100.0, 0.0).unit("ct").decimals(1).module("Screw"),
    ParamDef::float("wow_rate", "Wow Rate", 0.1, 10.0, 0.7).unit("Hz").curve(Curve::Log).decimals(2).module("Screw"),
    ParamDef::float("flutter", "Flutter", 0.0, 100.0, 0.0).unit("%").decimals(0).module("Screw"),
    ParamDef::float("lowpass", "Lowpass", 800.0, 20_000.0, 20_000.0).unit("Hz").curve(Curve::Log).decimals(0).module("Tone"),
    ParamDef::float("bass", "Warmth", -6.0, 12.0, 0.0).unit("dB").decimals(1).module("Tone"),
    ParamDef::float("rev_mix", "Reverb", 0.0, 100.0, 25.0).unit("%").decimals(0).module("Space"),
    ParamDef::float("rev_decay", "Decay", 0.2, 20.0, 3.5).unit("s").curve(Curve::Log).decimals(2).module("Space"),
    ParamDef::float("rev_size", "Size", 0.5, 2.0, 1.0).unit("x").decimals(2).module("Space"),
    ParamDef::float("rev_damp", "Damping", 500.0, 20_000.0, 6000.0).unit("Hz").curve(Curve::Log).decimals(0).module("Space"),
    ParamDef::float("rev_pre", "Pre-delay", 0.0, 250.0, 20.0).unit("ms").curve(Curve::Power(2.0)).decimals(0).module("Space"),
    ParamDef::choice("chop_pattern", "Chop Pattern", PATTERN_NAMES, 0).module("Chop"),
    ParamDef::choice("chop_rate", "Chop Rate", RATE_NAMES, 1).module("Chop"),
    ParamDef::float("chop_depth", "Chop Depth", 0.0, 100.0, 100.0).unit("%").decimals(0).module("Chop"),
    ParamDef::float("width", "Width", 0.0, 200.0, 100.0).unit("%").decimals(0).module("Global"),
    ParamDef::bool("limiter", "Limiter", true).module("Output"),
    ParamDef::float("ceiling", "Ceiling", -12.0, 0.0, -1.0).unit("dB").decimals(1).module("Output"),
];

const BYPASS: ParamId = ParamId::of(PARAMS, "bypass");
const IN_GAIN: ParamId = ParamId::of(PARAMS, "in_gain");
const OUT_GAIN: ParamId = ParamId::of(PARAMS, "out_gain");
const MIX: ParamId = ParamId::of(PARAMS, "mix");
const PITCH: ParamId = ParamId::of(PARAMS, "pitch");
const WOW: ParamId = ParamId::of(PARAMS, "wow");
const WOW_RATE: ParamId = ParamId::of(PARAMS, "wow_rate");
const FLUTTER: ParamId = ParamId::of(PARAMS, "flutter");
const LOWPASS: ParamId = ParamId::of(PARAMS, "lowpass");
const BASS: ParamId = ParamId::of(PARAMS, "bass");
const REV_MIX: ParamId = ParamId::of(PARAMS, "rev_mix");
const REV_DECAY: ParamId = ParamId::of(PARAMS, "rev_decay");
const REV_SIZE: ParamId = ParamId::of(PARAMS, "rev_size");
const REV_DAMP: ParamId = ParamId::of(PARAMS, "rev_damp");
const REV_PRE: ParamId = ParamId::of(PARAMS, "rev_pre");
const CHOP_PATTERN: ParamId = ParamId::of(PARAMS, "chop_pattern");
const CHOP_RATE: ParamId = ParamId::of(PARAMS, "chop_rate");
const CHOP_DEPTH: ParamId = ParamId::of(PARAMS, "chop_depth");
const WIDTH: ParamId = ParamId::of(PARAMS, "width");
const LIMITER: ParamId = ParamId::of(PARAMS, "limiter");
const CEILING: ParamId = ParamId::of(PARAMS, "ceiling");

/// What a host's generic knobs (Bitwig, Ableton macros, hardware controllers) page through.
const REMOTE: &[RemotePage] = &[
    RemotePage { section: "Screw", name: "Screw", params: ["pitch", "mix", "wow", "wow_rate", "flutter", "lowpass", "bass", "out_gain"] },
    RemotePage { section: "Space", name: "Space", params: ["rev_mix", "rev_decay", "rev_size", "rev_damp", "rev_pre", "width", "mix", "out_gain"] },
    RemotePage { section: "Chop", name: "Chop", params: ["chop_pattern", "chop_rate", "chop_depth", "pitch", "rev_mix", "ceiling", "in_gain", "out_gain"] },
];

#[cfg(feature = "ui")]
const UI: &[Asset] = assets![
    "index.html" => "../ui/index.html",
    "style.css" => "../ui/style.css",
    "app.js" => "../ui/app.js",
    "fonts/space-grotesk.woff2" => "../ui/fonts/space-grotesk.woff2",
    "fonts/bebas-neue.woff2" => "../ui/fonts/bebas-neue.woff2",
    "fonts/jetbrains-mono.woff2" => "../ui/fonts/jetbrains-mono.woff2",
];

/// The plugin.
pub struct NoDawScrew {
    engine: Engine,
    shared: Arc<Shared>,
    /// Trial over and not activated: half a second of silence every 45 s (license.rs).
    dropout: license::Dropout,
}

/// The engine's settings from the parameter values. No allocation.
fn settings(params: &Params, shared: &Shared) -> Settings {
    let pat = params.get_index(CHOP_PATTERN);
    let pattern = if pat == CUSTOM_PATTERN { shared.custom_pattern() } else { chop::PATTERNS[pat.min(chop::PATTERNS.len() - 1)].1 };
    let rate = params.get_index(CHOP_RATE).min(chop::RATES.len() - 1);
    Settings {
        bypass: params.get_bool(BYPASS),
        in_db: params.get_f32(IN_GAIN),
        out_db: params.get_f32(OUT_GAIN),
        mix: params.get_f32(MIX) * 0.01,
        pitch_st: params.get_f32(PITCH),
        wow_cents: params.get_f32(WOW),
        wow_rate_hz: params.get_f32(WOW_RATE),
        flutter: params.get_f32(FLUTTER) * 0.01,
        lowpass_hz: params.get_f32(LOWPASS),
        bass_db: params.get_f32(BASS),
        rev_mix: params.get_f32(REV_MIX) * 0.01,
        reverb: ReverbParams {
            decay: params.get_f32(REV_DECAY),
            size: params.get_f32(REV_SIZE),
            damp_hz: params.get_f32(REV_DAMP),
            predelay_ms: params.get_f32(REV_PRE),
        },
        pattern,
        chop_steps_per_beat: chop::RATES[rate].1,
        chop_depth: params.get_f32(CHOP_DEPTH) * 0.01,
        width: params.get_f32(WIDTH) * 0.01,
        limiter_on: params.get_bool(LIMITER),
        ceiling_db: params.get_f32(CEILING),
    }
}

impl Plugin for NoDawScrew {
    const INFO: PluginInfo = PluginInfo {
        id: "com.nodaw.screw",
        name: "NoDAW Screw",
        vendor: "NoDAW",
        url: "https://nodaw.app",
        version: env!("CARGO_PKG_VERSION"),
        description: "Chopped-and-screwed pitch, tape wobble, chop gate, reverb and a true-peak limiter",
        features: &["audio-effect", "stereo"],
    };
    const LAYOUT: AudioLayout = AudioLayout::EFFECT_STEREO;
    const PARAMS: &'static [ParamDef] = PARAMS;
    const REMOTE_CONTROLS: &'static [RemotePage] = REMOTE;
    const FACTORY_PRESETS: &'static [&'static str] = apf::presets![
        "../presets/Init.apfpreset",
        "../presets/Classic Screw.apfpreset",
        "../presets/Slowed + Reverb.apfpreset",
        "../presets/Houston Drip.apfpreset",
        "../presets/Tape Warble.apfpreset",
        "../presets/Chopped Gate.apfpreset",
        "../presets/Trap Roll Chop.apfpreset",
        "../presets/Cathedral Drag.apfpreset",
        "../presets/Lo-Fi Vinyl Drift.apfpreset",
        "../presets/Night Drive.apfpreset",
        "../presets/Wide Dream.apfpreset",
        "../presets/Half-Time Haze.apfpreset",
    ];

    fn new() -> Self {
        Self { engine: Engine::new(48_000.0), shared: Arc::new(Shared::new()), dropout: license::Dropout::new(48_000.0) }
    }

    fn controller(&mut self, host: HostHandle) -> Option<Arc<dyn Controller>> {
        // A licensed copy whose last online check is over 30 days old asks again, off-thread.
        self.shared.license.maybe_recheck(license::now_secs());
        Some(Arc::new(ScrewController::new(self.shared.clone(), Some(host))))
    }

    // Main thread: allocate everything here, for the sample rate the host will use.
    fn activate(&mut self, config: &ProcessConfig, params: &Params) {
        if (self.engine.sample_rate() - config.sample_rate).abs() > 0.5 {
            self.engine = Engine::new(config.sample_rate);
            self.dropout = license::Dropout::new(config.sample_rate);
        }
        // A trial that ran out while the session was open takes effect here (and when the
        // window asks for the license state).
        self.shared.license.refresh(license::now_secs());
        self.engine.reset_to(&settings(params, &self.shared));
        self.shared.sample_rate.store((config.sample_rate as f32).to_bits(), Ordering::Relaxed);
        self.shared.latency.store(self.engine.latency() as u32, Ordering::Relaxed);
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn latency(&self) -> u32 {
        self.engine.latency() as u32
    }

    fn tail(&self) -> u32 {
        (Reverb::MAX_TAIL_SECONDS as f64 * self.engine.sample_rate()).ceil() as u32
    }

    fn process(&mut self, audio: &mut AudioBuffer, ctx: &ProcessContext) -> ProcessStatus {
        let Some((l, r)) = audio.stereo() else { return ProcessStatus::Error };
        let s = settings(ctx.params, &self.shared);
        let sh = &*self.shared;
        sh.in_meter.record(l, r);
        sh.in_scope.push_stereo(l, r);
        let info = self.engine.process(l, r, &s, ctx.transport.song_pos_beats, ctx.transport.playing, ctx.transport.tempo);
        // Lock-free: one relaxed atomic load. Bypassed audio is the dry signal: left alone.
        self.dropout.process(l, r, sh.license.demo_flag() && !s.bypass);
        sh.out_meter.record(l, r);
        sh.out_scope.push_stereo(l, r);
        GainReduction::record(&sh.gr, info.min_gain);
        sh.step.store(if s.pattern.iter().any(|&v| v != 2) { info.step as u8 } else { 255 }, Ordering::Relaxed);
        sh.playing.store(ctx.transport.playing as u8, Ordering::Relaxed);
        ProcessStatus::Continue
    }

    #[cfg(feature = "ui")]
    fn view() -> Option<Box<dyn View>> {
        Some(Box::new(WebView::new(960, 600, UI).resizable(true).size_limits((760, 480), (2600, 1700)).fit_screen((48, 96)).background(10, 10, 10)))
    }
}

apf::export_plugin!(NoDawScrew);

#[allow(dead_code)]
const _: usize = STEPS;
