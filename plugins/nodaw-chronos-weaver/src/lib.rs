//! NoDAW Chronos Weaver (CLAP and VST3): spectral morphing between the live signal and a second
//! spectrum (a captured snapshot, or the signal's own past) as a blend or a cross-synthesis, a
//! spectral freeze, a granular cloud and a tempo-synced morph LFO. Every control is a host
//! parameter; the other session state is the faceplate, the A/B slots and the captured
//! snapshot (saved with the session, see [`snap`]). See `docs/PLUGINS.md`.

pub mod engine;
pub mod snap;

use std::sync::Arc;

use apf::prelude::*;
use nodaw_kit::ctl::LookExt;
use nodaw_kit::license::Product;
use nodaw_kit::Core;
use snap::{SnapStore, WeaverExt};

use engine::{Clock, Engine, MorphMode, Settings, Shape, Source, B_BANDS};

#[cfg(feature = "ui")]
use apf_webview::{assets, Asset, WebView};

/// Trial and licensing for this plugin (its own trial, its own Gumroad product).
pub static PRODUCT: Product = Product {
    name: "NoDAW Chronos Weaver",
    folder: "Chronos Weaver",
    // TODO(owner): the Gumroad product ID of NoDAW Chronos Weaver.
    gumroad_product_id: "TODO_GUMROAD_PRODUCT_ID_CHRONOS_WEAVER",
    // TODO(owner): the Gumroad page for NoDAW Chronos Weaver.
    buy_url: "https://nodawlabs.gumroad.com/l/nodaw-chronos-weaver",
    hmac_key: b"nodaw-chronos-weaver/license/v1/e2a94c17-3d58-4b0f-8c61-9f7d2b05a4e3",
    user_agent: concat!("NoDAW-Chronos-Weaver/", env!("CARGO_PKG_VERSION")),
};

const MODES: &[&str] = &["Blend", "Cross"];
const SOURCES: &[&str] = &["Past", "Snapshot"];
const LFO_SYNC: &[&str] = &["Off", "1/4", "1/2", "1 Bar", "2 Bars", "4 Bars"];
const LFO_SHAPES: &[&str] = &["Sine", "Triangle", "Ramp"];

