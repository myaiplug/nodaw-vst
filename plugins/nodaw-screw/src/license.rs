//! Trial and license state: a 14-day full trial from the first launch, then activation with a
//! Gumroad license key. After the trial, unlicensed copies keep working (audio, presets and
//! sessions) but drop to silence for half a second every 45 seconds (see [`Dropout`]).
//!
//! This is anti-casual protection, not DRM: the state file is signed with an HMAC so editing
//! it by hand ends the trial, and a second copy of the trial start date survives deleting it.
//!
//! Threading: [`Manager`] is used from the main thread (the controller) and from its own
//! background threads (network). The audio thread only reads [`Manager::demo_flag`], an atomic.
//! Nothing here ever panics on an unreadable or unwritable file: the state then lives in memory.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use apf::json::{self, Value};
use hmac::{Hmac, Mac};
use sha2::Sha256;

// ── the two constants the owner fills in ───────────────────────────────────────────────────

/// TODO(owner): the Gumroad product ID of NoDAW Screw (Gumroad dashboard: Product > Content >
/// License key > "product_id"). Until it is set, activation tells the user it is not available.
pub const GUMROAD_PRODUCT_ID: &str = "TODO_GUMROAD_PRODUCT_ID";

/// TODO(owner): the public Gumroad product page that the Buy button opens.
pub const BUY_URL: &str = "https://nodawlabs.gumroad.com/l/nodaw-screw";

// ── policy ─────────────────────────────────────────────────────────────────────────────────

pub const TRIAL_DAYS: u64 = 14;
pub const DAY: u64 = 86_400;
/// A licensed copy asks Gumroad again at most this often; offline it stays licensed.
pub const RECHECK_SECS: u64 = 30 * DAY;
const VERIFY_URL: &str = "https://api.gumroad.com/v2/licenses/verify";
const NET_TIMEOUT: Duration = Duration::from_secs(12);
/// Signs the state file. Embedded on purpose: this only stops casual editing.
const HMAC_KEY: &[u8] = b"nodaw-screw/license/v1/7c1e5b0a-93f4-4d1e-b6a2-2f8d0c4e9a17";

// ── the record that is stored ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    /// Unix seconds of the first launch.
    pub trial_start: u64,
    /// The latest time any launch saw: winding the clock back does not extend the trial.
    pub last_seen: u64,
    /// The license key, once Gumroad accepted it.
    pub key: Option<String>,
    /// When Gumroad last confirmed the key.
    pub verified_at: u64,
    /// The buyer's email from Gumroad (shown in the dialog).
    pub email: Option<String>,
}

fn mac_hex(msg: &str) -> String {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(HMAC_KEY).expect("any key length works");
    m.update(msg.as_bytes());
    m.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn canon(r: &Record) -> String {
    format!("v1|{}|{}|{}|{}|{}", r.trial_start, r.last_seen, r.key.as_deref().unwrap_or(""), r.verified_at, r.email.as_deref().unwrap_or(""))
}

fn shadow_canon(start: u64) -> String {
    format!("shadow1|{start}")
}

impl Record {
    pub fn fresh(now: u64) -> Self {
        Self { trial_start: now, last_seen: now, ..Default::default() }
    }

    pub fn to_json(&self) -> String {
        let mut o = String::from("{\"v\":1,\"trial_start\":");
        json::write_num(&mut o, self.trial_start as f64);
        o.push_str(",\"last_seen\":");
        json::write_num(&mut o, self.last_seen as f64);
        o.push_str(",\"key\":");
        match &self.key {
            Some(k) => json::write_str(&mut o, k),
            None => o.push_str("null"),
        }
        o.push_str(",\"verified_at\":");
        json::write_num(&mut o, self.verified_at as f64);
        o.push_str(",\"email\":");
        match &self.email {
            Some(e) => json::write_str(&mut o, e),
            None => o.push_str("null"),
        }
        o.push_str(",\"mac\":");
        json::write_str(&mut o, &mac_hex(&canon(self)));
        o.push('}');
        o
    }

    /// `Ok(record)` when the text is a record with a valid signature, `Err(Tampered)` when it
    /// parses but the signature does not match, `Err(Unreadable)` otherwise.
    pub fn from_json(text: &str) -> Result<Self, ParseError> {
        let v = json::parse(text).map_err(|_| ParseError::Unreadable)?;
        let num = |k: &str| v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite() && *x >= 0.0).map(|x| x as u64);
        let opt_str = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let r = Record {
            trial_start: num("trial_start").ok_or(ParseError::Unreadable)?,
            last_seen: num("last_seen").ok_or(ParseError::Unreadable)?,
            key: opt_str("key").filter(|k| !k.is_empty()),
            verified_at: num("verified_at").unwrap_or(0),
            email: opt_str("email"),
        };
        let mac = v.get("mac").and_then(Value::as_str).ok_or(ParseError::Tampered)?;
        if !constant_eq(mac.as_bytes(), mac_hex(&canon(&r)).as_bytes()) {
            return Err(ParseError::Tampered);
        }
        Ok(r)
    }
}

