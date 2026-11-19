#!/usr/bin/env bash
# CI (windows-2022, Git Bash): build every plugin in scripts/plugins.txt as .vst3 + .clap with
# the Visual Studio generator and a static CRT (no VC++ redistributable needed), and gather them
# in dist/windows/pkg/.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
export RUSTFLAGS="-C target-feature=+crt-static" CARGO_TARGET_DIR=target/crt-static
R=$(cygpath -m "$ROOT")
mkdir -p dist/windows/pkg
mapfile -t PLUGINS < <(grep -v '^#' scripts/plugins.txt | tr -d '\r' | grep -v '^$')
for line in "${PLUGINS[@]}"; do
  IFS='|' read -r crate name target id version sub _preset _match <<< "$line"
  [ -n "$crate" ] || continue
  lib=${crate//-/_}
  LIBS=$(cargo rustc --color never --release -p "$crate" --lib --crate-type staticlib -- --print native-static-libs 2>&1 | tr -d '\r' | grep -o 'native-static-libs: .*' | sed 's/native-static-libs: //' | tr ' ' '\n' | grep -v '^/defaultlib' | grep -v '^$' | paste -sd';')
  echo "$name native libs: $LIBS"
  B="C:/b/$target"
  cmake -S vendor/apf/wrapper -B "$B" -G "Visual Studio 17 2022" -A x64 \
    -DFETCHCONTENT_BASE_DIR="$B/_deps" -DCPM_SOURCE_CACHE=C:/cpm \
    -DAPF_STATICLIB="$R/target/crt-static/release/$lib.lib" "-DAPF_NATIVE_LIBS=$LIBS" \
    "-DAPF_NAME=$name" -DAPF_TARGET="$target" -DAPF_BUNDLE_ID="$id" -DAPF_CLAP_ID="$id" \
    -DAPF_VERSION="$version" "-DAPF_FORMATS=CLAP;VST3" -DAPF_STANDALONE=OFF \
    -DAPF_AU_TYPE=aufx -DAPF_AU_SUBTYPE="$sub" -DAPF_AU_MANUFACTURER=NdDw -DAPF_AU_MANUFACTURER_NAME=NoDAW \
    -DAPF_ASSET_DIR="$R/dist/windows"
  cmake --build "$B" --config Release --target "${target}_all"
  # The Visual Studio generator puts them in VST3/ and CLAP/Release/: gather both in pkg/.
  cp -r "dist/windows/VST3/$name.vst3" dist/windows/pkg/
  cp "$(find dist/windows/CLAP -name "$name.clap" | head -1)" dist/windows/pkg/
  test -f "dist/windows/pkg/$name.vst3/Contents/x86_64-win/$name.vst3"
  test -f "dist/windows/pkg/$name.clap"
done
find dist/windows/pkg -maxdepth 4
