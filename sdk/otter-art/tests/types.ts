import type { ArtImage, JsonValue } from "../native-art";

NeuroSurface.define({ mount() {}, onLiveTrigger(event) {
    const callback: "onLiveTrigger" = event.delivery.callback; void callback;
    const observation: JsonValue = event.delivery.observation.value; void observation;
    // @ts-expect-error Events do not expose signed grants or writable host identity.
    void event.grant;
    // @ts-expect-error Callback delivery identity is a read-only snapshot and cannot mutate live-trigger grants.
    event.delivery.deliveryId = "forged";
} });
// @ts-expect-error No API allows scripts to self-register or resume live triggers.
NeuroSurface.configureLiveTrigger({ enabled: true });

const image: ArtImage = NeuroSurface.image("art-input:0");
const canvas = document.createElement("canvas");
canvas.width = image.width;
canvas.height = image.height;
canvas.getContext("2d").drawImage(image, 0, 0);
canvas.getContext("2d").drawImage(image, 0, 0, 1, 1, 0, 0, 10, 10);
canvas.getContext("2d").bezierCurveTo(0, 0, 1, 1, 2, 2);
const input: JsonValue = { optional: null, values: [true, 42, "text"] };
const output: Promise<JsonValue> = NeuroSurface.service("capability:vendor:run", input, 5000);
void output;
const confirmation: Promise<JsonValue> = NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{images:[image]});
void confirmation;
void NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{imageOutput:true,canvases:[canvas]}).then(value=> {
    canvas.getContext("2d").drawImage(value.image,0,0);
});
// @ts-expect-error Requesting confirmation is not approval; scripts cannot self-report approve.
NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{approve:true});
// @ts-expect-error Host gesture tokens are not part of the script contract.
NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{userGestureToken:"forged"});
void NeuroSurface.serviceImages("capability:vendor:run", input, [image], 5000);
void NeuroSurface.serviceCanvases("capability:vendor:run", input, [canvas], 5000);
void NeuroSurface.serviceImage("capability:vendor:run", input, { images: [image], timeoutMs: 5000 });
void NeuroSurface.serviceImage("capability:vendor:run", input, { canvases: [canvas] }).then(result => {
    canvas.getContext("2d").drawImage(result.image, 0, 0);
    const metadata: JsonValue = result.output; void metadata;
});
// @ts-expect-error Image output calls cannot select both input source modes simultaneously.
NeuroSurface.serviceImage("capability:vendor:run", input, { images: [image], canvases: [canvas] });
// @ts-expect-error Arbitrary URL or file-path options are not accepted for service outputs.
NeuroSurface.serviceImage("capability:vendor:run", input, { path: "c:/picture.png" });
// @ts-expect-error Canvas snapshot API does not accept installed ArtImage handles as canvases.
NeuroSurface.serviceCanvases("capability:vendor:run", input, [image]);
// @ts-expect-error Canvas snapshot API does not accept raw PNG data URLs.
NeuroSurface.serviceCanvases("capability:vendor:run", input, ["data:image/png;base64,AA=="]);
// @ts-expect-error Image service accepts only installed handles, not URLs.
NeuroSurface.serviceImages("capability:vendor:run", input, ["https://example.invalid/a.png"]);
// @ts-expect-error Mutable canvases cannot masquerade as immutable installed image inputs.
NeuroSurface.serviceImages("capability:vendor:run", input, [canvas]);
const queued: true = NeuroSurface.commitResult({
    image: { kind: "canvas", nodeId: "formal" }, value: { kind: "value", value: input },
});
void queued;
// @ts-expect-error Arbitrary URLs are not accepted as installed ArtImage handles.
canvas.getContext("2d").drawImage("https://example.invalid/picture.png", 0, 0);
// @ts-expect-error Matching dimensions cannot forge the nominal ArtImage identity.
const fake: ArtImage = { width: 2, height: 2, naturalWidth: 2, naturalHeight: 2 };
void fake;
// @ts-expect-error Service input accepts JSON values only; functions are not serializable.
NeuroSurface.service("capability:vendor:run", { callback: () => 42 });
// @ts-expect-error Formal image outputs must reference a mounted Canvas node, not arbitrary base64.
NeuroSurface.commitResult({ image: { kind: "image", base64: "AA==" } });
// @ts-expect-error Native Art SDK does not advertise legacy Surface remote emit actions.
NeuroSurface.emit({ nodeId: "x", action: "run", event: "click" });
// @ts-expect-error Native Art environment provides no browser fetch.
fetch("https://example.invalid/");
// @ts-expect-error Native Art environment provides no Node process object.
process.exit(0);
// @ts-expect-error Unsupported HTML img elements are not declared in the native DOM.
document.createElement("img");
// @ts-expect-error Unimplemented quadraticCurveTo is not advertised by the native path recorder.
canvas.getContext("2d").quadraticCurveTo(1, 1, 2, 2);
// @ts-expect-error Unimplemented ellipse is not advertised by the native path recorder.
canvas.getContext("2d").ellipse(0, 0, 1, 1, 0, 0, 1);
// @ts-expect-error Current Canvas nodes cannot be drawn as installed ArtImage inputs.
canvas.getContext("2d").drawImage(canvas, 0, 0);
// @ts-expect-error Native Canvas provides no browser pixel-readback getImageData API.
canvas.getContext("2d").getImageData(0, 0, 1, 1);
