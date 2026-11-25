// 已鉴权原生宿主意图的原子状态修改；Claim 的领取围栏在 HTTP 响应之前落位。
fn configure_local_art_trigger(
    record: &mut LiveSessionRecord,
    actor: &str,
    grant: &loom_protocol::local_art::Grant,
    configuration: local_trigger::Configure,
    policy_revision: Option<u64>,
) -> std::result::Result<local_trigger::Reply, LiveRuntimeError> {
    ensure_live_collaboration(record)?;
    ensure_viewer(record, actor)?;
    if record.viewer_connections.get(actor).copied().unwrap_or(0) == 0 {
        return Err(local_trigger_error(409, "local_art_trigger_viewer_offline"));
    }
    let local_trigger::Configure {
        scope,
        condition,
        sequence,
        ..
    } = configuration;
    if record.trigger_registrations.contains_key(&scope.binding_id) {
        return Err(local_trigger_error(
            409,
            "local_art_trigger_binding_conflict",
        ));
    }
    if let Some(old) = record.local_art_triggers.0.get(&scope.binding_id) {
        if old.reply.binding != grant.binding || old.reply.grant_id != grant.grant_id {
            return Err(local_trigger_error(403, "local_art_trigger_scope"));
        }
        if condition.revision <= old.reply.condition.revision
            || scope.authorization_id == old.reply.scope.authorization_id
        {
            return Err(local_trigger_error(409, "local_art_trigger_revision"));
        }
    } else if record.trigger_registrations.len() + record.local_art_triggers.len()
        >= loom_protocol::LIVE_MAX_TRIGGER_BINDINGS
    {
        return Err(local_trigger_error(429, "live_trigger_binding_limit"));
    }
    let policy_revision = policy_revision
        .ok_or_else(|| local_trigger_error(403, "local_art_trigger_approval_required"))?;
    let now = unix_time_millis();
    if now < grant.issued_at_ms || now >= grant.expires_at_ms || actor != grant.binding.device_id {
        return Err(local_trigger_error(403, "local_art_trigger_scope"));
    }
    // 复用原 Live 控制序列，不为本地 Art 增加可并行重放的第二个配置游标。
    accept_control_sequence(record, actor, scope.epoch, sequence)?;
    let reply = local_trigger::Reply {
        protocol_version: local_trigger::PROTOCOL.into(),
        binding: grant.binding.clone(),
        grant_id: grant.grant_id.clone(),
        scope: scope.clone(),
        condition,
        enabled: true,
        status: local_trigger::Status::Idle,
        reason: None,
        delivery_id: None,
        receipt: None,
        delivery: None,
    };
    record.local_art_triggers.0.insert(
        scope.binding_id,
        LocalArtTrigger {
            reply: reply.clone(),
            state: LiveTriggerConditionState {
                armed: true,
                ..Default::default()
            },
            payload: None,
            grant_deadline: Instant::now() + Duration::from_millis(grant.expires_at_ms - now),
            delivery_deadline: None,
            delivery_expires_at_ms: None,
            observed_wall: now,
            expires_at_ms: grant.expires_at_ms,
            policy_revision,
        },
    );
    Ok(reply)
}
fn apply_local_art_trigger(
    entry: &mut LocalArtTrigger,
    operation: local_trigger::Operation,
) -> std::result::Result<local_trigger::Reply, LiveRuntimeError> {
    use local_trigger::{Operation, Status};
    let mut delivery = None;
    match operation {
        Operation::Inspect { .. } => {}
        Operation::Stop { .. } => {
            entry.retire(Status::Stopped, local_trigger::StopReason::UserStopped)
        }
        Operation::Claim { .. } => {
            if !entry.reply.enabled {
                return Err(local_trigger_error(409, "local_art_trigger_inactive"));
            }
            if entry.reply.status == Status::Ready {
                delivery = entry.payload.take();
                entry.reply.status = Status::Claimed;
            }
        }
        Operation::Ack {
            delivery_id,
            receipt,
            ..
        } => {
            if !entry.reply.enabled || entry.reply.delivery_id.as_deref() != Some(&delivery_id) {
                return Err(local_trigger_error(409, "local_art_trigger_delivery_stale"));
            }
            match entry.reply.status {
                Status::Claimed => {
                    entry.reply.status = Status::Acknowledged;
                    entry.reply.receipt = Some(receipt);
                    entry.delivery_deadline = None;
                    entry.delivery_expires_at_ms = None;
                }
                Status::Acknowledged if entry.reply.receipt.as_ref() == Some(&receipt) => {}
                _ => return Err(local_trigger_error(409, "local_art_trigger_ack_conflict")),
            }
        }
        Operation::Configure { .. } => unreachable!("configuration is handled before scope lookup"),
    }
    Ok(local_trigger::Reply {
        delivery,
        ..entry.reply.clone()
    })
}
