//! Prints the parameter table and ranges as JSON for the UI mock (scripts/ui-mock).
//!
//!     cargo run -p nodaw-harmonic-space --no-default-features --example ui_fixture
fn main() {
    println!(
        "{}",
        nodaw_kit::fixture_json(
            nodaw_harmonic_space::PARAMS,
            "\"plugin\":\"harmonic-space\",\"latency\":0,\"endpoints\":{\"skin\":\"dark\"}"
        )
    );
}
