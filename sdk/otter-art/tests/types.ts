import type { ArtImage, JsonValue } from "../native-art";

NeuroSurface.define({ mount() {}, onLiveTrigger(event) {
    const callback: "onLiveTrigger" = event.delivery.callback; void callback;
    const observation: JsonValue = event.delivery.observation.value; void observation;
    // @ts-expect-error 事件不暴露签名grant或可写宿主身份。
    void event.grant;
    // @ts-expect-error 回调身份是只读快照，不授修改自动授权能力。
    event.delivery.deliveryId = "forged";
} });
// @ts-expect-error 不提供脚本自行注册/恢复自动触发的API。
NeuroSurface.configureLiveTrigger({ enabled: true });

const image: ArtImage = NeuroSurface.image("art-input:0");
const canvas = document.createElement("canvas");
canvas.width = image.width;
canvas.height = image.height;
canvas.getContext("2d").drawImage(image, 0, 0);
canvas.getContext("2d").drawImage(image, 0, 0, 1, 1, 0, 0, 10, 10);
canvas.getContext("2d").bezierCurveTo(0, 0, 1, 1, 2, 2);
const input: JsonValue = { optional: null, values: [true, 42, "文字"] };
const output: Promise<JsonValue> = NeuroSurface.service("capability:vendor:run", input, 5000);
void output;
const confirmation: Promise<JsonValue> = NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{images:[image]});
void confirmation;
void NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{imageOutput:true,canvases:[canvas]}).then(value=> {
    canvas.getContext("2d").drawImage(value.image,0,0);
});
// @ts-expect-error 请求询问不是批准，不能由脚本自报approve。
NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{approve:true});
// @ts-expect-error 宿主gesture token不属于脚本合同。
NeuroSurface.serviceWithConfirmation("capability:vendor:run",input,{userGestureToken:"forged"});
void NeuroSurface.serviceImages("capability:vendor:run", input, [image], 5000);
void NeuroSurface.serviceCanvases("capability:vendor:run", input, [canvas], 5000);
void NeuroSurface.serviceImage("capability:vendor:run", input, { images: [image], timeoutMs: 5000 });
void NeuroSurface.serviceImage("capability:vendor:run", input, { canvases: [canvas] }).then(result => {
    canvas.getContext("2d").drawImage(result.image, 0, 0);
    const metadata: JsonValue = result.output; void metadata;
});
// @ts-expect-error 图像输出不能同时选择两个输入来源模式。
NeuroSurface.serviceImage("capability:vendor:run", input, { images: [image], canvases: [canvas] });
// @ts-expect-error 不接受服务输出的任意URL/文件路径参数。
NeuroSurface.serviceImage("capability:vendor:run", input, { path: "c:/picture.png" });
// @ts-expect-error Canvas快照接口不把已安装图片冒充Canvas。
NeuroSurface.serviceCanvases("capability:vendor:run", input, [image]);
// @ts-expect-error Canvas快照不接受任意PNG字节。
NeuroSurface.serviceCanvases("capability:vendor:run", input, ["data:image/png;base64,AA=="]);
// @ts-expect-error 图像服务只接已安装句柄，不接受URL。
NeuroSurface.serviceImages("capability:vendor:run", input, ["https://example.invalid/a.png"]);
// @ts-expect-error 不将可变Canvas冒充已安装的不可变输入。
NeuroSurface.serviceImages("capability:vendor:run", input, [canvas]);
const queued: true = NeuroSurface.commitResult({
    image: { kind: "canvas", nodeId: "formal" }, value: { kind: "value", value: input },
});
void queued;
// @ts-expect-error 不把任意URL当作已安装图片句柄。
canvas.getContext("2d").drawImage("https://example.invalid/picture.png", 0, 0);
// @ts-expect-error 即便尺寸相同也不能伪造图片对象身份。
const fake: ArtImage = { width: 2, height: 2, naturalWidth: 2, naturalHeight: 2 };
void fake;
// @ts-expect-error service只接JSON，函数不是可传输输入。
NeuroSurface.service("capability:vendor:run", { callback: () => 42 });
// @ts-expect-error 正式图片必须引用当前Canvas，不接受任意base64。
NeuroSurface.commitResult({ image: { kind: "image", base64: "AA==" } });
// @ts-expect-error 新SDK不广告旧Surface的远程emit操作。
NeuroSurface.emit({ nodeId: "x", action: "run", event: "click" });
// @ts-expect-error 原生环境没有浏览器fetch。
fetch("https://example.invalid/");
// @ts-expect-error 原生环境没有Node进程接口。
process.exit(0);
// @ts-expect-error 不声明当前原生不支持的HTML图片元素。
document.createElement("img");
// @ts-expect-error 不广告当前路径记录器未实现的二次曲线。
canvas.getContext("2d").quadraticCurveTo(1, 1, 2, 2);
// @ts-expect-error 不广告当前路径记录器未实现的椭圆。
canvas.getContext("2d").ellipse(0, 0, 1, 1, 0, 0, 1);
// @ts-expect-error 不把当前Canvas当作已安装输入图片。
canvas.getContext("2d").drawImage(canvas, 0, 0);
// @ts-expect-error 原生Canvas不提供浏览器像素读取接口。
canvas.getContext("2d").getImageData(0, 0, 1, 1);
