use super::{model::*, receiver::*};
use std::collections::{BTreeMap, VecDeque};

const MAX_RECEIVERS: usize = 256;
const MAX_RECEIPTS: usize = 32;

#[derive(Default)]
pub(crate) struct Store {
    receivers: BTreeMap<String, Receiver>,
}

impl Store {
    pub(crate) fn revoke(&mut self, device: &str) {
        self.receivers.remove(device);
    }

    pub(crate) fn view(&self, device: &str, epoch: u64, now: u64) -> Option<View> {
        self.receivers
            .get(device)
            .filter(|receiver| receiver.epoch == epoch)
            .map(|receiver| receiver.view(now))
    }

    pub(super) fn report(
        &mut self,
        device: &str,
        epoch: u64,
        authorized_until: u64,
        input: Report,
        now: u64,
    ) -> Result<View> {
        input.validate()?;
        self.receivers
            .retain(|_, value| now < value.reported_at.saturating_add(RETAIN_MS));
        let previous = self.receivers.get(device);
        if let Some(generation) = input.receiver_generation.as_deref() {
            let receiver = previous.ok_or(Error(409, "locale_receiver_changed"))?;
            if receiver.epoch != epoch
                || generation != receiver.generation
                || input.receiver_id != receiver.id
            {
                return Err(Error(409, "locale_receiver_changed"));
            }
            if !receiver.active(now) {
                return Err(Error(409, "locale_receiver_expired"));
            }
        } else if let Some(receiver) =
            previous.filter(|value| value.epoch == epoch && value.active(now))
        {
            if receiver.id != input.receiver_id {
                return Err(Error(409, "locale_receiver_busy"));
            }
        } else {
            if !self.receivers.contains_key(device) && self.receivers.len() >= MAX_RECEIVERS {
                return Err(Error(429, "locale_capacity"));
            }
            self.receivers.insert(
                device.to_owned(),
                Receiver {
                    id: input.receiver_id.clone(),
                    generation: uuid::Uuid::new_v4().to_string(),
                    epoch,
                    language: input.language.clone(),
                    revision: input.revision,
                    supported: input.supported_languages.clone(),
                    reported_at: now,
                    authorized_until,
                    receipts: VecDeque::new(),
                },
            );
        }
        let receiver = self.receivers.get_mut(device).expect("receiver claimed");
        receiver.update(input, authorized_until, now)?;
        Ok(receiver.view(now))
    }

    pub(super) fn request(
        &mut self,
        device: &str,
        epoch: u64,
        input: RequestInput,
        now: u64,
    ) -> Result<(bool, View)> {
        input.validate()?;
        let receiver = self
            .receivers
            .get_mut(device)
            .filter(|value| value.epoch == epoch)
            .ok_or(Error(409, "locale_receiver_unavailable"))?;
        if receiver.generation != input.receiver_generation {
            return Err(Error(409, "locale_receiver_changed"));
        }
        // Retry lookup precedes current liveness/revision checks: a lost response must
        // never turn the same request into a second write.
        if let Some(receipt) = receiver
            .receipts
            .iter()
            .find(|value| value.input.request_id == input.request_id)
        {
            if receipt.input != input {
                return Err(Error(409, "locale_request_id_conflict"));
            }
            let mut view = receiver.view(now);
            view.request = Some(receipt.view(now));
            return Ok((false, view));
        }
        if !receiver.active(now) {
            return Err(Error(409, "locale_receiver_unavailable"));
        }
        if receiver.revision != input.expected_revision {
            return Err(Error(409, "locale_revision_conflict"));
        }
        if input.expected_revision == MAX_REVISION {
            return Err(Error(409, "locale_revision_exhausted"));
        }
        if !receiver.supported.contains(&input.language) {
            return Err(Error(400, "locale_language_unsupported"));
        }
        if receiver.receipts.iter().any(|value| value.pending(now)) {
            return Err(Error(409, "locale_request_in_flight"));
        }
        receiver
            .receipts
            .retain(|value| now < value.expires_at.saturating_add(RETAIN_MS));
        if receiver.receipts.len() >= MAX_RECEIPTS {
            return Err(Error(429, "locale_capacity"));
        }
        receiver.receipts.push_back(Receipt {
            input,
            expires_at: now.saturating_add(REQUEST_MS),
            outcome: None,
        });
        Ok((true, receiver.view(now)))
    }
}
