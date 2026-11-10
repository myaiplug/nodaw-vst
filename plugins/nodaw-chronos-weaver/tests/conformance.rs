//! The host-level tests every NoDAW plugin passes (nodaw-kit/src/testing.rs): automatable
//! parameters, exact latency, bypass and mix null tests, no allocation in process, state,
//! 44.1/48/96 kHz at blocks of 1..4096, and a render of every factory preset.

nodaw_kit::conformance_tests!(
    nodaw_chronos_weaver,
    nodaw_chronos_weaver::NoDawChronosWeaver,
    nodaw_chronos_weaver::PARAMS,
    heavy = [
        ("Morph", "100 %"),
        ("Morph Mode", "Cross"),
        ("Freeze", "On"),
        ("Cloud", "100 %"),
        ("Density", "10 %"),
        ("Scatter", "100 %"),
        ("LFO Depth", "100 %"),
        ("LFO Sync", "1/4")
    ],
    neutral = [("Mix", "100 %"), ("Sustain", "50 %")],
    null_tol = 2e-4
);
