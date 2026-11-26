// Owns bounded polling and receipt tracking, never a persistent language preference.
import { useCallback, useEffect, useRef, useState } from "react";
import { uiText } from "../../i18n/index.ts";
import { listOtterLocaleDevices, requestOtterLocale } from "../../services/loomApi/otterLocale.ts";
import type { LoomManagedDevice } from "../../services/loomApi/otterTypes.ts";
import { localeUnavailableReason, observeLocaleRequest, type TrackedLocaleRequest } from "./otterLanguageState.ts";

export function useOtterLanguageSettings(baseUrl: string, online: boolean, active: boolean) {
  const [devices, setDevices] = useState<LoomManagedDevice[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [tracked, setTracked] = useState<TrackedLocaleRequest | null>(null);
  const generation = useRef(0);
  const receiptVersion = useRef(0);
  const mutationInFlight = useRef(false);
  const refreshInFlight = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    const version = generation.current;
    if (!active || !online || refreshInFlight.current === version) return;
    const readReceiptVersion = receiptVersion.current;
    refreshInFlight.current = version;
    try {
      const response = await listOtterLocaleDevices(baseUrl);
      if (version !== generation.current || readReceiptVersion !== receiptVersion.current) return;
      setDevices(response);
      setLoadError(null);
      setTracked(current => current
        ? observeLocaleRequest(current, response.find(device => device.id === current.deviceId)?.locale, performance.now())
        : null);
    } catch (error) {
      if (version === generation.current) {
        setLoadError(error instanceof Error ? error.message : uiText("settings.otterLanguage.loadFailed"));
      }
    } finally {
      if (refreshInFlight.current === version) refreshInFlight.current = null;
      if (version === generation.current) setLoading(false);
    }
  }, [active, baseUrl, online]);

  useEffect(() => {
    generation.current += 1;
    mutationInFlight.current = false;
    setDevices([]);
    setSelectedId("");
    setTracked(null);
    setLoadError(null);
    setSubmitError(null);
    setLoading(active && online);
    void refresh();
    const timer = active && online ? window.setInterval(() => {
      setTracked(current => current ? observeLocaleRequest(current, null, performance.now()) : null);
      void refresh();
    }, 2000) : undefined;
    return () => {
      generation.current += 1;
      window.clearInterval(timer);
    };
  }, [active, online, refresh]);

  const selected = devices.find(device => device.id === selectedId);
  const pending = tracked?.status === "pending";
  const unavailable = localeUnavailableReason(selected, online);

  const chooseDevice = (deviceId: string) => {
    if (mutationInFlight.current || pending) return;
    setSelectedId(deviceId);
    setTracked(null);
    setSubmitError(null);
  };

  const chooseLanguage = async (language: string) => {
    const locale = selected?.locale;
    if (!active || !selected || !locale || unavailable || pending || loadError
      || mutationInFlight.current || language === locale.language
      || !locale.supportedLanguages.includes(language)) return;
    const version = generation.current;
    const request = {
      requestId: crypto.randomUUID(),
      receiverGeneration: locale.receiverGeneration,
      expectedRevision: locale.revision,
      language,
    };
    mutationInFlight.current = true;
    setSubmitError(null);
    setTracked({ ...request, deviceId: selected.id, deadline: performance.now() + 15_000, status: "pending" });
    try {
      const state = await requestOtterLocale(baseUrl, selected.id, request);
      if (version !== generation.current) return;
      receiptVersion.current += 1;
      setDevices(current => current.map(device => device.id === selected.id ? { ...device, locale: state } : device));
      setTracked(current => current ? observeLocaleRequest(current, state, performance.now()) : null);
    } catch (error) {
      if (version !== generation.current) return;
      // A lost HTTP response cannot establish whether Otter persisted the request.
      setSubmitError(error instanceof Error ? error.message : uiText("settings.otterLanguage.submitUnconfirmed"));
      setTracked(current => current?.requestId === request.requestId ? { ...current, status: "unknown" } : current);
    } finally {
      if (version === generation.current) {
        mutationInFlight.current = false;
        receiptVersion.current += 1;
        void refresh();
      }
    }
  };

  return {
    devices, selectedId, selected, loading, loadError, submitError, tracked, pending, unavailable,
    chooseDevice, chooseLanguage, refresh,
  };
}
