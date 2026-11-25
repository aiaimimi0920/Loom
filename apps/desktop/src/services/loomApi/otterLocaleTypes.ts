// Device-scoped display language; Otter remains the only persistent settings owner.
export type OtterLocaleRequestStatus = "pending" | "applied" | "conflict" | "rejected" | "unknown";

export interface OtterLocaleRequestInput {
  requestId: string;
  receiverGeneration: string;
  expectedRevision: number;
  language: string;
}

export interface OtterLocaleRequest extends OtterLocaleRequestInput {
  expiresAt: number;
  validForMs: number;
  status: OtterLocaleRequestStatus;
  code?: string;
}

export interface OtterLocaleState {
  receiverGeneration: string;
  language: string;
  revision: number;
  supportedLanguages: string[];
  active: boolean;
  reportedAt: number;
  request: OtterLocaleRequest | null;
}

const statuses = new Set<OtterLocaleRequestStatus>(["pending", "applied", "conflict", "rejected", "unknown"]);
const record = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const identifier = (value: unknown): value is string =>
  typeof value === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(value);
const language = (value: unknown): value is string =>
  typeof value === "string" && value.length <= 64 && /^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/.test(value);
const integer = (value: unknown): value is number => Number.isSafeInteger(value) && Number(value) >= 0;

export function parseOtterLocale(value: unknown): OtterLocaleState | null {
  if (value === undefined || value === null) return null;
  if (!record(value)
    || !identifier(value.receiverGeneration) || !language(value.language)
    || !integer(value.revision) || !integer(value.reportedAt) || typeof value.active !== "boolean"
    || !Array.isArray(value.supportedLanguages) || value.supportedLanguages.length > 64
    || !value.supportedLanguages.every(language)
    || !value.supportedLanguages.includes(value.language)
    || new Set(value.supportedLanguages).size !== value.supportedLanguages.length) {
    throw new Error("Invalid Otter display-language state");
  }
  const request = value.request;
  if (request !== null && (
    !record(request) || !identifier(request.requestId) || !identifier(request.receiverGeneration)
    || !integer(request.expectedRevision) || !language(request.language)
    || !integer(request.expiresAt) || !integer(request.validForMs) || request.validForMs > 15_000
    || !statuses.has(request.status as OtterLocaleRequestStatus)
    || (request.code !== undefined && (typeof request.code !== "string" || request.code.length > 128))
  )) {
    throw new Error("Invalid Otter display-language receipt");
  }
  return value as unknown as OtterLocaleState;
}
