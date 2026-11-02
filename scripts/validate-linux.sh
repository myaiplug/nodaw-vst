#!/usr/bin/env bash
# Runs every automated check on the headless Linux build. Results land in docs/results/.
# Env (paths to the tools; see docs/TESTING.md for how to build them):
#   VST3_VALIDATOR   Steinberg `validator` (VST3 SDK 3.8, built with -DSMTG_ENABLE_VST3_HOSTING_EXAMPLES=ON)
#   CLAP_VALIDATOR   free-audio/clap-validator 0.4.1 binary (apply vendor/apf/scripts/clap-validator-0.4.1-preset-deadlock.patch first)
#   STATE_CHECK      vst3-state-check built from vendor/apf/tools
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
R=docs/results; mkdir -p "$R"; fail=0
V3="dist/linux/NoDAW Screw.vst3"; CL="dist/linux/NoDAW Screw.clap"
[ -e "$V3" ] || ./scripts/build-linux.sh
cargo test --workspace --no-default-features 2>&1 | grep -E "^test result|FAILED|panicked" | tee $R/cargo-test.txt; [ "${PIPESTATUS[0]}" = 0 ] || fail=1
cargo clippy --workspace --all-targets --no-default-features -- -D warnings || fail=1
"${VST3_VALIDATOR:?}" "$V3" > $R/vst3-validator.txt 2>&1 || fail=1
"${CLAP_VALIDATOR:?}" validate "$CL" > $R/clap-validator.txt 2>&1 || fail=1
"${STATE_CHECK:?}" "$V3" > $R/vst3-state-check.txt 2>&1 || fail=1
"$STATE_CHECK" "$V3" --preset "Trap Roll Chop" > $R/vst3-state-check-preset.txt 2>&1 || fail=1
# Negative control: the harness must FAIL when stale state is injected, or it proves nothing.
if "$STATE_CHECK" "$V3" --inject-stale > $R/vst3-state-check-inject-stale.txt 2>&1; then echo "negative control did not fail"; fail=1; fi
echo "exit=$fail"; exit $fail
