import base64
import colorsys
import json
import math
import os
import struct
import sys
import zlib

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


def paeth(left, above, upper_left):
    estimate = left + above - upper_left
    left_distance = abs(estimate - left)
    above_distance = abs(estimate - above)
    upper_left_distance = abs(estimate - upper_left)
    if left_distance <= above_distance and left_distance <= upper_left_distance:
        return left
    if above_distance <= upper_left_distance:
        return above
    return upper_left


def decode_png(path):
    payload = open(path, "rb").read()
    if payload[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("only PNG input is supported")
    offset = 8
    header = None
    compressed = bytearray()
    while offset < len(payload):
        length = struct.unpack(">I", payload[offset:offset + 4])[0]
        chunk_type = payload[offset + 4:offset + 8]
        chunk_data = payload[offset + 8:offset + 8 + length]
        offset += 12 + length
        if chunk_type == b"IHDR":
            header = struct.unpack(">IIBBBBB", chunk_data)
        elif chunk_type == b"IDAT":
            compressed.extend(chunk_data)
        elif chunk_type == b"IEND":
            break
    if header is None:
        raise ValueError("PNG is missing IHDR")
    width, height, bit_depth, color_type, compression, filtering, interlace = header
    if bit_depth != 8 or compression != 0 or filtering != 0 or interlace != 0:
        raise ValueError("only non-interlaced 8-bit PNG input is supported")
    channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(color_type)
    if channels is None:
        raise ValueError(f"unsupported PNG color type: {color_type}")
    raw = zlib.decompress(bytes(compressed))
    stride = width * channels
    previous = bytearray(stride)
    rgba = bytearray(width * height * 4)
    raw_offset = 0
    rgba_offset = 0
    for _ in range(height):
        filter_type = raw[raw_offset]
        raw_offset += 1
        encoded = raw[raw_offset:raw_offset + stride]
        raw_offset += stride
        row = bytearray(stride)
        for index, current in enumerate(encoded):
            left = row[index - channels] if index >= channels else 0
            above = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_type == 0:
                decoded = current
            elif filter_type == 1:
                decoded = current + left
            elif filter_type == 2:
                decoded = current + above
            elif filter_type == 3:
                decoded = current + ((left + above) // 2)
            elif filter_type == 4:
                decoded = current + paeth(left, above, upper_left)
            else:
                raise ValueError(f"unsupported PNG filter: {filter_type}")
            row[index] = decoded & 0xFF
        for column in range(width):
            start = column * channels
            if color_type == 0:
                red = green = blue = row[start]
                alpha = 255
            elif color_type == 2:
                red, green, blue = row[start:start + 3]
                alpha = 255
            elif color_type == 4:
                red = green = blue = row[start]
                alpha = row[start + 1]
            else:
                red, green, blue, alpha = row[start:start + 4]
            rgba[rgba_offset:rgba_offset + 4] = bytes((red, green, blue, alpha))
            rgba_offset += 4
        previous = row
    return width, height, rgba


def png_chunk(kind, payload):
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )


def encode_png_bytes(width, height, rgba):
    scanlines = bytearray()
    stride = width * 4
    for row in range(height):
        scanlines.append(0)
        start = row * stride
        scanlines.extend(rgba[start:start + stride])
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", zlib.compress(bytes(scanlines), 9))
        + png_chunk(b"IEND", b"")
    )


def encode_png(path, width, height, rgba):
    payload = encode_png_bytes(width, height, rgba)
    with open(path, "wb") as handle:
        handle.write(payload)


