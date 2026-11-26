// Cloud request composition; parsing and draft ownership remain in the wizard controller.
import { Button, Field, Input, Select, Textarea } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import type { useAddArtWizardController } from "./useAddArtWizardController";

export function CloudArtRequestFields({ controller }: { controller: ReturnType<typeof useAddArtWizardController> }) {
  const { t } = useUiText();
  const {
    bodyText,
    cloudCurlText,
    cloudResponseText,
    contentType,
    endpoint,
    headersText,
    importCloudSmartTemplate,
    method,
    setBodyText,
    setCloudCurlText,
    setCloudResponseText,
    setContentType,
    setEndpoint,
    setHeadersText,
    setMethod,
  } = controller;
  return (
    <div className="art-creator-fields">
      <div className="art-creator-field--wide">
        <Field label={t("art.wizard.cloud.endpoint")}>
          <Input
            value={endpoint}
            onChange={(event) => setEndpoint(event.target.value)}
            placeholder="https://api.example.com/v1/process"
          />
        </Field>
      </div>
      <Field label={t("art.wizard.cloud.method")}>
        <Select value={method} onChange={(event) => setMethod(event.target.value)}>
          <option value="POST">POST</option>
          <option value="PUT">PUT</option>
          <option value="PATCH">PATCH</option>
          <option value="GET">GET</option>
          <option value="DELETE">DELETE</option>
        </Select>
      </Field>
      <Field label={t("art.wizard.cloud.contentType")}>
        <Input value={contentType} onChange={(event) => setContentType(event.target.value)} />
      </Field>
      <Field label={t("art.wizard.cloud.headers")}>
        <Textarea
          compact
          value={headersText}
          onChange={(event) => setHeadersText(event.target.value)}
          spellCheck={false}
        />
      </Field>
      <Field label={t("art.wizard.cloud.body")}>
        <Textarea compact value={bodyText} onChange={(event) => setBodyText(event.target.value)} spellCheck={false} />
      </Field>
      <Field label={t("art.wizard.cloud.curl")}>
        <Textarea
          compact
          value={cloudCurlText}
          onChange={(event) => setCloudCurlText(event.target.value)}
          spellCheck={false}
        />
      </Field>
      <Field label={t("art.wizard.cloud.responseSample")}>
        <Textarea
          compact
          value={cloudResponseText}
          onChange={(event) => setCloudResponseText(event.target.value)}
          spellCheck={false}
        />
      </Field>
      <div className="art-creator-inline-action">
        <Button variant="secondary" type="button" onClick={importCloudSmartTemplate}>
          {t("art.wizard.cloud.importCurl")}
        </Button>
      </div>
    </div>
  );
}
