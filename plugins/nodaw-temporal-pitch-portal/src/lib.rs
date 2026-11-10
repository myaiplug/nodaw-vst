//! NoDAW Temporal Pitch Portal (CLAP and VST3): a tempo-synced time warp (50 % half-time to
//! 200 % double-time, as a varispeed tape or pitch-keeping grains) and an independent pitch
//! shifter (±12 st with glide and a drifting micro pitch), a tilt tone and a portal echo. The
//! prototype's Warp Zones and ScrewAI Flavors are the factory presets. Every control is a host
//! parameter; the faceplate and the A/B slots are the only other session state. See
//! `docs/PLUGINS.md`.

pub mod engine;

use std::sync::Arc;

use apf::prelude::*;
use nodaw_kit::ctl::LookExt;
use nodaw_kit::license::Product;
use nodaw_kit::Core;

use engine::{Clock, Engine, Settings, Sync};

#[cfg(feature = "ui")]
use apf_webview::{assets, Asset, WebView};

/// Trial and licensing for this plugin (its own trial, its own Gumroad product).
pub static PRODUCT: Product = Product {
    name: "NoDAW Temporal Pitch Portal",
    folder: "Temporal Pitch Portal",
    // TODO(owner): the Gumroad product ID of NoDAW Temporal Pitch Portal.
    gumroad_product_id: "TODO_GUMROAD_PRODUCT_ID_TEMPORAL_PITCH_PORTAL",
    // TODO(owner): the Gumroad page for NoDAW Temporal Pitch Portal.
    buy_url: "https://nodawlabs.gumroad.com/l/nodaw-temporal-pitch-portal",
    hmac_key: b"nodaw-temporal-pitch-portal/license/v1/5c1f9e02-7b44-4d6a-9e38-a0d2c7f41b93",
    user_agent: concat!("NoDAW-Temporal-Pitch-Portal/", env!("CARGO_PKG_VERSION")),
};

const MODES: &[&str] = &["Stretch", "Tape"];
const SYNCS: &[&str] = &["1/4", "1/2 Bar", "1 Bar", "2 Bars"];

