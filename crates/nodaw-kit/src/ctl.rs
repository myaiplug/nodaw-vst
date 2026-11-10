//! The web view's controller, shared by every NoDAW plugin (the same endpoints and frame layout
//! as NoDAW Screw's, so the UI kit reads them the same way), plus [`Ext`] for each plugin's own.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use apf::json::{self, Value};
use apf::param::ParamDef;
use apf::prelude::*;
use nodaw_dsp::fft::SpectrumAnalyzer;
use nodaw_dsp::meter::{GainReduction, LevelMeter, ScopeRing};

use crate::license::{self, Manager, Product};

/// Bands the analyser sends to the page.
pub const BANDS: usize = 160;
/// Floats before the spectra in a frame; see [`KitController`] "frame".
pub const HEADER: usize = 16;
/// Header slots a plugin may fill itself ([`Meters::set_custom`]): header 9, 13, 14, 15.
pub const CUSTOM_SLOTS: [usize; 4] = [9, 13, 14, 15];
const FFT: usize = 8192;
const F_LO: f32 = 20.0;
const F_HI: f32 = 20_000.0;

/// Written by the audio thread (atomics only), read by the controller.
pub struct Meters {
    pub in_meter: LevelMeter,
    pub out_meter: LevelMeter,
    pub gr: GainReduction,
    pub in_scope: ScopeRing,
    pub out_scope: ScopeRing,
    pub playing: AtomicU8,
    pub sample_rate: AtomicU32,
    pub latency: AtomicU32,
    custom: [AtomicU32; 4],
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            in_meter: LevelMeter::default(),
            out_meter: LevelMeter::default(),
            gr: GainReduction::default(),
            in_scope: ScopeRing::new(FFT * 2),
            out_scope: ScopeRing::new(FFT * 2),
            playing: AtomicU8::new(0),
            sample_rate: AtomicU32::new(48_000f32.to_bits()),
            latency: AtomicU32::new(0),
            custom: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

impl Meters {
    /// Audio thread, at the end of a block: input and output levels and scopes.
    pub fn record(&self, in_l: &[f32], in_r: &[f32], out_l: &[f32], out_r: &[f32]) {
        self.in_meter.record(in_l, in_r);
        self.in_scope.push_stereo(in_l, in_r);
        self.out_meter.record(out_l, out_r);
        self.out_scope.push_stereo(out_l, out_r);
    }

    /// Audio thread, before processing: the input level and scope.
    pub fn record_in(&self, l: &[f32], r: &[f32]) {
        self.in_meter.record(l, r);
        self.in_scope.push_stereo(l, r);
    }

    /// Audio thread, after processing: the output level and scope.
    pub fn record_out(&self, l: &[f32], r: &[f32]) {
        self.out_meter.record(l, r);
        self.out_scope.push_stereo(l, r);
    }

