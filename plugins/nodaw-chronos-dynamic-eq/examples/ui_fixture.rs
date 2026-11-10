//! Prints the parameter table and ranges as JSON for the UI mock (scripts/ui-mock).
//!
//!     cargo run -p nodaw-chronos-dynamic-eq --no-default-features --example ui_fixture
fn main() {
    println!(
        "{}",
        nodaw_kit::fixture_json(
            nodaw_chronos_dynamic_eq::PARAMS,
            "\"plugin\":\"chronos-dynamic-eq\",\"latency\":0,\"endpoints\":{\"skin\":\"violet\"}"
        )
    );
}
