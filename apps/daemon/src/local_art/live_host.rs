//! Shared current native-host approval boundary; immutable disk verification is not repeated per key.
use super::*;
pub(super) fn authorize<T>(
    request: &ParsedHttpRequest,
    actor: &str,
    s: &Services<'_>,
    guard: DeviceGuard,
    grant: &Grant,
    check_source: bool,
    apply: impl FnOnce() -> LocalResult<T>,
) -> LocalResult<T> {
    let approval = policy::approved(s.root, grant, unix_time_millis())?;
    if check_source {
        PREPARING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| fail(429, "local_art_prepare_busy"))?;
        let _preparation = Preparation;
        let bytes = source::load(s, grant)?;
        if bytes.len() as u64 != grant.source.size || sha256_bytes(&bytes) != grant.source.digest {
            return Err(fail(403, "local_art_source_changed"));
        }
    }
    // Keep final approval and mutation serialized; never call service Lease::valid under this lock.
    let _policy = policy::LOCK
        .lock()
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    let now = unix_time_millis();
    if policy::approved_unlocked(s.root, grant, now)? != approval {
        return Err(fail(409, "local_art_approval_changed"));
    }
    source::enabled(s, grant)?;
    current_device(request, actor, s, &guard, Some(grant.expires_at_ms))?;
    apply()
}
pub(super) fn current_device(
    request: &ParsedHttpRequest,
    actor: &str,
    s: &Services<'_>,
    guard: &DeviceGuard,
    expires: Option<u64>,
) -> LocalResult<()> {
    let current = device_guard(request, actor, s.devices)?;
    if !Arc::ptr_eq(&guard.revoked, &current.revoked)
        || current.revoked.load(Ordering::Acquire)
        || unix_time_millis() >= expires.unwrap_or(current.expires).min(current.expires)
    {
        return Err(fail(403, "local_art_authorization_changed"));
    }
    Ok(())
}
