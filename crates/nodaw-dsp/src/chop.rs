//! The chop gate: a 16-step, tempo-synced pattern that mutes or ducks the signal ahead of the
//! reverb, so the tails ring out between the hits ("chopped and screwed").

/// Number of steps in a pattern.
pub const STEPS: usize = 16;

/// Step levels: 0 = closed, 1 = half, 2 = open.
pub type Pattern = [u8; STEPS];

/// The factory patterns. Index 0 is "Off" (always open).
pub const PATTERNS: &[(&str, Pattern)] = &[
    ("Off", [2; STEPS]),
    ("Pulse", [2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0]),
    ("Offbeat", [0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 2, 0]),
    ("Skip", [2, 2, 0, 2, 2, 0, 2, 0, 2, 2, 0, 2, 2, 0, 0, 2]),
    ("Stutter", [2, 2, 2, 2, 0, 0, 2, 2, 2, 2, 2, 2, 0, 0, 2, 0]),
    ("Trap Roll", [2, 0, 1, 0, 2, 0, 1, 1, 2, 0, 1, 0, 2, 1, 2, 1]),
    ("Half-time", [2, 1, 0, 0, 2, 1, 0, 0, 2, 1, 0, 0, 2, 1, 0, 0]),
    ("Gallop", [2, 0, 2, 2, 0, 2, 0, 2, 2, 0, 2, 2, 0, 2, 0, 0]),
];

/// Pattern names, for the host's parameter list.
pub fn pattern_names() -> Vec<&'static str> {
    PATTERNS.iter().map(|p| p.0).collect()
}

/// Rate names and steps per beat (a beat is a quarter note).
pub const RATES: &[(&str, f64)] = &[("1/4", 1.0), ("1/8", 2.0), ("1/16", 4.0), ("1/32", 8.0), ("1/8T", 3.0), ("1/16T", 6.0)];

/// The step playing at `beats` (host song position) for a given steps-per-beat rate.
#[inline]
pub fn step_at(beats: f64, steps_per_beat: f64) -> usize {
    let s = (beats * steps_per_beat).floor();
    (s.rem_euclid(STEPS as f64)) as usize
}

/// Gain target of a level at the given depth (0..1): open = 1, closed = 1 - depth.
#[inline]
pub fn level_gain(level: u8, depth: f32) -> f32 {
    let open = match level {
        0 => 0.0,
        1 => 0.5,
        _ => 1.0,
    };
    1.0 - depth * (1.0 - open)
}

/// Tracks the musical position when the host is not playing (free-run) and when it is.
#[derive(Debug, Clone)]
pub struct ChopClock {
    free_beats: f64,
}

impl Default for ChopClock {
    fn default() -> Self {
        Self { free_beats: 0.0 }
    }
}

impl ChopClock {
    /// Beat position at the start of a block: the host's, if it is playing and says so;
    /// otherwise the free-running one. Returns the position and the per-sample increment.
    pub fn begin_block(&mut self, host_pos: Option<f64>, playing: bool, tempo: Option<f64>, sr: f64) -> (f64, f64) {
        let tempo = tempo.filter(|t| t.is_finite() && *t > 20.0 && *t < 400.0).unwrap_or(120.0);
        let inc = tempo / (60.0 * sr);
        let start = match (playing, host_pos) {
            (true, Some(p)) if p.is_finite() => p,
            _ => self.free_beats,
        };
        (start, inc)
    }
    /// Record where the block ended.
    pub fn end_block(&mut self, start: f64, inc: f64, frames: usize) {
        self.free_beats = start + inc * frames as f64;
        if !self.free_beats.is_finite() || self.free_beats > 1.0e9 {
            self.free_beats = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_follow_the_beat_grid() {
        assert_eq!(step_at(0.0, 4.0), 0);
        assert_eq!(step_at(0.24, 4.0), 0);
        assert_eq!(step_at(0.25, 4.0), 1);
        assert_eq!(step_at(3.99, 4.0), 15);
        assert_eq!(step_at(4.0, 4.0), 0, "the pattern wraps after 16 sixteenths");
        assert_eq!(step_at(-0.1, 4.0), 15, "negative positions wrap too");
        assert_eq!(step_at(1.0, 3.0), 3);
    }

    #[test]
    fn every_pattern_is_16_steps_of_valid_levels() {
        for (name, p) in PATTERNS {
            assert_eq!(p.len(), STEPS, "{name}");
            assert!(p.iter().all(|&l| l <= 2), "{name}");
        }
        assert!(PATTERNS[0].1.iter().all(|&l| l == 2), "Off is always open");
    }

    #[test]
    fn depth_scales_the_closed_steps() {
        assert_eq!(level_gain(2, 1.0), 1.0);
        assert_eq!(level_gain(0, 1.0), 0.0);
        assert_eq!(level_gain(0, 0.5), 0.5);
        assert_eq!(level_gain(1, 1.0), 0.5);
        assert_eq!(level_gain(0, 0.0), 1.0);
    }

    #[test]
    fn the_clock_prefers_the_host_while_playing_and_free_runs_otherwise() {
        let mut c = ChopClock::default();
        let (s, inc) = c.begin_block(Some(8.0), true, Some(120.0), 48_000.0);
        assert_eq!(s, 8.0);
        assert!((inc - 120.0 / (60.0 * 48_000.0)).abs() < 1e-15);
        c.end_block(s, inc, 48_000);
        assert!((c.free_beats - 10.0).abs() < 1e-9);
        let (s2, _) = c.begin_block(Some(0.0), false, None, 48_000.0);
        assert!((s2 - 10.0).abs() < 1e-9, "stopped host: the free-running clock continues");
        let (_, inc2) = c.begin_block(None, false, Some(f64::NAN), 48_000.0);
        assert!(inc2 > 0.0, "a nonsense tempo falls back to 120");
    }
}
