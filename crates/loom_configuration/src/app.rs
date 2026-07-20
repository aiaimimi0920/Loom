use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use crate::{ManagedConfigError, ManagedConfigErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedAppId {
    Tea,
    Hook,
    Talk,
}

impl ManagedAppId {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tea => "tea",
            Self::Hook => "hook",
            Self::Talk => "talk",
        }
    }

    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Tea => "Tea",
            Self::Hook => "Hook",
            Self::Talk => "Talk",
        }
    }

    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Tea, Self::Hook, Self::Talk]
    }
}

impl fmt::Display for ManagedAppId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for ManagedAppId {
    type Err = ManagedConfigError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tea" => Ok(Self::Tea),
            "hook" => Ok(Self::Hook),
            "talk" => Ok(Self::Talk),
            other => Err(ManagedConfigError::new(
                ManagedConfigErrorCode::UnknownApp,
                format!("unknown managed app: {other}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagedAppSet {
    apps: BTreeSet<ManagedAppId>,
}

impl ManagedAppSet {
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let apps = raw
            .split(',')
            .filter_map(|part| part.parse::<ManagedAppId>().ok())
            .collect::<BTreeSet<_>>();
        Self { apps }
    }

    #[must_use]
    pub fn contains(&self, app: ManagedAppId) -> bool {
        self.apps.contains(&app)
    }

    #[must_use]
    pub fn managed_apps(&self) -> Vec<ManagedAppId> {
        self.apps.iter().copied().collect()
    }
}
