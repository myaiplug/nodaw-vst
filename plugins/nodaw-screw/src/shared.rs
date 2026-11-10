//! State shared between the audio thread and the web view's controller. Everything the audio
//! thread touches is an atomic: it never waits for the UI.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use apf::json::{self, Value};
use apf::param::{Curve, ParamKind};
use apf::prelude::*;
use nodaw_dsp::chop::{self, Pattern, STEPS};
use nodaw_dsp::fft::SpectrumAnalyzer;
use nodaw_dsp::meter::{GainReduction, LevelMeter, ScopeRing};

use crate::license::{self, Manager};

/// Bands the analyser sends to the page.
pub const BANDS: usize = 160;
/// Floats before the spectra in a frame; see [`Controller::request`] "frame".
pub const HEADER: usize = 16;
const FFT: usize = 8192;
const F_LO: f32 = 20.0;
const F_HI: f32 = 20_000.0;
/// What an untouched Custom pattern holds (the "Trap Roll" factory pattern).
const DEFAULT_CUSTOM: usize = 5;

/// Written by the audio thread, read by the controller.
pub struct Shared {
    pub in_meter: LevelMeter,
    pub out_meter: LevelMeter,
    pub gr: GainReduction,
    pub in_scope: ScopeRing,
    pub out_scope: ScopeRing,
    /// The chop step at the end of the last block (255: gate not running).
    pub step: AtomicU8,
    pub playing: AtomicU8,
    pub sample_rate: AtomicU32,
    pub latency: AtomicU32,
    /// The Custom chop pattern (levels 0 ..= 2).
    pub custom: [AtomicU8; STEPS],
    /// Bumped by every edit of the controller's state.
    pub revision: AtomicU64,
    /// Trial / license state. The audio thread only reads its atomic `demo_flag()`.
    pub license: Arc<Manager>,
}

impl Shared {
    pub fn new() -> Self {
        Self::with_license(license::load_default())
    }

    pub fn with_license(license: Arc<Manager>) -> Self {
        let s = Self {
            in_meter: LevelMeter::default(),
            out_meter: LevelMeter::default(),
            gr: GainReduction::default(),
            in_scope: ScopeRing::new(FFT * 2),
            out_scope: ScopeRing::new(FFT * 2),
            step: AtomicU8::new(255),
            playing: AtomicU8::new(0),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            latency: AtomicU32::new(0),
            custom: std::array::from_fn(|_| AtomicU8::new(2)),
            revision: AtomicU64::new(0),
            license,
        };
        s.set_custom(&chop::PATTERNS[DEFAULT_CUSTOM].1);
        s
    }

    pub fn set_custom(&self, p: &Pattern) {
        for (a, v) in self.custom.iter().zip(p) {
            a.store((*v).min(2), Ordering::Relaxed);
        }
        self.revision.fetch_add(1, Ordering::AcqRel);
    }

    /// Audio thread: the custom pattern, without allocating.
    pub fn custom_pattern(&self) -> Pattern {
        std::array::from_fn(|i| self.custom[i].load(Ordering::Relaxed).min(2))
    }
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

struct Analysis {
    fft_in: SpectrumAnalyzer,
    fft_out: SpectrumAnalyzer,
    buf: Vec<f32>,
}

/// Answers the page and keeps the session state that is not a parameter: the Custom chop
/// pattern and the page's A/B compare slots.
pub struct ScrewController {
    shared: Arc<Shared>,
    host: Option<HostHandle>,
    ab: Mutex<String>,
    analysis: Mutex<Analysis>,
}

impl ScrewController {
    pub fn new(shared: Arc<Shared>, host: Option<HostHandle>) -> Self {
        Self {
            shared,
            host,
            ab: Mutex::new("null".into()),
            analysis: Mutex::new(Analysis {
                fft_in: SpectrumAnalyzer::new(FFT),
                fft_out: SpectrumAnalyzer::new(FFT),
                buf: vec![0.0; FFT],
            }),
        }
    }

    fn dirty(&self) {
        if let Some(h) = &self.host {
            h.mark_dirty();
        }
    }

    fn steps_json(&self) -> String {
        let v: Vec<String> = self
            .shared
            .custom_pattern()
            .iter()
            .map(|x| x.to_string())
            .collect();
        format!("[{}]", v.join(","))
    }

