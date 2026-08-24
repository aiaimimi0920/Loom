# Materialize the local cloud and MCP service fixtures in the caller scope.
$cloudScript = @"
import base64
import http.server
import json
import sys

PORT = int(sys.argv[1])
EVIDENCE_PATH = sys.argv[2]
IMAGE_DATA = sys.argv[3]
ALT_IMAGE_DATA = sys.argv[4]
IMAGE_BYTES = base64.b64decode(IMAGE_DATA.split(",", 1)[1])
ALT_IMAGE_BYTES = base64.b64decode(ALT_IMAGE_DATA.split(",", 1)[1])


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/raw-image.png":
            body = IMAGE_BYTES
        elif self.path == "/raw-image-alt.png":
            body = ALT_IMAGE_BYTES
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "image/png")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)
        payload = {
            "path": self.path,
            "contentType": self.headers.get("Content-Type", ""),
            "bodyLength": length,
            "bodyPreview": body[:256].decode("utf-8", "replace"),
        }
        with open(EVIDENCE_PATH, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)
        response = {
            "content": [
                {
                    "type": "image",
                    "data": IMAGE_DATA,
                    "mimeType": "image/png",
                }
            ]
        }
        encoded = json.dumps(response, ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, format, *args):
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
"@
Write-Utf8NoBomFile -Path $cloudScriptPath -Content $cloudScript

$mcpScript = @"
import json
import sys

EVIDENCE_PATH = sys.argv[1]
IMAGE_URL = sys.argv[2]
ALT_IMAGE_URL = sys.argv[3]


def write_message(message):
    sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
    sys.stdout.flush()


for raw_line in sys.stdin:
    raw_line = raw_line.strip()
    if not raw_line:
        continue
    request = json.loads(raw_line)
    method = request.get("method", "")
    if method == "initialize":
        write_message({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "store-fixture", "version": "1.0.0"},
            },
        })
    elif method == "notifications/initialized":
        continue
    elif method == "tools/list":
        write_message({
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "result": {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo fixture text",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "text": {"type": "string"}
                            }
                        },
                    },
                    {
                        "name": "brave_image_search",
                        "description": "Return structured image-search results",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": {"type": "string"},
                                "count": {"type": "integer"},
                                "safesearch": {"type": "string"},
                                "spellcheck": {"type": "boolean"},
                            },
                            "required": ["query"],
                        },
                    }
                ]
            },
        })
    elif method == "tools/call":
        arguments = request.get("params", {}).get("arguments", {})
        payload = {
            "toolName": request.get("params", {}).get("name"),
            "arguments": arguments,
        }
        with open(EVIDENCE_PATH, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, ensure_ascii=False, indent=2)
        tool_name = request.get("params", {}).get("name", "")
        if tool_name == "brave_image_search":
            query = str(arguments.get("query", ""))
            count = max(1, int(arguments.get("count", 1)))
            items = [
                {
                    "title": "Fixture image",
                    "url": "https://example.invalid/page",
                    "properties": {
                        "url": IMAGE_URL,
                        "width": 1,
                        "height": 1,
                    },
                }
            ]
            if count >= 2:
                items.append({
                    "title": "Fixture image alt",
                    "url": "https://example.invalid/page-alt",
                    "properties": {
                        "url": ALT_IMAGE_URL,
                        "width": 1,
                        "height": 1,
                    },
                })
            write_message({
                "jsonrpc": "2.0",
                "id": request.get("id"),
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": f"fixture brave_image_search results for {query}",
                        }
                    ],
                    "structuredContent": {
                        "type": "object",
                        "items": items,
                    },
                },
            })
        else:
            text = str(arguments.get("text", ""))
            write_message({
                "jsonrpc": "2.0",
                "id": request.get("id"),
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": text,
                        }
                    ]
                },
            })
"@
Write-Utf8NoBomFile -Path $mcpScriptPath -Content $mcpScript

$fixturePythonLiteral = $fixturePythonCommand.Replace("'", "''")
$fixturePythonPrefix = if ($fixturePythonArgsPrefix.Count -gt 0) {
    "@(" + (($fixturePythonArgsPrefix | ForEach-Object { "'" + ([string]$_).Replace("'", "''") + "'" }) -join ", ") + ")"
} else {
    "@()"
}
$mcpLauncher = @"
param(
    [Parameter(ValueFromRemainingArguments = `$true)]
    [string[]]`$McpArguments
)
`$pythonPrefix = $fixturePythonPrefix
& '$fixturePythonLiteral' @pythonPrefix (Join-Path `$PSScriptRoot 'fake-mcp-server.py') @McpArguments
exit `$LASTEXITCODE
"@
$mcpServerManifest = @{
    schemaVersion = 1
    id = "store-fixture"
    name = "Store Fixture MCP"
    description = "Independent MCP package used by the framework/store smoke"
    version = "1.0.0"
    publisher = @{ id = "neuro.official"; name = "Neuro" }
    transport = "stdio"
    entry = @{
        command = "runtime/server.ps1"
        args = @($mcpEvidencePath, "http://127.0.0.1:$cloudPort/raw-image.png", "http://127.0.0.1:$cloudPort/raw-image-alt.png")
    }
    tools = @("echo", "brave_image_search")
    credentials = @()
}
New-ZipFixture -ZipPath $mcpPackagePath -TextFiles @{
    "mcp.server.json" = (ConvertTo-NormalizedJson $mcpServerManifest)
    "runtime/server.ps1" = $mcpLauncher
    "runtime/fake-mcp-server.py" = $mcpScript
}
