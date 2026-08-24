// Aggregates feature-owned source for focused desktop contract tests.
import { readFileSync } from "node:fs";

import { desktopStyleSource } from "./desktopStyleSource.ts";

export const appSource = [
  "../App.tsx",
  "../components/app/appShell.tsx",
  "../components/art/ArtEditDialog.tsx",
  "../components/art/artWizardModel.ts",
  "../components/art/ArtWizardFields.tsx",
  "../components/art/AddArtWizard.tsx",
  "../components/art/useAddArtWizardController.ts",
  "../components/art/ArtCreationDialog.tsx",
  "../components/art/RegistryPanel.tsx",
  "../components/art/ArtMarketplace.tsx",
  "../components/art/FrameworkManagementDialog.tsx",
  "../components/art/ArtPanel.tsx",
  "../components/devices/DeviceManagementPanel.tsx",
  "../components/feedback/AppFeedback.tsx",
  "../components/security/PluginSecurityPanel.tsx",
  "../components/settings/settingsModel.tsx",
  "../components/settings/SettingsPanels.tsx",
  "../components/settings/SettingsPanel.tsx",
  "../components/settings/useApplicationLinks.ts",
  "../components/settings/useSettingsPanelController.ts",
  "../components/settings/AboutPanel.tsx",
].map((relativePath) => readFileSync(new URL(relativePath, import.meta.url), "utf8")).join("\n");

export const styleSource = desktopStyleSource;
