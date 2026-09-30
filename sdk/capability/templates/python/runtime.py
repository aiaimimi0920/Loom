import json
import struct
import sys

MAX_FRAME_BYTES = 4 * 1024 * 1024
PROTOCOL = "loom.capability.runtime.v1"


def read_exact(length: int) -> bytes | None:
    chunks = bytearray()
    while len(chunks) < length:
        chunk = sys.stdin.buffer.read(length - len(chunks))
        if not chunk:
            return None if not chunks else _fail("truncated runtime frame")
        chunks.extend(chunk)
    return bytes(chunks)


def _fail(message: str):
    raise RuntimeError(message)


def read_frame() -> dict | None:
    header = read_exact(4)
    if header is None:
        return None
    length = struct.unpack(">I", header)[0]
    if length > MAX_FRAME_BYTES:
        _fail("frame exceeds 4 MiB")
    payload = read_exact(length)
    if payload is None:
        _fail("truncated runtime frame")
    return json.loads(payload.decode("utf-8"))


def write_frame(value: dict) -> None:
    payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
    if len(payload) > MAX_FRAME_BYTES:
        _fail("frame exceeds 4 MiB")
    sys.stdout.buffer.write(struct.pack(">I", len(payload)))
    sys.stdout.buffer.write(payload)
    sys.stdout.buffer.flush()


def response(request: dict) -> dict:
    input_value = request.get("payload", {}).get("input", {})
    text = input_value.get("text", "") if isinstance(input_value, dict) else ""
    return {
        "type": "response",
        "protocol": PROTOCOL,
        "apiVersion": "1.0",
        "requestId": request.get("requestId", "invalid"),
        "status": "succeeded",
        "payload": {
            "output": {"templateLanguage": "python", "text": text},
            "effects": [],
        },
    }


def main() -> None:
    while request := read_frame():
        if request.get("type") != "request" or request.get("protocol") != PROTOCOL:
            _fail("unsupported runtime request")
        write_frame(response(request))
        if request.get("method") == "deactivate":
            return


if __name__ == "__main__":
    main()
