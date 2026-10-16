use super::*;

#[test]
fn cloud_actions_have_bounded_network_budget_without_extending_local_routes() {
    for path in [
        "/v1/naccount/refresh",
        "/v1/naccount/logout",
        "/v1/asset-library/library",
        "/v1/asset-library/resources",
    ] {
        assert_eq!(daemon_action_timeout(path), Duration::from_secs(90));
    }
    assert_eq!(
        daemon_action_timeout("/v1/asset-library/download"),
        Duration::from_secs(120)
    );
    for path in [
        "/v1/runs",
        "/v1/asset-library/download/other",
        "/v1/naccount/unknown",
    ] {
        assert_eq!(
            daemon_action_timeout(path),
            LOOM_DAEMON_DEFAULT_REQUEST_TIMEOUT
        );
    }
}
