//! The host-level tests every NoDAW plugin passes (nodaw-kit/src/testing.rs): automatable
//! parameters, exact latency, bypass and mix null tests, no allocation in process, state,
//! 44.1/48/96 kHz at blocks of 1..4096, and a render of every factory preset.

nodaw_kit::conformance_tests!(
    nodaw_chronos_dynamic_eq,
    nodaw_chronos_dynamic_eq::NoDawChronosDynamicEq,
    nodaw_chronos_dynamic_eq::PARAMS,
    heavy = [
        ("Band 1 Gain", "-12 dB"),
        ("Band 1 Dynamics", "Expand"),
        ("Band 1 Range", "24 dB"),
        ("Band 1 Threshold", "-60 dB"),
        ("Band 4 Q", "18"),
        ("Band 4 Gain", "-24 dB"),
        ("Band 4 Dynamics", "Compress"),
        ("Band 4 Attack", "0.1 ms"),
        ("Band 7 Type", "Bell"),
        ("Band 7 Q", "0.1"),
        ("Band 7 Dynamics", "Expand"),
        ("Dynamics", "200 %")
    ],
    neutral = [],
    null_tol = 1e-6
);
