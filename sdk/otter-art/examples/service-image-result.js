// @ts-check
/// <reference path="../native-art.d.ts" />
// 显式单PNG服务结果经不可伪造image绘制，正式提交仍需workspace_result许可。
NeuroSurface.define({
    mount({ root, snapshot }) {
        const asset = snapshot.state.package?.picture;
        if (!asset) throw Error("picture required");
        const input = NeuroSurface.image(asset.resourceId);
        const canvas = document.createElement("canvas");
        canvas.id = "formal";
        canvas.width = input.width;
        canvas.height = input.height;
        const button = document.createElement("button");
        button.textContent = "运行";
        root.replaceChildren(button, canvas);
        const run = async () => {
            const result = await NeuroSurface.serviceImage(
                "capability:vendor:service.run", { returnImage: true },
                { images: [input], timeoutMs: 5000 });
            const value = result.output;
            if (!value || typeof value !== "object" || !("answer" in value) || value.answer !== 42)
                throw Error("invalid metadata");
            canvas.width = result.image.width;
            canvas.height = result.image.height;
            canvas.getContext("2d").drawImage(result.image, 0, 0);
            NeuroSurface.commitResult({
                output_image: { kind: "canvas", nodeId: canvas.id },
                answer: { kind: "value", value: value.answer },
                mode: { kind: "value", value: "image_output" },
            });
        };
        button.addEventListener("click", run);
        return () => button.removeEventListener("click", run);
    },
});
