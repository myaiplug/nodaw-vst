//! NoDAW Harmonic Space (the Spectral Space Designer, CLAP and VST3): an amp stage, a 7-band
//! EQ with band solo and mute, a sub shelf, high- and low-pass filters, and three parallel space
//! effects (feedback delay, FDN reverb, stereo flanger), with the prototype's 20 Space Recipes as
//! factory presets. Every control is a host parameter; the light or dark faceplate and the A/B
//! slots are the only other session state. See `docs/PLUGINS.md`.

pub mod engine;

use std::sync::Arc;

use apf::prelude::*;
use nodaw_kit::ctl::LookExt;
use nodaw_kit::license::Product;
use nodaw_kit::Core;

use engine::{Band, Engine, Settings, BANDS};

#[cfg(feature = "ui")]
use apf_webview::{assets, Asset, WebView};

/// Trial and licensing for this plugin (its own trial, its own Gumroad product).
pub static PRODUCT: Product = Product {
    name: "NoDAW Harmonic Space",
    folder: "Harmonic Space",
    // TODO(owner): the Gumroad product ID of NoDAW Harmonic Space.
    gumroad_product_id: "TODO_GUMROAD_PRODUCT_ID_HARMONIC_SPACE",
    // TODO(owner): the Gumroad page for NoDAW Harmonic Space.
    buy_url: "https://nodawlabs.gumroad.com/l/nodaw-harmonic-space",
    hmac_key: b"nodaw-harmonic-space/license/v1/8b2e61d4-0f3a-4c9e-a7d5-3e91c0b6f218",
    user_agent: concat!("NoDAW-Harmonic-Space/", env!("CARGO_PKG_VERSION")),
};