pub fn shadow_json(start: u64) -> String {
    format!("{{\"s\":{start},\"m\":\"{}\"}}", mac_hex(&shadow_canon(start)))
}

pub fn parse_shadow(text: &str) -> Option<u64> {
    let v = json::parse(text).ok()?;
    let s = v.get("s")?.as_f64()?;
    if !(s.is_finite() && s >= 0.0) {
        return None;
    }
    let s = s as u64;
    let m = v.get("m")?.as_str()?;
    constant_eq(m.as_bytes(), mac_hex(&shadow_canon(s)).as_bytes()).then_some(s)
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Unreadable,
    Tampered,
}

// ── what the state means now ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Full features. `days_left` counts today: 14 on the first day, 1 on the last.
    Trial { days_left: u64 },
    /// Trial over (or the state file was edited): audio dropouts until activated.
    Expired { tampered: bool },
    /// Full features for good. `recheck_due`: ask Gumroad again in the background.
    Licensed { recheck_due: bool },
}

impl Status {
    /// Whether the audio thread should apply the dropouts.
    pub fn demo(&self) -> bool {
        matches!(self, Status::Expired { .. })
    }
}

/// The pure rule: a record, whether it was tampered with, and the time.
pub fn evaluate(r: &Record, tampered: bool, now: u64) -> Status {
    if r.key.is_some() {
        // Never locked out offline: the age of the last check only decides whether to ask again.
        let age = now.saturating_sub(r.verified_at);
        let clock_back = r.verified_at > now + DAY;
        return Status::Licensed { recheck_due: age >= RECHECK_SECS || clock_back };
    }
    if tampered {
        return Status::Expired { tampered: true };
    }
    let latest = now.max(r.last_seen);
    let elapsed = latest.saturating_sub(r.trial_start);
    let total = TRIAL_DAYS * DAY;
    if elapsed >= total {
        Status::Expired { tampered: false }
    } else {
        Status::Trial { days_left: (total - elapsed).div_ceil(DAY) }
    }
}

// ── Gumroad ────────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Gumroad accepted the key (email if it sent one).
    Valid { email: Option<String> },
    /// Gumroad answered and the key is not (or no longer) good: unknown, refunded, charged back.
    Invalid(String),
    /// No answer we can trust (offline, timeout, server error): change nothing.
    Unreachable(String),
}

