//! NoDAW Chronos Dynamic EQ (CLAP and VST3): seven dynamic EQ bands (bell or shelf, each a
//! static gain plus a split-band compressor or upward expander), a master section and the
//! analyzer with the live dynamic curve. Every control is a host parameter; the A/B slots are
//! the only other session state. See `docs/PLUGINS.md`.

pub mod engine;

use std::sync::Arc;

use apf::prelude::*;
use nodaw_dsp::dynamics::{BandKind, DynMode, DynSettings};
use nodaw_kit::ctl::LookExt;
use nodaw_kit::license::Product;
use nodaw_kit::Core;

use engine::{Engine, Settings, BANDS};

#[cfg(feature = "ui")]
use apf_webview::{assets, Asset, WebView};

/// Trial and licensing for this plugin (its own trial, its own Gumroad product).
pub static PRODUCT: Product = Product {
    name: "NoDAW Chronos Dynamic EQ",
    folder: "Chronos Dynamic EQ",
    // TODO(owner): the Gumroad product ID of NoDAW Chronos Dynamic EQ.
    gumroad_product_id: "TODO_GUMROAD_PRODUCT_ID_CHRONOS_DYNAMIC_EQ",
    // TODO(owner): the Gumroad page for NoDAW Chronos Dynamic EQ.
    buy_url: "https://nodawlabs.gumroad.com/l/nodaw-chronos-dynamic-eq",
    hmac_key: b"nodaw-chronos-dynamic-eq/license/v1/5d7a0e3c-92b1-4f68-b0c4-6e2f81a9d357",
    user_agent: concat!("NoDAW-Chronos-Dynamic-EQ/", env!("CARGO_PKG_VERSION")),
};

