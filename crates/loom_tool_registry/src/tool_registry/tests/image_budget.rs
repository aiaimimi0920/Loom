//! Image candidate budget coverage.

use super::*;

/// A candidate whose URL cannot become a request at all, so the download loop rejects it without
/// spending any of its network budget. The cap and deadline tests need many failing candidates
/// and cannot afford a real connection attempt for each one.
pub(super) fn unfetchable_mcp_image_candidate(index: usize) -> McpImageCandidate {
    McpImageCandidate {
        image_url: format!("not-a-url-{index}"),
        alternate_image_url: None,
        title: None,
        thumbnail_url: None,
        source_page_url: None,
        width: None,
        height: None,
    }
}

pub(super) fn fixture_mcp_image_candidate(image_url: String) -> McpImageCandidate {
    McpImageCandidate {
        image_url,
        alternate_image_url: None,
        title: Some("Fixture image".to_owned()),
        thumbnail_url: None,
        source_page_url: None,
        width: Some(1),
        height: Some(1),
    }
}

#[test]
pub(super) fn mcp_image_candidate_downloads_stop_at_the_attempt_cap() {
    let within_cap = HttpImageFixture::start("image/png", fixture_image_bytes());
    let mut candidates: Vec<McpImageCandidate> = (0..MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS - 1)
        .map(unfetchable_mcp_image_candidate)
        .collect();
    candidates.push(fixture_mcp_image_candidate(within_cap.url("/fixture.png")));

    let (_, selected) = image_response_from_mcp_candidates(
        &candidates,
        0,
        &loopback_mcp_image_policy(),
        McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
    )
    .expect("the last candidate inside the attempt cap is still tried");
    assert_eq!(selected, MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS - 1);

    // One candidate further out is never requested: the list length is chosen by the MCP server,
    // and a result full of unfetchable candidates must end rather than walk all of them.
    let past_cap = HttpImageFixture::start("image/png", fixture_image_bytes());
    let mut candidates: Vec<McpImageCandidate> = (0..MAX_MCP_IMAGE_DOWNLOAD_ATTEMPTS)
        .map(unfetchable_mcp_image_candidate)
        .collect();
    candidates.push(fixture_mcp_image_candidate(past_cap.url("/fixture.png")));

    assert!(image_response_from_mcp_candidates(
        &candidates,
        0,
        &loopback_mcp_image_policy(),
        McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
    )
    .is_none());
}

#[test]
pub(super) fn an_mcp_image_download_deadline_bounds_each_attempt() {
    let exhausted = McpImageDownloadDeadline::starting_now(Duration::ZERO);
    assert!(exhausted.next_attempt_timeout().is_none());

    // A fresh budget is larger than one request's own timeout, so the first attempt is bounded by
    // the request timeout rather than by the loop budget.
    let fresh = McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET);
    assert_eq!(fresh.next_attempt_timeout(), Some(CLOUD_API_TIMEOUT));

    // Once less than one request timeout is left, the attempt gets only what remains.
    let nearly_spent = McpImageDownloadDeadline::starting_now(MIN_MCP_IMAGE_ATTEMPT_TIMEOUT * 2);
    let remaining = nearly_spent
        .next_attempt_timeout()
        .expect("a budget above the attempt minimum still allows one more request");
    assert!(remaining >= MIN_MCP_IMAGE_ATTEMPT_TIMEOUT);
    assert!(remaining <= MIN_MCP_IMAGE_ATTEMPT_TIMEOUT * 2);
}

#[test]
pub(super) fn an_exhausted_mcp_image_budget_stops_before_the_next_request() {
    let fixture = HttpImageFixture::start("image/png", fixture_image_bytes());
    let candidates = vec![fixture_mcp_image_candidate(fixture.url("/fixture.png"))];

    assert!(image_response_from_mcp_candidates(
        &candidates,
        0,
        &loopback_mcp_image_policy(),
        McpImageDownloadDeadline::starting_now(Duration::ZERO),
    )
    .is_none());

    // The same candidate downloads once there is budget for it, so the refusal above is the
    // deadline and not an unreachable fixture.
    let (response, selected) = image_response_from_mcp_candidates(
        &candidates,
        0,
        &loopback_mcp_image_policy(),
        McpImageDownloadDeadline::starting_now(MCP_IMAGE_DOWNLOAD_BUDGET),
    )
    .expect("download the candidate while budget is left");
    assert_eq!(selected, 0);
    assert_eq!(response["content"][0]["data"], CLOUD_FIXTURE_IMAGE);
}
