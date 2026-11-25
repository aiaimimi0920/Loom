"""Versioned Shader source output; the host chooses ABI, not a runtime fallback."""
import os


def source(filename):
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), filename)
    with open(path, "r", encoding="utf-8") as handle:
        return handle.read()


def make_shader_output(texture, params, abi=None):
    result = {
        "type": "shader",
        "uniforms": {
            name: (1.0 if value else 0.0) if isinstance(value, bool) else value
            for name, value in params.items()
        },
        "textures": {"lut": texture},
        "algorithm": "oklab-statistical-transfer-shader",
    }
    if abi == "otter.hlsl.v1":
        result.update(abi=abi, source=source("color_transfer.hlsl"))
    elif abi is None:
        result.update(
            vertex_shader=source("color_transfer.vert"),
            fragment_shader=source("color_transfer.frag"),
        )
    else:
        raise ValueError("unsupported shader ABI")
    return result
