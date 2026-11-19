//! The captured snapshot (source B), saved with the session.
//!
//! The audio thread captures it; [`SnapStore`] holds a copy in atomics so the controller can
//! save it (main thread) and a restored session can hand one back (the audio thread picks it
//! up at its next block, without locks or allocation). [`WeaverExt`] is the controller part:
//! the faceplate (as [`LookExt`]) plus `"snap"`: the sample rate it was taken at, its bins per
//! channel and the magnitudes as hex `f32` bits (exact, so a session sounds as it was saved).

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use apf::json::Value;
use apf::prelude::*;
use nodaw_kit::ctl::{Ext, LookExt};

/// The most bins per channel (a 4096-point FFT).
pub const MAX_BINS: usize = 4096 / 2 + 1;

/// The snapshot, shared between the audio thread and the controller.
pub struct SnapStore {
    held: AtomicBool,
    bins: AtomicU32,
    sr: AtomicU32,
    data: Box<[AtomicU32]>,
    /// Bumped by the controller when a session gives (or takes away) a snapshot.
    restored: AtomicU64,
    /// Bumped by every change (for the host's "the session changed").
    rev: AtomicU64,
}

impl Default for SnapStore {
    fn default() -> Self {
        Self {
            held: AtomicBool::new(false),
            bins: AtomicU32::new(0),
            sr: AtomicU32::new(0),
            data: (0..2 * MAX_BINS).map(|_| AtomicU32::new(0)).collect(),
            restored: AtomicU64::new(0),
            rev: AtomicU64::new(0),
        }
    }
}

impl SnapStore {
    /// Audio thread: keep a new capture (`l`, `r` the same length, at most [`MAX_BINS`]).
    pub fn publish(&self, sample_rate: f64, l: &[f32], r: &[f32]) {
        let n = l.len().min(r.len()).min(MAX_BINS);
        for k in 0..n {
            self.data[k].store(l[k].to_bits(), Ordering::Relaxed);
            self.data[MAX_BINS + k].store(r[k].to_bits(), Ordering::Relaxed);
        }
        self.bins.store(n as u32, Ordering::Relaxed);
        self.sr
            .store((sample_rate as f32).to_bits(), Ordering::Relaxed);
        self.held.store(true, Ordering::Release);
        self.rev.fetch_add(1, Ordering::AcqRel);
    }

    /// The restore count: when it changes, the audio thread takes the snapshot (or its absence).
    pub fn restored(&self) -> u64 {
        self.restored.load(Ordering::Acquire)
    }

    /// The held snapshot's sample rate and bins, if there is one.
    pub fn held(&self) -> Option<(f64, usize)> {
        if !self.held.load(Ordering::Acquire) {
            return None;
        }
        let bins = self.bins.load(Ordering::Relaxed) as usize;
        let sr = f32::from_bits(self.sr.load(Ordering::Relaxed)) as f64;
        (bins >= 2).then_some((sr, bins))
    }

    /// Bin `k` of channel `ch`.
    #[inline]
    pub fn get(&self, ch: usize, k: usize) -> f32 {
        f32::from_bits(
            self.data[ch.min(1) * MAX_BINS + k.min(MAX_BINS - 1)].load(Ordering::Relaxed),
        )
    }

    fn set_restored(&self, snap: Option<(f64, &[f32], &[f32])>) {
        match snap {
            Some((sr, l, r)) => {
                for k in 0..l.len() {
                    self.data[k].store(l[k].to_bits(), Ordering::Relaxed);
                    self.data[MAX_BINS + k].store(r[k].to_bits(), Ordering::Relaxed);
                }
                self.bins.store(l.len() as u32, Ordering::Relaxed);
                self.sr.store((sr as f32).to_bits(), Ordering::Relaxed);
                self.held.store(true, Ordering::Release);
            }
            None => self.held.store(false, Ordering::Release),
        }
        self.restored.fetch_add(1, Ordering::AcqRel);
        self.rev.fetch_add(1, Ordering::AcqRel);
    }

    fn save(&self) -> Option<String> {
        let (sr, bins) = self.held()?;
        let mut o = format!("{{\"sr\":{},\"bins\":{bins}", sr.round() as u64);
        for (name, ch) in [("l", 0), ("r", 1)] {
            o.push_str(&format!(",\"{name}\":\""));
            for k in 0..bins {
                o.push_str(&format!("{:08x}", self.get(ch, k).to_bits()));
            }
            o.push('"');
        }
        o.push('}');
        Some(o)
    }

    /// Check and decode a saved snapshot; nothing changes on error.
    fn parse(v: &Value) -> Result<(f64, Vec<f32>, Vec<f32>), String> {
        let sr = v
            .get("sr")
            .and_then(Value::as_f64)
            .ok_or("snap: no sample rate")?;
        if !(8_000.0..=768_000.0).contains(&sr) {
            return Err("snap: bad sample rate".into());
        }
        let bins = v
            .get("bins")
            .and_then(Value::as_f64)
            .ok_or("snap: no bins")?;
        if !(2.0..=MAX_BINS as f64).contains(&bins) || bins.fract() != 0.0 {
            return Err("snap: bad bin count".into());
        }
        let bins = bins as usize;
        let ch = |name: &str| -> Result<Vec<f32>, String> {
            let hex = v.get(name).and_then(Value::as_str).ok_or("snap: no data")?;
            if hex.len() != bins * 8 || !hex.is_ascii() {
                return Err("snap: wrong data length".into());
            }
            (0..bins)
                .map(|k| {
                    let x = u32::from_str_radix(&hex[k * 8..k * 8 + 8], 16)
                        .map(f32::from_bits)
                        .map_err(|_| "snap: bad data".to_string())?;
                    if x.is_finite() && x >= 0.0 {
                        Ok(x)
                    } else {
                        Err("snap: magnitudes must be finite and positive".to_string())
                    }
                })
                .collect()
        };
        Ok((sr, ch("l")?, ch("r")?))
    }
}

