# Third-party licences

Policy: only MIT / Apache-2.0 / BSD / ISC / Zlib / OFL (fonts) for anything shipped in a paid closed-source plugin.
GPL / AGPL / LGPL / non-commercial / unlicensed code is not used. JUCE (AGPL or paid) and nih-plug's VST3 bindings (GPLv3) are deliberately avoided.

## Bundled / linked outside Cargo

| Component | Version | Licence | Notes |
|---|---|---|---|
| APF | upstream 6800d6c (vendored, patched) | MIT, (c) 2026 Miha Smrekar | `vendor/apf/LICENSE`; patch in `patches/apf-nodaw.patch` |
| clap-wrapper | 0.17.0 | MIT, (c) 2022 defiantnerd | builds VST3 from the CLAP plugin; fetched by CMake at build time |
| CLAP SDK | d05ad08 | MIT | headers only |
| Steinberg VST3 SDK | 3.8.0 (b7e0693) | MIT (since 3.8) | trademark: "VST" is a registered trademark of Steinberg Media Technologies GmbH; follow their trademark guidelines |
| Microsoft WebView2 Runtime | evergreen, on the user's PC | Microsoft's own terms (not redistributed by us) | the WebView2 loader is statically linked via webview2-com-sys (MIT crate); **check Microsoft's WebView2 SDK licence terms for the embedded loader before commercial release** |
| Space Grotesk | | SIL OFL 1.1 | `docs/licenses/SpaceGrotesk-OFL.txt` |
| Bebas Neue | | SIL OFL 1.1 | `docs/licenses/BebasNeue-OFL.txt` |
| JetBrains Mono | | SIL OFL 1.1 | `docs/licenses/JetBrainsMono-OFL.txt` |
| option-ext (replacement) | vendor/apf/third_party | MIT | replaces an MPL-2.0 transitive dependency of wry |

## Rust crates in the Windows build graph (cargo metadata, x86_64-pc-windows-msvc)

All are permissive. Licence texts are in each crate's source (crates.io) and are reproduced by `cargo about` / `cargo license` at release time.

| Crate | Version | Licence |
|---|---|---|
| apf | 0.1.0 | MIT |
| apf-test | 0.1.0 | MIT |
| apf-webview | 0.1.0 | MIT |
| bytes | 1.12.1 | MIT |
| clap-sys | 0.5.0 | MIT/Apache-2.0 |
| cookie | 0.18.2 | MIT OR Apache-2.0 |
| deranged | 0.5.9 | MIT OR Apache-2.0 |
| dpi | 0.1.2 | Apache-2.0 AND MIT |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| num-conv | 0.2.2 | MIT OR Apache-2.0 |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| time | 0.3.55 | MIT OR Apache-2.0 |
| time-core | 0.1.9 | MIT OR Apache-2.0 |
| time-macros | 0.2.32 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| version_check | 0.9.5 | MIT/Apache-2.0 |
| webview2-com | 0.39.1 | MIT |
| webview2-com-macros | 0.8.1 | MIT |
| webview2-com-sys | 0.39.1 | MIT |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 |
| windows-version | 0.1.7 | MIT OR Apache-2.0 |
| wry | 0.57.1 | Apache-2.0 OR MIT |

Dev tooling only (not shipped): puppeteer-core (Apache-2.0) in `scripts/ui-mock`, Steinberg VST3 SDK validator (MIT), clap-validator (MIT), cargo-xwin (MIT/Apache-2.0).
Microsoft CRT/SDK headers and libs fetched by `xwin` for cross-compiling are under Microsoft's licence terms (accepted at download); the produced binary links the static CRT (`+crt-static`).

Regenerate before each release: `cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc` and run `cargo deny check licenses` (config in vendor/apf/deny.toml).