def resize_rgba(width, height, pixels, target_width, target_height):
    if width == target_width and height == target_height:
        return pixels
    resized = bytearray(target_width * target_height * 4)
    for y in range(target_height):
        source_y = min(height - 1, y * height // target_height)
        for x in range(target_width):
            source_x = min(width - 1, x * width // target_width)
            source_offset = (source_y * width + source_x) * 4
            target_offset = (y * target_width + x) * 4
            resized[target_offset:target_offset + 4] = pixels[source_offset:source_offset + 4]
    return resized


PARAMETER_SPECS = {
    "strength": (100.0, 0.0, 100.0),
    "gamma": (1.0, 0.1, 3.0),
    "exposure": (0.0, -4.0, 4.0),
    "contrast": (0.0, -100.0, 100.0),
    "highlights": (0.0, -100.0, 100.0),
    "shadows": (0.0, -100.0, 100.0),
    "whites": (0.0, -100.0, 100.0),
    "blacks": (0.0, -100.0, 100.0),
    "temperature": (0.0, -100.0, 100.0),
    "tint": (0.0, -100.0, 100.0),
    "saturation": (0.0, -100.0, 100.0),
    "vibrance": (0.0, -100.0, 100.0),
    "hue": (0.0, 0.0, 360.0),
    "split_h_hue": (30.0, 0.0, 360.0),
    "split_h_sat": (0.0, 0.0, 100.0),
    "split_s_hue": (210.0, 0.0, 360.0),
    "split_s_sat": (0.0, 0.0, 100.0),
    "split_balance": (0.0, -100.0, 100.0),
}


def clamp(number, minimum=0.0, maximum=1.0):
    return max(minimum, min(maximum, number))


def number_param(params, name, default, minimum, maximum):
    raw = params.get(name, default) if isinstance(params, dict) else default
    try:
        parsed = float(raw)
    except (TypeError, ValueError):
        parsed = default
    if not math.isfinite(parsed):
        parsed = default
    return clamp(parsed, minimum, maximum)


def boolean_param(params, name, default=False):
    raw = params.get(name, default) if isinstance(params, dict) else default
    if isinstance(raw, bool):
        return raw
    if isinstance(raw, (int, float)):
        return raw != 0
    if isinstance(raw, str):
        return raw.strip().lower() in ("1", "true", "yes", "on")
    return default


def normalized_parameters(params):
    normalized = {}
    for name, (default, minimum, maximum) in PARAMETER_SPECS.items():
        normalized[name] = number_param(params, name, default, minimum, maximum)
    normalized["skin_protection"] = boolean_param(params, "skin_protection")
    return normalized


def srgb_to_linear(channel):
    if channel <= 0.04045:
        return channel / 12.92
    return ((channel + 0.055) / 1.055) ** 2.4


def linear_to_srgb(channel):
    if channel <= 0.0031308:
        return channel * 12.92
    return 1.055 * (max(channel, 0.0) ** (1.0 / 2.4)) - 0.055


def linear_rgb_to_oklab(red, green, blue):
    light = 0.4122214708 * red + 0.5363325363 * green + 0.0514459929 * blue
    medium = 0.2119034982 * red + 0.6806995451 * green + 0.1073969566 * blue
    short = 0.0883024619 * red + 0.2817188376 * green + 0.6299787005 * blue
    light_root = max(light, 0.0) ** (1.0 / 3.0)
    medium_root = max(medium, 0.0) ** (1.0 / 3.0)
    short_root = max(short, 0.0) ** (1.0 / 3.0)
    return (
        0.2104542553 * light_root + 0.7936177850 * medium_root - 0.0040720468 * short_root,
        1.9779984951 * light_root - 2.4285922050 * medium_root + 0.4505937099 * short_root,
        0.0259040371 * light_root + 0.7827717662 * medium_root - 0.8086757660 * short_root,
    )


def oklab_to_linear_rgb(lightness, axis_a, axis_b):
    light_root = lightness + 0.3963377774 * axis_a + 0.2158037573 * axis_b
    medium_root = lightness - 0.1055613458 * axis_a - 0.0638541728 * axis_b
    short_root = lightness - 0.0894841775 * axis_a - 1.2914855480 * axis_b
    light = light_root ** 3
    medium = medium_root ** 3
    short = short_root ** 3
    return (
        4.0767416621 * light - 3.3077115913 * medium + 0.2309699292 * short,
        -1.2684380046 * light + 2.6097574011 * medium - 0.3413193965 * short,
        -0.0041960863 * light - 0.7034186147 * medium + 1.7076147010 * short,
    )


def oklab_to_srgb_gamut(lightness, axis_a, axis_b):
    for chroma_scale in (1.0, 0.8, 0.6, 0.4, 0.2, 0.0):
        red, green, blue = oklab_to_linear_rgb(
            lightness,
            axis_a * chroma_scale,
            axis_b * chroma_scale,
        )
        if all(-0.01 <= channel <= 1.01 for channel in (red, green, blue)):
            return tuple(clamp(linear_to_srgb(channel)) for channel in (red, green, blue))
    return tuple(clamp(linear_to_srgb(channel)) for channel in (red, green, blue))


def compute_oklab_stats(width, height, pixels):
    sample_limit = 16384
    sample_step = max(1, int(math.sqrt(max(1, width * height) / sample_limit)))
    while math.ceil(width / sample_step) * math.ceil(height / sample_step) > sample_limit:
        sample_step += 1
    samples = []
    for y in range(0, height, sample_step):
        for x in range(0, width, sample_step):
            offset = (y * width + x) * 4
            if pixels[offset + 3] == 0:
                continue
            red = pixels[offset] / 255.0
            green = pixels[offset + 1] / 255.0
            blue = pixels[offset + 2] / 255.0
            lab = linear_rgb_to_oklab(
                srgb_to_linear(red),
                srgb_to_linear(green),
                srgb_to_linear(blue),
            )
            samples.append((lab, (red + green + blue) / 3.0))
    if not samples:
        return {"mean": (0.5, 0.0, 0.0), "std": (0.25, 0.1, 0.1)}
    visible = [lab for lab, brightness in samples if brightness > 0.01]
    if len(visible) < 100:
        visible = [lab for lab, _ in samples]
    count = len(visible)
    mean = tuple(sum(sample[channel] for sample in visible) / count for channel in range(3))
    variance = tuple(
        sum((sample[channel] - mean[channel]) ** 2 for sample in visible) / count
        for channel in range(3)
    )
    return {
        "mean": mean,
        "std": tuple(max(math.sqrt(value), 0.001) for value in variance),
    }


def smoothstep(edge_zero, edge_one, number):
    if edge_zero == edge_one:
        return 0.0
    position = clamp((number - edge_zero) / (edge_one - edge_zero))
    return position * position * (3.0 - 2.0 * position)


def mix(first, second, amount):
    return first * (1.0 - amount) + second * amount


def mix_color(first, second, amount):
    return tuple(mix(first[index], second[index], amount) for index in range(3))


def skin_mask(color):
    red, green, blue = color
    chroma_blue = -0.1687 * red - 0.3313 * green + 0.5 * blue + 0.5
    chroma_red = 0.5 * red - 0.4187 * green - 0.0813 * blue + 0.5
    blue_distance = chroma_blue - 0.42
    red_distance = chroma_red - 0.59
    distance = math.sqrt(blue_distance * blue_distance * 2.0 + red_distance * red_distance * 1.5)
    return 1.0 - smoothstep(0.04, 0.15, distance)


def luma(color):
    return color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722


def adjust_tone(color, params):
    exposure_scale = 2.0 ** params["exposure"]
    adjusted = tuple(channel * exposure_scale for channel in color)
    brightness = luma(adjusted)
    shadow_factor = 1.0 + (params["shadows"] / 100.0) * (1.0 - smoothstep(0.0, 0.5, brightness))
    highlight_factor = 1.0 + (params["highlights"] / 100.0) * smoothstep(0.5, 1.0, brightness)
    adjusted = tuple(channel * shadow_factor * highlight_factor for channel in adjusted)
    whites = params["whites"] / 100.0
    blacks = params["blacks"] / 100.0
    denominator = 1.0 + whites * 0.2 - blacks * 0.2
    adjusted = tuple((channel - blacks * 0.2) / denominator for channel in adjusted)
    inverse_gamma = 1.0 / params["gamma"]
    adjusted = tuple(max(channel, 0.0) ** inverse_gamma for channel in adjusted)
    contrast = 1.0 + params["contrast"] / 100.0
    return tuple(clamp((channel - 0.5) * contrast + 0.5) for channel in adjusted)


def adjust_color(color, params):
    temperature = params["temperature"] / 100.0
    tint = params["tint"] / 100.0
    adjusted = (
        color[0] + temperature * 0.2,
        color[1] + tint * 0.2,
        color[2] - temperature * 0.2,
    )
    hue, saturation, brightness = colorsys.rgb_to_hsv(*(clamp(channel) for channel in adjusted))
    hue = (hue + params["hue"] / 360.0) % 1.0
    vibrance = params["vibrance"] / 100.0
    saturation = saturation * (1.0 + vibrance * (1.0 - saturation))
    saturation *= 1.0 + params["saturation"] / 100.0
    return colorsys.hsv_to_rgb(hue, clamp(saturation), clamp(brightness))


def split_tone(color, params):
    if params["split_h_sat"] <= 0.0 and params["split_s_sat"] <= 0.0:
        return color
    highlight = colorsys.hsv_to_rgb(
        (params["split_h_hue"] / 360.0) % 1.0,
        params["split_h_sat"] / 100.0,
        1.0,
    )
    shadow = colorsys.hsv_to_rgb(
        (params["split_s_hue"] / 360.0) % 1.0,
        params["split_s_sat"] / 100.0,
        1.0,
    )
    balance = 0.5 + params["split_balance"] / 200.0
    amount = smoothstep(0.0, 1.0, luma(color) - balance + 0.5)
    toned = tuple(
        mix(shadow[index] * color[index], highlight[index] * color[index], amount)
        for index in range(3)
    )
    return mix_color(color, toned, 0.3)


def transfer_lut_color(color, source_stats, reference_stats):
    linear = tuple(srgb_to_linear(channel) for channel in color)
    lightness, axis_a, axis_b = linear_rgb_to_oklab(*linear)
    source_mean = source_stats["mean"]
    source_std = source_stats["std"]
    reference_mean = reference_stats["mean"]
    reference_std = reference_stats["std"]
    scale_lightness = clamp(reference_std[0] / source_std[0], 0.5, 1.5)
    scale_a = clamp(reference_std[1] / source_std[1], 0.5, 1.5) * 0.8
    scale_b = clamp(reference_std[2] / source_std[2], 0.5, 1.5) * 0.8
    mapped_lightness = (lightness - source_mean[0]) * scale_lightness + reference_mean[0]
    lightness_blend = clamp(lightness * (1.0 - lightness) * 4.0)
    new_lightness = lightness + (mapped_lightness - lightness) * lightness_blend * 0.6
    new_lightness = lightness * 0.4 + new_lightness * 0.6
    new_a = (axis_a - source_mean[1]) * scale_a + reference_mean[1]
    new_b = (axis_b - source_mean[2]) * scale_b + reference_mean[2]
    chroma_blend = 1.0
    if new_lightness < 0.1:
        chroma_blend = new_lightness / 0.1
    elif new_lightness > 0.9:
        chroma_blend = (1.0 - new_lightness) / 0.1
    chroma_blend = clamp(chroma_blend)
    new_a = mix(axis_a, new_a, chroma_blend)
    new_b = mix(axis_b, new_b, chroma_blend)
    return oklab_to_srgb_gamut(new_lightness, new_a, new_b)


def transferred_color(color, source_stats, reference_stats, params):
    styled = transfer_lut_color(color, source_stats, reference_stats)
    if params["skin_protection"]:
        styled = mix_color(styled, color, skin_mask(color) * 0.85)
    adjusted = mix_color(color, styled, params["strength"] / 100.0)
    adjusted = adjust_tone(adjusted, params)
    adjusted = adjust_color(adjusted, params)
    return split_tone(adjusted, params)


def build_color_lut(source_stats, reference_stats, params, size=16):
    table = []
    maximum = size - 1
    for blue in range(size):
        for green in range(size):
            for red in range(size):
                color = (red / maximum, green / maximum, blue / maximum)
                table.append(transferred_color(color, source_stats, reference_stats, params))
    return table


def build_transfer_lut(source_stats, reference_stats, size=16):
    table = []
    maximum = size - 1
    for blue in range(size):
        for green in range(size):
            for red in range(size):
                color = (red / maximum, green / maximum, blue / maximum)
                table.append(transfer_lut_color(color, source_stats, reference_stats))
    return table


def lut_texture_data_url(table, size=16):
    width = size * size
    height = size
    rgba = bytearray(width * height * 4)
    for blue in range(size):
        for green in range(size):
            for red in range(size):
                color = table[(blue * size + green) * size + red]
                offset = (green * width + blue * size + red) * 4
                rgba[offset] = round(clamp(color[0]) * 255.0)
                rgba[offset + 1] = round(clamp(color[1]) * 255.0)
                rgba[offset + 2] = round(clamp(color[2]) * 255.0)
                rgba[offset + 3] = 255
    payload = encode_png_bytes(width, height, rgba)
    return "data:image/png;base64," + base64.b64encode(payload).decode("ascii")


def shader_source(filename):
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), filename)
    with open(path, "r", encoding="utf-8") as handle:
        return handle.read()


