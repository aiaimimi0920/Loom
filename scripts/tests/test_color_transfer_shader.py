"""Exercise the real sample entry point and explicit native Shader negotiation."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = ROOT / "art-packages/samples/color-transfer/runtime"
sys.path.insert(0, str(RUNTIME))
import main as sample


class ShaderContract(unittest.TestCase):
    def run_sample(self, abi=None, input_abi=None):
        with tempfile.TemporaryDirectory(dir=os.environ["TEMP"]) as temp:
            path = Path(temp) / "input.png"
            sample.encode_png(path, 3, 1, bytes([255, 128, 64, 128, 16, 255, 77, 0, 1, 2, 3, 255]))
            params = {"output_mode": "shader", "strength": 0, "skin_protection": True}
            if abi is not None:
                params["shader_abi"] = abi
            request = {"inputs": {"input": str(path), "reference": str(path)},
                       "params": params, "context": {"tempDir": temp}}
            if input_abi is not None:
                request["inputs"]["shader_abi"] = input_abi
            result = subprocess.run(
                [sys.executable, "-B", str(RUNTIME / "main.py")],
                input=json.dumps(request), text=True, capture_output=True,
                cwd=temp, timeout=15, check=True,
            )
            self.assertEqual(result.stderr, "")
            return json.loads(result.stdout)

    def test_native_entry_returns_versioned_source_without_glsl(self):
        response = self.run_sample("otter.hlsl.v1")
        self.assertEqual(response["status"], "success")
        output = response["output"]
        self.assertEqual(output["abi"], "otter.hlsl.v1")
        self.assertEqual(output["type"], "shader")
        self.assertIn("float4 otter_main(float2 uv)", output["source"])
        self.assertNotIn("vertex_shader", output)
        self.assertNotIn("fragment_shader", output)
        self.assertEqual(output["uniforms"]["skin_protection"], 1.0)
        self.assertTrue(output["textures"]["lut"].startswith("data:image/png;base64,"))
        destination = os.environ.get("OTTER_HLSL_SAMPLE_OUTPUT")
        if destination:
            with open(destination, "x", encoding="utf-8") as handle:
                json.dump(output, handle, separators=(",", ":"))

    def test_unknown_abi_never_falls_back_to_legacy(self):
        response = self.run_sample("otter.hlsl.v2")
        self.assertEqual(response["status"], "error")
        self.assertEqual(response["error"]["message"], "unsupported shader ABI")

    def test_flat_hook_input_keeps_abi(self):
        response = self.run_sample(input_abi="otter.hlsl.v1")
        self.assertEqual(response["status"], "success")
        self.assertEqual(response["output"]["abi"], "otter.hlsl.v1")

        response = self.run_sample("otter.hlsl.v1", input_abi="otter.hlsl.v2")
        self.assertEqual(response["status"], "success")
        self.assertEqual(response["output"]["abi"], "otter.hlsl.v1")

        response = self.run_sample(input_abi="otter.hlsl.v2")
        self.assertEqual(response["status"], "error")
        self.assertEqual(response["error"]["message"], "unsupported shader ABI")

    def test_existing_browser_contract_remains_explicitly_legacy(self):
        response = self.run_sample(None)
        self.assertEqual(response["status"], "success")
        output = response["output"]
        self.assertIn("#version 300 es", output["fragment_shader"])
        self.assertIn("vertex_shader", output)
        self.assertNotIn("abi", output)
        self.assertNotIn("source", output)


if __name__ == "__main__":
    unittest.main()
