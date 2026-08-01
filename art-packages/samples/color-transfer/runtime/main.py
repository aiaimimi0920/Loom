import base64
import json
import os
import sys

from PIL import Image


def value(obj, *names, default=None):
    if not isinstance(obj, dict):
        return default
    for name in names:
        if name in obj and obj[name] is not None:
            return obj[name]
    return default


def image_path(value_obj, label, work_root):
    if isinstance(value_obj, dict):
        nested = value(value_obj, "path", "filePath", "imagePath", "url", "data", "base64", "value")
        return image_path(nested, label, work_root)
    if not isinstance(value_obj, str):
        return None
    text = value_obj.strip()
    if text.startswith("data:image/") and ";base64," in text:
        payload = text.split(",", 1)[1]
        path = os.path.join(work_root, f"{label}-input.png")
        with open(path, "wb") as handle:
            handle.write(base64.b64decode(payload))
        return path
    if text.startswith("file://"):
        text = text[7:]
    return text if os.path.isfile(text) else None


def data_url(path):
    with open(path, "rb") as handle:
        return "data:image/png;base64," + base64.b64encode(handle.read()).decode("ascii")


def main(request):
    work_root = value(request.get("context", {}), "tempDir", "cacheDir", default=os.path.join(os.getcwd(), ".cache"))
    os.makedirs(work_root, exist_ok=True)
    inputs = request.get("inputs", {})
    params = request.get("params", {})
    source = image_path(value(inputs, "input", "image", "source"), "color-source", work_root)
    reference = image_path(value(inputs, "reference", "referenceImage", "ref"), "color-reference", work_root)
    if not source or not reference:
        raise ValueError("input and reference images are required")

    strength = max(0.0, min(1.0, float(value(params, "strength", "mix_ratio", default=50)) / 100.0))
    source_image = Image.open(source).convert("RGBA")
    reference_image = Image.open(reference).convert("RGBA").resize(source_image.size, Image.Resampling.LANCZOS)
    blended = Image.blend(source_image, reference_image, strength)
    output_path = os.path.join(work_root, "color-transfer-output.png")
    blended.save(output_path, format="PNG")
    encoded = data_url(output_path)
    return {
        "status": "success",
        "output": {
            "output_base64": encoded,
            "output_path": output_path,
            "width": blended.width,
            "height": blended.height,
            "strength": strength,
            "content": [{"type": "image", "data": encoded, "mimeType": "image/png"}],
        },
    }


if __name__ == "__main__":
    try:
        request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
        print(json.dumps(main(request), ensure_ascii=False, separators=(",", ":")))
    except Exception as exc:
        print(json.dumps({"status": "error", "error": {"code": "color_transfer_failed", "message": str(exc)}}))
