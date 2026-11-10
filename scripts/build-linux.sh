#!/usr/bin/env bash
# Headless Linux build of the NoDAW plugins (CLAP + VST3, no web view: APF has no Linux GUI).
# Used for the validators and CI. Needs: rust, cmake>=3.24, ninja, and a C++ compiler
# (set CC/CXX; on a box without libstdc++ use `zig cc` / `zig c++` wrappers).
#
#     scripts/build-linux.sh                 # every plugin in scripts/plugins.txt
#     scripts/build-linux.sh nodaw-racks     # one
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${OUT:-$ROOT/dist/linux}"
CPM="${CPM_SOURCE_CACHE:-$ROOT/target/cpm-cache}"
cd "$ROOT"
mapfile -t PLUGINS < <(grep -v '^#' scripts/plugins.txt | tr -d '\r' | grep -v '^$')
for line in "${PLUGINS[@]}"; do
  IFS='|' read -r crate name target id version sub _preset _match <<< "$line"
  [ -n "$crate" ] || continue
  if [ $# -gt 0 ] && [[ " $* " != *" $crate "* ]]; then continue; fi
  B="$ROOT/target/wrapper-linux/$target"
  lib=${crate//-/_}
  LIBS=$(cargo rustc --color never --release -p "$crate" --no-default-features --lib --crate-type staticlib -- --print native-static-libs 2>&1 | grep -o 'native-static-libs: .*' | sed 's/native-static-libs: //' | tr ' ' '\n' | sed 's/^-l//' | grep -v '^$' | awk '!seen[$0]++' | paste -sd';')
  # (--color never: in CI cargo colours the note, and the escape codes would end up in the last library name)
  echo "$name native libs: $LIBS"
  cmake -S vendor/apf/wrapper -B "$B" -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
    -DCPM_SOURCE_CACHE="$CPM" -DFETCHCONTENT_BASE_DIR="$B/_deps" \
    -DAPF_STATICLIB="$ROOT/target/release/lib$lib.a" -DAPF_NATIVE_LIBS="$LIBS" \
    -DAPF_NAME="$name" -DAPF_TARGET="$target" -DAPF_BUNDLE_ID="$id" \
    -DAPF_CLAP_ID="$id" -DAPF_VERSION="$version" "-DAPF_FORMATS=CLAP;VST3" -DAPF_STANDALONE=OFF \
    -DAPF_AU_TYPE=aufx -DAPF_AU_SUBTYPE="$sub" -DAPF_AU_MANUFACTURER=NdDw -DAPF_AU_MANUFACTURER_NAME=NoDAW \
    -DAPF_ASSET_DIR="$OUT"
  cmake --build "$B" --target "${target}_all"
done
find "$OUT" -maxdepth 3