    /// One of the plugin's own header values (`i` in 0..4; see [`CUSTOM_SLOTS`]).
    pub fn set_custom(&self, i: usize, v: f32) {
        if let Some(a) = self.custom.get(i) {
            a.store(v.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn custom(&self, i: usize) -> f32 {
        self.custom.get(i).map(|a| f32::from_bits(a.load(Ordering::Relaxed))).unwrap_or(0.0)
    }

    pub fn sample_rate(&self) -> f32 {
        f32::from_bits(self.sample_rate.load(Ordering::Relaxed))
    }
}

/// A fixed set of f32 values shared lock-free between the audio thread and the page: per-band
/// gain reduction, module levels, a captured spectrum.
pub struct Slots {
    v: Box<[AtomicU32]>,
}

impl Slots {
    /// `n` slots at 0. Main thread.
    pub fn new(n: usize) -> Self {
        Self { v: (0..n).map(|_| AtomicU32::new(0)).collect() }
    }
    pub fn len(&self) -> usize {
        self.v.len()
    }
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
    pub fn set(&self, i: usize, x: f32) {
        if let Some(a) = self.v.get(i) {
            a.store(x.to_bits(), Ordering::Relaxed);
        }
    }
    pub fn get(&self, i: usize) -> f32 {
        self.v.get(i).map(|a| f32::from_bits(a.load(Ordering::Relaxed))).unwrap_or(0.0)
    }
    /// Keep the largest non-negative value since the last [`Slots::take`] (peak meters).
    pub fn max(&self, i: usize, x: f32) {
        if let Some(a) = self.v.get(i) {
            if x.is_finite() && x > 0.0 {
                // Non-negative floats order like their bit patterns.
                a.fetch_max(x.to_bits(), Ordering::Relaxed);
            }
        }
    }
    /// Read and clear.
    pub fn take(&self, i: usize) -> f32 {
        self.v.get(i).map(|a| f32::from_bits(a.swap(0, Ordering::Relaxed))).unwrap_or(0.0)
    }
    /// Every value, in order.
    pub fn read_all(&self, out: &mut Vec<f32>) {
        out.extend(self.v.iter().map(|a| f32::from_bits(a.load(Ordering::Relaxed))));
    }
}

/// A plugin's own part of the controller. Every method has a default, so `()` is a valid
/// extension for a plugin that only needs the shared endpoints.
pub trait Ext: Send + Sync + 'static {
    /// Floats appended to every frame, after the two spectra.
    fn frame_extra(&self, _out: &mut Vec<f32>) {}
    /// Answer a path the kit does not know; `None` for 404.
    fn request(&self, _r: &ControllerRequest) -> Option<ControllerResponse> {
        None
    }
    /// The plugin's session state (not parameters) as JSON, saved under `"x"`.
    fn save(&self) -> Option<String> {
        None
    }
    /// Restore what [`Ext::save`] wrote; `None` when the state has no `"x"` (an older preset).
    /// Validate everything before changing anything.
    fn load(&self, _v: Option<&Value>) -> Result<(), String> {
        Ok(())
    }
    /// Back to a new instance's state.
    fn reset(&self) {}
    /// Bumped by every edit of the extension's state.
    fn revision(&self) -> u64 {
        0
    }
}

impl Ext for () {}

struct Analysis {
    fft_in: SpectrumAnalyzer,
    fft_out: SpectrumAnalyzer,
    buf: Vec<f32>,
}

/// Answers the page: the meter frame, A/B slots, the license, the range table, and whatever the
/// plugin's [`Ext`] adds.
pub struct KitController<E: Ext> {
    pub meters: Arc<Meters>,
    pub license: Arc<Manager>,
    pub ext: Arc<E>,
    params: &'static [ParamDef],
    host: Option<HostHandle>,
    ab: Mutex<String>,
    rev: AtomicU64,
    analysis: Mutex<Analysis>,
}

impl<E: Ext> KitController<E> {
    pub fn new(params: &'static [ParamDef], meters: Arc<Meters>, license: Arc<Manager>, ext: Arc<E>, host: Option<HostHandle>) -> Self {
        Self {
            meters,
            license,
            ext,
            params,
            host,
            ab: Mutex::new("null".into()),
            rev: AtomicU64::new(0),
            analysis: Mutex::new(Analysis { fft_in: SpectrumAnalyzer::new(FFT), fft_out: SpectrumAnalyzer::new(FFT), buf: vec![0.0; FFT] }),
        }
    }

    /// Tell the host the session changed (for the plugin's own endpoints).
    pub fn dirty(&self) {
        if let Some(h) = &self.host {
            h.mark_dirty();
        }
    }

    fn product(&self) -> &'static Product {
        self.license.product()
    }

    /// The binary frame: `HEADER` little-endian f32, `BANDS` input and `BANDS` output levels in
    /// dBFS, then the extension's floats.
    ///
    /// Header: 0..4 input peak L/R, output peak L/R; 4..8 input RMS L/R, output RMS L/R;
    /// 8 gain reduction (linear); 10 sample rate; 11 latency (samples); 12 transport playing;
    /// 9, 13, 14, 15 the plugin's own ([`Meters::set_custom`]).
    pub fn frame(&self) -> Vec<u8> {
        let sh = &self.meters;
        let (i, o) = (sh.in_meter.take(), sh.out_meter.take());
        let mut head = [0.0f32; HEADER];
        head[..2].copy_from_slice(&i[..2]);
        head[2..4].copy_from_slice(&o[..2]);
        head[4..6].copy_from_slice(&i[2..4]);
        head[6..8].copy_from_slice(&o[2..4]);
        head[8] = sh.gr.take();
        let sr = sh.sample_rate();
        head[10] = sr;
        head[11] = sh.latency.load(Ordering::Relaxed) as f32;
        head[12] = sh.playing.load(Ordering::Relaxed) as f32;
        for (k, &slot) in CUSTOM_SLOTS.iter().enumerate() {
            head[slot] = sh.custom(k);
        }
        let mut bands = [[-120.0f32; BANDS]; 2];
        if let Ok(mut a) = self.analysis.lock() {
            let a = &mut *a;
            sh.in_scope.snapshot(&mut a.buf);
            a.fft_in.analyze(&a.buf);
            a.fft_in.log_bands(sr as f64, F_LO, F_HI, &mut bands[0]);
            sh.out_scope.snapshot(&mut a.buf);
            a.fft_out.analyze(&a.buf);
            a.fft_out.log_bands(sr as f64, F_LO, F_HI, &mut bands[1]);
        }
        let mut extra = Vec::new();
        self.ext.frame_extra(&mut extra);
        let mut out = Vec::with_capacity((HEADER + 2 * BANDS + extra.len()) * 4);
        for v in head.iter().chain(bands[0].iter()).chain(bands[1].iter()).chain(extra.iter()) {
            out.extend_from_slice(&(if v.is_finite() { *v } else { -120.0 }).to_le_bytes());
        }
        out
    }

    fn ab_text(&self) -> String {
        self.ab.lock().map(|g| g.clone()).unwrap_or_else(|_| "null".into())
    }
}

impl<E: Ext> Controller for KitController<E> {
    fn save(&self) -> String {
        let mut o = format!("{{\"ab\":{}", self.ab_text());
        if let Some(x) = self.ext.save() {
            o.push_str(",\"x\":");
            o.push_str(&x);
        }
        o.push('}');
        o
    }

