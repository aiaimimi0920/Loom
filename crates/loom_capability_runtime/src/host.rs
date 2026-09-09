use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use loom_protocol::{
    CapabilityContributions, CapabilityPackageManifest, CapabilityProcessModel,
    CapabilityProtocolError, CapabilityRuntimeMessage, CapabilityRuntimeMethod,
    CapabilityRuntimeStatus, ExtensionResourceRef, ExtensionTarget, ExtensionUnitAttachment,
    PackageTrustStatus,
};
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{CapabilityHostError, HostResult};
use crate::process::{RuntimeProcess, RuntimeProcessClient};
use crate::schema::{compile_command_schemas, CommandSchemaValidators};
use crate::session::{
    call_method, ensure_process, next_request_id, record_failure, runtime_request, start_runtime,
    validate_runtime_package,
};
use crate::snapshot::{build_contribution_snapshot, SnapshotRegistration};

mod admission;
mod invocation;
mod resources;

use admission::InvocationAdmission;

const USER_GESTURE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct RuntimeHostLimits {
    pub max_active_plugins: usize,
    pub max_global_inflight: usize,
    pub max_plugin_inflight: usize,
    pub idle_timeout: Duration,
    pub maximum_failures: u32,
}

impl Default for RuntimeHostLimits {
    fn default() -> Self {
        Self {
            max_active_plugins: 32,
            max_global_inflight: 32,
            max_plugin_inflight: 1,
            idle_timeout: Duration::from_secs(60),
            maximum_failures: 5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapabilityRuntimePackage {
    pub manifest: CapabilityPackageManifest,
    pub package_dir: PathBuf,
    pub digest: String,
    pub trust_store_path: PathBuf,
    pub trust_status: PackageTrustStatus,
    pub permission_grant_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserGestureTarget {
    pub unit_id: String,
    pub revision: u64,
}

#[derive(Clone, Debug)]
pub struct CapabilityInvocation {
    pub request_id: String,
    pub command_id: String,
    pub input: Value,
    pub target: Option<ExtensionTarget>,
    pub resource_refs: Vec<ExtensionResourceRef>,
    pub unit_attachments: Vec<ExtensionUnitAttachment>,
    pub staged_resources: Vec<CapabilityStagedResource>,
    pub user_gesture_token: Option<String>,
    pub timeout: Option<Duration>,
}

/// Host-owned, read-only materialization of one opaque resource reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStagedResource {
    pub resource_ref: ExtensionResourceRef,
    pub staged_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct CapabilityInvocationOutput {
    pub plugin_id: String,
    pub package_digest: String,
    pub status: CapabilityRuntimeStatus,
    pub payload: Option<Value>,
    pub error: Option<CapabilityProtocolError>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapabilityRuntimeHealth {
    pub plugin_id: String,
    pub package_digest: String,
    pub payload: Option<Value>,
}

pub(super) struct ActivePackage {
    /// Shared rather than owned: `invoke` needs the package to outlive the per-package lock it
    /// releases across the blocking runtime call, and `contribution_snapshot` needs one per
    /// active plugin. Owning it made both deep-copy the entire manifest — every contribution,
    /// schema and entrypoint — on paths that only ever read it.
    pub(super) package: Arc<CapabilityRuntimePackage>,
    pub(super) effective_contributions: CapabilityContributions,
    command_schemas: HashMap<String, CommandSchemaValidators>,
    scope_id: String,
    pub(super) process: Option<RuntimeProcess>,
    pub(super) last_used: Instant,
    pub(super) failures: u32,
    pub(super) restart_not_before: Instant,
}

struct UserGestureGrant {
    plugin_id: String,
    command_id: String,
    target: Option<UserGestureTarget>,
    expires_at: Instant,
}

#[derive(Clone)]
struct InflightInvocation {
    plugin_id: String,
    runtime_request_id: String,
    process: RuntimeProcessClient,
}

/// Owns all plugin processes and resolves commands without plugin-specific branches.
pub struct CapabilityRuntimeHost {
    limits: RuntimeHostLimits,
    packages: Mutex<HashMap<String, Arc<Mutex<ActivePackage>>>>,
    commands: Mutex<HashMap<String, String>>,
    gestures: Mutex<HashMap<String, UserGestureGrant>>,
    inflight: Mutex<HashMap<String, InflightInvocation>>,
    admission: Mutex<InvocationAdmission>,
    generation: AtomicU64,
}

impl CapabilityRuntimeHost {
    #[must_use]
    pub fn new(limits: RuntimeHostLimits) -> Self {
        Self {
            limits,
            packages: Mutex::new(HashMap::new()),
            commands: Mutex::new(HashMap::new()),
            gestures: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            admission: Mutex::new(InvocationAdmission::default()),
            generation: AtomicU64::new(0),
        }
    }

    pub fn activate(&self, package: CapabilityRuntimePackage) -> HostResult<()> {
        validate_runtime_package(&package)?;
        let command_schemas = compile_command_schemas(&package)?;
        let plugin_id = package.manifest.qualified_id();
        {
            let packages = lock(&self.packages)?;
            if packages
                .get(&plugin_id)
                .and_then(|active| active.lock().ok())
                .is_some_and(|active| active.package.digest == package.digest)
            {
                return Ok(());
            }
            if !packages.contains_key(&plugin_id)
                && packages.len() >= self.limits.max_active_plugins
            {
                return Err(CapabilityHostError::Busy);
            }
        }

        let (process, effective_contributions) = if package.manifest.entrypoints.service.is_some() {
            let (process, contributions) = start_runtime(&package)?;
            let persistent = package
                .manifest
                .entrypoints
                .service
                .as_ref()
                .is_some_and(|service| service.process_model == CapabilityProcessModel::Persistent);
            (persistent.then_some(process), contributions)
        } else {
            (None, package.manifest.contributes.clone())
        };
        let registered_contributions = effective_contributions.clone();
        let scope_id = format!("scope:{}", Uuid::new_v4().simple());
        let active = ActivePackage {
            package: Arc::new(package),
            effective_contributions,
            command_schemas,
            scope_id,
            process,
            last_used: Instant::now(),
            failures: 0,
            restart_not_before: Instant::now(),
        };
        let active = Arc::new(Mutex::new(active));
        let old = {
            // Commands and packages become visible together; a failed replacement leaves the
            // previous runtime and contribution set untouched.
            let mut commands = lock(&self.commands)?;
            let mut packages = lock(&self.packages)?;
            if !packages.contains_key(&plugin_id)
                && packages.len() >= self.limits.max_active_plugins
            {
                return Err(CapabilityHostError::Busy);
            }
            validate_command_conflicts(&commands, &plugin_id, &registered_contributions)?;
            self.invalidate_plugin_gestures(&plugin_id)?;
            commands.retain(|_, owner| owner != &plugin_id);
            for command in &registered_contributions.commands {
                commands.insert(command.id.clone(), plugin_id.clone());
            }
            let old = packages.insert(plugin_id.clone(), active);
            self.generation.fetch_add(1, Ordering::SeqCst);
            old
        };
        drop(old);
        Ok(())
    }

    /// Issues an opaque, single-use token bound to one command and one target revision.
    pub fn issue_user_gesture(
        &self,
        command_id: &str,
        target: Option<UserGestureTarget>,
    ) -> HostResult<String> {
        let plugin_id = lock(&self.commands)?
            .get(command_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::NotFound(command_id.to_owned()))?;
        let token = Uuid::new_v4().simple().to_string();
        let mut gestures = lock(&self.gestures)?;
        gestures.retain(|_, grant| grant.expires_at > Instant::now());
        gestures.insert(
            token.clone(),
            UserGestureGrant {
                plugin_id,
                command_id: command_id.to_owned(),
                target,
                expires_at: Instant::now() + USER_GESTURE_TTL,
            },
        );
        Ok(token)
    }

    pub fn invalidate_user_gestures(&self) {
        if let Ok(mut gestures) = self.gestures.lock() {
            gestures.clear();
        }
    }

    /// Runs the protocol health method under the same restart and timeout policy as commands.
    pub fn health(&self, plugin_id: &str) -> HostResult<CapabilityRuntimeHealth> {
        let active = lock(&self.packages)?
            .get(plugin_id)
            .cloned()
            .ok_or_else(|| CapabilityHostError::NotFound(plugin_id.to_owned()))?;
        let mut active = lock(&active)?;
        if active.package.manifest.entrypoints.service.is_none() {
            return Err(CapabilityHostError::Unavailable(
                "plugin has no service runtime".to_owned(),
            ));
        }
        ensure_process(&mut active, &self.limits)?;
        let timeout = Duration::from_secs(2).min(Duration::from_secs(
            active.package.manifest.resources.timeout_seconds.max(1),
        ));
        let response = active.process.as_mut().expect("process was ensured").call(
            runtime_request(
                next_request_id(),
                CapabilityRuntimeMethod::Health,
                json!({}),
            ),
            timeout,
        );
        active.last_used = Instant::now();
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                active.process.take();
                record_failure(&mut active);
                return Err(error);
            }
        };
        let CapabilityRuntimeMessage::Response {
            status: CapabilityRuntimeStatus::Succeeded,
            payload,
            ..
        } = response
        else {
            active.process.take();
            record_failure(&mut active);
            return Err(CapabilityHostError::Unavailable(
                "runtime health check failed".to_owned(),
            ));
        };
        active.failures = 0;
        Ok(CapabilityRuntimeHealth {
            plugin_id: plugin_id.to_owned(),
            package_digest: active.package.digest.clone(),
            payload,
        })
    }

    pub fn deactivate(&self, plugin_id: &str) -> HostResult<bool> {
        self.cancel_plugin_inflight(plugin_id)?;
        let active = {
            let mut commands = lock(&self.commands)?;
            let mut packages = lock(&self.packages)?;
            self.invalidate_plugin_gestures(plugin_id)?;
            let active = packages.remove(plugin_id);
            if active.is_some() {
                commands.retain(|_, owner| owner != plugin_id);
                self.generation.fetch_add(1, Ordering::SeqCst);
            }
            active
        };
        let Some(active) = active else {
            return Ok(false);
        };
        if let Ok(mut active) = active.lock() {
            if let Some(mut process) = active.process.take() {
                let _ = call_method(
                    &mut process,
                    CapabilityRuntimeMethod::Deactivate,
                    json!({ "reason": "plugin_disabled" }),
                    Duration::from_secs(2),
                );
            }
        }
        Ok(true)
    }

    pub fn deactivate_all(&self) {
        let plugin_ids = self
            .packages
            .lock()
            .map(|packages| packages.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for plugin_id in plugin_ids {
            let _ = self.deactivate(&plugin_id);
        }
    }

    pub fn prune_idle(&self) -> HostResult<usize> {
        // A command runs with the package lock released, so `last_used` alone cannot tell an
        // idle runtime from one that is busy answering a long request. Skipping plugins with
        // in-flight invocations keeps the reaper from killing a live OCR pass whose declared
        // budget is as long as the idle timeout itself. The in-flight snapshot is taken and
        // released before any package lock: `invoke` acquires them the other way round, so
        // holding both here would close a deadlock cycle.
        let busy = lock(&self.inflight)?
            .values()
            .map(|inflight| inflight.plugin_id.clone())
            .collect::<HashSet<_>>();
        let active = lock(&self.packages)?
            .iter()
            .filter(|(plugin_id, _)| !busy.contains(plugin_id.as_str()))
            .map(|(_, active)| Arc::clone(active))
            .collect::<Vec<_>>();
        let mut pruned = 0usize;
        for active in active {
            let mut active = lock(&active)?;
            if active.process.is_some() && active.last_used.elapsed() >= self.limits.idle_timeout {
                if let Some(mut process) = active.process.take() {
                    process.terminate();
                }
                pruned += 1;
            }
        }
        Ok(pruned)
    }

    pub fn active_plugin_count(&self) -> usize {
        self.packages.lock().map(|values| values.len()).unwrap_or(0)
    }

    /// Resolves a command to its signed plugin owner without exposing process state.
    pub fn command_owner(&self, command_id: &str) -> HostResult<Option<String>> {
        Ok(lock(&self.commands)?.get(command_id).cloned())
    }

    /// Returns the current registration scope used to bind invocation-owned resources.
    pub fn plugin_scope(&self, plugin_id: &str) -> HostResult<Option<String>> {
        let active = lock(&self.packages)?.get(plugin_id).cloned();
        active
            .map(|active| lock(&active).map(|active| active.scope_id.clone()))
            .transpose()
    }

    pub fn contribution_snapshot(&self) -> HostResult<loom_protocol::ContributionSnapshot> {
        let packages = lock(&self.packages)?;
        let mut registrations = Vec::with_capacity(packages.len());
        for active in packages.values() {
            let active = lock(active)?;
            registrations.push(SnapshotRegistration {
                package: Arc::clone(&active.package),
                contributions: active.effective_contributions.clone(),
                scope_id: active.scope_id.clone(),
            });
        }
        build_contribution_snapshot(self.generation.load(Ordering::SeqCst), registrations)
    }

    /// Reads the contribution generation without materialising a snapshot.
    ///
    /// `contribution_snapshot` locks and deep-clones every active package, so a caller that only
    /// needs to know whether contributions moved should compare this counter instead.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn process_ids(&self) -> Vec<u32> {
        self.packages
            .lock()
            .map(|packages| {
                packages
                    .values()
                    .filter_map(|active| {
                        active
                            .lock()
                            .ok()
                            .and_then(|active| active.process.as_ref().and_then(RuntimeProcess::id))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn has_inflight_request(&self, request_id: &str) -> bool {
        self.inflight
            .lock()
            .is_ok_and(|inflight| inflight.contains_key(request_id))
    }

    fn invalidate_plugin_gestures(&self, plugin_id: &str) -> HostResult<()> {
        lock(&self.gestures)?.retain(|_, grant| grant.plugin_id != plugin_id);
        Ok(())
    }
}

impl Drop for CapabilityRuntimeHost {
    fn drop(&mut self) {
        self.deactivate_all();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> HostResult<std::sync::MutexGuard<'_, T>> {
    mutex
        .lock()
        .map_err(|_| CapabilityHostError::Unavailable("runtime host lock poisoned".to_owned()))
}

fn validate_command_conflicts(
    commands: &HashMap<String, String>,
    plugin_id: &str,
    contributions: &CapabilityContributions,
) -> HostResult<()> {
    for command in &contributions.commands {
        if commands
            .get(&command.id)
            .is_some_and(|owner| owner != plugin_id)
        {
            return Err(CapabilityHostError::Protocol(format!(
                "command contribution conflicts with another plugin: {}",
                command.id
            )));
        }
    }
    Ok(())
}