/// Reads Gumroad's answer. Pure, so it is unit-tested without a network.
pub fn parse_verify(status: u16, body: &str) -> Verdict {
    let Ok(v) = json::parse(body) else {
        return if status >= 500 || status == 0 { Verdict::Unreachable(format!("Gumroad error ({status})")) } else { Verdict::Unreachable("Unexpected answer from Gumroad".into()) };
    };
    let success = matches!(v.get("success"), Some(Value::Bool(true)));
    let message = v.get("message").and_then(Value::as_str).map(str::to_string);
    if !success {
        // 404 (no such license for this product) and 400 are definitive answers; 5xx are not.
        if (400..500).contains(&status) || status == 200 {
            return Verdict::Invalid(message.unwrap_or_else(|| "That license key was not accepted.".into()));
        }
        return Verdict::Unreachable(message.unwrap_or_else(|| format!("Gumroad error ({status})")));
    }
    let p = v.get("purchase");
    let flag = |k: &str| matches!(p.and_then(|p| p.get(k)), Some(Value::Bool(true)));
    let set = |k: &str| p.and_then(|p| p.get(k)).is_some_and(|x| !matches!(x, Value::Null) && x.as_str() != Some(""));
    if flag("refunded") {
        return Verdict::Invalid("This purchase was refunded.".into());
    }
    if flag("chargebacked") || (flag("disputed") && !flag("dispute_won")) {
        return Verdict::Invalid("This purchase was charged back.".into());
    }
    if set("subscription_ended_at") || set("subscription_failed_at") {
        return Verdict::Invalid("This subscription has ended.".into());
    }
    Verdict::Valid { email: p.and_then(|p| p.get("email")).and_then(Value::as_str).map(str::to_string) }
}