    fn state_json(&self) -> String {
        let ab = self
            .ab
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| "null".into());
        format!("{{\"steps\":{},\"ab\":{}}}", self.steps_json(), ab)
    }

    fn parse_steps(v: &Value) -> Result<Pattern, String> {
        let Value::Arr(items) = v else {
            return Err("steps must be an array".into());
        };
        if items.len() != STEPS {
            return Err(format!("steps needs {STEPS} entries"));
        }
        let mut p: Pattern = [2; STEPS];
        for (slot, it) in p.iter_mut().zip(items) {
            let f = it.as_f64().ok_or("steps are numbers")?;
            *slot = f.round().clamp(0.0, 2.0) as u8;
        }
        Ok(p)
    }

    /// The binary frame: `HEADER` little-endian f32, then `BANDS` input and `BANDS` output
    /// levels in dBFS.
    ///
    /// Header: 0..4 input peak L/R, output peak L/R; 4..8 input RMS L/R, output RMS L/R;
    /// 8 limiter gain (linear); 9 chop step (255: none); 10 sample rate; 11 latency (samples);
    /// 12 transport playing.
    fn frame(&self) -> Vec<u8> {
        let sh = &self.shared;
        let (i, o) = (sh.in_meter.take(), sh.out_meter.take());
        let mut head = [0.0f32; HEADER];
        head[0] = i[0];
        head[1] = i[1];
        head[2] = o[0];
        head[3] = o[1];
        head[4] = i[2];
        head[5] = i[3];
        head[6] = o[2];
        head[7] = o[3];
        head[8] = sh.gr.take();
        head[9] = sh.step.load(Ordering::Relaxed) as f32;
        let sr = f32::from_bits(sh.sample_rate.load(Ordering::Relaxed));
        head[10] = sr;
        head[11] = sh.latency.load(Ordering::Relaxed) as f32;
        head[12] = sh.playing.load(Ordering::Relaxed) as f32;
        let mut bands = [[0.0f32; BANDS]; 2];
        if let Ok(mut a) = self.analysis.lock() {
            let a = &mut *a;
            sh.in_scope.snapshot(&mut a.buf);
            a.fft_in.analyze(&a.buf);
            a.fft_in.log_bands(sr as f64, F_LO, F_HI, &mut bands[0]);
            sh.out_scope.snapshot(&mut a.buf);
            a.fft_out.analyze(&a.buf);
            a.fft_out.log_bands(sr as f64, F_LO, F_HI, &mut bands[1]);
        }
        let mut out = Vec::with_capacity((HEADER + 2 * BANDS) * 4);
        for v in head.iter().chain(bands[0].iter()).chain(bands[1].iter()) {
            out.extend_from_slice(&(if v.is_finite() { *v } else { -120.0 }).to_le_bytes());
        }
        out
    }
}

impl Controller for ScrewController {
    fn save(&self) -> String {
        self.state_json()
    }

    fn load(&self, text: &str) -> Result<(), String> {
        let root = json::parse(text)?;
        let steps = match root.get("steps") {
            Some(v) => Some(Self::parse_steps(v)?),
            None => None,
        };
        // A preset without A/B slots leaves the session's slots alone.
        let ab = root.get("ab").map(json_text);
        if let Some(p) = steps {
            self.shared.set_custom(&p);
        }
        if let Some(ab) = ab {
            if let Ok(mut g) = self.ab.lock() {
                *g = ab;
            }
            self.shared.revision.fetch_add(1, Ordering::AcqRel);
        }
        Ok(())
    }

    fn reset(&self) {
        self.shared.set_custom(&chop::PATTERNS[DEFAULT_CUSTOM].1);
    }

    fn revision(&self) -> u64 {
        self.shared.revision.load(Ordering::Acquire)
    }