/// The controller extension: the faceplate and the snapshot.
pub struct WeaverExt {
    /// The faceplate and the frame values.
    pub look: LookExt,
    /// The snapshot.
    pub snap: Arc<SnapStore>,
}

impl Ext for WeaverExt {
    fn frame_extra(&self, out: &mut Vec<f32>) {
        self.look.frame_extra(out);
    }
    fn request(&self, r: &ControllerRequest) -> Option<ControllerResponse> {
        self.look.request(r)
    }
    fn save(&self) -> Option<String> {
        let look = self.look.save().unwrap_or_else(|| "{}".into());
        // `{"skin":"…"}` + `,"snap":{…}}` (`null`: the session has no snapshot)
        let snap = self.snap.save().unwrap_or_else(|| "null".into());
        Some(format!("{},\"snap\":{snap}}}", look.trim_end_matches('}')))
    }
    fn load(&self, v: Option<&Value>) -> Result<(), String> {
        // A session says `"snap": null` or gives one; a factory preset (no `"snap"`, or no
        // `"x"` at all) leaves the snapshot alone: a preset is a sound, B is the user's capture.
        let snap = match v.and_then(|v| v.get("snap")) {
            None => None,
            Some(Value::Null) => Some(None),
            Some(s) => Some(Some(SnapStore::parse(s)?)),
        };
        self.look.load(v)?;
        if let Some(snap) = snap {
            self.snap
                .set_restored(snap.as_ref().map(|(sr, l, r)| (*sr, &l[..], &r[..])));
        }
        Ok(())
    }
    fn reset(&self) {
        self.look.reset();
        self.snap.set_restored(None);
    }
    fn revision(&self) -> u64 {
        self.look
            .revision()
            .wrapping_add(self.snap.rev.load(Ordering::Acquire))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext() -> WeaverExt {
        WeaverExt {
            look: LookExt::new(&["nebula", "ice"], 0, 1),
            snap: Arc::new(SnapStore::default()),
        }
    }

    #[test]
    fn a_saved_snapshot_comes_back_bit_for_bit() {
        let a = ext();
        let l: Vec<f32> = (0..1025).map(|k| 0.1 + k as f32 * 1.37e-3).collect();
        let r: Vec<f32> = (0..1025)
            .map(|k| (k as f32 * 0.01).sin().abs() * 3.0)
            .collect();
        a.look.set_look("ice").unwrap();
        let r0 = a.revision();
        a.snap.publish(48_000.0, &l, &r);
        assert_ne!(a.revision(), r0, "a capture changes the session");
        let text = a.save().unwrap();
        let v = apf::json::parse(&text).unwrap();
        let b = ext();
        let before = b.snap.restored();
        b.load(Some(&v)).unwrap();
        assert_eq!(b.look.look(), "ice");
        assert_ne!(b.snap.restored(), before);
        assert_eq!(b.snap.held(), Some((48_000.0, 1025)));
        for k in 0..1025 {
            assert_eq!(b.snap.get(0, k).to_bits(), l[k].to_bits());
            assert_eq!(b.snap.get(1, k).to_bits(), r[k].to_bits());
        }
        // a preset (no "x", or no "snap") leaves it; a session without one takes it away
        b.load(None).unwrap();
        assert!(b.snap.held().is_some());
        b.load(Some(&apf::json::parse("{\"skin\":\"nebula\"}").unwrap()))
            .unwrap();
        assert!(b.snap.held().is_some());
        let empty = ext();
        let text = empty.save().unwrap();
        assert!(text.contains("\"snap\":null"), "{text}");
        b.load(Some(&apf::json::parse(&text).unwrap())).unwrap();
        assert!(b.snap.held().is_none());
    }

    #[test]
    fn a_damaged_snapshot_is_refused_and_changes_nothing() {
        let a = ext();
        a.snap.publish(48_000.0, &[1.0; 4], &[2.0; 4]);
        let before = a.snap.restored();
        for bad in [
            "{\"snap\":{\"sr\":48000,\"bins\":2,\"l\":\"3f800000\",\"r\":\"3f8000003f800000\"}}",
            "{\"snap\":{\"sr\":48000,\"bins\":2,\"l\":\"7fc000007fc00000\",\"r\":\"3f8000003f800000\"}}",
            "{\"snap\":{\"sr\":1,\"bins\":2,\"l\":\"3f8000003f800000\",\"r\":\"3f8000003f800000\"}}",
            "{\"snap\":{\"sr\":48000,\"bins\":9999,\"l\":\"\",\"r\":\"\"}}",
            "{\"snap\":{\"sr\":48000,\"bins\":2,\"l\":\"zz8000003f800000\",\"r\":\"3f8000003f800000\"}}",
        ] {
            assert!(a.load(Some(&apf::json::parse(bad).unwrap())).is_err(), "{bad}");
        }
        assert_eq!(a.snap.restored(), before);
        assert_eq!(a.snap.held(), Some((48_000.0, 4)));
        assert_eq!(a.snap.get(1, 3), 2.0);
    }
}
