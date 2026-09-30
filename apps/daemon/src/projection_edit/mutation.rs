use super::*;

pub(super) fn apply(
    session: &mut Session,
    request: Request,
    access: &Access,
    digest: String,
) -> EditResult<bool> {
    match request {
        Request::Mode {
            op_id,
            base_mode_revision,
            mode,
            ..
        } => {
            access.source_only()?;
            if session.replay(&op_id, &access.actor, &digest)? {
                return Ok(false);
            }
            if session.mode_revision != base_mode_revision {
                return Err(error(409, "projection_edit_mode_conflict"));
            }
            session.revision = session.next_revision()?;
            session.mode_revision = session.revision;
            session.mode = mode;
            session.receipt(op_id, &access.actor, digest);
        }
        Request::Apply {
            op_id,
            base_revision,
            mode_revision,
            changes,
            ..
        } => {
            if mode_revision != session.mode_revision {
                return Err(error(409, "projection_edit_mode_conflict"));
            }
            if !access.is_source && session.mode != Mode::TwoWay {
                return Err(error(403, "projection_edit_read_only"));
            }
            if session.replay(&op_id, &access.actor, &digest)? {
                return Ok(false);
            }
            if base_revision < session.checkpoint_revision || base_revision > session.revision {
                return Err(error(409, "projection_edit_revision_conflict"));
            }
            let mut ids = std::collections::BTreeSet::new();
            if changes.is_empty()
                || changes.len() > 32
                || changes.iter().any(|change| {
                    !identifier(&change.object_id)
                        || !ids.insert(&change.object_id)
                        || change
                            .value
                            .as_ref()
                            .is_some_and(|value| !value_valid(&change.object_id, value))
                })
            {
                return Err(error(400, "projection_edit_invalid_operation"));
            }
            // Whole-operation atomicity: check every conflict before changing any object.
            if changes.iter().any(|change| {
                session
                    .objects
                    .get(&change.object_id)
                    .is_some_and(|object| object.revision > base_revision)
            }) {
                return Err(error(409, "projection_edit_object_conflict"));
            }
            let revision = session.next_revision()?;
            for change in changes {
                session.objects.insert(
                    change.object_id,
                    Object {
                        revision,
                        value: change.value,
                    },
                );
            }
            session.revision = revision;
            session.receipt(op_id, &access.actor, digest);
        }
        Request::Checkpoint {
            expected_revision, ..
        } => {
            access.source_only()?;
            if session.checkpoint_revision == expected_revision.saturating_add(1) {
                return Ok(false);
            }
            if expected_revision != session.revision {
                return Err(error(409, "projection_edit_revision_conflict"));
            }
            // Advance both fences before dropping history: old packets can never replay.
            session.revision = session.next_revision()?;
            session.checkpoint_revision = session.revision;
            session.mode_revision = session.revision;
            session.receipts.clear();
            session.objects.retain(|_, object| object.value.is_some());
        }
        _ => return Err(error(400, "projection_invalid_request")),
    }
    Ok(true)
}
