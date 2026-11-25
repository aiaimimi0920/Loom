// Composes device workspaces without transferring their lifetimes into the UI library.
import { Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import { useState } from "react";
import { ManagedDevicesPanel } from "./ManagedDevicesPanel";
import { WallManagementPanel } from "./WallManagementPanel";
import { ProjectionPeersPanel } from "./ProjectionPeersPanel";
import { ProjectionSettingsPanel } from "./ProjectionSettingsPanel";
import { OtterConnectionSettings } from "./OtterConnectionSettings";
export { deviceKindLabels } from "./useManagedDevicesController";
export { CredentialFieldDialog } from "../security/CredentialFieldDialog";
const workspaces = [
  { value: "devices", label: "设备" },
  { value: "walls", label: "屏幕墙" },
  { value: "projection", label: "投射连接" },
  { value: "projection-settings", label: "投射规则" },
] as const;
export function DeviceManagementPanel(props: { baseUrl: string; online: boolean }) {
  const [page, setPage] = useState<"devices" | "walls" | "projection" | "projection-settings">(
    "devices",
  );
  return (
    <TabsRoot asChild value={page} onValueChange={(value) => setPage(value as typeof page)}>
      <section className="loom-device-workspaces">
        <TabList label="设备管理页面">
          {workspaces.map((item) => (
            <Tab key={item.value} value={item.value}>
              {item.label}
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