/// The parameters. Append only.
pub const PARAMS: &[ParamDef] = &[
    ParamDef::bypass("bypass"),
    ParamDef::float("morph", "Morph", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Morph"),
    ParamDef::choice("morph_mode", "Morph Mode", MODES, 0).module("Morph"),
    ParamDef::choice("source", "Source B", SOURCES, 0).module("Morph"),
    ParamDef::float("weave", "Weave Time", 20.0, 2000.0, 500.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Morph"),
    ParamDef::bool("capture", "Capture B", false).module("Morph"),
    ParamDef::bool("freeze", "Freeze", false).module("Freeze"),
    ParamDef::float("sustain", "Sustain", 0.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Freeze"),
    ParamDef::float("cloud", "Cloud", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Cloud"),
    ParamDef::float("grain", "Grain", 5.0, 2000.0, 150.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Cloud"),
    ParamDef::float("density", "Density", 2.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Cloud"),
    ParamDef::float("scatter", "Scatter", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Cloud"),
    ParamDef::float("lfo_depth", "LFO Depth", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("LFO"),
    ParamDef::choice("lfo_sync", "LFO Sync", LFO_SYNC, 3).module("LFO"),
    ParamDef::choice("lfo_shape", "LFO Shape", LFO_SHAPES, 0).module("LFO"),
    ParamDef::float("out_gain", "Output", -24.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Master"),
    ParamDef::float("mix", "Mix", 0.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Master"),
];

const BYPASS: ParamId = ParamId::of(PARAMS, "bypass");
const MORPH: ParamId = ParamId::of(PARAMS, "morph");
const MORPH_MODE: ParamId = ParamId::of(PARAMS, "morph_mode");
const SOURCE: ParamId = ParamId::of(PARAMS, "source");
const WEAVE: ParamId = ParamId::of(PARAMS, "weave");
const CAPTURE: ParamId = ParamId::of(PARAMS, "capture");
const FREEZE: ParamId = ParamId::of(PARAMS, "freeze");
const SUSTAIN: ParamId = ParamId::of(PARAMS, "sustain");
const CLOUD: ParamId = ParamId::of(PARAMS, "cloud");
const GRAIN: ParamId = ParamId::of(PARAMS, "grain");
const DENSITY: ParamId = ParamId::of(PARAMS, "density");
const SCATTER: ParamId = ParamId::of(PARAMS, "scatter");
const LFO_DEPTH: ParamId = ParamId::of(PARAMS, "lfo_depth");
const LFO_SYNC_ID: ParamId = ParamId::of(PARAMS, "lfo_sync");
const LFO_SHAPE: ParamId = ParamId::of(PARAMS, "lfo_shape");
const OUT_GAIN: ParamId = ParamId::of(PARAMS, "out_gain");
const MIX: ParamId = ParamId::of(PARAMS, "mix");

const REMOTE: &[RemotePage] = &[
    RemotePage {
        section: "Weaver",
        name: "Main",
        params: [
            "morph", "weave", "freeze", "sustain", "cloud", "grain", "density", "scatter",
        ],
    },
    RemotePage {
        section: "Weaver",
        name: "Mod & Mix",
        params: [
            "morph_mode",
            "source",
            "capture",
            "lfo_depth",
            "lfo_sync",
            "lfo_shape",
            "out_gain",
            "mix",
        ],
    },
];

#[cfg(feature = "ui")]
const UI: &[Asset] = assets![
    "index.html" => "../ui/index.html",
    "style.css" => "../ui/style.css",
    "app.js" => "../ui/app.js",
    "kit.js" => "../../../ui-kit/kit.js",
    "kit.css" => "../../../ui-kit/kit.css",
    "fonts/space-grotesk.woff2" => "../../../ui-kit/fonts/space-grotesk.woff2",
    "fonts/bebas-neue.woff2" => "../../../ui-kit/fonts/bebas-neue.woff2",
    "fonts/jetbrains-mono.woff2" => "../../../ui-kit/fonts/jetbrains-mono.woff2",
];

/// The faceplates (page only; saved with the session).
pub const LOOKS: &[&str] = &["nebula", "ice"];

/// Frame extras after the peaks: morph, freeze, has-snapshot, then B's spectrum.
pub const VALUES: usize = 3 + B_BANDS;

/// The plugin.
pub struct NoDawChronosWeaver {
    engine: Engine,
    core: Core,
    ext: Arc<WeaverExt>,
    tail_s: f32,
    // the store's restore count last taken in (None: take whatever it holds at the next block)
    seen_restore: Option<u64>,
}

fn lfo_beats(i: usize, bar: f64) -> f64 {
    match i {
        1 => 1.0,
        2 => 0.5 * bar,
        3 => bar,
        4 => 2.0 * bar,
        5 => 4.0 * bar,
        _ => 0.0,
    }
}

/// The engine's settings from the parameters (`bar` beats per bar). No allocation.
pub fn settings(p: &Params, bar: f64) -> Settings {
    Settings {
        bypass: p.get_bool(BYPASS),
        morph: p.get_f32(MORPH) * 0.01,
        mode: if p.get_index(MORPH_MODE) == 1 {
            MorphMode::Cross
        } else {
            MorphMode::Blend
        },
        source: if p.get_index(SOURCE) == 1 {
            Source::Snapshot
        } else {
            Source::Past
        },
        weave_s: p.get_f32(WEAVE) * 0.001,
        capture: p.get_bool(CAPTURE),
        freeze: p.get_bool(FREEZE),
        sustain: p.get_f32(SUSTAIN) * 0.01,
        cloud: p.get_f32(CLOUD) * 0.01,
        grain_s: p.get_f32(GRAIN) * 0.001,
        density: p.get_f32(DENSITY) * 0.01,
        scatter: p.get_f32(SCATTER) * 0.01,
        lfo_depth: p.get_f32(LFO_DEPTH) * 0.01,
        lfo_beats: lfo_beats(p.get_index(LFO_SYNC_ID), bar),
        lfo_shape: match p.get_index(LFO_SHAPE) {
            1 => Shape::Triangle,
            2 => Shape::Ramp,
            _ => Shape::Sine,
        },
        out_db: p.get_f32(OUT_GAIN),
        mix: p.get_f32(MIX) * 0.01,
    }
}

impl Plugin for NoDawChronosWeaver {
    const INFO: PluginInfo = PluginInfo {
        id: "com.nodaw.chronos-weaver",
        name: "NoDAW Chronos Weaver",
        vendor: "NoDAW",
        url: "https://nodaw.app",
        version: env!("CARGO_PKG_VERSION"),
        description: "Spectral morph (blend or cross-synthesis) against a snapshot or the past, spectral freeze, granular cloud and a synced morph LFO",
        features: &["audio-effect", "stereo", "glitch"],
    };
    const LAYOUT: AudioLayout = AudioLayout::EFFECT_STEREO;
    const PARAMS: &'static [ParamDef] = PARAMS;
    const REMOTE_CONTROLS: &'static [RemotePage] = REMOTE;
    const FACTORY_PRESETS: &'static [&'static str] = apf::presets![
        "../presets/Init.apfpreset",
        "../presets/Time Smear.apfpreset",
        "../presets/Echoes Of Itself.apfpreset",
        "../presets/Infinite Drone.apfpreset",
        "../presets/Frozen Choir.apfpreset",
        "../presets/Granular Haze.apfpreset",
        "../presets/Glass Cloud.apfpreset",
        "../presets/Formant Swap.apfpreset",
        "../presets/Breathing Morph.apfpreset",
        "../presets/Shattered Bar.apfpreset",
        "../presets/Ghost Pad.apfpreset",
        "../presets/Spectral Tremor.apfpreset",
    ];

    fn new() -> Self {
        Self {
            engine: Engine::new(48_000.0),
            core: Core::new(&PRODUCT),
            ext: Arc::new(WeaverExt {
                look: LookExt::new(LOOKS, 0, VALUES),
                snap: Arc::new(SnapStore::default()),
            }),
            tail_s: 0.5,
            seen_restore: None,
        }
    }

    fn controller(&mut self, host: HostHandle) -> Option<Arc<dyn Controller>> {
        Some(self.core.controller(PARAMS, self.ext.clone(), host))
    }

    fn activate(&mut self, config: &ProcessConfig, params: &Params) {
        if (self.engine.sample_rate() - config.sample_rate).abs() > 0.5 {
            self.engine = Engine::new(config.sample_rate);
            // the new engine has no snapshot: take the one the session holds
            self.seen_restore = None;
        }
        self.engine.reset();
        let s = settings(params, 4.0);
        self.tail_s = Engine::tail_seconds(&s);
        self.core
            .activate(config.sample_rate, self.engine.latency());
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn latency(&self) -> u32 {
        self.engine.latency() as u32
    }

    fn tail(&self) -> u32 {
        (self.tail_s as f64 * self.engine.sample_rate()) as u32 + self.engine.latency() as u32
    }

    fn process(&mut self, audio: &mut AudioBuffer, ctx: &ProcessContext) -> ProcessStatus {
        let Some((l, r)) = audio.stereo() else {
            return ProcessStatus::Error;
        };
        let t = &ctx.transport;
        let bar = t
            .time_signature
            .filter(|(n, d)| *n > 0 && *d > 0)
            .map(|(n, d)| n as f64 * 4.0 / d as f64)
            .unwrap_or(4.0);
        let s = settings(ctx.params, bar);
        self.tail_s = Engine::tail_seconds(&s);
        let clock = Clock {
            song_pos_beats: t.song_pos_beats,
            playing: t.playing,
            tempo: t.tempo,
        };
        // A restored session's snapshot (or its absence), or the held one for a new engine.
        let store = &*self.ext.snap;
        let restored = store.restored();
        if self.seen_restore != Some(restored) {
            let fresh_engine = self.seen_restore.is_none();
            self.seen_restore = Some(restored);
            match store.held() {
                Some((sr, bins)) => self
                    .engine
                    .restore_snapshot(sr, bins, s.capture, |c, k| store.get(c, k)),
                None if !fresh_engine => self.engine.forget_snapshot(s.capture),
                None => {}
            }
        }
        self.core.begin(l, r);
        let info = self.engine.process(l, r, &s, &clock);
        if info.captured {
            if let Some((a, b)) = self.engine.snapshot() {
                self.ext.snap.publish(self.engine.sample_rate(), a, b);
            }
        }
        self.core.end(l, r, s.bypass, t.playing, 1.0);
        self.ext.look.values.set(0, info.morph);
        self.ext.look.values.set(1, info.freeze);
        self.ext
            .look
            .values
            .set(2, if info.has_snapshot { 1.0 } else { 0.0 });
        for (j, v) in info.b_db.iter().enumerate() {
            self.ext.look.values.set(3 + j, *v);
        }
        ProcessStatus::Continue
    }

    #[cfg(feature = "ui")]
    fn view() -> Option<Box<dyn View>> {
        Some(Box::new(
            WebView::new(980, 620, UI)
                .resizable(true)
                .size_limits((820, 520), (2600, 1700))
                .fit_screen((48, 96))
                .background(7, 6, 14),
        ))
    }
}

apf::export_plugin!(NoDawChronosWeaver);
