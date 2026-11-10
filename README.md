# NoDAW VST (private)

NoDAW's plugin line. First plugin: **NoDAW Screw** (chopped & screwed / slowed + reverb effect), CLAP + VST3 on Windows x64 (Linux headless build for CI).
Built on [APF](https://github.com/mihasm/apf) (MIT), vendored in `vendor/apf` with a small patch.

* `crates/nodaw-dsp`  pure DSP, no dependencies, unit-tested
* `plugins/nodaw-screw` the plugin, UI (`ui/`), presets
* `scripts/` build (`build-windows.sh`, `build-linux.sh`), validators (`validate-linux.sh`), UI mock + screenshot rig (`ui-mock/`)
* `docs/` evaluation, design, testing, CI workflow template (`ci-workflow.yml`), validator outputs
* `patches/apf-nodaw.patch` diff of our APF changes vs upstream 6800d6c

Install (Windows, after building or from a release): copy `NoDAW Screw.vst3` folder to `C:\Program Files\Common Files\VST3\`. Needs the WebView2 runtime.

Trial: 14 days full-featured, then activation with a Gumroad license key; see `docs/LICENSING.md` (two TODO constants to fill before selling).

CI: `.github/workflows/ci.yml` (Linux tests + Steinberg VST3 validator + clap-validator + state check; Windows tests, build, VST3 validator, artifact `NoDAW-Screw-windows`).

Status: **unreleased, untested in any DAW, unsigned.** Licence of NoDAW's own code: *all rights reserved placeholder*; owner to decide. See NOTICE and THIRD_PARTY_LICENSES.md.