    fn load(&self, text: &str) -> Result<(), String> {
        let root = json::parse(text)?;
        self.ext.load(root.get("x"))?;
        // A preset without A/B slots leaves the session's slots alone.
        if let Some(ab) = root.get("ab").map(json_text) {
            if let Ok(mut g) = self.ab.lock() {
                *g = ab;
            }
            self.rev.fetch_add(1, Ordering::AcqRel);
        }
        Ok(())
    }

    fn reset(&self) {
        self.ext.reset();
        self.rev.fetch_add(1, Ordering::AcqRel);
    }

    fn revision(&self) -> u64 {
        self.rev.load(Ordering::Acquire).wrapping_add(self.ext.revision())
    }

    fn request(&self, r: &ControllerRequest) -> ControllerResponse {
        match r.path {
            "frame" => ControllerResponse::new(200, "application/octet-stream", self.frame()),
            "ab" => {
                if r.is_post() {
                    let text = r.body_text().unwrap_or("").to_string();
                    if let Err(e) = json::parse(&text) {
                        return ControllerResponse::new(400, "text/plain", e.into_bytes());
                    }
                    if let Ok(mut g) = self.ab.lock() {
                        // Not a revision bump: slots are not part of the sound.
                        *g = text;
                    }
                    self.dirty();
                }
                ControllerResponse::json(self.ab_text())
            }
            "license" => {
                let now = license::now_secs();
                self.license.maybe_recheck(now);
                ControllerResponse::json(self.license.to_json(now))
            }
            "license-activate" => {
                if !r.is_post() {
                    return ControllerResponse::new(405, "text/plain", b"POST".to_vec());
                }
                let key = json::parse(r.body_text().unwrap_or("")).ok().and_then(|v| v.get("key").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
                if let Err(e) = self.license.activate(&key) {
                    return ControllerResponse::new(400, "text/plain", e.into_bytes());
                }
                ControllerResponse::json(self.license.to_json(license::now_secs()))
            }
            "license-buy" => {
                let url = self.product().buy_url;
                let ok = r.is_post() && license::open_buy_page(url);
                let mut o = format!("{{\"opened\":{ok},\"url\":");
                json::write_str(&mut o, url);
                o.push('}');
                ControllerResponse::json(o)
            }
            "ranges" => ControllerResponse::json(crate::ranges_json(self.params)),
            _ => self.ext.request(r).unwrap_or_else(ControllerResponse::not_found),
        }
    }
}

/// Re-serialise a parsed JSON value.
pub fn json_text(v: &Value) -> String {
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

/// A float array as JSON (`[1,2.5,...]`).
pub fn floats_json(v: &[f32]) -> String {
    let mut o = String::from("[");
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        json::write_num(&mut o, if x.is_finite() { *x as f64 } else { 0.0 });
    }
    o.push(']');
    o
}

/// Read a JSON array of numbers into `out` (exactly `out.len()` entries).
pub fn parse_floats(v: &Value, out: &mut [f32]) -> Result<(), String> {
    let Value::Arr(items) = v else { return Err("expected an array".into()) };
    if items.len() != out.len() {
        return Err(format!("expected {} numbers", out.len()));
    }
    for (o, it) in out.iter_mut().zip(items) {
        let f = it.as_f64().ok_or("expected numbers")?;
        if !f.is_finite() {
            return Err("numbers must be finite".into());
        }
        *o = f as f32;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::{Store, Verdict, TEST_PRODUCT};
    use std::sync::atomic::AtomicI32;

    const P: &[ParamDef] = &[ParamDef::bypass("bypass"), ParamDef::float("gain", "Gain", -24.0, 24.0, 0.0).unit("dB")];

    #[derive(Default)]
    struct Counter {
        n: AtomicI32,
        rev: AtomicU64,
    }

    impl Ext for Counter {
        fn frame_extra(&self, out: &mut Vec<f32>) {
            out.push(self.n.load(Ordering::Relaxed) as f32);
        }
        fn request(&self, r: &ControllerRequest) -> Option<ControllerResponse> {
            (r.path == "count").then(|| {
                if r.is_post() {
                    self.n.fetch_add(1, Ordering::Relaxed);
                    self.rev.fetch_add(1, Ordering::Relaxed);
                }
                ControllerResponse::json(self.n.load(Ordering::Relaxed).to_string())
            })
        }
        fn save(&self) -> Option<String> {
            Some(format!("{{\"n\":{}}}", self.n.load(Ordering::Relaxed)))
        }
        fn load(&self, v: Option<&Value>) -> Result<(), String> {
            if let Some(v) = v {
                let n = v.get("n").and_then(Value::as_f64).ok_or("n")?;
                self.n.store(n as i32, Ordering::Relaxed);
            }
            Ok(())
        }
        fn reset(&self) {
            self.n.store(0, Ordering::Relaxed);
        }
        fn revision(&self) -> u64 {
            self.rev.load(Ordering::Relaxed)
        }
    }

    fn ctl() -> KitController<Counter> {
        let lic = Manager::load(&TEST_PRODUCT, Store::default(), license::now_secs(), |_, _| Verdict::Unreachable("test".into()));
        KitController::new(P, Arc::new(Meters::default()), lic, Arc::new(Counter::default()), None)
    }

    fn call<E: Ext>(c: &KitController<E>, method: &str, path: &str, body: &str) -> ControllerResponse {
        c.request(&ControllerRequest { method, path, query: "", body: body.as_bytes() })
    }

    #[test]
    fn state_round_trips_with_the_extension_and_bumps_the_revision() {
        let a = ctl();
        let r0 = a.revision();
        call(&a, "POST", "count", "");
        call(&a, "POST", "count", "");
        assert!(a.revision() > r0);
        call(&a, "POST", "ab", "{\"active\":\"B\"}");
        let saved = a.save();
        let b = ctl();
        b.load(&saved).unwrap();
        assert_eq!(b.save(), saved);
        assert_eq!(b.ext.n.load(Ordering::Relaxed), 2);
        b.reset();
        assert_eq!(b.ext.n.load(Ordering::Relaxed), 0);
        assert!(b.save().contains("\"active\":\"B\""), "reset keeps the A/B slots");
    }

    #[test]
    fn bad_input_is_refused_and_unknown_paths_are_404() {
        let c = ctl();
        let before = c.save();
        assert_eq!(call(&c, "POST", "ab", "{oops").status, 400);
        assert!(c.load("{\"x\":{\"n\":\"no\"}}").is_err());
        assert!(c.load("nope").is_err());
        assert_eq!(c.save(), before);
        assert_eq!(call(&c, "GET", "nothing", "").status, 404);
        assert_eq!(call(&c, "GET", "license-activate", "").status, 405);
    }

    #[test]
    fn the_license_endpoint_reports_a_fresh_trial_for_the_product() {
        let c = ctl();
        let r = call(&c, "GET", "license", "");
        let v = json::parse(std::str::from_utf8(&r.body).unwrap()).unwrap();
        assert_eq!(v.get("state").and_then(Value::as_str), Some("trial"));
        assert_eq!(v.get("product").and_then(Value::as_str), Some("NoDAW Test"));
        let r = call(&c, "GET", "license-buy", "");
        assert!(std::str::from_utf8(&r.body).unwrap().contains("example.com"));
    }

    #[test]
    fn the_frame_has_the_documented_layout() {
        let c = ctl();
        c.meters.record(&[0.5; 64], &[0.25; 64], &[0.1; 64], &[0.1; 64]);
        c.meters.set_custom(1, 7.0);
        call(&c, "POST", "count", "");
        let f = c.frame();
        assert_eq!(f.len(), (HEADER + 2 * BANDS + 1) * 4);
        let v: Vec<f32> = f.chunks(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        assert!(v.iter().all(|x| x.is_finite()));
        assert_eq!((v[0], v[1]), (0.5, 0.25));
        assert_eq!(v[10], 48_000.0);
        assert_eq!(v[13], 7.0);
        assert_eq!(*v.last().unwrap(), 1.0);
    }

    #[test]
    fn slots_keep_the_peak_until_taken() {
        let s = Slots::new(2);
        s.max(0, 0.5);
        s.max(0, 0.25);
        s.max(0, f32::NAN);
        assert_eq!(s.take(0), 0.5);
        assert_eq!(s.take(0), 0.0);
        s.set(1, -3.0);
        assert_eq!(s.get(1), -3.0);
        let mut v = Vec::new();
        s.read_all(&mut v);
        assert_eq!(v, vec![0.0, -3.0]);
    }

    #[test]
    fn float_helpers_round_trip() {
        let s = floats_json(&[1.0, -2.5, f32::NAN]);
        let mut out = [0.0; 3];
        parse_floats(&json::parse(&s).unwrap(), &mut out).unwrap();
        assert_eq!(out, [1.0, -2.5, 0.0]);
        assert!(parse_floats(&json::parse("[1]").unwrap(), &mut out).is_err());
    }
}
