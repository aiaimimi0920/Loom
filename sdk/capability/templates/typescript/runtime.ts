import { stdin, stdout } from "node:process";

const MAX_FRAME_BYTES = 4 * 1024 * 1024;
const PROTOCOL = "loom.capability.runtime.v1";

function writeFrame(value) {
  const payload = Buffer.from(JSON.stringify(value), "utf8");
  if (payload.length > MAX_FRAME_BYTES) throw new Error("frame exceeds 4 MiB");
  const header = Buffer.allocUnsafe(4);
  header.writeUInt32BE(payload.length);
  stdout.write(header);
  stdout.write(payload);
}

function respond(request) {
  const text = typeof request.payload?.input?.text === "string" ? request.payload.input.text : "";
  writeFrame({
    type: "response",
    protocol: PROTOCOL,
    apiVersion: "1.0",
    requestId: request.requestId,
    status: "succeeded",
    payload: {
      output: { templateLanguage: "typescript", text },
      effects: [],
    },
  });
}

let pending = Buffer.alloc(0);
for await (const chunk of stdin) {
  pending = Buffer.concat([pending, Buffer.from(chunk)]);
  while (pending.length >= 4) {
    const length = pending.readUInt32BE(0);
    if (length > MAX_FRAME_BYTES) throw new Error("frame exceeds 4 MiB");
    if (pending.length < length + 4) break;
    const request = JSON.parse(pending.subarray(4, length + 4).toString("utf8"));
    pending = pending.subarray(length + 4);
    if (request.type !== "request" || request.protocol !== PROTOCOL) {
      throw new Error("unsupported runtime request");
    }
    respond(request);
    if (request.method === "deactivate") process.exit(0);
  }
}

if (pending.length !== 0) throw new Error("truncated runtime frame");
