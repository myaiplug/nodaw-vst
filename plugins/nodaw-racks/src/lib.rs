//! NoDAW Racks: four analog-style processors in one rack (CLAP and VST3): a 2x-oversampled
//! Saturator (Tape, Tube, Diode), a 7-band EQ, a Spatial processor (width, M/S, Haas, pan,
//! mono bass) and the Reactor true-peak limiter, in any order. Every control is a host
//! parameter, including the chain order; the only session state besides parameters is the
//! hardware skin and the A/B slots. See `docs/PLUGINS.md`.

pub mod engine;

use std::sync::Arc;

use apf::prelude::*;
use nodaw_dsp::eq::Shape;
use nodaw_dsp::sat::Curve as SatCurve;
use nodaw_kit::ctl::LookExt;
use nodaw_kit::license::Product;
use nodaw_kit::Core;

use engine::{Band, Engine, Settings, BANDS, ORDERS, ORDER_NAMES};

#[cfg(feature = "ui")]
use apf_webview::{assets, Asset, WebView};

/// Trial and licensing for this plugin (its own trial, its own Gumroad product).
pub static PRODUCT: Product = Product {
    name: "NoDAW Racks",
    folder: "Racks",
    // TODO(owner): the Gumroad product ID of NoDAW Racks.
    gumroad_product_id: "TODO_GUMROAD_PRODUCT_ID_RACKS",
    // TODO(owner): the Gumroad page for NoDAW Racks.
    buy_url: "https://nodawlabs.gumroad.com/l/nodaw-racks",
    hmac_key: b"nodaw-racks/license/v1/4f0c2d9e-6a1b-4c37-9e85-1b7d3a6f20c4",
    user_agent: concat!("NoDAW-Racks/", env!("CARGO_PKG_VERSION")),
};

const SAT_CURVES: &[&str] = &["Tape", "Tube", "Diode"];
const LOW_TYPES: &[&str] = &["Low Shelf", "Low Cut", "Bell"];
const HIGH_TYPES: &[&str] = &["High Shelf", "High Cut", "Bell"];