def shader_output(source_stats, reference_stats, params, size=16):
    lut = build_transfer_lut(source_stats, reference_stats, size)
    uniforms = {
        name: (1.0 if value else 0.0) if isinstance(value, bool) else value
        for name, value in params.items()
    }
    return {
        "type": "shader",
        "vertex_shader": shader_source("color_transfer.vert"),
        "fragment_shader": shader_source("color_transfer.frag"),
        "uniforms": uniforms,
        "textures": {"lut": lut_texture_data_url(lut, size)},
        "algorithm": "oklab-statistical-transfer-shader",
    }


def sample_color_lut(table, size, color):
    positions = [clamp(channel) * (size - 1) for channel in color]
    lower = [int(math.floor(position)) for position in positions]
    upper = [min(value + 1, size - 1) for value in lower]
    fraction = [positions[index] - lower[index] for index in range(3)]

    def at(red, green, blue):
        return table[(blue * size + green) * size + red]

    low_blue_low_green = mix_color(at(lower[0], lower[1], lower[2]), at(upper[0], lower[1], lower[2]), fraction[0])
    low_blue_high_green = mix_color(at(lower[0], upper[1], lower[2]), at(upper[0], upper[1], lower[2]), fraction[0])
    high_blue_low_green = mix_color(at(lower[0], lower[1], upper[2]), at(upper[0], lower[1], upper[2]), fraction[0])
    high_blue_high_green = mix_color(at(lower[0], upper[1], upper[2]), at(upper[0], upper[1], upper[2]), fraction[0])
    low_blue = mix_color(low_blue_low_green, low_blue_high_green, fraction[1])
    high_blue = mix_color(high_blue_low_green, high_blue_high_green, fraction[1])
    return mix_color(low_blue, high_blue, fraction[2])


