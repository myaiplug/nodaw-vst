//! The host-level tests every NoDAW plugin passes (nodaw-kit/src/testing.rs): automatable
//! parameters, exact latency, bypass and mix null tests, no allocation in process, state,
//! 44.1/48/96 kHz at blocks of 1..4096, and a render of every factory preset.

nodaw_kit::conformance_tests!(
    nodaw_temporal_pitch_portal,
    nodaw_temporal_pitch_portal::NoDawTemporalPitchPortal,
    nodaw_temporal_pitch_portal::PARAMS,
    heavy = [
        ("Time Shift", "50 %"),
        ("Pitch Bend", "-12 st"),
        ("Drift", "50 ct"),
        ("Tone", "-100 %"),
        ("Echo", "80 %"),
        ("Echo Feedback", "70 %"),
        ("Warp Sync", "1/4")
    ],
    neutral = [("Mix", "100 %"), ("Glide", "0 ms")],
    null_tol = 1e-5
);
