//! Compile and exercise the actual database-independent live route in isolation.
//! The full server still requires the workspace's PostgreSQL dependency checks.
#[path = "../../server/src/live_api.rs"]
mod live_api;

#[test]
fn live_route_has_a_network_free_embedding_default() {
    let options = live_api::LiveOptions::offline();
    assert!(!options.enabled);
    assert!(!options.source_checks_passed);
    // Reference constructors without inspecting or changing secret environment values.
    let _configured: fn() -> live_api::LiveOptions = live_api::LiveOptions::from_env;
}
