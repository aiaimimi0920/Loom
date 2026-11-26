/// <reference path="../native-art.d.ts" />
// Runs automatically after the host explicitly configures trigger conditions; grants no click, network, path, or self-registration rights.
/** @type {import("../native-art").ArtElement | undefined} */
let label;
let count = 0;
NeuroSurface.define({
  mount({ root }) {
    label = document.createElement("span");
    label.textContent = "0";
    root.replaceChildren(label);
  },
  onLiveTrigger(event) {
    if (!label) throw new Error("Art not mounted");
    count += 1;
    label.textContent = String(count);
    NeuroSurface.commitResult({
      data: { kind: "value", value: {
        count, sessionId: event.sessionId, epoch: event.epoch,
        deliveryId: event.delivery.deliveryId,
        observation: event.delivery.observation.value,
      } },
    });
  },
});
