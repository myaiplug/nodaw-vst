//! NoDAW kit: the parts every NoDAW plugin shares.
//!
//! * [`license`]: the 14-day trial, Gumroad activation and the post-trial dropouts (one
//!   [`license::Product`] per plugin, so each has its own trial and its own license).
//! * [`ctl`]: the web view's controller: the binary meter/spectrum frame, A/B slots, the
//!   license endpoints, the range table the page uses to map host values to plain units, and a
//!   hook ([`ctl::Ext`]) for each plugin's own endpoints and session state.
//! * [`fixture_json`]: the parameter table as JSON for the UI mock (`scripts/ui-mock`).
//!
//! The audio thread only ever touches atomics from here (meters, `demo_flag`), never a lock.

pub mod ctl;
pub mod license;
pub mod testing;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use apf::prelude::{Controller, HostHandle};
use nodaw_dsp::meter::GainReduction;

/// What every plugin instance carries besides its engine: meters for the page, the license
/// manager, and the post-trial dropout. The audio-thread methods touch atomics only.
pub struct Core {
    pub meters: Arc<ctl::Meters>,
    pub license: Arc<license::Manager>,
    dropout: license::Dropout,
    sr: f64,
}

impl Core {
    /// Main thread (the plugin's `new`): loads the product's license state from disk.
    pub fn new(product: &'static license::Product) -> Self {
        Self::with_license(license::Manager::load_default(product))
    }

    pub fn with_license(license: Arc<license::Manager>) -> Self {
        Self { meters: Arc::new(ctl::Meters::default()), license, dropout: license::Dropout::new(48_000.0), sr: 48_000.0 }
    }

    /// Main thread (`activate`): a trial that ran out while the session was open takes effect
    /// here; the page reads the sample rate and latency from the meters.
    pub fn activate(&mut self, sample_rate: f64, latency: usize) {
        if (self.sr - sample_rate).abs() > 0.5 {
            self.sr = sample_rate;
            self.dropout = license::Dropout::new(sample_rate);
        }
        self.license.refresh(license::now_secs());
        self.meters.sample_rate.store((sample_rate as f32).to_bits(), Ordering::Relaxed);
        self.meters.latency.store(latency as u32, Ordering::Relaxed);
    }

    /// Main thread (`controller`): the page's controller, with the plugin's own extension.
    pub fn controller<E: ctl::Ext>(&self, params: &'static [apf::param::ParamDef], ext: Arc<E>, host: HostHandle) -> Arc<dyn Controller> {
        // A licensed copy whose last online check is over 30 days old asks again, off-thread.
        self.license.maybe_recheck(license::now_secs());
        Arc::new(ctl::KitController::new(params, self.meters.clone(), self.license.clone(), ext, Some(host)))
    }

    /// Audio thread, before processing.
    pub fn begin(&self, l: &[f32], r: &[f32]) {
        self.meters.record_in(l, r);
    }

    /// Audio thread, after processing: the trial dropout (one relaxed atomic load; bypassed
    /// audio is left alone), then the output meters.
    pub fn end(&mut self, l: &mut [f32], r: &mut [f32], bypass: bool, playing: bool, gain_reduction: f32) {
        self.dropout.process(l, r, self.license.demo_flag() && !bypass);
        self.meters.record_out(l, r);
        GainReduction::record(&self.meters.gr, gain_reduction);
        self.meters.playing.store(playing as u8, Ordering::Relaxed);
    }
}

use apf::param::{Curve, ParamDef, ParamKind};

/// How each float parameter maps its host value (0..1) to the plain value the user sees.
pub fn ranges_json(params: &[ParamDef]) -> String {
    let mut o = String::from("{");
    let mut first = true;
    for def in params {
        if let ParamKind::Float { min, max, default, curve, decimals } = def.kind {
            if !first {
                o.push(',');
            }
            first = false;
            let (c, e) = match curve {
                Curve::Linear => ("lin", 1.0),
                Curve::Log => ("log", 1.0),
                Curve::Power(e) => ("pow", e),
            };
            o.push_str(&format!(
                "\"{}\":{{\"min\":{min},\"max\":{max},\"default\":{default},\"curve\":\"{c}\",\"exp\":{e},\"decimals\":{decimals},\"unit\":\"{}\",\"mid\":{}}}",
                def.key,
                def.unit,
                def.host_to_plain(0.37)
            ));
        }
    }
    o.push('}');
    o
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The parameter table, the ranges and `extra` (a JSON object's members, without braces) as
/// one JSON object, for the UI mock server.
pub fn fixture_json(params: &[ParamDef], extra: &str) -> String {
    let mut o = String::from("{\"params\":[");
    for (i, d) in params.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let kind = match d.kind {
            ParamKind::Float { min, max, default, curve, decimals } => {
                let (c, e) = match curve {
                    Curve::Linear => ("lin", 1.0),
                    Curve::Log => ("log", 1.0),
                    Curve::Power(e) => ("pow", e),
                };
                format!("\"kind\":\"float\",\"min\":{min},\"max\":{max},\"default\":{default},\"curve\":\"{c}\",\"exp\":{e},\"decimals\":{decimals}")
            }
            ParamKind::Int { min, max, default } => format!("\"kind\":\"int\",\"min\":{min},\"max\":{max},\"default\":{default}"),
            ParamKind::Bool { default } => format!("\"kind\":\"bool\",\"default\":{}", default as u8),
            ParamKind::Choice { options, default } => {
                let opts: Vec<String> = options.iter().map(|s| format!("\"{}\"", esc(s))).collect();
                format!("\"kind\":\"choice\",\"default\":{default},\"options\":[{}]", opts.join(","))
            }
        };
        o.push_str(&format!("{{\"key\":\"{}\",\"name\":\"{}\",\"unit\":\"{}\",\"module\":\"{}\",{kind}}}", d.key, esc(d.name), esc(d.unit), esc(d.module)));
    }
    o.push_str("],\"ranges\":");
    o.push_str(&ranges_json(params));
    if !extra.is_empty() {
        o.push(',');
        o.push_str(extra);
    }
    o.push('}');
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use apf::json;

    const P: &[ParamDef] = &[
        ParamDef::bypass("bypass"),
        ParamDef::float("gain", "Gain", -24.0, 24.0, 0.0).unit("dB"),
        ParamDef::float("freq", "Freq", 20.0, 20_000.0, 1000.0).curve(Curve::Log),
        ParamDef::choice("mode", "Mode", &["A", "B"], 1),
    ];

    #[test]
    fn ranges_and_the_fixture_are_valid_json() {
        let r = json::parse(&ranges_json(P)).unwrap();
        assert_eq!(r.get("freq").and_then(|v| v.get("curve")).and_then(|v| v.as_str()), Some("log"));
        assert!(r.get("mode").is_none());
        let f = json::parse(&fixture_json(P, "\"extra\":4")).unwrap();
        assert_eq!(f.get("extra").and_then(|v| v.as_f64()), Some(4.0));
    }
}
