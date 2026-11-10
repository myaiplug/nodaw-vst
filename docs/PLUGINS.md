# The NoDAW plugin suite

Six CLAP + VST3 effects on the same stack: Rust on the vendored, patched APF (`vendor/apf`), a
WebView UI (WebView2 on Windows) built on the shared kit (`ui-kit/`), DSP in `crates/nodaw-dsp`,
and the trial, licence, controller and host-test code in `crates/nodaw-kit`. NoDAW Screw is
documented in the README; this file covers the five plugins added with it.

Common to every plugin:

- **Parameters:** every control is a host parameter (automatable, visible, unique stable IDs,
  append-only lists). The only other session state is the faceplate/colourway and the A/B
  slots. Undo/redo (Ctrl+Z / Ctrl+Shift+Z), A/B compare and copy, factory and user presets,
  MIDI learn (right-click) and typed values (click a value) come from the kit.
- **Trial (the same as Screw, per product):** 14 days of full use from the first launch, then
  Gumroad licence activation. After the trial an unlicensed copy keeps working but goes silent
  for about 0.5 s every 45 s (click-free ramps). Audio is never blocked. A licensed copy
  re-checks online every 30 days and stays licensed when it is offline. The state file is
  HMAC-signed, with a shadow copy of the trial start. Each plugin has its own state folder
  (`%APPDATA%\NoDAW Labs\<Plugin>\`), its own HMAC key, and its own Gumroad product (TODO
  constants below).
- **Real-time safety:** no allocation, lock or I/O in `process()` (tested with a counting
  allocator while every parameter is automated). Latency is constant per sample rate and
  reported exactly. Bypass and mix are sample-aligned against the latency-delayed dry signal.
- **Tests (`cargo test --workspace --no-default-features`):** the kit's conformance suite
  (`nodaw_kit::conformance_tests!`) runs on each plugin. It checks parameter flags and IDs,
  exact latency, bypass and mix-0 null tests, a neutral-settings null test, no allocation, the
  state round trip and garbage rejection, stability (finite and denormal-free, no runaway) at
  44.1/48/96 kHz with blocks of 1/16/64/512/4096, and a sanity check plus a WAV render of every
  factory preset (`target/renders/<crate>/*.wav`, from the kit's test signal). It also runs the
  page↔parameter binding test (`ui_binds_tests!`) and each engine's own DSP unit tests.

| Plugin | Crate | CLAP/VST3 id | AU subtype | Latency @48k | Params | Presets |
|---|---|---|---|---|---|---|
| NoDAW Racks | `nodaw-racks` | `com.nodaw.racks` | NdRk | 112 smp (2.3 ms) | 54 | 12 |
| NoDAW Harmonic Space | `nodaw-harmonic-space` | `com.nodaw.harmonic-space` | NdHs | 0 | 58 | 21 |
| NoDAW Chronos Dynamic EQ | `nodaw-chronos-dynamic-eq` | `com.nodaw.chronos-dynamic-eq` | NdDq | 0 | 89 | 12 |
| NoDAW Temporal Pitch Portal | `nodaw-temporal-pitch-portal` | `com.nodaw.temporal-pitch-portal` | NdTp | 3320 smp (69 ms) | 14 | 14 |
| NoDAW Chronos Weaver | `nodaw-chronos-weaver` | `com.nodaw.chronos-weaver` | NdCw | 2048 smp (43 ms; 4096 above 50 kHz) | 17 | 12 |

## TODO(owner) constants

Each plugin's `src/lib.rs` has a `PRODUCT` with:

| Plugin | `gumroad_product_id` | `buy_url` |
|---|---|---|
| Racks | `TODO_GUMROAD_PRODUCT_ID_RACKS` | https://nodawlabs.gumroad.com/l/nodaw-racks |
| Harmonic Space | `TODO_GUMROAD_PRODUCT_ID_HARMONIC_SPACE` | https://nodawlabs.gumroad.com/l/nodaw-harmonic-space |
| Chronos Dynamic EQ | `TODO_GUMROAD_PRODUCT_ID_CHRONOS_DYNAMIC_EQ` | https://nodawlabs.gumroad.com/l/nodaw-chronos-dynamic-eq |
| Temporal Pitch Portal | `TODO_GUMROAD_PRODUCT_ID_TEMPORAL_PITCH_PORTAL` | https://nodawlabs.gumroad.com/l/nodaw-temporal-pitch-portal |
| Chronos Weaver | `TODO_GUMROAD_PRODUCT_ID_CHRONOS_WEAVER` | https://nodawlabs.gumroad.com/l/nodaw-chronos-weaver |

Screw's are in `plugins/nodaw-screw/src/license.rs` (`GUMROAD_PRODUCT_ID`, `BUY_URL`), which now
uses the shared `nodaw_kit::license` with Screw's unchanged product, key and folder. Until a
product ID is set, the activation dialog says activation is not available yet; the trial works.
The buy URLs are guesses at the Gumroad slugs.

## NoDAW Racks

A modular channel rack: four modules in a reorderable chain (drag the module headers, or use
the Chain Order parameter; 24 orders), each with its own power switch, then a master section.

- **Saturator:** Tape, Tube and Diode curves with 2× oversampling, Drive, Tone, Sat Mix and
  Auto Gain.
- **7-band EQ:** bands 1 and 7 are switchable shelf/cut (cut = 12 dB/oct high/low-pass); the
  other bands are bells. Drag the nodes on the analyser.
- **Spatial:** Width, Haas, Pan, Mid and Side gain, and Mono Bass below a set frequency, with a
  correlation meter.
- **Reactor:** a limiter with a fixed 1.5 ms lookahead and true-peak detection; Push, Ceiling,
  Release and Adaptive release.
- **Master:** Input, Mix and Output.
- **Display and looks:** a spectrum with the EQ curve, in/out meters and GR. Four skins: cobalt,
  obsidian, silver and plastic.

## NoDAW Harmonic Space (the Spectral Space Designer)

- **Chain:** Amp → a 7-band EQ (60 Hz low shelf, five bells, 16 kHz high shelf) → Sub shelf →
  HPF/LPF (12 dB/oct) → three parallel returns (a feedback delay with tone and ping-pong, an FDN
  reverb with decay, size, pre-delay and damping, and a stereo flanger) → Width → Output → Mix.
- **Band Solo and Mute:** Solo auditions the band's region (a band-pass, or a low/high-pass on
  the shelves, crossfaded). Mute takes the band to −36 dB.
- **Space Recipes:** the 20 recipes from the prototype, converted with the prototype's own
  knob mappings, are the factory presets, grouped by family (Vocals, Low end, Space,
  Character, Fix-it).
- **Display and looks:** the analyser shows the whole shape, with a node per band plus the H/L
  filter nodes. There are effect activity LEDs and a correlation meter. Faceplates: dark, or
  brushed aluminium (light).

## NoDAW Chronos Dynamic EQ

- **Bands:** 7 bands in series. Each has a type (Bell, Low Shelf or High Shelf), Freq, Gain
  (±24 dB) and Q (0.1–18), and a dynamics mode:
  - Static, Compress (cuts when the band is over the threshold) or Expand (lifts it);
  - Threshold, Ratio, Range (to 24 dB), Attack and Release;
  - On and Solo (audition).
- **How a band works:** each band is `x + (G−1)·F(x)`, so an idle band is bit-transparent and
  gain changes are zipper-free.
- **Master:** Input, Output, Mix and Dynamics amount (0–200 %, which scales every band's range,
  capped at 24 dB).
- **Display:** the live curve (static plus dynamic gain), the dashed static curve, a threshold
  marker, node rings showing each band's dynamic gain, and band chips with ring meters. The
  GR meter shows the deepest cut.
- **Not implemented:** the marketing page's "linear phase mode". The bands are minimum-phase
  IIR with zero latency.

## NoDAW Temporal Pitch Portal

- **Time Shift (50–200 %):** a time warp synced to the host tempo. A plugin cannot change the
  host's timeline, so time is warped inside each sync period (1/4, ½ bar, 1 bar or 2 bars). At
  50 % the first half of each period is played over the whole period (half-time); at 200 % the
  previous period is played twice as fast.
  - **Stretch** keeps the pitch: WSOLA grains, aligned by cross-correlation.
  - **Tape** bends the pitch with the speed: a varispeed deck.
  - The longest period is 4 s; longer bars are clamped.
- **Pitch Bend (±12 st):** independent of time. It uses the SOLA shifter from Screw and has
  Glide.
- **Effects:** Drift is a wandering micro-pitch in cents, with its own rate. Tone is a tilt.
  Echo is a return on the shifted signal, with time and feedback.
- **Master:** Output and Mix.
- **Presets:** the prototype's 5 Warp Zones and 5 ScrewAI Flavors, plus 3 extras. The prototype
  coupled time and pitch; here they are independent.
- **Display:** the Dimensional Coordinates are read from the engine (pitch actually applied,
  temporal pull, drift, speed ripple). The animated portal's rings turn with the time and warp
  with the pitch, and its outer arc is the warp period. The Dimensional History shows a bubble
  per edit (click to return; the last 20 are kept). There is also a spectrum, meters, and a
  dark or light look.

## NoDAW Chronos Weaver

- **STFT:** Hann windows at 75 % overlap; the FFT size is fixed per sample rate and is the
  latency.
- **Morph A→B (0–100 %):**
  - **Source B:** Past is the signal's own spectrum Weave ms ago (20–2000 ms). Snapshot is a
    capture taken with Capture B, a parameter the button pulses.
  - **Mode:** Blend is a log-magnitude morph. Cross is cross-synthesis: B's spectral envelope
    on A's fine structure.
- **Freeze:** captures the spectrum and sustains it with phase-vocoder phase advance. Sustain
  runs from about 1 s to infinite.
- **Cloud:** a granular cloud in the spectral domain. Each bin is smeared in time (Grain
  5–2000 ms), gated by Density (energy-compensated) and phase-scattered.
- **Morph LFO:** Depth, tempo sync (¼ to 4 bars) and shape (sine, triangle or ramp).
- **Master:** Output and Mix.
- **Display:** a 3D spectrum (frequency, time and level; drag to orbit) with source B's
  spectrum on the front edge, a morph track showing the knob and the live LFO position, a
  freeze badge and meters. Colourways: nebula and ice.
- **Not implemented / limits:**
  - The captured snapshot is not saved with the session (capture again after reloading).
  - The marketing page's "two audio sources" would need a sidechain input, which APF does not
    have yet. Here source B is the snapshot or the past.
  - There is one morph target, not four snapshot slots, and no drawable morph envelope.
  - The 6 "engines" of the marketing page are covered by the Blend/Cross modes, Freeze and
    Cloud.

## Building and checking

- **Windows CI** (`.github/workflows/ci.yml`, job `windows`):
  - `scripts/build-windows-ci.sh` builds every plugin in `scripts/plugins.txt` as `.vst3` and
    `.clap` with a static CRT.
  - Steinberg's validator runs on each `.vst3`.
  - The artifact **NoDAW-plugins-windows** holds the binaries and the validator logs.
- **Linux CI** (job `linux-validators`):
  - The headless builds (`scripts/build-linux.sh`) go through the VST3 validator,
    clap-validator 0.4.1 and `vst3-state-check` (plain, with a preset, and a negative control),
    for every plugin.
  - Results are in the artifact **validator-results**.
- **PULP** (job `pulp-bundle`):
  - Builds PULP from `nodaw/luxury-ui` and copies every `.vst3` into `dist/PULP/Plugins`.
  - Runs `scripts/pulp_suite_check.txt`: PULP must find and load each plugin, play audio
    through them, and open and capture each plugin window.
  - Artifacts: **PULP-with-NoDAW-plugins-windows** (the bundle) and **pulp-bundle-check** (the
    log and window captures).
- **UI mock:** each plugin's page runs in Chrome against `scripts/ui-mock/suite_server.py` (a
  model of the controller and meter frames) for the screenshots in
  `docs/screenshots/<plugin>/`.