fn form_escape(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

/// Blocking: call only from a background thread.
pub fn verify_online(key: &str) -> Verdict {
    if GUMROAD_PRODUCT_ID.starts_with("TODO") {
        return Verdict::Unreachable("Activation is not set up in this build yet.".into());
    }
    let body = format!("product_id={}&license_key={}&increment_uses_count=false", form_escape(GUMROAD_PRODUCT_ID), form_escape(key));
    let agent = ureq::AgentBuilder::new().timeout(NET_TIMEOUT).user_agent(concat!("NoDAW-Screw/", env!("CARGO_PKG_VERSION"))).build();
    match agent.post(VERIFY_URL).set("Content-Type", "application/x-www-form-urlencoded").send_string(&body) {
        Ok(resp) => {
            let status = resp.status();
            parse_verify(status, &resp.into_string().unwrap_or_default())
        }
        Err(ureq::Error::Status(status, resp)) => parse_verify(status, &resp.into_string().unwrap_or_default()),
        Err(e) => Verdict::Unreachable(format!("Could not reach Gumroad: {}", short(&e.to_string()))),
    }
}

fn short(s: &str) -> String {
    s.chars().take(120).collect()
}

// ── storage ────────────────────────────────────────────────────────────────────────────────

/// Where the state lives. `None` paths: memory only (tests, or no home directory).
#[derive(Debug, Clone, Default)]
pub struct Store {
    pub main: Option<PathBuf>,
    pub shadow: Option<PathBuf>,
}

impl Store {
    /// Windows: `%APPDATA%\NoDAW Labs\Screw\license.json` and a shadow copy of the trial start in
    /// `%LOCALAPPDATA%\NoDAW Labs\Screw\.cache`. macOS: `~/Library/Application Support/NoDAW
    /// Labs/Screw` and `~/Library/Caches/NoDAW Labs/Screw`. Linux: `$XDG_CONFIG_HOME` / `$XDG_CACHE_HOME`.
    pub fn default_paths() -> Self {
        let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let (main_dir, shadow_dir) = if cfg!(target_os = "windows") {
            (env("APPDATA"), env("LOCALAPPDATA"))
        } else if cfg!(target_os = "macos") {
            let h = env("HOME");
            (h.as_ref().map(|h| h.join("Library/Application Support")), h.map(|h| h.join("Library/Caches")))
        } else {
            let h = env("HOME");
            (env("XDG_CONFIG_HOME").or_else(|| h.as_ref().map(|h| h.join(".config"))), env("XDG_CACHE_HOME").or_else(|| h.map(|h| h.join(".cache"))))
        };
        let sub = |d: PathBuf| d.join("NoDAW Labs").join("Screw");
        Self { main: main_dir.map(|d| sub(d).join("license.json")), shadow: shadow_dir.map(|d| sub(d).join(".cache")) }
    }

    fn read(p: &Option<PathBuf>) -> Option<String> {
        p.as_ref().and_then(|p| std::fs::read_to_string(p).ok())
    }

    /// Never fails: any error leaves the state in memory only.
    fn write(p: &Option<PathBuf>, text: &str) -> bool {
        let Some(p) = p else { return false };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, p)).is_ok() || std::fs::write(p, text).is_ok()
    }

    /// Loads (or starts) the state. Returns the record and whether it had been tampered with.
    pub fn load(&self, now: u64) -> (Record, bool) {
        let shadow = Self::read(&self.shadow).and_then(|t| parse_shadow(&t));
        let (mut rec, tampered) = match Self::read(&self.main).map(|t| Record::from_json(&t)) {
            Some(Ok(r)) => (r, false),
            Some(Err(ParseError::Tampered)) => (Record { trial_start: shadow.unwrap_or(0), last_seen: now, ..Default::default() }, true),
            // Missing or garbage: a new trial, unless the shadow remembers an older start.
            Some(Err(ParseError::Unreadable)) | None => (Record::fresh(shadow.unwrap_or(now).min(now)), false),
        };
        if let Some(s) = shadow {
            rec.trial_start = rec.trial_start.min(s);
        }
        rec.last_seen = rec.last_seen.max(now);
        if !tampered {
            self.save(&rec);
        }
        (rec, tampered)
    }

    pub fn save(&self, r: &Record) {
        Self::write(&self.main, &r.to_json());
        if Self::read(&self.shadow).and_then(|t| parse_shadow(&t)) != Some(r.trial_start) {
            Self::write(&self.shadow, &shadow_json(r.trial_start));
        }
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ── the live state shared by the plugin, its controller and the network threads ────────────

struct Inner {
    rec: Record,
    tampered: bool,
    busy: bool,
    message: Option<(bool, String)>,
}

pub struct Manager {
    store: Store,
    inner: Mutex<Inner>,
    demo: AtomicBool,
    /// Tests swap the network for a fake.
    verifier: fn(&str) -> Verdict,
}

impl Manager {
    pub fn load(store: Store, now: u64, verifier: fn(&str) -> Verdict) -> Arc<Self> {
        let (rec, tampered) = store.load(now);
        let m = Arc::new(Self { store, inner: Mutex::new(Inner { rec, tampered, busy: false, message: None }), demo: AtomicBool::new(false), verifier });
        m.refresh(now);
        m
    }

    /// The real one: files in the user's profile, Gumroad over HTTPS.
    pub fn load_default() -> Arc<Self> {
        Self::load(Store::default_paths(), now_secs(), verify_online)
    }

    /// Audio thread: whether the dropouts apply. Lock-free.
    pub fn demo_flag(&self) -> bool {
        self.demo.load(Ordering::Relaxed)
    }

    pub fn status(&self, now: u64) -> Status {
        match self.inner.lock() {
            Ok(g) => evaluate(&g.rec, g.tampered, now),
            Err(_) => Status::Trial { days_left: 1 },
        }
    }

    /// Recomputes the status (call now and then from the main thread) and returns it.
    pub fn refresh(&self, now: u64) -> Status {
        let s = self.status(now);
        self.demo.store(s.demo(), Ordering::Relaxed);
        s
    }

    /// A licensed copy whose last check is old asks Gumroad again, in the background.
    pub fn maybe_recheck(self: &Arc<Self>, now: u64) {
        let key = {
            let Ok(mut g) = self.inner.lock() else { return };
            if g.busy || !matches!(evaluate(&g.rec, g.tampered, now), Status::Licensed { recheck_due: true }) {
                return;
            }
            g.busy = true;
            g.rec.key.clone()
        };
        if let Some(key) = key {
            self.spawn(key, false);
        }
    }

    /// Starts activating `key` in the background; the page polls [`Manager::to_json`].
    pub fn activate(self: &Arc<Self>, key: &str) -> Result<(), String> {
        let key: String = key.trim().chars().filter(|c| !c.is_whitespace()).collect();
        if key.len() < 8 || key.len() > 128 {
            return Err("Paste the license key from your Gumroad receipt.".into());
        }
        {
            let mut g = self.inner.lock().map_err(|_| "busy".to_string())?;
            if g.busy {
                return Err("Already checking a key.".into());
            }
            g.busy = true;
            g.message = None;
        }
        self.spawn(key, true);
        Ok(())
    }

    fn spawn(self: &Arc<Self>, key: String, user: bool) {
        let me = self.clone();
        let run = move || {
            let v = (me.verifier)(&key);
            me.apply(&key, v, user, now_secs());
        };
        if std::thread::Builder::new().name("screw-license".into()).spawn(run).is_err() {
            if let Ok(mut g) = self.inner.lock() {
                g.busy = false;
            }
        }
    }

    /// Applies a verdict (from the network thread, or directly in tests).
    pub fn apply(&self, key: &str, v: Verdict, user: bool, now: u64) {
        let Ok(mut g) = self.inner.lock() else { return };
        g.busy = false;
        match v {
            Verdict::Valid { email } => {
                g.rec.key = Some(key.to_string());
                g.rec.verified_at = now;
                if email.is_some() {
                    g.rec.email = email;
                }
                g.tampered = false;
                g.message = user.then(|| (true, "Activated. Thank you for supporting NoDAW!".to_string()));
                self.store.save(&g.rec);
            }
            Verdict::Invalid(msg) => {
                // Only a definitive "no" from Gumroad removes a stored license.
                if !user || g.rec.key.as_deref() == Some(key) {
                    g.rec.key = None;
                    g.rec.email = None;
                    g.rec.verified_at = 0;
                    self.store.save(&g.rec);
                }
                g.message = Some((false, msg));
            }
            Verdict::Unreachable(msg) => {
                if user {
                    g.message = Some((false, msg));
                }
            }
        }
        let s = evaluate(&g.rec, g.tampered, now);
        self.demo.store(s.demo(), Ordering::Relaxed);
    }

    /// For the page: `{state, days_left, trial_days, busy, message, ok, email, key_hint, buy_url}`.
    pub fn to_json(&self, now: u64) -> String {
        let s = self.refresh(now);
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return "{\"state\":\"trial\",\"days_left\":1}".into(),
        };
        let (state, days) = match s {
            Status::Trial { days_left } => ("trial", days_left),
            Status::Expired { tampered: false } => ("expired", 0),
            Status::Expired { tampered: true } => ("tampered", 0),
            Status::Licensed { .. } => ("licensed", 0),
        };
        let mut o = format!("{{\"state\":\"{state}\",\"days_left\":{days},\"trial_days\":{TRIAL_DAYS},\"busy\":{}", g.busy);
        o.push_str(",\"message\":");
        match &g.message {
            Some((_, m)) => json::write_str(&mut o, m),
            None => o.push_str("null"),
        }
        o.push_str(&format!(",\"ok\":{}", g.message.as_ref().is_some_and(|m| m.0)));
        o.push_str(",\"email\":");
        match &g.rec.email {
            Some(e) if matches!(s, Status::Licensed { .. }) => json::write_str(&mut o, e),
            _ => o.push_str("null"),
        }
        o.push_str(",\"key_hint\":");
        match &g.rec.key {
            Some(k) => {
                let tail: String = k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
                json::write_str(&mut o, &format!("••••{tail}"));
            }
            None => o.push_str("null"),
        }
        o.push_str(",\"buy_url\":");
        json::write_str(&mut o, BUY_URL);
        o.push_str(&format!(",\"activation_ready\":{}", !GUMROAD_PRODUCT_ID.starts_with("TODO")));
        o.push('}');
        o
    }
}

