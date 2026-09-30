//! A boot-scoped monotonic clock and bounded, complete-revision scene activation deadlines.
use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallScene {
    pub wall_id: String,
    pub revision: u64,
    pub prepared_at_ms: u64,
    pub activate_at_ms: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallTiming {
    pub clock_id: String,
    pub server_time_ms: u64,
    pub scenes: Vec<WallScene>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WallSceneReport {
    pub revision: u64,
    pub prepared: bool,
    pub applied_at_ms: Option<u64>,
    pub clock_uncertainty_ms: Option<u32>,
}

pub(super) struct WallTimeline {
    id: String,
    origin: Instant,
    unix_origin_ms: u64,
    scenes: BTreeMap<String, WallScene>,
}

impl WallTimeline {
    pub(super) fn new(layouts: &[WallLayout]) -> Self {
        let mut timeline = Self {
            id: uuid::Uuid::new_v4().to_string(),
            origin: Instant::now(),
            unix_origin_ms: crate::unix_time_millis(),
            scenes: BTreeMap::new(),
        };
        timeline.reconcile(layouts);
        timeline
    }
    pub(super) fn at(&self, instant: Instant) -> u64 {
        self.unix_origin_ms
            .saturating_add(instant.saturating_duration_since(self.origin).as_millis() as u64)
    }
    pub(super) fn reconcile(&mut self, layouts: &[WallLayout]) {
        self.scenes
            .retain(|id, _| layouts.iter().any(|layout| &layout.wall_id == id));
        let now = self.at(Instant::now());
        for layout in layouts {
            if self
                .scenes
                .get(&layout.wall_id)
                .is_none_or(|scene| scene.revision != layout.revision)
            {
                self.scenes.insert(
                    layout.wall_id.clone(),
                    WallScene {
                        wall_id: layout.wall_id.clone(),
                        revision: layout.revision,
                        prepared_at_ms: now,
                        activate_at_ms: now,
                    },
                );
            }
        }
    }
    pub(super) fn defer(&mut self, wall_id: &str, delay_ms: u64) {
        if let Some(scene) = self.scenes.get_mut(wall_id) {
            scene.activate_at_ms = scene.prepared_at_ms.saturating_add(delay_ms);
        }
    }
    pub(super) fn snapshot(&self, layouts: &[WallLayout], now: Instant) -> WallTiming {
        WallTiming {
            clock_id: self.id.clone(),
            server_time_ms: self.at(now),
            scenes: layouts
                .iter()
                .filter_map(|layout| self.scenes.get(&layout.wall_id).cloned())
                .collect(),
        }
    }
    pub(super) fn validate_report(
        &self,
        layout: Option<&WallLayout>,
        applied: Option<u64>,
        report: Option<WallSceneReport>,
    ) -> WallResult<()> {
        let now = self.at(Instant::now());
        let scene = layout.and_then(|layout| self.scenes.get(&layout.wall_id));
        if applied.is_some() && scene.is_some_and(|scene| now < scene.activate_at_ms) {
            return Err(WallStoreError::new(
                409,
                "wall_scene_not_active",
                "scene activation deadline has not arrived",
            ));
        }
        if let Some(report) = report {
            let scene = scene
                .ok_or_else(|| WallStoreError::conflict("scene report has no assigned layout"))?;
            if report.revision != scene.revision
                || report.applied_at_ms.is_some() != applied.is_some()
                || report
                    .clock_uncertainty_ms
                    .is_some_and(|value| value > 1000)
            {
                return Err(WallStoreError::invalid(
                    "invalid scene report identity or clock estimate",
                ));
            }
            if let Some(at) = report.applied_at_ms {
                if !report.prepared
                    || report.clock_uncertainty_ms.is_none()
                    || at < scene.activate_at_ms
                    || at > now.saturating_add(u64::from(report.clock_uncertainty_ms.unwrap_or(0)))
                {
                    return Err(WallStoreError::invalid("invalid scene application time"));
                }
            }
        }
        Ok(())
    }
}

impl WallStore {
    pub(crate) fn media_timestamp(&self, received_at: Instant) -> WallResult<u64> {
        Ok(self.lock()?.timeline.at(received_at))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_requires_the_current_scene_deadline_and_a_bounded_clock_estimate() {
        let layout = WallLayout {
            protocol_version: WALL_PROTOCOL_VERSION.into(),
            wall_id: "wall:a".into(),
            revision: 1,
            bounds: loom_protocol::wall::WallRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            tiles: vec![],
            placements: vec![],
        };
        let mut timeline = WallTimeline::new(std::slice::from_ref(&layout));
        timeline.defer(&layout.wall_id, 10_000);
        assert_eq!(
            timeline
                .validate_report(Some(&layout), Some(1), None)
                .unwrap_err()
                .code,
            "wall_scene_not_active"
        );
        let ready = WallSceneReport {
            revision: 1,
            prepared: true,
            applied_at_ms: None,
            clock_uncertainty_ms: Some(5),
        };
        assert!(timeline
            .validate_report(Some(&layout), None, Some(ready))
            .is_ok());
        assert!(timeline
            .validate_report(
                Some(&layout),
                None,
                Some(WallSceneReport {
                    revision: 2,
                    ..ready
                })
            )
            .is_err());
        assert!(timeline
            .validate_report(
                Some(&layout),
                None,
                Some(WallSceneReport {
                    clock_uncertainty_ms: Some(1001),
                    ..ready
                })
            )
            .is_err());
        timeline.reconcile(&[]);
        assert!(timeline.snapshot(&[], Instant::now()).scenes.is_empty());
    }
}
