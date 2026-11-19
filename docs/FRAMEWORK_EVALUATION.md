# Framework evaluation: APF vs alternatives (2026-11-02)

**Verdict: APF (MIT) is fit for NoDAW's first plugin, with the caveats below. It was used.**

## What APF is
Rust plugin framework (https://github.com/mihasm/apf, MIT, (c) 2026 Miha Smrekar, pre-1.0). Its own CLAP backend; the UI is a
web page served from memory (`apf://`) into a `wry` web view (WebView2 on Windows, WKWebView on macOS). **VST3/AU/standalone are produced by
[clap-wrapper](https://github.com/free-audio/clap-wrapper) v0.17.0 (MIT, C++/CMake)** wrapping the Rust staticlib. VST3 SDK 3.8.0 is MIT
(verified), so no GPL anywhere in the graph.

## Questions asked
| Question | Answer | Evidence |
|---|---|---|
| Builds a VST3 on Windows with WebView2? | **Yes.** Cross-compiled from Linux (cargo-xwin + clang-cl + Ninja): `NoDAW Screw.vst3` is a PE32+ DLL exporting `GetPluginFactory`/`InitDll`/`ExitDll`, with WebView2 statically-linked loader and the UI assets embedded. Upstream reports it working in REAPER 7.83 on Windows 11. **Not run on Windows by us.** | `scripts/build-windows.sh` |
| Threading | Audio thread never takes locks or allocates: atomic parameter store, wait-free SPSC for UI<->audio, FTZ/DAZ guard, panics caught at the C ABI. `apf-test` ships a counting allocator that fails tests on audio-thread allocation. | vendor/apf docs; `tests/process.rs` |
| Parameter automation | Every `ParamDef` is a CLAP parameter -> VST3 parameter (begin/set/end gestures from the UI). Smoothing is ours. | `vst3-state-check`, VST3 validator |
| State save/restore | Parameter values + plugin chunk (custom chop pattern) in the host's state. Found and fixed two real restore bugs in *our* engine using `vst3-state-check` (see TESTING.md). | docs/results |
| Host compatibility | Wrapper-based VST3 is mature (clap-wrapper is widely used) but APF itself has had little real-world exposure. | caveat |
| Latency | **Upstream has no latency/tail API.** We added `Plugin::latency()`/`tail()` + CLAP `latency`/`tail` extensions (patch 1), which the wrapper maps to VST3. | patches/apf-nodaw.patch |

## Caveats (be honest)
1. Pre-1.0, single maintainer, young. We vendor it and carry a small patch (~160 lines in 5 files) so we are not blocked by upstream.
2. **No Linux GUI**: the web view only exists on Windows/macOS. UI could only be verified by rendering the real `ui/` + real `apf.js` in headless Chrome against a mock server (`scripts/ui-mock`) with synthetic meter data; the Linux plugin is headless.
3. The VST3 path depends on CMake + C++ toolchain (clap-wrapper) and on downloading SDKs at build time (pinned commits).
4. Web UI state is polled (~30 Hz) for meters; the canvas renders at 60 fps by interpolating between frames.
5. Effect layout is stereo-only in our plugin (mono hosts get a stereo bus; the validators pass).
6. `clap-validator` 0.4.1 deadlocks on a preset-discovery test in this setup; a small patch is kept in `vendor/apf/scripts/`.
7. WebView2 runtime must exist on the user's PC (preinstalled on Windows 11 and current Windows 10).

## Alternatives
| Option | Licence | Verdict |
|---|---|---|
| **nih-plug** (+ nih-plug-webview) | nih-plug ISC, **but its VST3 bindings (`vst3-sys`) are GPLv3** (stated in nih-plug's README: VST3 plugins built with it "need to be able to comply with GPLv3" unless the bindings are replaced). nih-plug-webview README: "work in progress, not production-ready", only tested on macOS. | **Rejected for a paid closed VST3** unless using the separate MIT `vst3` crate bindings, which would be extra porting work. |
| **JUCE** | AGPLv3 or paid commercial | Avoided per licence rule. |
| **iPlug2** | zlib-style, MIT-compatible | Viable C++ alternative; UI is native/WebView per-platform; larger C++ surface; not evaluated further. |
| **Raw VST3 SDK 3.8 (MIT) + own UI** | MIT | Most control, most work. |
| **clack / clap-sys + wrapper** | MIT | APF is basically this with a UI and param layer already done. |

Only the nih-plug README/Cargo.toml were read; its code was not built.
