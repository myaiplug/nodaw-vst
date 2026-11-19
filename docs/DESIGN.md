# NoDAW Screw: design

Real-time "chopped and screwed" effect. First plugin because it is a real-time DSP effect usable in a DAW today (Liminal StemSplit is a
heavy offline ML job, a poor fit for a first VST).

## Signal flow
in -> input gain -> [pitch (SOLA varispeed) -> wobble (wow/flutter) -> lowpass -> 150 Hz low-shelf "Warmth" -> chop gate -> reverb (parallel) -> M/S width]
-> mix vs. delay-matched dry -> output gain -> true-peak limiter -> 8 ms bypass crossfade.

* Latency = pitch + wobble + limiter, **reported to the host** (1536 samples = 32 ms at 48 kHz; mean of the SOLA variable window). Tail 20.25 s.
* Dry path is delay-matched, so Mix at 0% is phase-aligned.
* Denormals: FTZ/DAZ guard on the audio thread plus explicit flush in filter/delay feedback paths.
* No allocation/locks in `process`; all buffers sized in `activate` (main thread). Sample-rate and max-block changes re-run `activate`.
* Smoothers snap on the first block after (re)load so a restored session starts exactly at the saved sound.

## Parameters (all host-automatable, 21)
Bypass; Input, Output (+-24 dB); Mix; Pitch (-12..12 st); Wow (ct), Wow Rate, Flutter; Lowpass; Warmth; Reverb, Decay, Size, Damping, Pre-delay;
Chop Pattern, Chop Rate, Chop Depth; Width; Limiter on/off, Ceiling. The custom chop step pattern is stored in the plugin state chunk (not a parameter). 12 factory presets.

## UI
Single web page (`plugins/nodaw-screw/ui`): HalfScrew design tokens, Space Grotesk / Bebas Neue / JetBrains Mono (embedded woff2), pro-dark FabFilter/iZotope-style layout.
* Resizable (760x480 minimum up to 4K), HiDPI-aware canvases (devicePixelRatio), CSS grid.
* Spectrum: 160 bands input/output, 60 fps canvas with peak-hold, cursor frequency/dB readout. Meters: in/out peak + limiter reduction.
* Knobs: drag (fine with Shift), double-click resets, value tooltip, focusable with ARIA value text; every control is bound via `data-param` so it is a host parameter (gesture begin/set/end).
* Chop pattern editor (click-drag steps), A/B compare, undo/redo (UI edits only, Ctrl+Z / Ctrl+Shift+Z), preset browser (prev/next/dropdown, save/delete user presets), bypass.
* `tests/ui_binds.rs` fails the build if the HTML/JS references a parameter that doesn't exist or a parameter has no control.

## Honest limits
* Screenshots in `/workspace/apf-shots` are from headless Chrome + mock server, not the WebView2 host; synthetic spectrum data.
* Host automation moves are not in the plugin's undo stack.
* SOLA pitch shifter: variable latency and artefacts on dense polyphonic material (fine for vocals, drums, beats; a phase-vocoder/hq mode is a future item).
* Wow/flutter and reverb are inherently stereo.
* Spectrum dB scale (0 dBFS = full-scale sine) not verified against the engine in a real host.
