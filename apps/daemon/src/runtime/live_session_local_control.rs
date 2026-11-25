// Native host adapter only: all controller exclusivity, membership, epochs and input ordering remain original.
impl LiveSessionStore {
    fn local_art_control(
        &self,
        actor: &str,
        operation: loom_protocol::local_art::live_control::Operation,
    ) -> std::result::Result<Value, LiveRuntimeError> {
        use loom_protocol::local_art::live_control::Operation;
        let acquire = matches!(operation, Operation::Acquire { .. });
        match operation {
            Operation::Acquire {
                session_id,
                epoch,
                sequence,
            }
            | Operation::Release {
                session_id,
                epoch,
                sequence,
            } => {
                if acquire {
                    use loom_protocol::LiveInteractionCapability as C;
                    let current = self.get_for_member(&session_id, actor)?;
                    let caps = &current.session.interaction_capabilities;
                    if !caps.contains(&C::Cancel)
                        || !caps.iter().any(|c| {
                            matches!(
                                c,
                                C::PointerMove | C::PointerButton | C::Wheel | C::Keyboard
                            )
                        })
                    {
                        return Err(LiveRuntimeError::new(
                            409,
                            "live_input_capability_unavailable",
                            "source did not permit native window control",
                        ));
                    }
                }
                let snapshot = self.change_controller(
                    actor,
                    &session_id,
                    &LiveControlLeaseRequest {
                        protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.into(),
                        surface_instance_id: None,
                        attachment_id: None,
                        action: if acquire {
                            LiveControlLeaseAction::Acquire
                        } else {
                            LiveControlLeaseAction::Release
                        },
                        sequence,
                        epoch,
                        lease_duration_ms: Some(30_000),
                    },
                )?;
                serde_json::to_value(snapshot).map_err(|_| unavailable())
            }
            Operation::Input { envelope } => {
                let session = envelope.session_id.clone();
                let accepted = self.forward_input(actor, &session, *envelope)?;
                Ok(
                    json!({"protocolVersion":loom_protocol::LIVE_PROTOCOL_VERSION,"acceptedSequence":accepted.sequence}),
                )
            }
        }
    }
}
