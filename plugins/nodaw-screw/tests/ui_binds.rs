//! The UI and the plugin agree: every parameter key the page binds to exists, and every
//! parameter the host can see has a control on the page.

use nodaw_screw::PARAMS;

const HTML: &str = include_str!("../ui/index.html");
const JS: &str = include_str!("../ui/app.js");

fn keys_after(text: &str, marker: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(marker) {
        rest = &rest[i + marker.len()..];
        let end = rest.find('"').unwrap_or(0);
        out.push(rest[..end].to_owned());
    }
    out
}

fn js_knob_keys() -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = JS;
    while let Some(i) = rest.find("k: \"") {
        rest = &rest[i + 4..];
        let end = rest.find('"').unwrap();
        out.push(rest[..end].to_owned());
    }
    out
}

#[test]
fn every_key_the_page_binds_is_a_real_parameter() {
    let mut used = Vec::new();
    for m in ["data-param=\"", "data-param-text=\"", "data-param-name=\""] {
        used.extend(keys_after(HTML, m));
    }
    used.extend(js_knob_keys());
    assert!(!used.is_empty());
    for k in used {
        assert!(
            PARAMS.iter().any(|p| p.key == k),
            "the page binds {k:?}, which is not a parameter"
        );
    }
}

#[test]
fn every_parameter_has_a_control_on_the_page() {
    let mut used = Vec::new();
    for m in ["data-param=\"", "data-param-text=\"", "data-param-name=\""] {
        used.extend(keys_after(HTML, m));
    }
    used.extend(js_knob_keys());
    // the chop pattern/rate selects and the bypass/limiter toggles are bound by attribute or
    // by `apf.param("key")` in app.js
    for p in PARAMS {
        let quoted = format!("\"{}\"", p.key);
        assert!(
            used.iter().any(|k| k == p.key) || JS.contains(&quoted),
            "parameter {} has no control on the page",
            p.key
        );
    }
}
