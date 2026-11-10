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
