//! Prints the parameter table and ranges as JSON for the UI mock (scripts/ui-mock).
//!
//!     cargo run -p nodaw-temporal-pitch-portal --no-default-features --example ui_fixture
fn main() {
    let lat = nodaw_temporal_pitch_portal::engine::Engine::new(48_000.0).latency();
    println!("{}", nodaw_kit::fixture_json(nodaw_temporal_pitch_portal::PARAMS, &format!("\"plugin\":\"temporal-pitch-portal\",\"latency\":{lat},\"endpoints\":{{\"skin\":\"dark\"}}")));
}