def main(request):
    work_root = value(request.get("context", {}), "tempDir", "cacheDir", default=os.path.join(os.getcwd(), ".cache"))
    os.makedirs(work_root, exist_ok=True)
    inputs = request.get("inputs", {})
    params = request.get("params", {})
    output_mode = str(value(params, "output_mode", "mode", default=value(inputs, "output_mode", "mode", default="image"))).strip().lower()
    source = image_path(value(inputs, "input_path", "input", "image", "source"), "color-source", work_root)
    reference = image_path(value(inputs, "reference_path", "reference", "referenceImage", "ref"), "color-reference", work_root)
    if not source or not reference:
        raise ValueError("input and reference images are required")

    width, height, source_pixels = decode_png(source)
    reference_width, reference_height, reference_pixels = decode_png(reference)
    applied_params = normalized_parameters(params)
    source_stats = compute_oklab_stats(width, height, source_pixels)
    reference_stats = compute_oklab_stats(reference_width, reference_height, reference_pixels)
    lut_size = 16

    if output_mode == "shader":
        return {
            "status": "success",
            "output": shader_output(source_stats, reference_stats, applied_params, lut_size),
        }

    color_lut = build_color_lut(source_stats, reference_stats, applied_params, lut_size)
    transferred = bytearray(len(source_pixels))
    for index in range(0, len(source_pixels), 4):
        source_color = (
            source_pixels[index] / 255.0,
            source_pixels[index + 1] / 255.0,
            source_pixels[index + 2] / 255.0,
        )
        output_color = sample_color_lut(color_lut, lut_size, source_color)
        transferred[index] = round(clamp(output_color[0]) * 255.0)
        transferred[index + 1] = round(clamp(output_color[1]) * 255.0)
        transferred[index + 2] = round(clamp(output_color[2]) * 255.0)
        transferred[index + 3] = source_pixels[index + 3]
    output_path = os.path.join(work_root, "color-transfer-output.png")
    encode_png(output_path, width, height, transferred)
    encoded = data_url(output_path)
    return {
        "status": "success",
        "output": {
            "output_base64": encoded,
            "output_path": output_path,
            "width": width,
            "height": height,
            "algorithm": "oklab-statistical-transfer",
            "applied_params": applied_params,
            "content": [{"type": "image", "data": encoded, "mimeType": "image/png"}],
        },
    }


if __name__ == "__main__":
    try:
        request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
        print(json.dumps(main(request), ensure_ascii=False, separators=(",", ":")))
    except Exception as exc:
        print(json.dumps({"status": "error", "error": {"code": "color_transfer_failed", "message": str(exc)}}))
