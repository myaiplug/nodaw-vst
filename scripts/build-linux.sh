#!/usr/bin/env bash
# Headless Linux build of NoDAW Screw (CLAP + VST3, no web view: APF has no Linux GUI).
# Used for the validators and CI. Needs: rust, cmake>=3.24, ninja, and a C++ compiler
# (set CC/CXX; on a box without libstdc++ use `zig cc` / `zig c++` wrappers).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OUT:-$ROOT/dist/linux}"; B="$ROOT/target/wrapper-linux"
CPM="${CPM_SOURCE_CACHE:-$ROOT/target/cpm-cache}"
cd "$ROOT"
LIBS=$(cargo rustc --release -p nodaw-screw --no-default-features --lib --crate-type staticlib -- --print native-static-libs 2>&1 | grep -o 'native-static-libs: .*' | sed 's/native-static-libs: //' | sed 's/-l//g; s/ /;/g')
cmake -S vendor/apf/wrapper -B "$B" -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DCPM_SOURCE_CACHE="$CPM" -DFETCHCONTENT_BASE_DIR="$B/_deps" \
  -DAPF_STATICLIB="$ROOT/target/release/libnodaw_screw.a" -DAPF_NATIVE_LIBS="$LIBS" \
  -DAPF_NAME="NoDAW Screw" -DAPF_TARGET=nodaw_screw -DAPF_BUNDLE_ID=com.nodaw.screw \
  -DAPF_CLAP_ID=com.nodaw.screw -DAPF_VERSION=0.1.0 "-DAPF_FORMATS=CLAP;VST3" -DAPF_STANDALONE=OFF \
  -DAPF_AU_TYPE=aufx -DAPF_AU_SUBTYPE=NdSc -DAPF_AU_MANUFACTURER=NdDw -DAPF_AU_MANUFACTURER_NAME=NoDAW \
  -DAPF_ASSET_DIR="$OUT"
cmake --build "$B" --target nodaw_screw_all
find "$OUT" -maxdepth 3
