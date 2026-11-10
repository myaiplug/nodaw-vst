//! Prints the parameter table and ranges as JSON for the UI mock (scripts/ui-mock).
//!
//!     cargo run -p nodaw-racks --no-default-features --example ui_fixture
fn main() {
    let lat = nodaw_racks::engine::Engine::new(48_000.0).latency();
    println!("{}", nodaw_kit::fixture_json(nodaw_racks::PARAMS, &format!("\"plugin\":\"racks\",\"latency\":{lat}")));
}
