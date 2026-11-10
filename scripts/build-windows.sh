#!/usr/bin/env bash
# Cross-compile the Windows x64 VST3 + CLAP of NoDAW Screw from Linux with cargo-xwin
# (clang-cl + the Microsoft CRT/SDK downloaded by xwin: you accept Microsoft's licence terms
# when xwin fetches them). On Windows itself use `cargo xtask bundle` from vendor/apf instead.
#
# Env: XWIN_CACHE_DIR (xwin download cache), LLVM bin dir on PATH (clang-cl, lld-link, llvm-lib),
#      cmake + ninja on PATH, CPM_SOURCE_CACHE (optional).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OUT:-$ROOT/dist/windows}"; B="$ROOT/target/wrapper-windows"
XC="${XWIN_CACHE_DIR:?set XWIN_CACHE_DIR}"
CPM="${CPM_SOURCE_CACHE:-$ROOT/target/cpm-cache}"
cd "$ROOT"
# crt-static for every crate (ring's C code too), so the CRT matches the wrapper's /MT.
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
LIBS=$(cargo xwin rustc --release --target x86_64-pc-windows-msvc -p nodaw-screw --lib --crate-type staticlib -- --print native-static-libs 2>&1 | grep -o 'native-static-libs: .*' | sed 's/native-static-libs: //' | tr ' ' '\n' | grep -v '^/defaultlib' | grep -v '^$' | paste -sd';')
echo "native libs: $LIBS"
cmake -S vendor/apf/wrapper -B "$B" -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_TOOLCHAIN_FILE="$XC/cmake/clang-cl/x86_64-pc-windows-msvc-toolchain.cmake" \
  -DFETCHCONTENT_BASE_DIR="$B/_deps" -DCPM_SOURCE_CACHE="$CPM" \
  -DAPF_STATICLIB="$ROOT/target/x86_64-pc-windows-msvc/release/nodaw_screw.lib" -DAPF_NATIVE_LIBS="$LIBS" \
  -DAPF_NAME="NoDAW Screw" -DAPF_TARGET=nodaw_screw -DAPF_BUNDLE_ID=com.nodaw.screw \
  -DAPF_VERSION=0.2.0 "-DAPF_FORMATS=CLAP;VST3" -DAPF_STANDALONE=OFF \
  -DAPF_AU_TYPE=aufx -DAPF_AU_SUBTYPE=NdSc -DAPF_AU_MANUFACTURER=NdDw -DAPF_AU_MANUFACTURER_NAME=NoDAW \
  -DAPF_ASSET_DIR="$OUT"
cmake --build "$B" --target nodaw_screw_all
find "$OUT" -maxdepth 4
