// Keep Surface store internals in one private lexical module while separating
// persistence, lifecycle, event, validation, and JSON mutation responsibilities.
include!("surface_store/model.rs");
include!("surface_store/read_create.rs");
include!("surface_store/attachments.rs");
include!("surface_store/lifecycle_results.rs");
include!("surface_store/events_confirmations.rs");
include!("surface_store/cancel_persist.rs");
include!("surface_store/validation.rs");
include!("surface_store/resource_json_ops.rs");
include!("surface_store/json_pointer.rs");

#[cfg(test)]
mod tests {
    include!("surface_store/tests/fixtures.rs");
    include!("surface_store/tests/json_pointer.rs");
    include!("surface_store/tests/persistence.rs");
    include!("surface_store/tests/generation_failure.rs");
    include!("surface_store/tests/events_lifecycle.rs");
    include!("surface_store/tests/projection_expiry.rs");
}
