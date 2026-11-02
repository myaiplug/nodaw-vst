# NoDAW patches to APF

Base: https://github.com/mihasm/apf @ 6800d6c24a3a56aa5887050d16396960cafd5a99 (MIT, (c) Miha Smrekar).
Everything else in this folder is unmodified upstream, apart from the trimmed workspace
`Cargo.toml` (no examples/xtask/host crates) and this file. A unified diff against upstream is kept in
`../../patches/apf-nodaw.patch`.

1. `Plugin::latency()` / `Plugin::tail()` + CLAP `clap.latency` and `clap.tail` extensions
   (upstream has neither). clap-wrapper maps them to VST3 `getLatencySamples` / `getTailSamples`,
   so hosts can do plugin delay compensation. Values are read on the main thread right after
   `activate` and stored in atomics; the host's query never touches the audio-thread plugin object.
2. `apf-test`: `Instance::latency()` / `Instance::tail()` for tests.
3. `apf-webview`: `APF_UI_DIR` and `APF_UI_INJECT_JS` environment switches only work in debug/`devtools`
   builds (upstream docs/todo.md lists this as open). Shipped plugins do not read them.