/// Opens the Buy page in the user's browser (main thread, never blocks: the child is detached).
pub fn open_buy_page() -> bool {
    let url = BUY_URL;
    if !url.starts_with("https://") {
        return false;
    }
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer.exe").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    r.is_ok()
}

// ── the dropout, on the audio thread ───────────────────────────────────────────────────────

/// Silence for `GAP` seconds every `PERIOD` seconds, with raised-cosine fades so it never
/// clicks. The first dropout comes `PERIOD` seconds after the plugin starts processing.
pub struct Dropout {
    pos: u64,
    period: u64,
    gap: u64,
    fade: u64,
}

impl Dropout {
    pub const PERIOD: f64 = 45.0;
    pub const GAP: f64 = 0.5;
    pub const FADE: f64 = 0.015;

    pub fn new(sample_rate: f64) -> Self {
        let s = |x: f64| ((x * sample_rate).round() as u64).max(1);
        Self { pos: 0, period: s(Self::PERIOD), gap: s(Self::GAP), fade: s(Self::FADE) }
    }

    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// The gain at sample `pos` (0..=1).
    pub fn gain_at(&self, pos: u64) -> f32 {
        let total = self.gap + 2 * self.fade;
        let c = pos % self.period;
        let start = self.period - total;
        if c < start {
            return 1.0;
        }
        let t = c - start;
        let ramp = |x: f64| (0.5 + 0.5 * (std::f64::consts::PI * x).cos()) as f32;
        if t < self.fade {
            ramp(t as f64 / self.fade as f64)
        } else if t < self.fade + self.gap {
            0.0
        } else {
            1.0 - ramp((t - self.fade - self.gap) as f64 / self.fade as f64)
        }
    }

