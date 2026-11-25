/// <reference path="../native-art.d.ts" />
// 宿主显式配置条件后自动运行；没有点击、网络、路径或自行登记授权。
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