    fn request(&self, r: &ControllerRequest) -> ControllerResponse {
        match r.path {
            "frame" => ControllerResponse::new(200, "application/octet-stream", self.frame()),
            "steps" => {
                if r.is_post() {
                    let parsed = json::parse(r.body_text().unwrap_or(""))
                        .and_then(|v| Self::parse_steps(&v));
                    match parsed {
                        Ok(p) => {
                            self.shared.set_custom(&p);
                            self.dirty();
                        }
                        Err(e) => {
                            return ControllerResponse::new(400, "text/plain", e.into_bytes())
                        }
                    }
                }
                ControllerResponse::json(self.steps_json())
            }
            "ab" => {
                if r.is_post() {
                    let text = r.body_text().unwrap_or("").to_string();
                    if let Err(e) = json::parse(&text) {
                        return ControllerResponse::new(400, "text/plain", e.into_bytes());
                    }
                    if let Ok(mut g) = self.ab.lock() {
                        // Not a revision bump for the preset bar's "changed" mark: slots are
                        // not part of the sound. The host still hears that the session changed.
                        *g = text;
                    }
                    self.dirty();
                }
                let ab = self
                    .ab
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_else(|_| "null".into());
                ControllerResponse::json(ab)
            }
            "license" => {
                let now = license::now_secs();
                self.shared.license.maybe_recheck(now);
                ControllerResponse::json(self.shared.license.to_json(now))
            }
            "license-activate" => {
                if !r.is_post() {
                    return ControllerResponse::new(405, "text/plain", b"POST".to_vec());
                }
                let key = json::parse(r.body_text().unwrap_or(""))
                    .ok()
                    .and_then(|v| v.get("key").and_then(Value::as_str).map(str::to_string))
                    .unwrap_or_default();
                if let Err(e) = self.shared.license.activate(&key) {
                    return ControllerResponse::new(400, "text/plain", e.into_bytes());
                }
                ControllerResponse::json(self.shared.license.to_json(license::now_secs()))
            }
            "license-buy" => {
                let ok = r.is_post() && license::open_buy_page();
                ControllerResponse::json(format!(
                    "{{\"opened\":{ok},\"url\":\"{}\"}}",
                    license::BUY_URL
                ))
            }
            "ranges" => ControllerResponse::json(ranges_json()),
            "patterns" => ControllerResponse::json(patterns_json()),
            _ => ControllerResponse::not_found(),
        }
    }
}

