// Keep the large Otter canvas implementation in one lexical module so private
// helpers remain private while each responsibility stays reviewable in isolation.
include!("otter_canvas/model.rs");
include!("otter_canvas/document.rs");
include!("otter_canvas/cache.rs");
include!("otter_canvas/session.rs");
include!("otter_canvas/geometry.rs");
include!("otter_canvas/preview_candidates.rs");
include!("otter_canvas/preview_sources.rs");
include!("otter_canvas/graph_export.rs");

#[cfg(test)]
mod tests {
    include!("otter_canvas/tests/fixtures_core.rs");
    include!("otter_canvas/tests/geometry_core.rs");
    include!("otter_canvas/tests/preview_semantics.rs");
    include!("otter_canvas/tests/shape_retry.rs");
    include!("otter_canvas/tests/cache.rs");
}
