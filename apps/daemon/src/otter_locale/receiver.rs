use super::model::*;
use std::collections::VecDeque;

pub(super) struct Receipt {
    pub input: RequestInput,
    pub expires_at: u64,
    pub outcome: Option<Outcome>,
}

impl Receipt {
    pub fn pending(&self, now: u64) -> bool {
        self.outcome.is_none() && now < self.expires_at
    }

    pub fn view(&self, now: u64) -> RequestView {
        RequestView {
            input: self.input.clone(),
            expires_at: self.expires_at,
            valid_for_ms: self.expires_at.saturating_sub(now).min(REQUEST_MS),
            status: match self.outcome.as_ref().map(|value| value.status) {
                Some(OutcomeStatus::Applied) => "applied",
                Some(OutcomeStatus::Conflict) => "conflict",
                Some(OutcomeStatus::Rejected) => "rejected",
                None if self.pending(now) => "pending",
                None => "unknown",
            },
            applied_revision: self
                .outcome
                .as_ref()
                .and_then(|value| value.applied_revision),
            code: self.outcome.as_ref().and_then(|value| value.code.clone()),
        }
    }
}

pub(super) struct Receiver {
    pub id: String,
    pub generation: String,
    pub epoch: u64,
    pub language: String,
    pub revision: u64,
    pub supported: Vec<String>,
    pub reported_at: u64,
    pub authorized_until: u64,
    pub receipts: VecDeque<Receipt>,
}

impl Receiver {
    pub fn active(&self, now: u64) -> bool {
        now < self.reported_at.saturating_add(ACTIVE_MS) && now < self.authorized_until
    }

    pub fn view(&self, now: u64) -> View {
        View {
            receiver_generation: self.generation.clone(),
            language: self.language.clone(),
            revision: self.revision,
            supported_languages: self.supported.clone(),
            active: self.active(now),
            reported_at: self.reported_at,
            request: self.receipts.back().map(|request| request.view(now)),
        }
    }

    pub fn update(&mut self, input: Report, authorized_until: u64, now: u64) -> Result<()> {
        if input.revision < self.revision
            || (input.revision == self.revision && input.language != self.language)
            || input.supported_languages != self.supported
        {
            return Err(Error(409, "locale_report_conflict"));
        }
        if let Some(outcome) = input.result {
            let receipt = self
                .receipts
                .iter_mut()
                .find(|value| value.input.request_id == outcome.request_id)
                .ok_or(Error(409, "locale_request_unknown"))?;
            if receipt
                .outcome
                .as_ref()
                .is_some_and(|previous| previous != &outcome)
            {
                return Err(Error(409, "locale_result_conflict"));
            }
            if outcome.status == OutcomeStatus::Applied
                && (outcome.applied_revision != receipt.input.expected_revision.checked_add(1)
                    || (outcome.applied_revision == Some(input.revision)
                        && input.language != receipt.input.language))
            {
                return Err(Error(409, "locale_result_conflict"));
            }
            // A delayed acknowledgement can resolve unknown, but cannot roll state back.
            receipt.outcome = Some(outcome);
        }
        self.language = input.language;
        self.revision = input.revision;
        self.reported_at = now;
        self.authorized_until = authorized_until;
        Ok(())
    }
}