/// The parameters. Append only.
pub const PARAMS: &[ParamDef] = &[
    ParamDef::bypass("bypass"),
    ParamDef::float("time", "Time Shift", 50.0, 200.0, 100.0)
        .unit("%")
        .curve(Curve::Log)
        .decimals(0)
        .module("Portal"),
    ParamDef::float("pitch", "Pitch Bend", -12.0, 12.0, 0.0)
        .unit("st")
        .decimals(2)
        .module("Portal"),
    ParamDef::float("out_gain", "Output", -24.0, 12.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Master"),
    ParamDef::float("mix", "Mix", 0.0, 100.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Master"),
    ParamDef::choice("warp_mode", "Warp Mode", MODES, 0).module("Portal"),
    ParamDef::choice("warp_sync", "Warp Sync", SYNCS, 2).module("Portal"),
    ParamDef::float("glide", "Glide", 0.0, 2000.0, 30.0)
        .unit("ms")
        .curve(Curve::Power(2.5))
        .decimals(0)
        .module("Portal"),
    ParamDef::float("drift", "Drift", 0.0, 50.0, 0.0)
        .unit("ct")
        .decimals(1)
        .module("Drift"),
    ParamDef::float("drift_rate", "Drift Rate", 0.05, 8.0, 0.7)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(2)
        .module("Drift"),
    ParamDef::float("tone", "Tone", -100.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Tone"),
    ParamDef::float("echo_mix", "Echo", 0.0, 100.0, 0.0)
        .unit("%")
        .decimals(0)
        .module("Echo"),
    ParamDef::float("echo_time", "Echo Time", 20.0, 2000.0, 375.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Echo"),
    ParamDef::float("echo_fb", "Echo Feedback", 0.0, 90.0, 35.0)
        .unit("%")
        .decimals(0)
        .module("Echo"),
];

const BYPASS: ParamId = ParamId::of(PARAMS, "bypass");
const TIME: ParamId = ParamId::of(PARAMS, "time");
const PITCH: ParamId = ParamId::of(PARAMS, "pitch");
const OUT_GAIN: ParamId = ParamId::of(PARAMS, "out_gain");
const MIX: ParamId = ParamId::of(PARAMS, "mix");
const WARP_MODE: ParamId = ParamId::of(PARAMS, "warp_mode");
const WARP_SYNC: ParamId = ParamId::of(PARAMS, "warp_sync");
const GLIDE: ParamId = ParamId::of(PARAMS, "glide");
const DRIFT: ParamId = ParamId::of(PARAMS, "drift");
const DRIFT_RATE: ParamId = ParamId::of(PARAMS, "drift_rate");
const TONE: ParamId = ParamId::of(PARAMS, "tone");
const ECHO_MIX: ParamId = ParamId::of(PARAMS, "echo_mix");
const ECHO_TIME: ParamId = ParamId::of(PARAMS, "echo_time");
const ECHO_FB: ParamId = ParamId::of(PARAMS, "echo_fb");

const REMOTE: &[RemotePage] = &[
    RemotePage {
        section: "Portal",
        name: "Main",
        params: [
            "time",
            "pitch",
            "glide",
            "drift",
            "drift_rate",
            "tone",
            "echo_mix",
            "out_gain",
        ],
    },
    RemotePage {
        section: "Portal",
        name: "Echo & Mix",
        params: [
            "echo_mix",
            "echo_time",
            "echo_fb",
            "tone",
            "warp_mode",
            "warp_sync",
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

/// The faceplates (page only; saved with the session).
pub const LOOKS: &[&str] = &["dark", "light"];

/// The plugin.
pub struct NoDawTemporalPitchPortal {
    engine: Engine,
    core: Core,
    /// Frame extras: the echo peak, then pitch (st), drift (ct), warp phase and speed ripple.
    ext: Arc<LookExt>,
    tail_s: f32,
}

/// The engine's settings from the parameters. No allocation.
pub fn settings(p: &Params) -> Settings {
    Settings {
        bypass: p.get_bool(BYPASS),
        time: p.get_f32(TIME) * 0.01,
        stretch: p.get_index(WARP_MODE) == 0,
        sync: Sync::from_index(p.get_index(WARP_SYNC)),
        pitch_st: p.get_f32(PITCH),
        glide_ms: p.get_f32(GLIDE),
        drift_cents: p.get_f32(DRIFT),
        drift_hz: p.get_f32(DRIFT_RATE),
        tone: p.get_f32(TONE) * 0.01,
        echo_mix: p.get_f32(ECHO_MIX) * 0.01,
        echo_s: p.get_f32(ECHO_TIME) * 0.001,
        echo_fb: p.get_f32(ECHO_FB) * 0.01,
        out_db: p.get_f32(OUT_GAIN),
        mix: p.get_f32(MIX) * 0.01,
    }
}

impl Plugin for NoDawTemporalPitchPortal {
    const INFO: PluginInfo = PluginInfo {
        id: "com.nodaw.temporal-pitch-portal",
        name: "NoDAW Temporal Pitch Portal",
        vendor: "NoDAW",
        url: "https://nodaw.app",
        version: env!("CARGO_PKG_VERSION"),
        description: "Tempo-synced time warp (half- to double-time) and independent pitch shift with glide, drift, tone and echo",
        features: &["audio-effect", "stereo", "pitch-shifter", "delay"],
    };
    const LAYOUT: AudioLayout = AudioLayout::EFFECT_STEREO;
    const PARAMS: &'static [ParamDef] = PARAMS;
    const REMOTE_CONTROLS: &'static [RemotePage] = REMOTE;
    const FACTORY_PRESETS: &'static [&'static str] = apf::presets![
        "../presets/Init.apfpreset",
        "../presets/Classic Portal.apfpreset",
        "../presets/Deep Rift.apfpreset",
        "../presets/Zero Gravity.apfpreset",
        "../presets/Wormhole Clamp.apfpreset",
        "../presets/Glass Phase Portal.apfpreset",
        "../presets/Slowed Portal.apfpreset",
        "../presets/Double Cup Dimension.apfpreset",
        "../presets/Houston Rift.apfpreset",
        "../presets/Mane Hold Time.apfpreset",
        "../presets/Astro Portals.apfpreset",
        "../presets/Tape Half-Time.apfpreset",
        "../presets/Seasick Drift.apfpreset",
        "../presets/Octave Echo Down.apfpreset",
    ];

    fn new() -> Self {
        Self {
            engine: Engine::new(48_000.0),
            core: Core::new(&PRODUCT),
            ext: Arc::new(LookExt::new(LOOKS, 1, 4)),
            tail_s: 1.0,
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
        self.engine.latency() as u32
    }

    fn tail(&self) -> u32 {
        (self.tail_s as f64 * self.engine.sample_rate()) as u32 + self.engine.latency() as u32
    }

    fn process(&mut self, audio: &mut AudioBuffer, ctx: &ProcessContext) -> ProcessStatus {
        let Some((l, r)) = audio.stereo() else {
            return ProcessStatus::Error;
        };
        let s = settings(ctx.params);
        self.tail_s = Engine::tail_seconds(&s);
        let t = &ctx.transport;
        let bar_beats = t
            .time_signature
            .filter(|(n, d)| *n > 0 && *d > 0)
            .map(|(n, d)| n as f64 * 4.0 / d as f64)
            .unwrap_or(4.0);
        let clock = Clock {
            song_pos_beats: t.song_pos_beats,
            playing: t.playing,
            tempo: t.tempo,
            bar_beats,
        };
        self.core.begin(l, r);
        let info = self.engine.process(l, r, &s, &clock);
        self.core.end(l, r, s.bypass, t.playing, 1.0);
        self.ext.peaks.max(0, info.echo_peak);
        self.ext.values.set(0, info.pitch_st);
        self.ext.values.set(1, info.drift_cents);
        self.ext.values.set(2, info.phase);
        self.ext.values.set(3, info.ripple);
        ProcessStatus::Continue
    }

    #[cfg(feature = "ui")]
    fn view() -> Option<Box<dyn View>> {
        Some(Box::new(
            WebView::new(960, 620, UI)
                .resizable(true)
                .size_limits((800, 520), (2600, 1700))
                .fit_screen((48, 96))
                .background(8, 8, 14),
        ))
    }
}

apf::export_plugin!(NoDawTemporalPitchPortal);
