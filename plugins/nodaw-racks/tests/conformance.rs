//! The host-level tests every NoDAW plugin passes (nodaw-kit/src/testing.rs): automatable
//! parameters, exact latency, bypass and mix null tests, no allocation in process, state,
//! 44.1/48/96 kHz at blocks of 1..4096, and a render of every factory preset.

nodaw_kit::conformance_tests!(
    nodaw_racks,
    nodaw_racks::NoDawRacks,
    nodaw_racks::PARAMS,
    heavy = [
        ("Drive", "24 dB"),
        ("Curve", "Diode"),
        ("Band 1 Gain", "18 dB"),
        ("Band 7 Type", "High Cut"),
        ("Width", "200 %"),
        ("Haas", "20 ms"),
        ("Mono Bass", "200 Hz"),
        ("Push", "18 dB"),
        ("Chain Order", "LIM > SPACE > EQ > SAT")
    ],
    neutral = [("Saturator On", "Off"), ("Limiter On", "Off")],
    null_tol = 2e-3
);
