# Materialize the six framework-backed Art fixtures in the caller scope.

$processImageRuntime = @'
$ErrorActionPreference = "Stop"
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$inputValue = $request.inputs.input
if ($null -eq $inputValue) { $inputValue = $request.inputs.input_base64 }
if ($inputValue -isnot [string]) {
    $inputValue = [string]$inputValue.data
}
$response = [ordered]@{
    status = "success"
    output = [ordered]@{
        output_base64 = $inputValue
        content = @(
            [ordered]@{
                type = "image"
                data = $inputValue
                mimeType = "image/png"
            }
        )
    }
}
[Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
'@
$processRuntimeManifest = @{
    protocolVersion = "loom.art.runtime.v1"
    entry = @{
        command = "powershell.exe"
        args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "runtime/main.ps1")
    }
}
$cliManifest = @{
    id = "store-cli-art"
    name = "Store CLI Art"
    description = "Fake store command-backed process Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "neuro.official/process"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-cli-art" }
        dependencies = @{ framework = "neuro.official/process" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-cli-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $cliManifest)
    "art.runtime.json" = (ConvertTo-NormalizedJson $processRuntimeManifest)
    "runtime/main.ps1" = $processImageRuntime
}

$scriptManifest = @{
    id = "store-script-art"
    name = "Store Script Art"
    description = "Fake store script-backed process Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "neuro.official/process"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-script-art" }
        dependencies = @{ framework = "neuro.official/process" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-script-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $scriptManifest)
    "art.runtime.json" = (ConvertTo-NormalizedJson $processRuntimeManifest)
    "runtime/main.ps1" = $processImageRuntime
}

$cloudManifest = @{
    id = "store-cloud-art"
    name = "Store Cloud Art"
    description = "Fake store cloud_api Art"
    enabled = $true
    execution = @{
        type = "cloud_api"
        endpoint = "http://127.0.0.1:$cloudPort/image"
        method = "POST"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-cloud-art" }
        dependencies = @{ framework = "neuro.official/cloud_api" }
        # The endpoint is a loopback fixture, which a cloud Art only reaches when it declares it.
        permissionPolicy = @{ network = @{ allowLocalhost = $true } }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-cloud-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $cloudManifest)
}

$pythonMain = @'
#!/usr/bin/env python3
import json
import sys

request = json.loads(sys.stdin.buffer.read().decode("utf-8-sig"))
arguments = {}
arguments.update(request.get("inputs") or {})
arguments.update(request.get("params") or {})
text = str(arguments.get("text", ""))
print(json.dumps({
    "status": "success",
    "output": {
        "content": [{"type": "text", "text": f"python art saw {text}"}],
        "pythonExecutable": sys.executable,
    },
}, separators=(",", ":")))
'@
$pythonRuntimeManifest = @{
    protocolVersion = "loom.art.runtime.v1"
    entry = @{
        command = "python.exe"
        args = @("runtime/main.py")
    }
}
$pythonManifest = @{
    id = "store-python-art"
    name = "Store Python Art"
    description = "Fake store Python-backed process Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "neuro.official/process"
    }
    params = @(
        @{
            id = "text"
            label = "Text"
            widget = "text"
            default = ""
        }
    )
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-python-art" }
        dependencies = @{ framework = "neuro.official/process" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-python-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $pythonManifest)
    "art.runtime.json" = (ConvertTo-NormalizedJson $pythonRuntimeManifest)
    "runtime/main.py" = $pythonMain
}

$mcpManifest = @{
    id = "store-mcp-art"
    name = $imageSearchLabel
    description = "Fake store MCP image-search Art"
    enabled = $true
    execution = @{
        type = "framework_art"
        framework = "mcp"
    }
    outputs = @(
        @{
            name = "output"
            label = "output"
            type = "image"
            execution_type = "image_buffer"
        }
    )
    params = @(
        @{ id = "query"; default = "smoke mcp image search" },
        @{ id = "count"; default = 2 },
        @{ id = "result_index"; default = 0 }
    )
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-mcp-art" }
        dependencies = @{
            framework = "mcp"
            frameworkVersion = "^0.2"
            mcpServers = @(
                @{ id = "neuro.official/store-fixture"; version = "^1.0" }
            )
        }
        mcp = @{
            serverId = "store-fixture"
            packageId = "neuro.official/store-fixture"
            version = "^1.0"
            toolName = "brave_image_search"
        }
        permissionPolicy = @{ network = @{ allowLocalhost = $true } }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-mcp-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $mcpManifest)
} -FileCopies @{
    "art.runtime.json" = (Join-Path $repoRoot "art-packages\samples\image-search\art.runtime.json")
    "runtime/main.ps1" = (Join-Path $repoRoot "art-packages\samples\image-search\runtime\main.ps1")
    "runtime/common.ps1" = (Join-Path $repoRoot "art-packages\shared\image-runtime-common.ps1")
}

$workflowYaml = @"
name: Store Script Workflow
nodes:
  - id: image
    uses: neuro.official/store-script-art
"@
$workflowManifest = @{
    id = "store-workflow-art"
    name = "Store Workflow Art"
    description = "Fake store workflow Art"
    enabled = $true
    execution = @{
        type = "workflow"
        workflowId = "store-script-workflow"
    }
    inputs = @(@{ name = "input"; label = "Input"; type = "image"; execution_type = "image_buffer" })
    outputs = @(@{ name = "output"; label = "Output"; type = "image"; execution_type = "image_buffer" })
    metadata = @{
        packageSecurity = @{ version = "1.0.0"; publisher = @{ id = "neuro.official"; name = "Neuro"; icon = "N" } }
        art = @{ qualifiedId = "neuro.official/store-workflow-art" }
        dependencies = @{ framework = "neuro.official/workflow" }
    }
}
New-ZipFixture -ZipPath (Join-Path $storeRoot "arts\store-workflow-art\1.0.0.zip") -TextFiles @{
    "manifest.json" = (ConvertTo-NormalizedJson $workflowManifest)
    "workflow.yaml" = $workflowYaml
}
