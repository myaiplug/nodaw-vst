//! The host-level tests every NoDAW plugin passes (nodaw-kit/src/testing.rs): automatable
//! parameters, exact latency, bypass and mix null tests, no allocation in process, state,
//! 44.1/48/96 kHz at blocks of 1..4096, and a render of every factory preset.

nodaw_kit::conformance_tests!(
    nodaw_harmonic_space,
    nodaw_harmonic_space::NoDawHarmonicSpace,
    nodaw_harmonic_space::PARAMS,
    heavy = [("Amp", "6 dB"), ("Band 1 Gain", "12 dB"), ("Band 4 Q", "10"), ("Band 4 Gain", "12 dB"), ("Sub", "12 dB"), ("HPF", "120 Hz"), ("LPF", "5000 Hz"),
             ("Delay", "80 %"), ("Feedback", "90 %"), ("Delay Time", "40 ms"), ("Ping-Pong", "On"), ("Reverb", "100 %"), ("Decay", "12 s"),
             ("Flanger", "100 %"), ("Flanger Feedback", "90 %"), ("Width", "200 %")],
    neutral = [],
    null_tol = 1e-4
);
