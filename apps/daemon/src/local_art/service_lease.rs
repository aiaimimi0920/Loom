//! Service tasks retain only verified grant scope and session revocation handles; never store Device tokens or auto-extend grants.
use super::*;

pub(super) struct Lease {
    root: PathBuf,
    tools: ToolRegistry,
    grant: Grant,
    approval: policy::Approval,
    guard: DeviceGuard,
}
impl Lease {
    pub(super) fn new(
        s: &Services<'_>,
        grant: Grant,
        approval: policy::Approval,
        guard: DeviceGuard,
    ) -> Self {
        Self {
            root: s.root.to_path_buf(),
            tools: s.tools.clone(),
            grant,
            approval,
            guard,
        }
    }
    pub(super) fn expires(&self) -> u64 {
        self.grant
            .expires_at_ms
            .min(self.guard.expires)
            .min(self.approval.valid_until_ms)
    }
    pub(super) fn valid(&self) -> bool {
        let now = unix_time_millis();
        now >= self.grant.issued_at_ms
            && now < self.expires()
            && !self.guard.revoked.load(Ordering::Acquire)
            && policy::approved(&self.root, &self.grant, now)
                .is_ok_and(|value| value == self.approval)
            && self
                .tools
                .get_tool(&self.grant.binding.art_id)
                .ok()
                .flatten()
                .is_some_and(|tool| {
                    tool.enabled && tool.qualified_id() == self.grant.binding.art_id
                })
    }
    pub(super) fn same_session(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.guard.revoked, &other.guard.revoked)
    }
}