    /// Applies the dropout (if `on`) and advances the clock. No allocation.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], on: bool) {
        let n = l.len().min(r.len()) as u64;
        if on {
            let total = self.gap + 2 * self.fade;
            let start = self.period - total;
            let c0 = self.pos % self.period;
            // Fast path: the whole block is before the next dropout.
            if !(c0 + n <= start) {
                for i in 0..n as usize {
                    let g = self.gain_at(self.pos + i as u64);
                    l[i] *= g;
                    r[i] *= g;
                }
            }
        }
        self.pos = self.pos.wrapping_add(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000;

    #[test]
    fn a_new_trial_has_fourteen_days_and_counts_down() {
        let r = Record::fresh(T0);
        assert_eq!(evaluate(&r, false, T0), Status::Trial { days_left: 14 });
        assert_eq!(evaluate(&r, false, T0 + DAY - 1), Status::Trial { days_left: 14 });
        assert_eq!(evaluate(&r, false, T0 + DAY), Status::Trial { days_left: 13 });
        assert_eq!(evaluate(&r, false, T0 + 13 * DAY + 5), Status::Trial { days_left: 1 });
        assert!(!evaluate(&r, false, T0 + 13 * DAY).demo());
    }

    #[test]
    fn the_trial_ends_after_fourteen_days_and_turns_the_dropouts_on() {
        let r = Record::fresh(T0);
        let s = evaluate(&r, false, T0 + 14 * DAY);
        assert_eq!(s, Status::Expired { tampered: false });
        assert!(s.demo());
        assert!(evaluate(&r, false, T0 + 400 * DAY).demo());
    }

    #[test]
    fn winding_the_clock_back_does_not_extend_the_trial() {
        let mut r = Record::fresh(T0);
        r.last_seen = T0 + 20 * DAY;
        assert!(evaluate(&r, false, T0 + DAY).demo());
    }

    #[test]
    fn the_signed_record_round_trips_and_edits_are_detected() {
        let mut r = Record::fresh(T0);
        r.key = Some("ABCD-1234-EFGH-5678".into());
        r.verified_at = T0 + 5;
        r.email = Some("a@b.c".into());
        let text = r.to_json();
        assert_eq!(Record::from_json(&text), Ok(r.clone()));
        let edited = text.replace(&format!("\"trial_start\":{T0}"), &format!("\"trial_start\":{}", T0 + 30 * DAY));
        assert_ne!(edited, text);
        assert_eq!(Record::from_json(&edited), Err(ParseError::Tampered));
        let no_mac = r#"{"v":1,"trial_start":1,"last_seen":1,"key":null,"verified_at":0,"email":null}"#;
        assert_eq!(Record::from_json(no_mac), Err(ParseError::Tampered));
        assert_eq!(Record::from_json("garbage"), Err(ParseError::Unreadable));
        assert_eq!(evaluate(&Record::fresh(T0), true, T0), Status::Expired { tampered: true });
    }

    #[test]
    fn a_licensed_copy_stays_licensed_offline_and_only_asks_again_after_30_days() {
        let mut r = Record::fresh(T0);
        r.key = Some("KEY-KEY-KEY".into());
        r.verified_at = T0;
        assert_eq!(evaluate(&r, false, T0 + 29 * DAY), Status::Licensed { recheck_due: false });
        assert_eq!(evaluate(&r, false, T0 + 30 * DAY), Status::Licensed { recheck_due: true });
        // A year offline, trial long over: still licensed, no dropouts.
        let s = evaluate(&r, false, T0 + 365 * DAY);
        assert!(matches!(s, Status::Licensed { .. }) && !s.demo());
    }

    #[test]
    fn gumroad_answers_are_read_correctly() {
        let ok = r#"{"success":true,"uses":3,"purchase":{"email":"buyer@x.com","refunded":false,"chargebacked":false,"subscription_ended_at":null}}"#;
        assert_eq!(parse_verify(200, ok), Verdict::Valid { email: Some("buyer@x.com".into()) });
        let nope = r#"{"success":false,"message":"That license does not exist for the provided product."}"#;
        assert!(matches!(parse_verify(404, nope), Verdict::Invalid(m) if m.contains("does not exist")));
        let refunded = r#"{"success":true,"purchase":{"refunded":true}}"#;
        assert!(matches!(parse_verify(200, refunded), Verdict::Invalid(_)));
        let charged = r#"{"success":true,"purchase":{"disputed":true,"dispute_won":false}}"#;
        assert!(matches!(parse_verify(200, charged), Verdict::Invalid(_)));
        let ended = r#"{"success":true,"purchase":{"subscription_ended_at":"2026-01-01T00:00:00Z"}}"#;
        assert!(matches!(parse_verify(200, ended), Verdict::Invalid(_)));
        assert!(matches!(parse_verify(503, "<html>down</html>"), Verdict::Unreachable(_)));
        assert!(matches!(parse_verify(500, r#"{"success":false}"#), Verdict::Unreachable(_)));
    }

    fn tmp_store(tag: &str) -> (Store, PathBuf) {
        let d = std::env::temp_dir().join(format!("screw-lic-{tag}-{}-{}", std::process::id(), now_secs()));
        let _ = std::fs::remove_dir_all(&d);
        (Store { main: Some(d.join("a").join("license.json")), shadow: Some(d.join("b").join(".cache")) }, d)
    }

    #[test]
    fn the_store_survives_deleting_the_file_and_detects_edits() {
        let (st, d) = tmp_store("store");
        let (r, t) = st.load(T0);
        assert!(!t && r.trial_start == T0);
        // Deleting the main file does not restart the trial: the shadow remembers.
        std::fs::remove_file(st.main.as_ref().unwrap()).unwrap();
        let (r2, _) = st.load(T0 + 20 * DAY);
        assert_eq!(r2.trial_start, T0);
        assert!(evaluate(&r2, false, T0 + 20 * DAY).demo());
        // Editing the file by hand is detected.
        let p = st.main.clone().unwrap();
        let text = std::fs::read_to_string(&p).unwrap().replace(&format!("\"trial_start\":{T0}"), &format!("\"trial_start\":{}", T0 + 19 * DAY));
        std::fs::write(&p, text).unwrap();
        let (r3, t3) = st.load(T0 + 20 * DAY);
        assert!(t3);
        assert_eq!(evaluate(&r3, t3, T0 + 20 * DAY), Status::Expired { tampered: true });
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn an_unwritable_location_never_fails_and_keeps_a_trial_in_memory() {
        // A file where a folder should be: nothing under it can be created, on any OS.
        let blocker = std::env::temp_dir().join(format!("screw-lic-blocker-{}", std::process::id()));
        std::fs::write(&blocker, b"x").unwrap();
        let st = Store { main: Some(blocker.join("sub").join("license.json")), shadow: Some(blocker.join("s").join(".cache")) };
        let (r, t) = st.load(T0);
        let _ = std::fs::remove_file(&blocker);
        assert!(!t);
        assert_eq!(evaluate(&r, t, T0), Status::Trial { days_left: 14 });
        let m = Manager::load(Store::default(), T0, |_| Verdict::Unreachable("x".into()));
        assert!(!m.demo_flag());
    }

    #[test]
    fn activation_unlocks_and_a_definitive_refusal_relocks_but_offline_does_not() {
        let (st, d) = tmp_store("mgr");
        let m = Manager::load(st.clone(), T0, |_| Verdict::Unreachable("offline".into()));
        assert!(!m.demo_flag());
        m.refresh(T0 + 15 * DAY);
        assert!(m.demo_flag());
        assert!(m.activate("short").is_err());
        m.apply("GOOD-KEY-1234", Verdict::Valid { email: Some("b@x.com".into()) }, true, T0 + 15 * DAY);
        assert!(!m.demo_flag());
        assert!(m.to_json(T0 + 15 * DAY).contains("\"state\":\"licensed\""));
        // Reloaded from disk, months later, offline: still licensed.
        let m2 = Manager::load(st.clone(), T0 + 100 * DAY, |_| Verdict::Unreachable("offline".into()));
        assert!(!m2.demo_flag());
        m2.apply("GOOD-KEY-1234", Verdict::Unreachable("offline".into()), false, T0 + 100 * DAY);
        assert!(!m2.demo_flag());
        // Gumroad says refunded: back to the expired trial.
        m2.apply("GOOD-KEY-1234", Verdict::Invalid("refunded".into()), false, T0 + 100 * DAY);
        assert!(m2.demo_flag());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_dropout_is_silent_for_half_a_second_every_45_seconds_without_clicks() {
        let sr = 48_000.0;
        let mut d = Dropout::new(sr);
        let n = (sr * 46.0) as usize;
        let mut l = vec![1.0f32; n];
        let mut r = vec![1.0f32; n];
        for (cl, cr) in l.chunks_mut(512).zip(r.chunks_mut(512)) {
            d.process(cl, cr, true);
        }
        let silent = l.iter().filter(|&&x| x == 0.0).count();
        assert!((silent as f64 - 0.5 * sr).abs() < 10.0, "{silent}");
        // The first 44 seconds are untouched.
        assert!(l[..(44.0 * sr) as usize].iter().all(|&x| x == 1.0));
        // No step bigger than a 15 ms raised cosine allows.
        let max_step = l.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(max_step < 0.003, "{max_step}");
        // Off: nothing changes.
        let mut d2 = Dropout::new(sr);
        let mut a = vec![1.0f32; n];
        let mut b = vec![1.0f32; n];
        d2.process(&mut a, &mut b, false);
        assert!(a.iter().all(|&x| x == 1.0));
    }
}