const TYPES: &[&str] = &["Bell", "Low Shelf", "High Shelf"];
const MODES: &[&str] = &["Static", "Compress", "Expand"];

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
    ParamDef::float("dyn_amount", "Dynamics", 0.0, 200.0, 100.0)
        .unit("%")
        .decimals(0)
        .module("Master"),
    ParamDef::bool("b1_on", "Band 1 On", true).module("Band 1"),
    ParamDef::choice("b1_type", "Band 1 Type", TYPES, 1).module("Band 1"),
    ParamDef::float("b1_freq", "Band 1 Freq", 20.0, 20_000.0, 60.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 1"),
    ParamDef::float("b1_gain", "Band 1 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 1"),
    ParamDef::float("b1_q", "Band 1 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 1"),
    ParamDef::choice("b1_mode", "Band 1 Dynamics", MODES, 0).module("Band 1"),
    ParamDef::float("b1_thresh", "Band 1 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 1"),
    ParamDef::float("b1_ratio", "Band 1 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 1"),
    ParamDef::float("b1_range", "Band 1 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 1"),
    ParamDef::float("b1_attack", "Band 1 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 1"),
    ParamDef::float("b1_release", "Band 1 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 1"),
    ParamDef::bool("b1_solo", "Band 1 Solo", false).module("Band 1"),
    ParamDef::bool("b2_on", "Band 2 On", true).module("Band 2"),
    ParamDef::choice("b2_type", "Band 2 Type", TYPES, 0).module("Band 2"),
    ParamDef::float("b2_freq", "Band 2 Freq", 20.0, 20_000.0, 150.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 2"),
    ParamDef::float("b2_gain", "Band 2 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 2"),
    ParamDef::float("b2_q", "Band 2 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 2"),
    ParamDef::choice("b2_mode", "Band 2 Dynamics", MODES, 0).module("Band 2"),
    ParamDef::float("b2_thresh", "Band 2 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 2"),
    ParamDef::float("b2_ratio", "Band 2 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 2"),
    ParamDef::float("b2_range", "Band 2 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 2"),
    ParamDef::float("b2_attack", "Band 2 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 2"),
    ParamDef::float("b2_release", "Band 2 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 2"),
    ParamDef::bool("b2_solo", "Band 2 Solo", false).module("Band 2"),
    ParamDef::bool("b3_on", "Band 3 On", true).module("Band 3"),
    ParamDef::choice("b3_type", "Band 3 Type", TYPES, 0).module("Band 3"),
    ParamDef::float("b3_freq", "Band 3 Freq", 20.0, 20_000.0, 400.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 3"),
    ParamDef::float("b3_gain", "Band 3 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 3"),
    ParamDef::float("b3_q", "Band 3 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 3"),
    ParamDef::choice("b3_mode", "Band 3 Dynamics", MODES, 0).module("Band 3"),
    ParamDef::float("b3_thresh", "Band 3 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 3"),
    ParamDef::float("b3_ratio", "Band 3 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 3"),
    ParamDef::float("b3_range", "Band 3 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 3"),
    ParamDef::float("b3_attack", "Band 3 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 3"),
    ParamDef::float("b3_release", "Band 3 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 3"),
    ParamDef::bool("b3_solo", "Band 3 Solo", false).module("Band 3"),
    ParamDef::bool("b4_on", "Band 4 On", true).module("Band 4"),
    ParamDef::choice("b4_type", "Band 4 Type", TYPES, 0).module("Band 4"),
    ParamDef::float("b4_freq", "Band 4 Freq", 20.0, 20_000.0, 1000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 4"),
    ParamDef::float("b4_gain", "Band 4 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 4"),
    ParamDef::float("b4_q", "Band 4 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 4"),
    ParamDef::choice("b4_mode", "Band 4 Dynamics", MODES, 0).module("Band 4"),
    ParamDef::float("b4_thresh", "Band 4 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 4"),
    ParamDef::float("b4_ratio", "Band 4 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 4"),
    ParamDef::float("b4_range", "Band 4 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 4"),
    ParamDef::float("b4_attack", "Band 4 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 4"),
    ParamDef::float("b4_release", "Band 4 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 4"),
    ParamDef::bool("b4_solo", "Band 4 Solo", false).module("Band 4"),
    ParamDef::bool("b5_on", "Band 5 On", true).module("Band 5"),
    ParamDef::choice("b5_type", "Band 5 Type", TYPES, 0).module("Band 5"),
    ParamDef::float("b5_freq", "Band 5 Freq", 20.0, 20_000.0, 2500.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 5"),
    ParamDef::float("b5_gain", "Band 5 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 5"),
    ParamDef::float("b5_q", "Band 5 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 5"),
    ParamDef::choice("b5_mode", "Band 5 Dynamics", MODES, 0).module("Band 5"),
    ParamDef::float("b5_thresh", "Band 5 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 5"),
    ParamDef::float("b5_ratio", "Band 5 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 5"),
    ParamDef::float("b5_range", "Band 5 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 5"),
    ParamDef::float("b5_attack", "Band 5 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 5"),
    ParamDef::float("b5_release", "Band 5 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 5"),
    ParamDef::bool("b5_solo", "Band 5 Solo", false).module("Band 5"),
    ParamDef::bool("b6_on", "Band 6 On", true).module("Band 6"),
    ParamDef::choice("b6_type", "Band 6 Type", TYPES, 0).module("Band 6"),
    ParamDef::float("b6_freq", "Band 6 Freq", 20.0, 20_000.0, 6000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 6"),
    ParamDef::float("b6_gain", "Band 6 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 6"),
    ParamDef::float("b6_q", "Band 6 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 6"),
    ParamDef::choice("b6_mode", "Band 6 Dynamics", MODES, 0).module("Band 6"),
    ParamDef::float("b6_thresh", "Band 6 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 6"),
    ParamDef::float("b6_ratio", "Band 6 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 6"),
    ParamDef::float("b6_range", "Band 6 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 6"),
    ParamDef::float("b6_attack", "Band 6 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 6"),
    ParamDef::float("b6_release", "Band 6 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 6"),
    ParamDef::bool("b6_solo", "Band 6 Solo", false).module("Band 6"),
    ParamDef::bool("b7_on", "Band 7 On", true).module("Band 7"),
    ParamDef::choice("b7_type", "Band 7 Type", TYPES, 2).module("Band 7"),
    ParamDef::float("b7_freq", "Band 7 Freq", 20.0, 20_000.0, 12000.0)
        .unit("Hz")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 7"),
    ParamDef::float("b7_gain", "Band 7 Gain", -24.0, 24.0, 0.0)
        .unit("dB")
        .decimals(1)
        .module("Band 7"),
    ParamDef::float("b7_q", "Band 7 Q", 0.1, 18.0, 1.0)
        .curve(Curve::Log)
        .decimals(2)
        .module("Band 7"),
    ParamDef::choice("b7_mode", "Band 7 Dynamics", MODES, 0).module("Band 7"),
    ParamDef::float("b7_thresh", "Band 7 Threshold", -60.0, 0.0, -24.0)
        .unit("dB")
        .decimals(1)
        .module("Band 7"),
    ParamDef::float("b7_ratio", "Band 7 Ratio", 1.0, 20.0, 3.0)
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 7"),
    ParamDef::float("b7_range", "Band 7 Range", 0.0, 24.0, 6.0)
        .unit("dB")
        .decimals(1)
        .module("Band 7"),
    ParamDef::float("b7_attack", "Band 7 Attack", 0.1, 200.0, 5.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(1)
        .module("Band 7"),
    ParamDef::float("b7_release", "Band 7 Release", 5.0, 2000.0, 120.0)
        .unit("ms")
        .curve(Curve::Log)
        .decimals(0)
        .module("Band 7"),
    ParamDef::bool("b7_solo", "Band 7 Solo", false).module("Band 7"),
];

const BYPASS: ParamId = ParamId::of(PARAMS, "bypass");
const IN_GAIN: ParamId = ParamId::of(PARAMS, "in_gain");
const OUT_GAIN: ParamId = ParamId::of(PARAMS, "out_gain");
const MIX: ParamId = ParamId::of(PARAMS, "mix");
const DYN_AMOUNT: ParamId = ParamId::of(PARAMS, "dyn_amount");
const B_ON: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_on"),
    ParamId::of(PARAMS, "b2_on"),
    ParamId::of(PARAMS, "b3_on"),
    ParamId::of(PARAMS, "b4_on"),
    ParamId::of(PARAMS, "b5_on"),
    ParamId::of(PARAMS, "b6_on"),
    ParamId::of(PARAMS, "b7_on"),
];
const B_TYPE: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_type"),
    ParamId::of(PARAMS, "b2_type"),
    ParamId::of(PARAMS, "b3_type"),
    ParamId::of(PARAMS, "b4_type"),
    ParamId::of(PARAMS, "b5_type"),
    ParamId::of(PARAMS, "b6_type"),
    ParamId::of(PARAMS, "b7_type"),
];
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
const B_MODE: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_mode"),
    ParamId::of(PARAMS, "b2_mode"),
    ParamId::of(PARAMS, "b3_mode"),
    ParamId::of(PARAMS, "b4_mode"),
    ParamId::of(PARAMS, "b5_mode"),
    ParamId::of(PARAMS, "b6_mode"),
    ParamId::of(PARAMS, "b7_mode"),
];
const B_THRESH: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_thresh"),
    ParamId::of(PARAMS, "b2_thresh"),
    ParamId::of(PARAMS, "b3_thresh"),
    ParamId::of(PARAMS, "b4_thresh"),
    ParamId::of(PARAMS, "b5_thresh"),
    ParamId::of(PARAMS, "b6_thresh"),
    ParamId::of(PARAMS, "b7_thresh"),
];
const B_RATIO: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_ratio"),
    ParamId::of(PARAMS, "b2_ratio"),
    ParamId::of(PARAMS, "b3_ratio"),
    ParamId::of(PARAMS, "b4_ratio"),
    ParamId::of(PARAMS, "b5_ratio"),
    ParamId::of(PARAMS, "b6_ratio"),
    ParamId::of(PARAMS, "b7_ratio"),
];
const B_RANGE: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_range"),
    ParamId::of(PARAMS, "b2_range"),
    ParamId::of(PARAMS, "b3_range"),
    ParamId::of(PARAMS, "b4_range"),
    ParamId::of(PARAMS, "b5_range"),
    ParamId::of(PARAMS, "b6_range"),
    ParamId::of(PARAMS, "b7_range"),
];
const B_ATTACK: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_attack"),
    ParamId::of(PARAMS, "b2_attack"),
    ParamId::of(PARAMS, "b3_attack"),
    ParamId::of(PARAMS, "b4_attack"),
    ParamId::of(PARAMS, "b5_attack"),
    ParamId::of(PARAMS, "b6_attack"),
    ParamId::of(PARAMS, "b7_attack"),
];
const B_RELEASE: [ParamId; BANDS] = [
    ParamId::of(PARAMS, "b1_release"),
    ParamId::of(PARAMS, "b2_release"),
    ParamId::of(PARAMS, "b3_release"),
    ParamId::of(PARAMS, "b4_release"),
    ParamId::of(PARAMS, "b5_release"),
    ParamId::of(PARAMS, "b6_release"),
    ParamId::of(PARAMS, "b7_release"),
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

const REMOTE: &[RemotePage] = &[
    RemotePage {
        section: "Chronos",
        name: "Gains",
        params: [
            "b1_gain", "b2_gain", "b3_gain", "b4_gain", "b5_gain", "b6_gain", "b7_gain", "out_gain",
        ],
    },
    RemotePage {
        section: "Chronos",
        name: "Thresholds",
        params: [
            "b1_thresh",
            "b2_thresh",
            "b3_thresh",
            "b4_thresh",
            "b5_thresh",
            "b6_thresh",
            "b7_thresh",
            "dyn_amount",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 1",
        params: [
            "b1_freq",
            "b1_gain",
            "b1_q",
            "b1_thresh",
            "b1_ratio",
            "b1_range",
            "b1_attack",
            "b1_release",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 2",
        params: [
            "b2_freq",
            "b2_gain",
            "b2_q",
            "b2_thresh",
            "b2_ratio",
            "b2_range",
            "b2_attack",
            "b2_release",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 3",
        params: [
            "b3_freq",
            "b3_gain",
            "b3_q",
            "b3_thresh",
            "b3_ratio",
            "b3_range",
            "b3_attack",
            "b3_release",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 4",
        params: [
            "b4_freq",
            "b4_gain",
            "b4_q",
            "b4_thresh",
            "b4_ratio",
            "b4_range",
            "b4_attack",
            "b4_release",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 5",
        params: [
            "b5_freq",
            "b5_gain",
            "b5_q",
            "b5_thresh",
            "b5_ratio",
            "b5_range",
            "b5_attack",
            "b5_release",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 6",
        params: [
            "b6_freq",
            "b6_gain",
            "b6_q",
            "b6_thresh",
            "b6_ratio",
            "b6_range",
            "b6_attack",
            "b6_release",
        ],
    },
    RemotePage {
        section: "Bands",
        name: "Band 7",
        params: [
            "b7_freq",
            "b7_gain",
            "b7_q",
            "b7_thresh",
            "b7_ratio",
            "b7_range",
            "b7_attack",
            "b7_release",
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

/// The colourways (page only; saved with the session).
pub const LOOKS: &[&str] = &["violet", "amber"];

/// The plugin.
pub struct NoDawChronosDynamicEq {
    engine: Engine,
    core: Core,
    /// Frame extras: each band's dynamic gain in dB (latest value).
    ext: Arc<LookExt>,
}

/// The engine's settings from the parameters. No allocation.
pub fn settings(p: &Params) -> Settings {
    let bands: [DynSettings; BANDS] = std::array::from_fn(|i| DynSettings {
        on: p.get_bool(B_ON[i]),
        kind: BandKind::from_index(p.get_index(B_TYPE[i])),
        freq: p.get_f32(B_FREQ[i]),
        q: p.get_f32(B_Q[i]),
        gain_db: p.get_f32(B_GAIN[i]),
        mode: DynMode::from_index(p.get_index(B_MODE[i])),
        threshold_db: p.get_f32(B_THRESH[i]),
        ratio: p.get_f32(B_RATIO[i]),
        range_db: p.get_f32(B_RANGE[i]),
        attack_ms: p.get_f32(B_ATTACK[i]),
        release_ms: p.get_f32(B_RELEASE[i]),
        solo: p.get_bool(B_SOLO[i]),
    });
    Settings {
        bypass: p.get_bool(BYPASS),
        in_db: p.get_f32(IN_GAIN),
        out_db: p.get_f32(OUT_GAIN),
        mix: p.get_f32(MIX) * 0.01,
        dyn_amount: p.get_f32(DYN_AMOUNT) * 0.01,
        bands,
    }
}

impl Plugin for NoDawChronosDynamicEq {
    const INFO: PluginInfo = PluginInfo {
        id: "com.nodaw.chronos-dynamic-eq",
        name: "NoDAW Chronos Dynamic EQ",
        vendor: "NoDAW",
        url: "https://nodaw.app",
        version: env!("CARGO_PKG_VERSION"),
        description: "Seven-band dynamic EQ: static bells and shelves with per-band compression or upward expansion",
        features: &["audio-effect", "stereo", "equalizer", "compressor"],
    };
    const LAYOUT: AudioLayout = AudioLayout::EFFECT_STEREO;
    const PARAMS: &'static [ParamDef] = PARAMS;
    const REMOTE_CONTROLS: &'static [RemotePage] = REMOTE;
    const FACTORY_PRESETS: &'static [&'static str] = apf::presets![
        "../presets/Init.apfpreset",
        "../presets/Vocal De-Esser.apfpreset",
        "../presets/Tame The Boom.apfpreset",
        "../presets/Kick Punch.apfpreset",
        "../presets/De-Harsh Guitars.apfpreset",
        "../presets/Mix Bus Smooth.apfpreset",
        "../presets/Mastering Air.apfpreset",
        "../presets/Bass Control.apfpreset",
        "../presets/Podcast Voice.apfpreset",
        "../presets/Snare Snap.apfpreset",
        "../presets/Vocal Presence Pop.apfpreset",
        "../presets/Resonance Hunter.apfpreset",
    ];

    fn new() -> Self {
        Self {
            engine: Engine::new(48_000.0),
            core: Core::new(&PRODUCT),
            ext: Arc::new(LookExt::new(LOOKS, 0, BANDS)),
        }
    }

    fn controller(&mut self, host: HostHandle) -> Option<Arc<dyn Controller>> {
        Some(self.core.controller(PARAMS, self.ext.clone(), host))
    }

    fn activate(&mut self, config: &ProcessConfig, _params: &Params) {
        if (self.engine.sample_rate() - config.sample_rate).abs() > 0.5 {
            self.engine = Engine::new(config.sample_rate);
        }
        self.engine.reset();
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
        // the longest release settles well within half a second of silence... plus the filters
        (0.25 * self.engine.sample_rate()) as u32
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
        for (i, v) in info.dyn_db.iter().enumerate() {
            self.ext.values.set(i, *v);
        }
        ProcessStatus::Continue
    }

    #[cfg(feature = "ui")]
    fn view() -> Option<Box<dyn View>> {
        Some(Box::new(
            WebView::new(1000, 620, UI)
                .resizable(true)
                .size_limits((820, 520), (2600, 1700))
                .fit_screen((48, 96))
                .background(6, 6, 12),
        ))
    }
}

apf::export_plugin!(NoDawChronosDynamicEq);
