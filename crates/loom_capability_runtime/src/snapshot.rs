use loom_protocol::{
    CapabilityContribution, CapabilityContributions, ContributionSnapshot, ExtensionContribution,
    ExtensionContributions, ExtensionPluginBinding, ExtensionTrustStatus, PackageTrustStatus,
    CAPABILITY_API_VERSION, EXTENSION_PROTOCOL,
};
use serde_json::json;

use crate::error::HostResult;
use crate::host::CapabilityRuntimePackage;

pub(super) struct SnapshotRegistration {
    pub package: std::sync::Arc<CapabilityRuntimePackage>,
    pub contributions: CapabilityContributions,
    pub scope_id: String,
}

pub(super) fn build_contribution_snapshot(
    generation: u64,
    mut registrations: Vec<SnapshotRegistration>,
) -> HostResult<ContributionSnapshot> {
    registrations.sort_by_key(|registration| registration.package.manifest.qualified_id());
    let plugins = registrations.iter().map(plugin_binding).collect();
    let mut contributions = empty_contributions();
    for registration in &registrations {
        append_contributions(&mut contributions, registration)?;
    }
    Ok(ContributionSnapshot {
        protocol: EXTENSION_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        generation,
        plugins,
        contributions,
    })
}

fn plugin_binding(registration: &SnapshotRegistration) -> ExtensionPluginBinding {
    ExtensionPluginBinding {
        id: registration.package.manifest.qualified_id(),
        version: registration.package.manifest.version.clone(),
        package_digest: registration.package.digest.clone(),
        trust_status: extension_trust(&registration.package.trust_status),
        permission_grant_digest: registration.package.permission_grant_digest.clone(),
        scope_id: registration.scope_id.clone(),
    }
}

fn append_contributions(
    output: &mut ExtensionContributions,
    registration: &SnapshotRegistration,
) -> HostResult<()> {
    let plugin_id = registration.package.manifest.qualified_id();
    for command in &registration.contributions.commands {
        output.commands.push(ExtensionContribution {
            id: command.id.clone(),
            plugin_id: plugin_id.clone(),
            scope_id: registration.scope_id.clone(),
            title: Some(command.title.clone()),
            command_id: Some(command.id.clone()),
            when: command.when.clone(),
            placement: None,
            order: None,
            payload: serde_json::to_value(command)?,
        });
    }
    append_generic(
        &mut output.shortcuts,
        &registration.contributions.shortcuts,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.menus,
        &registration.contributions.menus,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.settings,
        &registration.contributions.settings,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.data_types,
        &registration.contributions.data_types,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.renderers,
        &registration.contributions.renderers,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.unit_overlays,
        &registration.contributions.unit_overlays,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.background_tasks,
        &registration.contributions.background_tasks,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.resource_providers,
        &registration.contributions.resource_providers,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.diagnostics,
        &registration.contributions.diagnostics,
        registration,
        &plugin_id,
    );
    append_generic(
        &mut output.event_subscriptions,
        &registration.contributions.event_subscriptions,
        registration,
        &plugin_id,
    );
    Ok(())
}

fn append_generic(
    output: &mut Vec<ExtensionContribution>,
    values: &[CapabilityContribution],
    registration: &SnapshotRegistration,
    plugin_id: &str,
) {
    output.extend(values.iter().map(|value| ExtensionContribution {
        id: value.id.clone(),
        plugin_id: plugin_id.to_owned(),
        scope_id: registration.scope_id.clone(),
        title: value.title.clone(),
        command_id: value.command.clone(),
        when: value.when.clone(),
        placement: value.placement.clone(),
        order: value.order,
        payload: json!({ "schema": value.schema, "payload": value.payload }),
    }));
}

fn empty_contributions() -> ExtensionContributions {
    ExtensionContributions {
        commands: Vec::new(),
        shortcuts: Vec::new(),
        menus: Vec::new(),
        settings: Vec::new(),
        data_types: Vec::new(),
        renderers: Vec::new(),
        unit_overlays: Vec::new(),
        background_tasks: Vec::new(),
        resource_providers: Vec::new(),
        diagnostics: Vec::new(),
        event_subscriptions: Vec::new(),
    }
}

fn extension_trust(status: &PackageTrustStatus) -> ExtensionTrustStatus {
    match status {
        PackageTrustStatus::Trusted => ExtensionTrustStatus::Trusted,
        PackageTrustStatus::Unsigned => ExtensionTrustStatus::UnsignedDeveloper,
        PackageTrustStatus::Revoked => ExtensionTrustStatus::Revoked,
        PackageTrustStatus::Verified | PackageTrustStatus::Invalid => {
            ExtensionTrustStatus::Untrusted
        }
    }
}
