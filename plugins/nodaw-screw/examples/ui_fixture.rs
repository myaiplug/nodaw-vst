//! Prints the parameter table, ranges and chop patterns as JSON. The UI mock server
//! (`scripts/ui-mock/server.py`) reads this, so the mock page sees exactly the keys, ranges
//! and defaults of the real plugin and nothing is typed in twice.
//!
//!     cargo run -p nodaw-screw --no-default-features --example ui_fixture

use apf::param::{Curve, ParamKind};
use nodaw_screw::{shared, PARAMS};

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let mut o = String::from("{\"params\":[");
    for (i, d) in PARAMS.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let kind = match d.kind {
            ParamKind::Float {
                min,
                max,
                default,
                curve,
                decimals,
            } => {
                let (c, e) = match curve {
                    Curve::Linear => ("lin", 1.0),
                    Curve::Log => ("log", 1.0),
                    Curve::Power(e) => ("pow", e),
                };
                format!("\"kind\":\"float\",\"min\":{min},\"max\":{max},\"default\":{default},\"curve\":\"{c}\",\"exp\":{e},\"decimals\":{decimals}")
            }
            ParamKind::Int { min, max, default } => {
                format!("\"kind\":\"int\",\"min\":{min},\"max\":{max},\"default\":{default}")
            }
            ParamKind::Bool { default } => {
                format!("\"kind\":\"bool\",\"default\":{}", default as u8)
            }
            ParamKind::Choice { options, default } => {
                let opts: Vec<String> = options.iter().map(|s| format!("\"{}\"", esc(s))).collect();
                format!(
                    "\"kind\":\"choice\",\"default\":{default},\"options\":[{}]",
                    opts.join(",")
                )
            }
        };
        o.push_str(&format!(
            "{{\"key\":\"{}\",\"name\":\"{}\",\"unit\":\"{}\",\"module\":\"{}\",{kind}}}",
            d.key,
            esc(d.name),
            esc(d.unit),
            esc(d.module)
        ));
    }
    o.push_str("],\"ranges\":");
    o.push_str(&shared::ranges_json());
    o.push_str(",\"patterns\":");
    o.push_str(&shared::patterns_json());
    o.push('}');
    println!("{o}");
}
