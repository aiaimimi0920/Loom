// Keep the large Hook canvas implementation in one lexical module so private
// helpers remain private while each responsibility stays reviewable in isolation.
include!("hook_canvas/model.rs");
include!("hook_canvas/document.rs");
include!("hook_canvas/session.rs");
include!("hook_canvas/geometry.rs");
include!("hook_canvas/preview_candidates.rs");
include!("hook_canvas/preview_sources.rs");
include!("hook_canvas/graph_export.rs");

#[cfg(test)]
mod tests {
    include!("hook_canvas/tests/fixtures_core.rs");
    include!("hook_canvas/tests/geometry_core.rs");
    include!("hook_canvas/tests/preview_semantics.rs");
    include!("hook_canvas/tests/shape_retry.rs");
}
