//! Package contracts grouped by their owning boundary.

use super::*;
use loom_plugin_security::{generate_signing_key, sign_package, SigningKeyDocument, TrustPolicy};
use loom_protocol::PublisherTrustRecord;
use std::io::{Read, Write};
use zip::write::SimpleFileOptions;

mod archive;
mod fixtures;
mod hardening;
mod install;
mod integrity;
mod trust;
mod validation;

use fixtures::*;
