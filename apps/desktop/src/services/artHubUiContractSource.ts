// Aggregates feature-owned source for focused desktop contract tests.
import { readFileSync } from "node:fs";

import { desktopStyleSource } from "./desktopStyleSource.ts";

export const appSource = [
  "../App.tsx",
  "../components/app/appShell.tsx",
  "../components/art/ArtEditDialog.tsx",
  "../components/art/ArtEditParameter.tsx",
  "../components/art/artEditModel.ts",
  "../components/art/useArtEditDialogController.ts",
  "../components/art/artWizardModel.ts",
  "../components/art/ArtWizardFields.tsx",
  "../components/art/AddArtWizard.tsx",
  "../components/art/CloudArtRequestFields.tsx",
  "../components/art/WorkflowArtPreviewPolicy.tsx",
  "../components/art/useAddArtWizardController.ts",
  "../components/art/useArtScriptSource.ts",
  "../components/art/useArtMcpDiscovery.ts",
  "../components/art/useArtWorkflowInterface.ts",
  "../components/art/ArtCreationDialog.tsx",
  "../components/art/RegistryPanel.tsx",
  "../components/art/ArtMarketplace.tsx",
  "../components/art/ArtPublishDialog.tsx",
  "../components/art/FrameworkFilter.tsx",
  "../components/art/useArtCreationSubmission.ts",
  "../components/art/FrameworkManagementDialog.tsx",
  "../components/art/ArtPanel.tsx",
  "../components/devices/DeviceManagementPanel.tsx",
  "../components/devices/ManagedDevicesPanel.tsx",
  "../components/devices/ManagedDeviceDialogs.tsx",
  "../components/devices/useManagedDevicesController.ts",
  "../components/security/CredentialFieldDialog.tsx",
  "../components/feedback/AppFeedback.tsx",
  "../components/security/PluginSecurityPanel.tsx",
  "../components/security/CredentialFieldList.tsx",
  "../components/settings/settingsModel.tsx",
  "../components/settings/SettingsPanels.tsx",
  "../components/settings/LoomSettingsSections.tsx",
  "../components/settings/SettingsPanel.tsx",
  "../components/settings/OtterShortcutSettings.tsx",
  "../components/settings/SettingsShortcutDialogs.tsx",
  "../components/settings/ShortcutSlotsEditor.tsx",
  "../components/settings/useApplicationLinks.ts",
  "../components/settings/useSettingsPanelController.ts",
  "../components/settings/useSettingsCacheControls.ts",
  "../components/settings/useSettingsShortcutEditors.ts",
  "../components/settings/AboutPanel.tsx",
]
  .map((relativePath) => readFileSync(new URL(relativePath, import.meta.url), "utf8"))
  .join("\n");

export const styleSource = desktopStyleSource;
