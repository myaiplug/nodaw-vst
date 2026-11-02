# Testing

Run everything: `scripts/validate-linux.sh` (needs the validator binaries below). Latest outputs are in `docs/results/`.

| Check | Result |
|---|---|
| `cargo test --workspace` | 73 tests pass: 42 DSP, 19 plugin lib, 8 process (zero audio-thread allocations with in-block automation, exact latency/tail, state round trip, transparent bypass, 6 sample rates 22.05-192 kHz x 5 block sizes 1-4096 all finite), 2 presets, 2 UI binding |
| `cargo clippy --all-targets -D warnings` | clean (our crates) |
| Steinberg VST3 SDK 3.8 `validator` | 47 passed, 0 failed |
| `clap-validator` 0.4.1 | 44 run, 36 passed, 0 failed, 8 skipped (double-precision and note-port tests; plugin has neither) |
| `vst3-state-check` (APF's tool): save -> load -> render-compare, 8 scenarios, default and "Trap Roll Chop" preset | 8/8 ok |
| Same tool with `--inject-stale` (negative control) | fails 8/8, as it must |

## Bugs this found in our own plugin
Restoring a session after activation made the output differ from the saved sound by up to 0.39 (sample difference) because (a) smoothers glided from old values,
(b) wobble noise seeds and the chop clock were not reset. Fixed (`reset_to`, `fresh` snap); now bit-for-bit repeatable.

## Not tested
* Any real DAW, any Windows run, the WebView2 UI inside a host, host-side plugin delay compensation, AU/macOS.
* The Linux binaries are the headless (no-UI) build.

## Building the validators
* VST3 validator: clone VST3 SDK 3.8, `cmake -DSMTG_ENABLE_VST3_HOSTING_EXAMPLES=ON`; on a box without X11 dev headers drop the X11 hosting examples.
* clap-validator: `cargo build --release` from free-audio/clap-validator v0.4.1 after `git apply vendor/apf/scripts/clap-validator-0.4.1-preset-deadlock.patch`.
* vst3-state-check: `cmake` in `vendor/apf/tools` against the VST3 SDK.
