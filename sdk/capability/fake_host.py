import argparse
import json
import struct
import subprocess
from concurrent.futures import ThreadPoolExecutor

MAX_FRAME_BYTES = 4 * 1024 * 1024
PROTOCOL = "loom.capability.runtime.v1"


def read_exact(stream, length: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < length:
        chunk = stream.read(length - len(chunks))
        if not chunk:
            raise RuntimeError("runtime closed stdout before completing a frame")
        chunks.extend(chunk)
    return bytes(chunks)


def read_frame(stream, executor: ThreadPoolExecutor, timeout: float) -> dict:
    header = executor.submit(read_exact, stream, 4).result(timeout)
    length = struct.unpack(">I", header)[0]
    if length == 0 or length > MAX_FRAME_BYTES:
        raise RuntimeError("runtime response length is outside the protocol budget")
    payload = executor.submit(read_exact, stream, length).result(timeout)
    value = json.loads(payload.decode("utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError("runtime response must be an object")
    return value


def write_frame(stream, value: dict) -> None:
    payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
    if len(payload) > MAX_FRAME_BYTES:
        raise RuntimeError("runtime request exceeds 4 MiB")
    stream.write(struct.pack(">I", len(payload)))
    stream.write(payload)
    stream.flush()


def call(process, executor, timeout: float, method: str, payload: dict, request_id: str) -> dict:
    write_frame(
        process.stdin,
        {
            "type": "request",
            "protocol": PROTOCOL,
            "apiVersion": "1.0",
            "requestId": request_id,
            "method": method,
            "payload": payload,
        },
    )
    response = read_frame(process.stdout, executor, timeout)
    expected = {
        "type": "response",
        "protocol": PROTOCOL,
        "apiVersion": "1.0",
        "requestId": request_id,
        "status": "succeeded",
    }
    for field, value in expected.items():
        if response.get(field) != value:
            raise RuntimeError(f"runtime response field {field} did not match {value!r}")
    return response


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Exercise a Loom capability runtime.")
    parser.add_argument("--expected-language")
    expected = parser.add_mutually_exclusive_group()
    expected.add_argument("--expected-text")
    expected.add_argument("--expected-text-utf8-hex")
    parser.add_argument("--target-language")
    parser.add_argument("--expected-translated", choices=("true", "false"))
    parser.add_argument("--command-id", default="publisher.example/template.run")
    parser.add_argument("--working-directory", required=True)
    parser.add_argument("--timeout-seconds", type=float, default=5.0)
    parser.add_argument("runtime_command")
    parser.add_argument("runtime_arguments", nargs=argparse.REMAINDER)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    expected_text = args.expected_text or "hello"
    if args.expected_text_utf8_hex:
        expected_text = bytes.fromhex(args.expected_text_utf8_hex).decode("utf-8")
    process = subprocess.Popen(
        [args.runtime_command, *args.runtime_arguments],
        cwd=args.working_directory,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    if process.stdin is None or process.stdout is None:
        raise RuntimeError("failed to open runtime pipes")
    executor = ThreadPoolExecutor(max_workers=1)
    try:
        call(process, executor, args.timeout_seconds, "initialize", {
            "hostFeatures": ["framed-json.v1"],
            "platformTarget": "windows-x64",
            "scopeId": "fake-host",
        }, "initialize-1")
        call(process, executor, args.timeout_seconds, "activate", {
            "packageDigest": "0" * 64,
        }, "activate-1")
        command_input = {"text": "hello"}
        if args.target_language:
            command_input["targetLanguage"] = args.target_language
        command = call(process, executor, args.timeout_seconds, "command", {
            "commandId": args.command_id,
            "input": command_input,
            "target": None,
            "resourceRefs": [],
            "unitAttachments": [],
            "stagedResources": [],
            "userGesture": False,
        }, "command-1")
        output = command.get("payload", {}).get("output", {})
        if args.expected_language and output.get("templateLanguage") != args.expected_language:
            raise RuntimeError("runtime language marker did not match the template contract")
        if args.target_language and output.get("targetLanguage") != args.target_language:
            raise RuntimeError("runtime target language did not match the command request")
        if args.expected_translated is not None:
            expected_translated = args.expected_translated == "true"
            if output.get("translated") is not expected_translated:
                raise RuntimeError("runtime translated marker did not match the expected result")
        if output.get("text") != expected_text:
            raise RuntimeError("runtime command result did not match the template contract")
        call(process, executor, args.timeout_seconds, "deactivate", {
            "reason": "fake_host_complete",
        }, "deactivate-1")
        process.stdin.close()
        if process.wait(args.timeout_seconds) != 0:
            raise RuntimeError(f"runtime exited with code {process.returncode}")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        if not process.stdin.closed:
            process.stdin.close()
        process.stdout.close()
        executor.shutdown(wait=True, cancel_futures=True)
    label = args.expected_language or args.runtime_command
    print(f"Capability runtime passed fake-host conformance: {label}")


if __name__ == "__main__":
    main()
