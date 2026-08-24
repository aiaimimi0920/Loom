use super::*;
use crate::art_settings::{ArtSettingsStore, ArtUserSettings};
use crate::ToolExecution;
use std::collections::BTreeMap;
use std::io::Write;
use zip::write::SimpleFileOptions;

mod activation;
mod dependencies;
mod fixtures;
mod install_core;
mod manifest_authoring;
mod package;
mod recovery;

use fixtures::*;
