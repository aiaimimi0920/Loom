//! Bounded wall-owned acknowledgements include transient continuous work omitted by the durable queue.
use std::collections::{BTreeMap, VecDeque};

use loom_protocol::{SurfaceActionAck, SurfaceActionStatus};
use serde::Serialize;

#[derive(Clone, Default)]
pub(crate) struct WallSurfaceRequests {
    entries: VecDeque<WallSurfacePending>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallSurfacePending {
    ack: SurfaceActionAck,
    action_id: String,
    cancelable: bool,
}

fn executing(status: &SurfaceActionStatus) -> bool {
    matches!(
        status,
        SurfaceActionStatus::Accepted
            | SurfaceActionStatus::Queued
            | SurfaceActionStatus::Running
            | SurfaceActionStatus::CancelRequested
    )
}

impl WallSurfaceRequests {
    fn current<'a>(
        entry: &'a WallSurfacePending,
        acks: &'a BTreeMap<String, SurfaceActionAck>,
    ) -> Option<&'a SurfaceActionAck> {
        // Admission retains every in-flight ack; a missing owned identity has already retired.
        acks.get(&entry.ack.event_id)
            .filter(|ack| ack.request_id == entry.ack.request_id)
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|entry| entry.ack.request_id == id)
    }

    pub(crate) fn make_room(&mut self, acks: &BTreeMap<String, SurfaceActionAck>) -> bool {
        if self.entries.len() < 64 {
            return true;
        }
        let completed = self.entries.iter().position(|entry| {
            Self::current(entry, acks).is_none_or(|ack| {
                matches!(
                    ack.status,
                    SurfaceActionStatus::Succeeded
                        | SurfaceActionStatus::Failed
                        | SurfaceActionStatus::Cancelled
                        | SurfaceActionStatus::Interrupted
                )
            })
        });
        if let Some(index) = completed {
            self.entries.remove(index);
            return true;
        }
        false
    }

    pub(crate) fn record(&mut self, ack: SurfaceActionAck, action_id: String, cancelable: bool) {
        if !self.contains(&ack.request_id) {
            self.entries.push_back(WallSurfacePending {
                ack,
                action_id,
                cancelable,
            });
        }
    }

    pub(crate) fn pending(
        &self,
        acks: &BTreeMap<String, SurfaceActionAck>,
    ) -> Vec<WallSurfacePending> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let ack = Self::current(entry, acks)?;
                if !executing(&ack.status) {
                    return None;
                }
                let mut pending = entry.clone();
                pending.ack = ack.clone();
                pending.ack.error = None;
                Some(pending)
            })
            .collect()
    }

    pub(crate) fn has_execution(&self, acks: &BTreeMap<String, SurfaceActionAck>) -> bool {
        self.entries
            .iter()
            .any(|entry| Self::current(entry, acks).is_some_and(|ack| executing(&ack.status)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ack(index: usize, status: SurfaceActionStatus) -> SurfaceActionAck {
        SurfaceActionAck {
            protocol_version: "loom.surface.v1".into(),
            instance_id: "art".into(),
            event_id: format!("event-{index}"),
            request_id: format!("request-{index}"),
            accepted: true,
            status,
            error: None,
        }
    }

    #[test]
    fn wall_surface_requests_keep_transient_work_pending_until_a_terminal_ack() {
        let mut requests = WallSurfaceRequests::default();
        let mut acks = BTreeMap::from([("event-0".into(), ack(0, SurfaceActionStatus::Queued))]);
        requests.record(ack(0, SurfaceActionStatus::Queued), "edit".into(), false);
        // Continuous admission now retains only a bounded transient acknowledgement, not payloads.
        assert_eq!(
            requests.pending(&acks)[0].ack.status,
            SurfaceActionStatus::Queued
        );
        assert!(requests.has_execution(&acks));
        acks.insert("event-0".into(), ack(0, SurfaceActionStatus::Running));
        assert_eq!(
            requests.pending(&acks)[0].ack.status,
            SurfaceActionStatus::Running
        );
        assert!(!requests.pending(&acks)[0].cancelable);
        acks.insert("event-0".into(), ack(0, SurfaceActionStatus::Succeeded));
        assert!(requests.pending(&acks).is_empty());
        assert!(!requests.has_execution(&acks));
        assert!(requests.contains("request-0"));
        acks.clear();
        assert!(requests.pending(&acks).is_empty());
        assert!(!requests.has_execution(&acks));
    }

    #[test]
    fn wall_surface_requests_do_not_expose_other_views_or_treat_confirmation_as_execution() {
        let mut requests = WallSurfaceRequests::default();
        requests.record(
            ack(0, SurfaceActionStatus::AwaitingConfirmation),
            "submit".into(),
            true,
        );
        let mut acks = BTreeMap::from([("event-1".into(), ack(1, SurfaceActionStatus::Running))]);
        assert!(requests.pending(&acks).is_empty());
        assert!(!requests.has_execution(&acks));
        acks.insert("event-0".into(), ack(0, SurfaceActionStatus::Queued));
        assert_eq!(requests.pending(&acks).len(), 1);
        assert_eq!(requests.pending(&acks)[0].action_id, "submit");
    }

    #[test]
    fn wall_surface_requests_never_evict_active_work_to_accept_an_unbounded_queue() {
        let mut requests = WallSurfaceRequests::default();
        let mut acks = BTreeMap::new();
        for index in 0..64 {
            assert!(requests.make_room(&acks));
            acks.insert(
                format!("event-{index}"),
                ack(index, SurfaceActionStatus::Queued),
            );
            requests.record(
                ack(index, SurfaceActionStatus::Queued),
                "edit".into(),
                false,
            );
        }
        assert!(!requests.make_room(&acks));
        assert_eq!(requests.pending(&acks).len(), 64);
        acks.insert("event-3".into(), ack(3, SurfaceActionStatus::Cancelled));
        assert!(requests.make_room(&acks));
        assert!(!requests.contains("request-3"));
        assert!(requests.contains("request-0"));
    }
}
