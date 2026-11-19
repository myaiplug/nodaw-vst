**NoDAW plugin trial builds for Windows x64 (VST3 + CLAP).** Every plugin runs fully featured for
14 days from its first launch; after that it keeps working but goes silent for 0.5 s every 45 s
until activated with a license key.

Plugins: NoDAW Screw, Racks, Harmonic Space, Chronos Dynamic EQ, Temporal Pitch Portal, Chronos Weaver.

Downloads:
* `NoDAW-Plugins-win64-trial-setup.exe`: installer (puts VST3s in Common Files\VST3 and CLAPs in Common Files\CLAP; pick which plugins).
* `NoDAW-Plugins-win64-trial.zip`: all six, copy by hand (see INSTALL-WINDOWS.txt inside).
* `NoDAW-<Plugin>-win64-trial.zip`: one plugin.

Known limits:
* **Not code-signed**: Windows SmartScreen may warn ("More info" > "Run anyway").
* Windows only for now; no macOS build yet.
* Passed the Steinberg VST3 validator in CI; not yet tested by hand in every DAW.
* Needs the Microsoft Edge WebView2 runtime (preinstalled on most Windows 10/11 PCs).
* Purchase/activation opens once the Gumroad products are live.
