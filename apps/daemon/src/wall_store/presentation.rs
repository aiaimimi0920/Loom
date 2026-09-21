//! Presentation controls revoke input immediately without destroying a retained frame's mapping.
use super::catalog::snapshot;
use super::persistence::next_document;
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WallPresentationMode {
    Running,
    Frozen,
    Black,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WallPresentation {
    pub wall_id: String,
    pub revision: u64,
    pub mode: WallPresentationMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WallPresentationOutcome {
    Applied,
    FrameUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WallPresentationReport {
    pub revision: u64,
    pub outcome: WallPresentationOutcome,
}

impl WallStore {
    pub(crate) fn set_presentation(
        &self,
        base_revision: u64,
        wall_id: &str,
        mode: WallPresentationMode,
    ) -> WallResult<WallStateSnapshot> {
        let mut state = self.lock()?;
        let mut next = next_document(&state, base_revision)?;
        let layout = next
            .layouts
            .iter_mut()
            .find(|layout| layout.wall_id == wall_id)
            .ok_or_else(|| WallStoreError::new(404, "wall_not_found", "wall was not found"))?;
        let current = next.presentations.iter().find(|p| p.wall_id == wall_id);
        if current.map_or(WallPresentationMode::Running, |p| p.mode) == mode {
            return Ok(snapshot(&state, None, Instant::now()));
        }
        let affected: Vec<_> = layout
            .tiles
            .iter()
            .map(|tile| tile.endpoint_id.clone())
            .collect();
        next.presentations.retain(|p| p.wall_id != wall_id);
        if mode == WallPresentationMode::Running {
            // Old queued input must remain invalid after a pause, even if geometry is identical.
            layout.revision = next.revision;
        } else {
            next.presentations.push(WallPresentation {
                wall_id: wall_id.to_owned(),
                revision: next.revision,
                mode,
            });
        }
        self.commit(&mut state, next)?;
        for endpoint in affected {
            if let Some(lease) = state.leases.get_mut(&endpoint) {
                lease.presentation = None;
                lease.scene = None;
                lease.identification = None;
                if mode == WallPresentationMode::Running {
                    lease.applied_revision = None;
                }
            }
        }
        Ok(snapshot(&state, None, Instant::now()))
    }
}

pub(super) fn require_running(document: &WallDocument, wall_id: &str) -> WallResult<()> {
    if document.presentations.iter().any(|p| p.wall_id == wall_id) {
        return Err(WallStoreError::new(
            409,
            "wall_presentation_paused",
            "wall input is paused",
        ));
    }
    Ok(())
}

pub(super) fn validate_presentations(document: &WallDocument) -> WallResult<()> {
    let mut walls = BTreeSet::new();
    if document.presentations.len() > document.layouts.len() {
        return Err(WallStoreError::invalid(
            "too many wall presentation controls",
        ));
    }
    for control in &document.presentations {
        if control.revision == 0
            || control.revision > document.revision
            || control.mode == WallPresentationMode::Running
            || !walls.insert(&control.wall_id)
            || !document
                .layouts
                .iter()
                .any(|layout| layout.wall_id == control.wall_id)
        {
            return Err(WallStoreError::invalid("invalid wall presentation control"));
        }
    }
    Ok(())
}

pub(super) fn validate_report(
    document: &WallDocument,
    layout: Option<&WallLayout>,
    applied: Option<u64>,
    report: WallPresentationReport,
) -> WallResult<()> {
    let control = layout.and_then(|layout| {
        document
            .presentations
            .iter()
            .find(|p| p.wall_id == layout.wall_id)
    });
    let valid = control.is_some_and(|control| {
        report.revision == control.revision
            && match (control.mode, report.outcome) {
                (WallPresentationMode::Frozen, WallPresentationOutcome::Applied) => {
                    applied == layout.map(|l| l.revision)
                }
                (WallPresentationMode::Frozen, WallPresentationOutcome::FrameUnavailable)
                | (WallPresentationMode::Black, WallPresentationOutcome::Applied) => {
                    applied.is_none()
                }
                _ => false,
            }
    });
    if !valid {
        return Err(WallStoreError::conflict(
            "presenter reports a stale or inconsistent control",
        ));
    }
    Ok(())
}
