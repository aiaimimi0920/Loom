// 显式自动回调登记的唯一 owner；每注册单槽，无线程、持久恢复或网络重放。
use loom_protocol::local_art::live_trigger as local_trigger;
#[derive(Default)]
struct LocalArtTriggers(BTreeMap<String, LocalArtTrigger>);
struct LocalArtTrigger {
    reply: local_trigger::Reply,
    state: LiveTriggerConditionState,
    payload: Option<local_trigger::Delivery>,
    grant_deadline: Instant,
    delivery_deadline: Option<Instant>,
    delivery_expires_at_ms: Option<u64>,
    observed_wall: u64,
    expires_at_ms: u64,
    policy_revision: u64,
}
impl LocalArtTrigger {
    fn retire(&mut self, status: local_trigger::Status, reason: local_trigger::StopReason) {
        self.reply.enabled = false;
        self.reply.status = status;
        self.reply.reason = Some(reason);
        self.payload = None;
        self.delivery_deadline = None;
        self.delivery_expires_at_ms = None;
    }
    fn expire(&mut self, now: u64, clock: Instant) {
        if !self.reply.enabled {
            return;
        }
        let reason = if now < self.observed_wall {
            Some(local_trigger::StopReason::ClockRegressed)
        } else if now >= self.expires_at_ms || clock >= self.grant_deadline {
            Some(local_trigger::StopReason::GrantExpired)
        } else if self
            .delivery_expires_at_ms
            .is_some_and(|expiry| now >= expiry)
            || self
                .delivery_deadline
                .is_some_and(|deadline| clock >= deadline)
        {
            Some(local_trigger::StopReason::DeliveryExpired)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.retire(local_trigger::Status::Expired, reason);
        }
        self.observed_wall = self.observed_wall.max(now);
    }
}
impl LocalArtTriggers {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn retire_device(&mut self, device: &str) {
        for registration in self
            .0
            .values_mut()
            .filter(|r| r.reply.binding.device_id == device)
        {
            registration.retire(
                local_trigger::Status::Stopped,
                local_trigger::StopReason::ViewerDisconnected,
            );
        }
    }
    fn evaluate(
        &mut self,
        session: &LiveScreenshotSession,
        epoch: u64,
        viewers: &BTreeMap<String, usize>,
        observation: &LiveObservation,
    ) {
        let now = unix_time_millis();
        let clock = Instant::now();
        for entry in self.0.values_mut() {
            entry.expire(now, clock);
            if !entry.reply.enabled
                || entry.reply.condition.observation_id != observation.observation_id
            {
                continue;
            }
            let online = viewers
                .get(&entry.reply.binding.device_id)
                .copied()
                .unwrap_or(0)
                > 0;
            let decision = evaluate_live_trigger_state(
                &entry.reply.condition,
                &mut entry.state,
                observation,
                online,
            );
            if decision != Some((None, true))
                || matches!(
                    entry.reply.status,
                    local_trigger::Status::Ready | local_trigger::Status::Claimed
                )
            {
                continue;
            }
            let binding = LiveTriggerBinding {
                binding_id: entry.reply.scope.binding_id.clone(),
                observation_id: observation.observation_id.clone(),
                condition_revision: entry.reply.condition.revision,
                authorized_by: entry.reply.binding.device_id.clone(),
                enabled: true,
            };
            let delivery_id = live_trigger_idempotency_key(
                &session.session_id,
                epoch,
                &binding,
                observation.sequence,
            );
            let expires_at_ms = observation
                .observed_at_ms
                .saturating_add(LIVE_OBSERVATION_MAX_AGE_MS)
                .min(now.saturating_add(LIVE_OBSERVATION_MAX_AGE_MS))
                .min(entry.expires_at_ms);
            let delivery = local_trigger::Delivery {
                delivery_id: delivery_id.clone(),
                callback: local_trigger::CALLBACK.into(),
                condition_revision: entry.reply.condition.revision,
                observation: observation.clone(),
                source_device_id: session.source_device_id.clone(),
                expires_at_ms,
            };
            if serde_json::to_vec(&delivery)
                .map_or(true, |bytes| bytes.len() > local_trigger::MAX_DELIVERY)
            {
                // 不能截断观察数据或无限累积；失败只保现有注册和固定状态。
                entry.retire(
                    local_trigger::Status::Stopped,
                    local_trigger::StopReason::DeliveryLimit,
                );
                continue;
            }
            entry.reply.status = local_trigger::Status::Ready;
            entry.reply.delivery_id = Some(delivery_id);
            entry.reply.receipt = None;
            entry.payload = Some(delivery);
            entry.delivery_deadline =
                Some(clock + Duration::from_millis(expires_at_ms.saturating_sub(now)));
            entry.delivery_expires_at_ms = Some(expires_at_ms);
        }
    }
}
impl LiveSessionStore {
    fn local_art_trigger(
        &self,
        actor: &str,
        grant: &loom_protocol::local_art::Grant,
        operation: local_trigger::Operation,
        policy_revision: Option<u64>,
    ) -> std::result::Result<local_trigger::Reply, LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        let scope = operation.scope();
        let record = active_record_mut(&mut sessions, &scope.session_id)?;
        if record.epoch != scope.epoch {
            return Err(local_trigger_error(409, "local_art_trigger_epoch"));
        }
        if let local_trigger::Operation::Configure { configuration } = operation {
            return configure_local_art_trigger(
                record,
                actor,
                grant,
                *configuration,
                policy_revision,
            );
        }
        let entry = record
            .local_art_triggers
            .0
            .get_mut(&scope.binding_id)
            .ok_or_else(|| local_trigger_error(404, "local_art_trigger_not_found"))?;
        if entry.reply.binding != grant.binding
            || entry.reply.grant_id != grant.grant_id
            || entry.reply.scope != *scope
            || entry.reply.binding.device_id != actor
        {
            return Err(local_trigger_error(403, "local_art_trigger_scope"));
        }
        entry.expire(unix_time_millis(), Instant::now());
        if let Some(revision) = policy_revision {
            if revision != entry.policy_revision {
                entry.retire(
                    local_trigger::Status::Stopped,
                    local_trigger::StopReason::ApprovalChanged,
                );
                return Err(local_trigger_error(
                    409,
                    "local_art_trigger_approval_changed",
                ));
            }
        }
        apply_local_art_trigger(entry, operation)
    }
}
fn local_trigger_error(status: u16, code: &'static str) -> LiveRuntimeError {
    LiveRuntimeError::new(status, code, code)
}
