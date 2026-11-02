//! Every factory preset is a valid preset file for this plugin: it parses, names only real
//! parameters, keeps each value inside its range, and has a unique name.

use apf::json::{self, Value};
use apf::plugin::Plugin;
use apf::preset::Preset;
use nodaw_screw::{NoDawScrew, PARAMS};

#[test]
fn there_are_twelve_factory_presets() {
    assert_eq!(NoDawScrew::FACTORY_PRESETS.len(), 12);
}

#[test]
fn every_factory_preset_is_valid() {
    let mut names = std::collections::HashSet::new();
    for text in NoDawScrew::FACTORY_PRESETS {
        let p = Preset::parse(text).expect("preset parses");
        assert!(!p.name.is_empty(), "preset without a name");
        assert!(names.insert(p.name.clone()), "duplicate preset name {}", p.name);
        let root = json::parse(text).unwrap();
        let Some(Value::Obj(members)) = root.get("params") else { panic!("{}: no params", p.name) };
        for (key, value) in members {
            let def = PARAMS.iter().find(|d| d.key == key).unwrap_or_else(|| panic!("{}: unknown parameter {key}", p.name));
            match value {
                Value::Num(n) => {
                    let host = def.plain_to_host(*n);
                    let back = def.host_to_plain(host);
                    if let apf::ParamKind::Float { min, max, .. } = def.kind {
                        assert!(*n >= min - 1e-9 && *n <= max + 1e-9, "{}: {key}={n} is outside {min}..{max}", p.name);
                        assert!((back - n).abs() < 1e-6 * (1.0 + n.abs()), "{}: {key} does not round-trip", p.name);
                    }
                }
                Value::Str(s) => assert!(def.parse(s).is_some(), "{}: {key}={s:?} is not an option", p.name),
                Value::Bool(_) => {}
                other => panic!("{}: {key} has an odd value {other:?}", p.name),
            }
        }
    }
}