/// The parameters. Append only.
pub const PARAMS: &[ParamDef] = &[
    ParamDef::bypass("bypass"),
    ParamDef::float("amp", "Amp", -20.0, 10.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Amp"),
    ParamDef::float("out_gain", "Output", -24.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Master"),
    ParamDef::float("mix", "Mix", 0.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Master"),
    ParamDef::float("width", "Width", 0.0, 200.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Master"),
    ParamDef::float("hpf", "HPF", 20.0, 500.0, 20.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Filters"),
    ParamDef::float("lpf", "LPF", 2000.0, 22_000.0, 22_000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Filters"),
    ParamDef::float("b1_freq", "Band 1 Freq", 20.0, 20_000.0, 60.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b1_gain", "Band 1 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b1_q", "Band 1 Q", 0.3, 10.0, 0.707)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b1_solo", "Band 1 Solo", false).module("EQ"),
    ParamDef::bool("b1_mute", "Band 1 Mute", false).module("EQ"),
    ParamDef::float("b2_freq", "Band 2 Freq", 20.0, 20_000.0, 200.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b2_gain", "Band 2 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b2_q", "Band 2 Q", 0.3, 10.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b2_solo", "Band 2 Solo", false).module("EQ"),
    ParamDef::bool("b2_mute", "Band 2 Mute", false).module("EQ"),
    ParamDef::float("b3_freq", "Band 3 Freq", 20.0, 20_000.0, 800.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b3_gain", "Band 3 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b3_q", "Band 3 Q", 0.3, 10.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b3_solo", "Band 3 Solo", false).module("EQ"),
    ParamDef::bool("b3_mute", "Band 3 Mute", false).module("EQ"),
    ParamDef::float("b4_freq", "Band 4 Freq", 20.0, 20_000.0, 2000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b4_gain", "Band 4 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b4_q", "Band 4 Q", 0.3, 10.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b4_solo", "Band 4 Solo", false).module("EQ"),
    ParamDef::bool("b4_mute", "Band 4 Mute", false).module("EQ"),
    ParamDef::float("b5_freq", "Band 5 Freq", 20.0, 20_000.0, 4000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b5_gain", "Band 5 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b5_q", "Band 5 Q", 0.3, 10.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b5_solo", "Band 5 Solo", false).module("EQ"),
    ParamDef::bool("b5_mute", "Band 5 Mute", false).module("EQ"),
    ParamDef::float("b6_freq", "Band 6 Freq", 20.0, 20_000.0, 8000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b6_gain", "Band 6 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b6_q", "Band 6 Q", 0.3, 10.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b6_solo", "Band 6 Solo", false).module("EQ"),
    ParamDef::bool("b6_mute", "Band 6 Mute", false).module("EQ"),
    ParamDef::float("b7_freq", "Band 7 Freq", 20.0, 20_000.0, 16000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("EQ"),
    ParamDef::float("b7_gain", "Band 7 Gain", -12.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("EQ"),
    ParamDef::float("b7_q", "Band 7 Q", 0.3, 10.0, 0.707)
        .curve(Curve::Log)
        .decimals(2)
        .module("EQ"),
    ParamDef::bool("b7_solo", "Band 7 Solo", false).module("EQ"),
    ParamDef::bool("b7_mute", "Band 7 Mute", false).module("EQ"),
    ParamDef::float("sub", "Sub", 0.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Sub"),
    ParamDef::float("sub_freq", "Sub Freq", 30.0, 160.0, 80.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Sub"),
    ParamDef::float("dly_mix", "Delay", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Delay"),
    ParamDef::float("dly_time", "Delay Time", 10.0, 2000.0, 300.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Delay"),
    ParamDef::float("dly_fb", "Feedback", 0.0, 90.0, 30.0)
        .unit("%")
        .decimals(0)
        .module("Delay"),
    ParamDef::float("dly_tone", "Delay Tone", 1000.0, 20_000.0, 8000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Delay"),
    ParamDef::bool("dly_pp", "Ping-Pong", false).module("Delay"),
    ParamDef::float("rev_mix", "Reverb", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Reverb"),
    ParamDef::float("rev_decay", "Decay", 0.2, 12.0, 2.0)
        .unit("s")
        .curve(Curve::Log)
        .decimals(2)
        .module("Reverb"),
    ParamDef::float("rev_size", "Size", 50.0, 200.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Reverb"),
    ParamDef::float("rev_pre", "Pre-Delay", 0.0, 200.0, 10.0)
        .unit("ms")
        .decimals(0)
        .module("Reverb"),
    ParamDef::float("rev_damp", "Damping", 1000.0, 20_000.0, 7000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Reverb"),
    ParamDef::float("fl_mix", "Flanger", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Flanger"),
    ParamDef::float("fl_rate", "Flanger Rate", 0.05, 5.0, 0.5)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(2)
        .module("Flanger"),
    ParamDef::float("fl_depth", "Flanger Depth", 0.0, 100.0, 25.0)
        .unit("%")
        .decimals(0)
        .module("Flanger"),
    ParamDef::float("fl_fb", "Flanger Feedback", -90.0, 90.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Flanger"),
];

const BYPASS: ParamId = ParamId::of(PARAMS, "bypass");
const AMP: ParamId = ParamId::of(PARAMS, "amp");
const OUT_GAIN: ParamId = ParamId::of(PARAMS, "out_gain");
const MIX: ParamId = ParamId::of(PARAMS, "mix");
const WIDTH: ParamId = ParamId::of(PARAMS, "width");
const HPF: ParamId = ParamId::of(PARAMS, "hpf");
const LPF: ParamId = ParamId::of(PARAMS, "lpf");
const B_FREQ: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_freq"),
    ParamId::of(PARAMS, "b2_freq"),
    ParamId::of(PARAMS, "b3_freq"),
    ParamId::of(PARAMS, "b4_freq"),
    ParamId::of(PARAMS, "b5_freq"),
    ParamId::of(PARAMS, "b6_freq"),
    ParamId::of(PARAMS, "b7_freq"),
];
const B_GAIN: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_gain"),
    ParamId::of(PARAMS, "b2_gain"),
    ParamId::of(PARAMS, "b3_gain"),
    ParamId::of(PARAMS, "b4_gain"),
    ParamId::of(PARAMS, "b5_gain"),
    ParamId::of(PARAMS, "b6_gain"),
    ParamId::of(PARAMS, "b7_gain"),
];
const B_Q: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_q"),
    ParamId::of(PARAMS, "b2_q"),
    ParamId::of(PARAMS, "b3_q"),
    ParamId::of(PARAMS, "b4_q"),
    ParamId::of(PARAMS, "b5_q"),
    ParamId::of(PARAMS, "b6_q"),
    ParamId::of(PARAMS, "b7_q"),
];
const B_SOLO: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_solo"),
    ParamId::of(PARAMS, "b2_solo"),
    ParamId::of(PARAMS, "b3_solo"),
    ParamId::of(PARAMS, "b4_solo"),
    ParamId::of(PARAMS, "b5_solo"),
    ParamId::of(PARAMS, "b6_solo"),
    ParamId::of(PARAMS, "b7_solo"),
];
const B_MUTE: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_mute"),
    ParamId::of(PARAMS, "b2_mute"),
    ParamId::of(PARAMS, "b3_mute"),
    ParamId::of(PARAMS, "b4_mute"),
    ParamId::of(PARAMS, "b5_mute"),
    ParamId::of(PARAMS, "b6_mute"),
    ParamId::of(PARAMS, "b7_mute"),
];
const SUB: ParamId = ParamId::of(PARAMS, "sub");
const SUB_FREQ: ParamId = ParamId::of(PARAMS, "sub_freq");
const DLY_MIX: ParamId = ParamId::of(PARAMS, "dly_mix");
const DLY_TIME: ParamId = ParamId::of(PARAMS, "dly_time");
const DLY_FB: ParamId = ParamId::of(PARAMS, "dly_fb");
const DLY_TONE: ParamId = ParamId::of(PARAMS, "dly_tone");
const DLY_PP: ParamId = ParamId::of(PARAMS, "dly_pp");
const REV_MIX: ParamId = ParamId::of(PARAMS, "rev_mix");
const REV_DECAY: ParamId = ParamId::of(PARAMS, "rev_decay");
const REV_SIZE: ParamId = ParamId::of(PARAMS, "rev_size");
const REV_PRE: ParamId = ParamId::of(PARAMS, "rev_pre");
const REV_DAMP: ParamId = ParamId::of(PARAMS, "rev_damp");
const FL_MIX: ParamId = ParamId::of(PARAMS, "fl_mix");
const FL_RATE: ParamId = ParamId::of(PARAMS, "fl_rate");
const FL_DEPTH: ParamId = ParamId::of(PARAMS, "fl_depth");
const FL_FB: ParamId = ParamId::of(PARAMS, "fl_fb");

const REMOTE: &[RemotePage] = &[
    RemotePage {
        section: "Space",
        name: "Main",
        params: [
            "amp", "hpf", "lpf", "dly_mix", "rev_mix", "sub", "fl_mix", "out_gain",
        ],
    },
    RemotePage {
        section: "Space",
        name: "EQ Gain",
        params: [
            "b1_gain", "b2_gain", "b3_gain", "b4_gain", "b5_gain", "b6_gain", "b7_gain", "mix",
        ],
    },
    RemotePage {
        section: "Space",
        name: "EQ Q",
        params: [
            "b1_q", "b2_q", "b3_q", "b4_q", "b5_q", "b6_q", "b7_q", "width",
        ],
    },
    RemotePage {
        section: "Space",
        name: "Delay & Reverb",
        params: [
            "dly_mix",
            "dly_time",
            "dly_fb",
            "dly_tone",
            "rev_mix",
            "rev_decay",
            "rev_size",
            "rev_damp",
        ],
    },
    RemotePage {
        section: "Space",
        name: "Flanger & Sub",
        params: [
            "fl_mix", "fl_rate", "fl_depth", "fl_fb", "sub", "sub_freq", "width", "mix",
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
pub const LOOKS: &[&str] = &["dark", "light"];

/// The plugin.
pub struct NoDawHarmonicSpace {
    engine: Engine,
    core: Core,
    /// Frame extras: delay, reverb and flanger return peaks, then the correlation.
    ext: Arc<LookExt>,
    tail_s: f32,
}

/// The engine's settings from the parameters. No allocation.
pub fn settings(p: &Params) -> Settings {
    let bands: [Band; BANDS] = std::array::from_fn(|i| Band {
        freq: p.get_f32(B_FREQ[i]),
        gain_db: p.get_f32(B_GAIN[i]),
        q: p.get_f32(B_Q[i]),
        solo: p.get_bool(B_SOLO[i]),
        mute: p.get_bool(B_MUTE[i]),
    });
    Settings {
        bypass: p.get_bool(BYPASS),
        amp_db: p.get_f32(AMP),
        hpf_hz: p.get_f32(HPF),
        lpf_hz: p.get_f32(LPF),
        bands,
        sub_db: p.get_f32(SUB),
        sub_hz: p.get_f32(SUB_FREQ),
        delay_mix: p.get_f32(DLY_MIX) * 0.01,
        delay_s: p.get_f32(DLY_TIME) * 0.001,
        delay_fb: p.get_f32(DLY_FB) * 0.01,
        delay_pp: p.get_bool(DLY_PP),
        delay_tone_hz: p.get_f32(DLY_TONE),
        reverb_mix: p.get_f32(REV_MIX) * 0.01,
        decay_s: p.get_f32(REV_DECAY),
        size: p.get_f32(REV_SIZE) * 0.01,
        predelay_ms: p.get_f32(REV_PRE),
        damp_hz: p.get_f32(REV_DAMP),
        flanger_mix: p.get_f32(FL_MIX) * 0.01,
        fl_rate: p.get_f32(FL_RATE),
        fl_depth: p.get_f32(FL_DEPTH) * 0.01,
        fl_fb: p.get_f32(FL_FB) * 0.01,
        width: p.get_f32(WIDTH) * 0.01,
        out_db: p.get_f32(OUT_GAIN),
        mix: p.get_f32(MIX) * 0.01,
    }
}

impl Plugin for NoDawHarmonicSpace {
    const INFO: PluginInfo = PluginInfo {
        id: "com.nodaw.harmonic-space",
        name: "NoDAW Harmonic Space",
        vendor: "NoDAW",
        url: "https://nodaw.app",
        version: env!("CARGO_PKG_VERSION"),
        description: "Spectral space designer: amp, 7-band EQ with solo and mute, sub, filters, delay, reverb and flanger",
        features: &["audio-effect", "stereo", "equalizer", "reverb", "delay"],
    };
    const LAYOUT: AudioLayout = AudioLayout::EFFECT_STEREO;
    const PARAMS: &'static [ParamDef] = PARAMS;
    const REMOTE_CONTROLS: &'static [RemotePage] = REMOTE;
    const FACTORY_PRESETS: &'static [&'static str] = apf::presets![
        "../presets/Init.apfpreset",
        "../presets/Air Lift.apfpreset",
        "../presets/De-Box.apfpreset",
        "../presets/Phone Hook.apfpreset",
        "../presets/Ad-lib Throw.apfpreset",
        "../presets/Whisper Double.apfpreset",
        "../presets/808 Weight.apfpreset",
        "../presets/Sub Glue.apfpreset",
        "../presets/Kick Room.apfpreset",
        "../presets/Tight Bass.apfpreset",
        "../presets/Small Booth.apfpreset",
        "../presets/Dark Hall.apfpreset",
        "../presets/Slapback Rap.apfpreset",
        "../presets/Ping-Pong Hook.apfpreset",
        "../presets/Lo-fi Room.apfpreset",
        "../presets/Tape Wobble.apfpreset",
        "../presets/Syrup Haze.apfpreset",
        "../presets/Radio Break.apfpreset",
        "../presets/Harsh Sample Tamer.apfpreset",
        "../presets/Mud Cut.apfpreset",
        "../presets/Wide-but-Mono-Safe.apfpreset",
    ];

    fn new() -> Self {
        Self {
            engine: Engine::new(48_000.0),
            core: Core::new(&PRODUCT),
            ext: Arc::new(LookExt::new(LOOKS, 3, 1)),
            tail_s: 2.0,
        }
    }

    fn controller(&mut self, host: HostHandle) -> Option<Arc<dyn Controller>> {
        Some(self.core.controller(PARAMS, self.ext.clone(), host))
    }

    fn activate(&mut self, config: &ProcessConfig, params: &Params) {
        if (self.engine.sample_rate() - config.sample_rate).abs() > 0.5 {
            self.engine = Engine::new(config.sample_rate);
        }
        let s = settings(params);
        self.engine.reset_to(&s);
        self.tail_s = Engine::tail_seconds(&s);
        self.core
            .activate(config.sample_rate, self.engine.latency());
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn latency(&self) -> u32 {
        0
    }

    fn tail(&self) -> u32 {
        (self.tail_s as f64 * self.engine.sample_rate()) as u32
    }

    fn process(&mut self, audio: &mut AudioBuffer, ctx: &ProcessContext) -> ProcessStatus {
        let Some((l, r)) = audio.stereo() else {
            return ProcessStatus::Error;
        };
        let s = settings(ctx.params);
        self.tail_s = Engine::tail_seconds(&s);
        self.core.begin(l, r);
        let info = self.engine.process(l, r, &s);
        self.core.end(l, r, s.bypass, ctx.transport.playing, 1.0);
        for (i, v) in info.fx_peak.iter().enumerate() {
            self.ext.peaks.max(i, *v);
        }
        self.ext.values.set(0, info.correlation);
        ProcessStatus::Continue
    }

    #[cfg(feature = "ui")]
    fn view() -> Option<Box<dyn View>> {
        Some(Box::new(
            WebView::new(960, 620, UI)
                .resizable(true)
                .size_limits((800, 520), (2600, 1700))
                .fit_screen((48, 96))
                .background(10, 11, 14),
        ))
    }
}

apf::export_plugin!(NoDawHarmonicSpace);
