//! Prints the parameter table and ranges as JSON for the UI mock (scripts/ui-mock).
//!
//!     cargo run -p nodaw-chronos-weaver --no-default-features --example ui_fixture
fn main() {
    let lat = nodaw_chronos_weaver::engine::Engine::new(48_000.0).latency();
    println!("{}", nodaw_kit::fixture_json(nodaw_chronos_weaver::PARAMS, &format!("\"plugin\":\"chronos-weaver\",\"latency\":{lat},\"endpoints\":{{\"skin\":\"nebula\"}}")));
}
