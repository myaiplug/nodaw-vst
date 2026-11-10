//! Conformance tests every NoDAW plugin runs (as integration tests, through the CLAP interface
//! the way a host drives it). The macros expand in the plugin's test crate, which needs
//! `apf-test` and `clap-sys` as dev-dependencies.
//!
//! ```ignore
//! nodaw_kit::conformance_tests!(nodaw_racks, nodaw_racks::NoDawRacks, nodaw_racks::PARAMS,
//!     heavy = [("Drive", "24 dB")], neutral = [("Saturator On", "Off")], null_tol = 2e-3);
//! nodaw_kit::ui_binds_tests!(nodaw_racks::PARAMS, "ui/index.html", "ui/app.js");
//! ```

/// A 16-bit-free float WAV writer for test renders (stereo, 32-bit float).
pub fn write_wav(path: &std::path::Path, sr: u32, l: &[f32], r: &[f32]) -> std::io::Result<()> {
    let n = l.len().min(r.len());
    let data_len = (n * 2 * 4) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&sr.to_le_bytes());
    b.extend_from_slice(&(sr * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..n {
        b.extend_from_slice(&l[i].to_le_bytes());
        b.extend_from_slice(&r[i].to_le_bytes());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, b)
}

/// The test programme every render uses: a log sine sweep, a pink-ish noise burst, kick-like
/// thumps and a stereo chord, at about -12 dBFS RMS. Deterministic.
pub fn test_signal(sr: f64, seconds: f64) -> (Vec<f32>, Vec<f32>) {
    let n = (sr * seconds) as usize;
    let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
    let mut seed = 0x1234_5678u32;
    let mut pink = 0.0f32;
    let mut phase = 0.0f64;
    for i in 0..n {
        let t = i as f64 / sr;
        let u = t / seconds;
        // sweep 40 Hz .. 16 kHz over the whole programme
        let f = 40.0 * (16_000.0f64 / 40.0).powf(u);
        phase += 2.0 * std::f64::consts::PI * f / sr;
        let sweep = 0.18 * phase.sin() as f32;
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let white = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        pink = 0.97 * pink + 0.03 * white;
        let burst = if (t % 1.0) < 0.25 { 0.6 * pink + 0.05 * white } else { 0.0 };
        let beat = t % 0.5;
        let kick = (-beat * 18.0).exp() as f32 * (2.0 * std::f64::consts::PI * (50.0 + 90.0 * (-beat * 30.0).exp()) * beat).sin() as f32 * 0.55;
        let chord = |hz: f64| (2.0 * std::f64::consts::PI * hz * t).sin() as f32;
        let (cl, cr) = (0.06 * (chord(220.0) + chord(277.18)), 0.06 * (chord(329.63) + chord(220.0 * 1.001)));
        l[i] = sweep + burst + kick + cl;
        r[i] = sweep * 0.9 + burst * 0.8 + kick + cr;
    }
    (l, r)
}

/// The plugin-level tests. See the module docs.
#[macro_export]
macro_rules! conformance_tests {
    ($krate:ident, $plugin:ty, $params:expr, heavy = [$(($hn:expr, $ht:expr)),* $(,)?], neutral = [$(($nn:expr, $nt:expr)),* $(,)?], null_tol = $tol:expr) => {
        use apf_test::{allocations_in_process, CountingAllocator, Event, Instance};

        #[global_allocator]
        static ALLOCATOR: CountingAllocator = CountingAllocator;

        fn plugin(sr: f64, max_frames: u32) -> Instance {
            let mut p = Instance::load($krate::apf_entry_init, $krate::apf_entry_get_factory);
            p.activate(sr, max_frames);
            p
        }

        fn sine(n: usize, hz: f64, sr: f64, amp: f32) -> Vec<f32> {
            (0..n).map(|i| (amp as f64 * (2.0 * std::f64::consts::PI * hz * i as f64 / sr).sin()) as f32).collect()
        }

        fn heavy(p: &mut Instance) {
            $( p.set_text($hn, $ht); )*
        }

        fn ids(p: &Instance) -> Vec<u32> {
            (0..p.param_count()).map(|i| p.param_info(i).id).collect()
        }

        /// A tiny deterministic generator for random host values.
        fn rng(seed: &mut u64) -> f64 {
            *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
        }

        fn is_bypass(p: &Instance, i: u32) -> bool {
            let info = p.param_info(i);
            info.flags & clap_sys::ext::params::CLAP_PARAM_IS_BYPASS != 0
        }

        #[test]
        fn every_control_is_an_automatable_visible_parameter_with_a_unique_id() {
            let p = plugin(48_000.0, 512);
            assert_eq!(p.param_count() as usize, $params.len());
            let mut seen = std::collections::HashSet::new();
            for i in 0..p.param_count() {
                let info = p.param_info(i);
                assert!(seen.insert(info.id), "duplicate host id");
                assert!(info.flags & clap_sys::ext::params::CLAP_PARAM_IS_AUTOMATABLE != 0, "param {i} is not automatable");
                assert!(info.flags & clap_sys::ext::params::CLAP_PARAM_IS_HIDDEN == 0, "param {i} is hidden");
            }
        }

        #[test]
        fn latency_is_reported_exactly_and_bypass_is_sample_aligned() {
            for sr in [44_100.0, 48_000.0, 96_000.0] {
                let mut p = plugin(sr, 1024);
                let lat = p.latency() as usize;
                heavy(&mut p);
                p.set_text("Bypass", "On");
                let n = 48_000 + lat;
                let mut x = vec![0.0f32; n];
                x[9000] = 1.0;
                let out = p.render_effect(&[], &x, &x, 480);
                let (idx, v) = out.left.iter().enumerate().skip(2000).fold((0, 0.0f32), |m, (i, v)| if v.abs() > m.1 { (i, v.abs()) } else { m });
                assert_eq!(idx, 9000 + lat, "{sr}: impulse came back at {idx}, latency says {lat}");
                assert!(v > 0.99, "{v}");
                assert_eq!(p.latency() as usize, lat, "latency changed with the settings");
            }
        }

        #[test]
        fn mix_at_zero_is_the_dry_signal_delayed_by_the_latency() {
            let sr = 48_000.0;
            let mut p = plugin(sr, 512);
            heavy(&mut p);
            p.set_text("Mix", "0 %");
            let x = sine(48_000, 330.0, sr, 0.25);
            let out = p.render_effect(&[], &x, &x, 512);
            let lat = p.latency() as usize;
            let mut worst = 0.0f32;
            for i in (lat + 12_000)..x.len() {
                worst = worst.max((out.left[i] - x[i - lat]).abs()).max((out.right[i] - x[i - lat]).abs());
            }
            assert!(worst < 2e-3, "mix 0 differs from the delayed dry by {worst}");
        }

        #[test]
        fn neutral_settings_null_against_the_delayed_input() {
            let neutral: &[(&str, &str)] = &[$(($nn, $nt)),*];
            if neutral.is_empty() {
                return;
            }
            let sr = 48_000.0;
            let mut p = plugin(sr, 512);
            for (n, t) in neutral {
                p.set_text(n, t);
            }
            let x = sine(48_000, 440.0, sr, 0.25);
            let out = p.render_effect(&[], &x, &x, 512);
            let lat = p.latency() as usize;
            let mut worst = 0.0f32;
            for i in (lat + 12_000)..x.len() {
                worst = worst.max((out.left[i] - x[i - lat]).abs());
            }
            assert!(worst < $tol, "neutral settings differ from the delayed input by {worst}");
        }

        #[test]
        fn process_never_allocates_even_with_every_parameter_automated() {
            let sr = 48_000.0;
            let mut p = plugin(sr, 512);
            heavy(&mut p);
            let ids = ids(&p);
            let x = sine(48_000, 220.0, sr, 0.5);
            let mut seed = 7u64;
            // warm up (first blocks may touch lazily-initialised OS state in the allocator hook)
            let _ = p.render_effect(&[], &x[..4096], &x[..4096], 512);
            let before = allocations_in_process();
            for block in [32usize, 100, 480, 512] {
                let events: Vec<Event> = ids.iter().enumerate().filter(|(i, _)| !is_bypass(&p, *i as u32)).map(|(k, id)| Event::param((k % 30) as u32, *id, rng(&mut seed))).collect();
                let _ = p.render_effect(&events, &x, &x, block);
            }
            assert_eq!(allocations_in_process(), before, "process() allocated");
        }

        #[test]
        fn state_round_trips_every_parameter() {
            let sr = 48_000.0;
            let mut a = plugin(sr, 512);
            let ids = ids(&a);
            let mut seed = 99u64;
            let events: Vec<Event> = ids.iter().map(|id| Event::param(0, *id, rng(&mut seed))).collect();
            let x = vec![0.0f32; 512];
            let _ = a.render_effect(&events, &x, &x, 512);
            let blob = a.save_state();
            assert!(!blob.is_empty());
            let mut b = plugin(44_100.0, 256);
            assert!(b.load_state(blob.clone()));
            for id in &ids {
                assert!((a.host_value(*id) - b.host_value(*id)).abs() < 1e-9, "param {id} differs after load");
            }
            assert_eq!(b.save_state(), blob, "saving again changed the state");
            let mut c = plugin(sr, 512);
            assert!(!c.load_state(vec![1, 2, 3, 4, 5]), "garbage was accepted");
        }

        #[test]
        fn stable_finite_and_denormal_free_at_44_48_96k_and_blocks_1_to_4096() {
            for sr in [44_100.0, 48_000.0, 96_000.0] {
                for frames in [1u32, 16, 64, 512, 4096] {
                    let mut p = plugin(sr, frames);
                    heavy(&mut p);
                    let n = if frames == 1 { 6000 } else { 12_000 };
                    let mut x = sine(n, 110.0, sr, 0.7);
                    for (i, v) in x.iter_mut().enumerate().skip(n / 3).take(200) {
                        *v = if i % 2 == 0 { 0.99 } else { -0.99 };
                    }
                    x.extend(std::iter::repeat(0.0).take(n));
                    let out = p.render_effect(&[], &x, &x, frames as usize);
                    for v in out.left.iter().chain(&out.right) {
                        assert!(v.is_finite(), "{sr} {frames}: non-finite output");
                        assert!(*v == 0.0 || v.abs() >= f32::MIN_POSITIVE, "{sr} {frames}: denormal output {v:e}");
                        assert!(v.abs() < 16.0, "{sr} {frames}: runaway output {v}");
                    }
                }
            }
        }

        #[test]
        fn every_factory_preset_renders_a_sane_signal() {
            use apf::plugin::Plugin;
            let sr = 48_000.0;
            let defs = apf::Params::new($params);
            let (xl, xr) = $crate::testing::test_signal(sr, 3.0);
            let in_rms = apf_test::rms(&xl);
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/renders").join(stringify!($krate));
            for text in <$plugin as Plugin>::FACTORY_PRESETS {
                let preset = apf::preset::Preset::parse(text).expect("preset parses");
                let mut p = plugin(sr, 512);
                let ids = ids(&p);
                let events: Vec<Event> = preset.host_values(&defs).iter().zip(&ids).map(|(v, id)| Event::param(0, *id, *v)).collect();
                let out = p.render_effect(&events, &xl, &xr, 512);
                assert!(out.left.iter().chain(&out.right).all(|v| v.is_finite()), "{}: non-finite", preset.name);
                let peak = out.left.iter().chain(&out.right).fold(0.0f32, |m, v| m.max(v.abs()));
                let rms = apf_test::rms(&out.left);
                assert!(peak < 4.0, "{}: peak {peak}", preset.name);
                assert!(rms > in_rms * 0.03, "{}: nearly silent (rms {rms} vs input {in_rms})", preset.name);
                let _ = $crate::testing::write_wav(&dir.join(format!("{}.wav", preset.name)), sr as u32, &out.left, &out.right);
            }
            let _ = $crate::testing::write_wav(&dir.join("_input.wav"), sr as u32, &xl, &xr);
        }

        #[test]
        fn every_factory_preset_is_valid() {
            use apf::json::{self, Value};
            use apf::plugin::Plugin;
            let mut names = std::collections::HashSet::new();
            assert!(<$plugin as Plugin>::FACTORY_PRESETS.len() >= 8, "a plugin ships at least 8 presets");
            for text in <$plugin as Plugin>::FACTORY_PRESETS {
                let p = apf::preset::Preset::parse(text).expect("preset parses");
                assert!(!p.name.is_empty());
                assert!(names.insert(p.name.clone()), "duplicate preset name {}", p.name);
                assert_eq!(p.plugin.as_deref(), Some(<$plugin as Plugin>::INFO.id), "{}: written for another plugin", p.name);
                let root = json::parse(text).unwrap();
                let Some(Value::Obj(members)) = root.get("params") else { panic!("{}: no params", p.name) };
                for (key, value) in members {
                    let def = $params.iter().find(|d| d.key == key).unwrap_or_else(|| panic!("{}: unknown parameter {key}", p.name));
                    match value {
                        Value::Num(n) => {
                            if let apf::ParamKind::Float { min, max, .. } = def.kind {
                                assert!(*n >= min - 1e-9 && *n <= max + 1e-9, "{}: {key}={n} is outside {min}..{max}", p.name);
                            }
                        }
                        Value::Str(s) => assert!(def.parse(s).is_some(), "{}: {key}={s:?} is not an option", p.name),
                        Value::Bool(_) => assert!(matches!(def.kind, apf::ParamKind::Bool { .. }), "{}: {key} is not a switch", p.name),
                        other => panic!("{}: {key} has an odd value {other:?}", p.name),
                    }
                }
            }
        }
    };
}

/// The page and the plugin agree: every key the page binds is a parameter, and every parameter
/// has a control on the page (bound by attribute, by a knob spec `k: "key"`, or named in the
/// page's script).
#[macro_export]
macro_rules! ui_binds_tests {
    ($params:expr, $html:expr, $js:expr) => {
        fn read(rel: &str) -> String {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
            std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        }
        thread_local! {
            static HTML_S: String = read($html);
            static JS_S: String = read($js);
            static KIT_S: String = read("../../ui-kit/kit.js");
        }
        fn html() -> String { HTML_S.with(|s| s.clone()) }
        fn js() -> String { JS_S.with(|s| s.clone()) }
        fn kit() -> String { KIT_S.with(|s| s.clone()) }

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

        fn bound() -> Vec<String> {
            let (h, j) = (html(), js());
            let mut used = Vec::new();
            for m in ["data-param=\"", "data-param-text=\"", "data-param-name=\""] {
                used.extend(keys_after(&h, m));
            }
            used.extend(keys_after(&j, "k: \""));
            used.retain(|k| !k.contains('$') && !k.is_empty());
            used
        }

        #[test]
        fn every_key_the_page_binds_is_a_real_parameter() {
            let used = bound();
            assert!(!used.is_empty());
            for k in used {
                assert!($params.iter().any(|p| p.key == k), "the page binds {k:?}, which is not a parameter");
            }
        }

        #[test]
        fn every_parameter_has_a_control_on_the_page() {
            let used = bound();
            let (h, j, k) = (html(), js(), kit());
            for p in $params {
                let quoted = format!("\"{}\"", p.key);
                let in_js = j.contains(&quoted) || h.contains(&quoted) || (p.key == "bypass" && k.contains("bypassKey"));
                assert!(used.iter().any(|k| k == p.key) || in_js, "parameter {} has no control on the page", p.key);
            }
        }

        #[test]
        fn the_page_loads_the_kit() {
            let h = html();
            assert!(h.contains("/__apf/apf.js"));
            assert!(h.contains("kit.js") && h.contains("kit.css"));
        }
    };
}
