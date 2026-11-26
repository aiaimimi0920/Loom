// @ts-check
/// <reference path="../native-art.d.ts" />
// Distribute this JS directly; config and images come from signed package resources without browser, file, or arbitrary network APIs.
NeuroSurface.define({
    mount({ root, snapshot }) {
        const assets = snapshot.state.package;
        if (!assets?.config || !assets.picture) throw Error("package resources required");
        const raw = NeuroSurface.resource(assets.config.resourceId);
        if (raw === undefined) throw Error("config unavailable");
        /** @type {unknown} */
        const config = JSON.parse(raw);
        if (!config || typeof config !== "object" || !("answer" in config)
            || config.answer !== 42) throw Error("invalid config");
        const answer = config.answer;
        const picture = NeuroSurface.image(assets.picture.resourceId);
        const canvas = document.createElement("canvas");
        canvas.id = "formal";
        canvas.width = picture.width;
        canvas.height = picture.height;
        canvas.getContext("2d").drawImage(picture, 0, 0);
        const button = document.createElement("button");
        button.textContent = "Run";
        const status = document.createElement("span");
        status.textContent = "Ready";
        root.replaceChildren(button, canvas, status);
        const run = async () => {
            let mode = "online";
            try {
                const value = await NeuroSurface.service(
                    "capability:vendor:service.run", { answer }, 5000);
                if (value !== null) throw Error("unexpected service result");
            } catch (error) {
                if (!(error instanceof Error) || ![
                    "offline", "service_unavailable", "service_transport", "service_busy",
                ].includes(error.message)) throw error;
                mode = error.message;
            }
            status.textContent = mode;
            NeuroSurface.commitResult({
                output_image: { kind: "canvas", nodeId: canvas.id },
                answer: { kind: "value", value: answer },
                mode: { kind: "value", value: mode },
            });
        };
        button.addEventListener("click", run);
        // Native dispose runs cleanup first; never retain listeners or async handles across Art instances.
        return () => button.removeEventListener("click", run);
    },
});
