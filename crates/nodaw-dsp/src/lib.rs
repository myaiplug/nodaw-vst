//! NoDAW DSP: the signal-processing blocks of the NoDAW plugins.
//!
//! Design rules, enforced by the tests in this crate:
//! * `#![forbid(unsafe_code)]
#![allow(clippy::needless_range_loop)] // index loops mirror the delay-line math` and no dependencies.
//! * Every buffer is allocated in a constructor (called from the plugin's `activate`, never from
//!   the audio thread). The per-sample `process`/`tick` methods never allocate and never lock.
//! * Feedback paths flush denormals themselves, so the plugin is quiet on CPUs and hosts that do
//!   not set FTZ/DAZ.
//! * Latency is an exact integer number of samples that each block reports via `latency()`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod chop;
pub mod delay;
pub mod fft;
pub mod filter;
pub mod limiter;
pub mod meter;
pub mod pitch;
pub mod reverb;
pub mod util;
pub mod wobble;

// Added for the plugin suite (Racks, Harmonic Space, Chronos, Temporal Pitch Portal).
pub mod dynamics;
pub mod eq;
pub mod modfx;
pub mod sat;
pub mod stft;
pub mod timewarp;

pub use util::{db_to_gain, gain_to_db};
