#!/usr/bin/env bash
# Runs every automated check on the headless Linux builds of every plugin in scripts/plugins.txt.
# Results land in docs/results/ (one file per plugin and check).
# Env (paths to the tools; see docs/TESTING.md for how to build them):
#   VST3_VALIDATOR   Steinberg `validator` (VST3 SDK 3.8, built with -DSMTG_ENABLE_VST3_HOSTING_EXAMPLES=ON)
#   CLAP_VALIDATOR   free-audio/clap-validator 0.4.1 binary (apply vendor/apf/scripts/clap-validator-0.4.1-preset-deadlock.patch first)
#   STATE_CHECK      vst3-state-check built from vendor/apf/tools
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
R=docs/results; mkdir -p "$R"; fail=0
cargo test --workspace --no-default-features 2>&1 | grep -E "^test result|FAILED|panicked" | tee $R/cargo-test.txt; [ "${PIPESTATUS[0]}" = 0 ] || fail=1
cargo clippy --workspace --all-targets --no-default-features -- -D warnings || fail=1
mapfile -t PLUGINS < <(grep -v '^#' scripts/plugins.txt | tr -d '\r' | grep -v '^$')
for line in "${PLUGINS[@]}"; do
  IFS='|' read -r crate name target id version sub preset _match <<< "$line"
  [ -n "$crate" ] || continue
  V3="dist/linux/$name.vst3"; CL="dist/linux/$name.clap"
  [ -e "$V3" ] || ./scripts/build-linux.sh "$crate"
  p=${crate#nodaw-}
  if "${VST3_VALIDATOR:?}" "$V3" > "$R/$p-vst3-validator.txt" 2>&1; then echo "$name: VST3 validator ok"; else echo "$name: VST3 validator FAILED"; fail=1; fi
  if "${CLAP_VALIDATOR:?}" validate "$CL" > "$R/$p-clap-validator.txt" 2>&1; then echo "$name: clap-validator ok"; else echo "$name: clap-validator FAILED"; fail=1; fi
  if "${STATE_CHECK:?}" "$V3" > "$R/$p-vst3-state-check.txt" 2>&1; then echo "$name: state check ok"; else echo "$name: state check FAILED"; fail=1; fi
  if "$STATE_CHECK" "$V3" --preset "$preset" > "$R/$p-vst3-state-check-preset.txt" 2>&1; then echo "$name: preset state check ok"; else echo "$name: preset state check FAILED"; fail=1; fi
  # Negative control: the harness must FAIL when stale state is injected, or it proves nothing.
  if "$STATE_CHECK" "$V3" --inject-stale > "$R/$p-vst3-state-check-inject-stale.txt" 2>&1; then echo "$name: negative control did not fail"; fail=1; fi
done
echo "exit=$fail"; exit $fail