/// The parameters. Append only.
pub const PARAMS: &[ParamDef] = &[
    ParamDef::bypass("bypass"),
    ParamDef::float("in_gain", "Input", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Master"),
    ParamDef::float("out_gain", "Output", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Master"),
    ParamDef::float("mix", "Mix", 0.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Master"),
    ParamDef::choice("order", "Chain Order", ORDER_NAMES, 0).module("Master"),
    ParamDef::bool("sat_on", "Saturator On", true).module("Saturator"),
    ParamDef::choice("sat_curve", "Curve", SAT_CURVES, 0).module("Saturator"),
    ParamDef::float("sat_drive", "Drive", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Saturator"),
    ParamDef::float("sat_tone", "Tone", -6.0, 6.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Saturator"),
    ParamDef::float("sat_mix", "Sat Mix", 0.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Saturator"),
    ParamDef::bool("sat_auto", "Auto Gain", true).module("Saturator"),
    ParamDef::bool("eq_on", "EQ On", true).module("EQ"),
    ParamDef::bool("eq1_on", "Band 1 On", true).module("EQ"),
    ParamDef::float("eq1_freq", "Band 1 Freq", 20.0, 20_000.0, 80.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq1_gain", "Band 1 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq1_q", "Band 1 Q", 0.1, 18.0, 0.707)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("eq2_on", "Band 2 On", true).module("EQ"),
    ParamDef::float("eq2_freq", "Band 2 Freq", 20.0, 20_000.0, 200.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq2_gain", "Band 2 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq2_q", "Band 2 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("eq3_on", "Band 3 On", true).module("EQ"),
    ParamDef::float("eq3_freq", "Band 3 Freq", 20.0, 20_000.0, 500.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq3_gain", "Band 3 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq3_q", "Band 3 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("eq4_on", "Band 4 On", true).module("EQ"),
    ParamDef::float("eq4_freq", "Band 4 Freq", 20.0, 20_000.0, 1200.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq4_gain", "Band 4 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq4_q", "Band 4 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("eq5_on", "Band 5 On", true).module("EQ"),
    ParamDef::float("eq5_freq", "Band 5 Freq", 20.0, 20_000.0, 3000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq5_gain", "Band 5 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq5_q", "Band 5 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("eq6_on", "Band 6 On", true).module("EQ"),
    ParamDef::float("eq6_freq", "Band 6 Freq", 20.0, 20_000.0, 7000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq6_gain", "Band 6 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq6_q", "Band 6 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("eq7_on", "Band 7 On", true).module("EQ"),
    ParamDef::float("eq7_freq", "Band 7 Freq", 20.0, 20_000.0, 12000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("eq7_gain", "Band 7 Gain", -18.0, 18.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("eq7_q", "Band 7 Q", 0.1, 18.0, 0.707)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::choice("eq1_type", "Band 1 Type", LOW_TYPES, 0).module("EQ"),
    ParamDef::choice("eq7_type", "Band 7 Type", HIGH_TYPES, 0).module("EQ"),
    ParamDef::bool("sp_on", "Spatial On", true).module("Spatial"),
    ParamDef::float("sp_width", "Width", 0.0, 200.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Spatial"),
    ParamDef::float("sp_haas", "Haas", -30.0, 30.0, 0.0)
        .unit("ms")
        .decimals(1)
        .module("Spatial"),
    ParamDef::float("sp_mid", "Mid", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Spatial"),
    ParamDef::float("sp_side", "Side", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Spatial"),
    ParamDef::float("sp_pan", "Pan", -100.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Spatial"),
    ParamDef::float("sp_mono", "Mono Bass", 20.0, 500.0, 20.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Spatial"),
    ParamDef::bool("lim_on", "Limiter On", true).module("Limiter"),
    ParamDef::float("lim_gain", "Push", 0.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Limiter"),
    ParamDef::float("lim_ceiling", "Ceiling", -12.0, 0.0, -1.0)
        .unit("dB")
        .decimals(1)
        .module("Limiter"),
    ParamDef::float("lim_release", "Release", 5.0, 1000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Limiter"),
    ParamDef::bool("lim_adaptive", "Adaptive", true).module("Limiter"),
];

const BYPASS: ParamId = ParamId::of(PARAMS, "bypass");
const IN_GAIN: ParamId = ParamId::of(PARAMS, "in_gain");
const OUT_GAIN: ParamId = ParamId::of(PARAMS, "out_gain");
const MIX: ParamId = ParamId::of(PARAMS, "mix");
const ORDER: ParamId = ParamId::of(PARAMS, "order");
const SAT_ON: ParamId = ParamId::of(PARAMS, "sat_on");
const SAT_CURVE: ParamId = ParamId::of(PARAMS, "sat_curve");
const SAT_DRIVE: ParamId = ParamId::of(PARAMS, "sat_drive");
const SAT_TONE: ParamId = ParamId::of(PARAMS, "sat_tone");
const SAT_MIX: ParamId = ParamId::of(PARAMS, "sat_mix");
const SAT_AUTO: ParamId = ParamId::of(PARAMS, "sat_auto");
const EQ_ON: ParamId = ParamId::of(PARAMS, "eq_on");
const BAND_ON: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "eq1_on"),
    ParamId::of(PARAMS, "eq2_on"),
    ParamId::of(PARAMS, "eq3_on"),
    ParamId::of(PARAMS, "eq4_on"),
    ParamId::of(PARAMS, "eq5_on"),
    ParamId::of(PARAMS, "eq6_on"),
    ParamId::of(PARAMS, "eq7_on"),
];
const BAND_FREQ: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "eq1_freq"),
    ParamId::of(PARAMS, "eq2_freq"),
    ParamId::of(PARAMS, "eq3_freq"),
    ParamId::of(PARAMS, "eq4_freq"),
    ParamId::of(PARAMS, "eq5_freq"),
    ParamId::of(PARAMS, "eq6_freq"),
    ParamId::of(PARAMS, "eq7_freq"),
];
const BAND_GAIN: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "eq1_gain"),
    ParamId::of(PARAMS, "eq2_gain"),
    ParamId::of(PARAMS, "eq3_gain"),
    ParamId::of(PARAMS, "eq4_gain"),
    ParamId::of(PARAMS, "eq5_gain"),
    ParamId::of(PARAMS, "eq6_gain"),
    ParamId::of(PARAMS, "eq7_gain"),
];
const BAND_Q: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "eq1_q"),
    ParamId::of(PARAMS, "eq2_q"),
    ParamId::of(PARAMS, "eq3_q"),
    ParamId::of(PARAMS, "eq4_q"),
    ParamId::of(PARAMS, "eq5_q"),
    ParamId::of(PARAMS, "eq6_q"),
    ParamId::of(PARAMS, "eq7_q"),
];
const EQ1_TYPE: ParamId = ParamId::of(PARAMS, "eq1_type");
const EQ7_TYPE: ParamId = ParamId::of(PARAMS, "eq7_type");
const SP_ON: ParamId = ParamId::of(PARAMS, "sp_on");
const SP_WIDTH: ParamId = ParamId::of(PARAMS, "sp_width");
const SP_HAAS: ParamId = ParamId::of(PARAMS, "sp_haas");
const SP_MID: ParamId = ParamId::of(PARAMS, "sp_mid");
const SP_SIDE: ParamId = ParamId::of(PARAMS, "sp_side");
const SP_PAN: ParamId = ParamId::of(PARAMS, "sp_pan");
const SP_MONO: ParamId = ParamId::of(PARAMS, "sp_mono");
const LIM_ON: ParamId = ParamId::of(PARAMS, "lim_on");
const LIM_GAIN: ParamId = ParamId::of(PARAMS, "lim_gain");
const LIM_CEILING: ParamId = ParamId::of(PARAMS, "lim_ceiling");
const LIM_RELEASE: ParamId = ParamId::of(PARAMS, "lim_release");
const LIM_ADAPTIVE: ParamId = ParamId::of(PARAMS, "lim_adaptive");

const REMOTE: &[RemotePage] = &[
    RemotePage {
        section: "Racks",
        name: "Saturator",
        params: [
            "sat_drive",
            "sat_tone",
            "sat_mix",
            "sat_curve",
            "sat_on",
            "in_gain",
            "mix",
            "out_gain",
        ],
    },
    RemotePage {
        section: "Racks",
        name: "EQ Low",
        params: [
            "eq1_gain", "eq1_freq", "eq2_gain", "eq2_freq", "eq3_gain", "eq3_freq", "eq4_gain",
            "eq4_freq",
        ],
    },
    RemotePage {
        section: "Racks",
        name: "EQ High",
        params: [
            "eq4_gain", "eq4_freq", "eq5_gain", "eq5_freq", "eq6_gain", "eq6_freq", "eq7_gain",
            "eq7_freq",
        ],
    },
    RemotePage {
        section: "Racks",
        name: "Spatial",
        params: [
            "sp_width", "sp_haas", "sp_mid", "sp_side", "sp_pan", "sp_mono", "sp_on", "mix",
        ],
    },
    RemotePage {
        section: "Racks",
        name: "Limiter",
        params: [
            "lim_gain",
            "lim_ceiling",
            "lim_release",
            "lim_adaptive",
            "lim_on",
            "in_gain",
            "mix",
            "out_gain",
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

/// The hardware skins (page only; saved with the session).
pub const SKINS: &[&str] = &["cobalt", "obsidian", "silver", "plastic"];

/// The plugin.
pub struct NoDawRacks {
    engine: Engine,
    core: Core,
    /// Frame extras: the four module peaks (by module index), then the correlation.
    ext: Arc<LookExt>,
}

/// The engine's settings from the parameters. No allocation.
pub fn settings(p: &Params) -> Settings {
    let mut bands = [Band {
        on: true,
        shape: Shape::Peak,
        freq: 1000.0,
        gain_db: 0.0,
        q: 1.0,
    }; BANDS];
    for (i, b) in bands.iter_mut().enumerate() {
        b.on = p.get_bool(BAND_ON[i]);
        b.freq = p.get_f32(BAND_FREQ[i]);
        b.gain_db = p.get_f32(BAND_GAIN[i]);
        b.q = p.get_f32(BAND_Q[i]);
        b.shape = match i {
            0 => [Shape::LowShelf, Shape::HighPass, Shape::Peak][p.get_index(EQ1_TYPE).min(2)],
            6 => [Shape::HighShelf, Shape::LowPass, Shape::Peak][p.get_index(EQ7_TYPE).min(2)],
            _ => Shape::Peak,
        };
    }
    Settings {
        bypass: p.get_bool(BYPASS),
        in_db: p.get_f32(IN_GAIN),
        out_db: p.get_f32(OUT_GAIN),
        mix: p.get_f32(MIX) * 0.01,
        order: ORDERS[p.get_index(ORDER).min(ORDERS.len() - 1)],
        sat_on: p.get_bool(SAT_ON),
        sat_curve: SatCurve::from_index(p.get_index(SAT_CURVE)),
        sat_drive_db: p.get_f32(SAT_DRIVE),
        sat_tone_db: p.get_f32(SAT_TONE),
        sat_mix: p.get_f32(SAT_MIX) * 0.01,
        sat_auto: p.get_bool(SAT_AUTO),
        eq_on: p.get_bool(EQ_ON),
        bands,
        sp_on: p.get_bool(SP_ON),
        width: p.get_f32(SP_WIDTH) * 0.01,
        haas_ms: p.get_f32(SP_HAAS),
        mid_db: p.get_f32(SP_MID),
        side_db: p.get_f32(SP_SIDE),
        pan: p.get_f32(SP_PAN) * 0.01,
        mono_hz: p.get_f32(SP_MONO),
        lim_on: p.get_bool(LIM_ON),
        lim_gain_db: p.get_f32(LIM_GAIN),
        ceiling_db: p.get_f32(LIM_CEILING),
        release_ms: p.get_f32(LIM_RELEASE),
        adaptive: p.get_bool(LIM_ADAPTIVE),
    }
}

impl Plugin for NoDawRacks {
    const INFO: PluginInfo = PluginInfo {
        id: "com.nodaw.racks",
        name: "NoDAW Racks",
        vendor: "NoDAW",
        url: "https://nodaw.app",
        version: env!("CARGO_PKG_VERSION"),
        description: "Saturator, 7-band EQ, spatial processor and true-peak limiter in one re-orderable analog rack",
        features: &["audio-effect", "stereo", "equalizer", "distortion", "limiter"],
    };
    const LAYOUT: AudioLayout = AudioLayout::EFFECT_STEREO;
    const PARAMS: &'static [ParamDef] = PARAMS;
    const REMOTE_CONTROLS: &'static [RemotePage] = REMOTE;
    const FACTORY_PRESETS: &'static [&'static str] = apf::presets![
        "../presets/Init.apfpreset",
        "../presets/Analog Glue.apfpreset",
        "../presets/Tape Warmth.apfpreset",
        "../presets/Tube Vocal.apfpreset",
        "../presets/Diode Crunch Drums.apfpreset",
        "../presets/Wide Master.apfpreset",
        "../presets/Mono Bass Club.apfpreset",
        "../presets/Air And Weight.apfpreset",
        "../presets/Loud Reactor.apfpreset",
        "../presets/Haas Guitar.apfpreset",
        "../presets/Lo-Fi Radio.apfpreset",
        "../presets/Gentle Polish.apfpreset",
    ];

    fn new() -> Self {
        Self {
            engine: Engine::new(48_000.0),
            core: Core::new(&PRODUCT),
            ext: Arc::new(LookExt::new(SKINS, 4, 1)),
        }
    }

    fn controller(&mut self, host: HostHandle) -> Option<Arc<dyn Controller>> {
        Some(self.core.controller(PARAMS, self.ext.clone(), host))
    }

    fn activate(&mut self, config: &ProcessConfig, params: &Params) {
        if (self.engine.sample_rate() - config.sample_rate).abs() > 0.5 {
            self.engine = Engine::new(config.sample_rate);
        }
        self.engine.reset_to(&settings(params));
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
        // The Haas delay and the limiter's release: well under half a second.
        (0.5 * self.engine.sample_rate()) as u32
    }

    fn process(&mut self, audio: &mut AudioBuffer, ctx: &ProcessContext) -> ProcessStatus {
        let Some((l, r)) = audio.stereo() else {
            return ProcessStatus::Error;
        };
        let s = settings(ctx.params);
        self.core.begin(l, r);
        let info = self.engine.process(l, r, &s);
        self.core
            .end(l, r, s.bypass, ctx.transport.playing, info.min_gain);
        for (i, v) in info.module_peak.iter().enumerate() {
            self.ext.peaks.max(i, *v);
        }
        self.ext.values.set(0, info.correlation);
        ProcessStatus::Continue
    }

    #[cfg(feature = "ui")]
    fn view() -> Option<Box<dyn View>> {
        Some(Box::new(
            WebView::new(960, 600, UI)
                .resizable(true)
                .size_limits((760, 480), (2600, 1700))
                .fit_screen((48, 96))
                .background(8, 9, 14),
        ))
    }
}

apf::export_plugin!(NoDawRacks);
