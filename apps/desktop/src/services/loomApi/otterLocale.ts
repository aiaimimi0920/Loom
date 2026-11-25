import { listManagedDevices } from "./otter.ts";
import { parseOtterLocale, type OtterLocaleRequestInput, type OtterLocaleState } from "./otterLocaleTypes.ts";
import { postJson } from "./transport.ts";

export async function listOtterLocaleDevices(baseUrl: string) {
  const response = await listManagedDevices(baseUrl);
  return response.devices.map(device => ({ ...device, locale: parseOtterLocale(device.locale) }));
}

export async function requestOtterLocale(
  baseUrl: string,
  deviceId: string,
  request: OtterLocaleRequestInput,
): Promise<OtterLocaleState> {
  const response = await postJson<{ locale: unknown }>(
    baseUrl, `/v1/devices/${encodeURIComponent(deviceId)}/locale-requests`, request,
  );
  const locale = parseOtterLocale(response.locale);
  if (!locale) throw new Error("Otter did not return a display-language receipt");
  return locale;
}
