// Composes device workspaces without transferring their lifetimes into the UI library.
import { Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import { useState } from "react";
import { useUiText } from "../../i18n/index.ts";
import { ManagedDevicesPanel } from "./ManagedDevicesPanel";
import { WallManagementPanel } from "./WallManagementPanel";
import { ProjectionPeersPanel } from "./ProjectionPeersPanel";
import { ProjectionSettingsPanel } from "./ProjectionSettingsPanel";
import { OtterConnectionSettings } from "./OtterConnectionSettings";
export { deviceKindLabels } from "./useManagedDevicesController";
export { CredentialFieldDialog } from "../security/CredentialFieldDialog";
const workspaces = [
  { value: "devices", labelKey: "devices.tabs.devices" },
  { value: "walls", labelKey: "devices.tabs.walls" },
  { value: "projection", labelKey: "devices.tabs.projection" },
  { value: "projection-settings", labelKey: "devices.tabs.projectionSettings" },
] as const;
export function DeviceManagementPanel(props: { baseUrl: string; online: boolean }) {
  const { t } = useUiText();
  const [page, setPage] = useState<"devices" | "walls" | "projection" | "projection-settings">(
    "devices",
  );
  return (
    <TabsRoot asChild value={page} onValueChange={(value) => setPage(value as typeof page)}>
      <section className="loom-device-workspaces">
        <TabList label={t("devices.tabs.ariaLabel")}>
          {workspaces.map((item) => (
            <Tab key={item.value} value={item.value}>
              {t(item.labelKey)}
            </Tab>
          ))}
        </TabList>
        <TabPanel value="devices" forceMount hidden={page !== "devices"}>
          <ManagedDevicesPanel {...props} />
          <OtterConnectionSettings key={props.baseUrl} {...props} />
        </TabPanel>
        <TabPanel value="walls" forceMount hidden={page !== "walls"}>
          <WallManagementPanel key={props.baseUrl} {...props} />
        </TabPanel>
        <TabPanel value="projection">
          {page === "projection" && <ProjectionPeersPanel key={props.baseUrl + String(props.online)} {...props} />}
        </TabPanel>
        <TabPanel value="projection-settings">
          {page === "projection-settings" && (
            <ProjectionSettingsPanel key={props.baseUrl + String(props.online)} {...props} />
          )}
        </TabPanel>
      </section>
    </TabsRoot>
  );
}