/// How each parameter maps its host value (0..1) to the plain value the user sees, so the
/// page can draw the filter curve and place its handles without a copy of the table. Floats
/// only; the page reads units and names from the framework.
pub fn ranges_json() -> String {
    let mut o = String::from("{");
    let mut first = true;
    for def in crate::PARAMS {
        if let ParamKind::Float {
            min,
            max,
            default,
            curve,
            decimals,
        } = def.kind
        {
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

/// The factory chop patterns, for the step display.
pub fn patterns_json() -> String {
    let mut o = String::from("[");
    for (i, (name, steps)) in chop::PATTERNS.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let v: Vec<String> = steps.iter().map(|x| x.to_string()).collect();
        o.push_str(&format!(
            "{{\"name\":\"{name}\",\"steps\":[{}]}}",
            v.join(",")
        ));
    }
    o.push(']');
    o
}

/// Re-serialise a parsed JSON value (the A/B slots are opaque to the plugin).
fn json_text(v: &Value) -> String {
    let mut o = String::new();
    write_value(&mut o, v);
    o
}

fn write_value(o: &mut String, v: &Value) {
    match v {
        Value::Null => o.push_str("null"),
        Value::Bool(b) => o.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => json::write_num(o, *n),
        Value::Str(s) => json::write_str(o, s),
        Value::Arr(a) => {
            o.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    o.push(',');
                }
                write_value(o, x);
            }
            o.push(']');
        }
        Value::Obj(m) => {
            o.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    o.push(',');
                }
                json::write_str(o, k);
                o.push(':');
                write_value(o, x);
            }
            o.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller() -> ScrewController {
        let lic = Manager::load(
            &license::PRODUCT,
            license::Store::default(),
            license::now_secs(),
            |_, _| license::Verdict::Unreachable("test".into()),
        );
        ScrewController::new(Arc::new(Shared::with_license(lic)), None)
    }

    fn call(c: &ScrewController, method: &str, path: &str, body: &str) -> ControllerResponse {
        c.request(&ControllerRequest {
            method,
            path,
            query: "",
            body: body.as_bytes(),
        })
    }

    #[test]
    fn the_session_state_round_trips_and_bumps_the_revision() {
        let a = controller();
        let r0 = a.revision();
        let r = call(&a, "POST", "steps", "[2,2,2,2,0,0,0,0,1,1,1,1,2,0,2,0]");
        assert_eq!(r.status, 200);
        assert!(a.revision() > r0);
        call(
            &a,
            "POST",
            "ab",
            "{\"active\":\"B\",\"A\":{\"pitch\":-3},\"B\":null}",
        );
        let saved = a.save();
        let b = controller();
        b.load(&saved).unwrap();
        assert_eq!(b.save(), saved);
        assert_eq!(
            b.shared.custom_pattern(),
            [2, 2, 2, 2, 0, 0, 0, 0, 1, 1, 1, 1, 2, 0, 2, 0]
        );
        assert!(saved.contains("\"active\":\"B\""));
    }

    #[test]
    fn bad_input_is_refused_and_changes_nothing() {
        let c = controller();
        let before = c.save();
        assert_eq!(call(&c, "POST", "steps", "[1,2,3]").status, 400);
        assert_eq!(call(&c, "POST", "steps", "not json").status, 400);
        assert_eq!(call(&c, "POST", "ab", "{oops").status, 400);
        assert!(c.load("{\"steps\": \"no\"}").is_err());
        assert_eq!(c.save(), before);
        assert_eq!(call(&c, "GET", "nothing", "").status, 404);
    }

    #[test]
    fn the_license_endpoints_answer_without_blocking() {
        let c = controller();
        let r = call(&c, "GET", "license", "");
        assert_eq!(r.status, 200);
        let v = json::parse(std::str::from_utf8(&r.body).unwrap()).unwrap();
        assert_eq!(v.get("state").and_then(Value::as_str), Some("trial"));
        assert_eq!(v.get("days_left").and_then(Value::as_f64), Some(14.0));
        assert_eq!(
            call(&c, "POST", "license-activate", "{\"key\":\"x\"}").status,
            400
        );
        assert_eq!(call(&c, "GET", "license-activate", "").status, 405);
    }

    #[test]
    fn ranges_and_patterns_are_valid_json_that_match_the_parameter_table() {
        let r = json::parse(&ranges_json()).unwrap();
        for def in crate::PARAMS {
            if let ParamKind::Float { min, max, .. } = def.kind {
                let e = r
                    .get(def.key)
                    .unwrap_or_else(|| panic!("{} missing", def.key));
                assert_eq!(e.get("min").and_then(|v| v.as_f64()), Some(min));
                assert_eq!(e.get("max").and_then(|v| v.as_f64()), Some(max));
                let mid = e.get("mid").and_then(|v| v.as_f64()).unwrap();
                assert!((mid - def.host_to_plain(0.37)).abs() < 1e-9);
            }
        }
        let p = json::parse(&patterns_json()).unwrap();
        let Value::Arr(items) = p else { panic!() };
        assert_eq!(items.len(), chop::PATTERNS.len());
    }

    #[test]
    fn levels_outside_the_range_are_clamped() {
        let c = controller();
        call(&c, "POST", "steps", "[9,-4,2.4,0,0,0,0,0,0,0,0,0,0,0,0,0]");
        assert_eq!(&c.shared.custom_pattern()[..3], &[2, 0, 2]);
    }

    #[test]
    fn a_preset_without_ab_slots_keeps_the_session_slots_and_reset_restores_the_pattern() {
        let c = controller();
        call(&c, "POST", "ab", "{\"x\":1}");
        c.load("{\"steps\":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}")
            .unwrap();
        assert!(c.save().contains("\"x\":1"));
        assert_eq!(c.shared.custom_pattern(), [0; STEPS]);
        c.reset();
        assert_eq!(c.shared.custom_pattern(), chop::PATTERNS[5].1);
    }

    #[test]
    fn the_frame_has_the_documented_size_and_finite_levels() {
        let c = controller();
        c.shared.in_meter.record(&[0.5; 64], &[0.25; 64]);
        c.shared.in_scope.push_stereo(&[0.3; 9000], &[0.3; 9000]);
        let f = c.frame();
        assert_eq!(f.len(), (HEADER + 2 * BANDS) * 4);
        let v: Vec<f32> = f
            .chunks(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert!(v.iter().all(|x| x.is_finite()));
        assert_eq!((v[0], v[1]), (0.5, 0.25));
        assert_eq!(v[10], 48_000.0);
        // taking resets the meter
        let v2: Vec<f32> = c
            .frame()
            .chunks(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(v2[0], 0.0);
    }
}
