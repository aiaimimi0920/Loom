use serde::{Deserialize, Serialize};

pub(super) const ACTIVE_MS: u64 = 15_000;
pub(super) const REQUEST_MS: u64 = 15_000;
pub(super) const RETAIN_MS: u64 = 5 * 60_000;
pub(super) const MAX_REVISION: u64 = 9_007_199_254_740_991;
pub(super) type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub(super) struct Error(pub u16, pub &'static str);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RequestInput {
    pub request_id: String,
    pub receiver_generation: String,
    pub expected_revision: u64,
    pub language: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Outcome {
    pub request_id: String,
    pub status: OutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OutcomeStatus {
    Applied,
    Conflict,
    Rejected,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Report {
    pub receiver_id: String,
    #[serde(default)]
    pub receiver_generation: Option<String>,
    pub language: String,
    pub revision: u64,
    pub supported_languages: Vec<String>,
    #[serde(default)]
    pub result: Option<Outcome>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestView {
    #[serde(flatten)]
    pub input: RequestInput,
    pub expires_at: u64,
    pub valid_for_ms: u64,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    pub receiver_generation: String,
    pub language: String,
    pub revision: u64,
    pub supported_languages: Vec<String>,
    pub active: bool,
    pub reported_at: u64,
    pub request: Option<RequestView>,
}

pub(super) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
}

fn language(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 35
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

impl RequestInput {
    pub(super) fn validate(&self) -> Result<()> {
        if !identifier(&self.request_id)
            || !identifier(&self.receiver_generation)
            || self.expected_revision > MAX_REVISION
            || !language(&self.language)
        {
            return Err(Error(400, "locale_input_invalid"));
        }
        Ok(())
    }
}

impl Report {
    pub(super) fn validate(&self) -> Result<()> {
        let supported = &self.supported_languages;
        if !identifier(&self.receiver_id)
            || self
                .receiver_generation
                .as_deref()
                .is_some_and(|value| !identifier(value))
            || self.revision > MAX_REVISION
            || supported.is_empty()
            || supported.len() > 64
            || supported.iter().any(|value| !language(value))
            || supported
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != supported.len()
            || !supported.contains(&self.language)
        {
            return Err(Error(400, "locale_input_invalid"));
        }
        if let Some(result) = &self.result {
            if self.receiver_generation.is_none()
                || !identifier(&result.request_id)
                || result
                    .code
                    .as_deref()
                    .is_some_and(|value| !identifier(value))
                || (result.status == OutcomeStatus::Applied
                    && (result
                        .code
                        .as_deref()
                        .is_some_and(|value| value != "applied")
                        || result.applied_revision.is_none()))
                || (result.status != OutcomeStatus::Applied && result.applied_revision.is_some())
                || result
                    .applied_revision
                    .is_some_and(|value| value > self.revision)
            {
                return Err(Error(400, "locale_result_invalid"));
            }
        }
        Ok(())
    }
}
