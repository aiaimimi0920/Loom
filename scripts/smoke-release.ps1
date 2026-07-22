[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$SmokePortMinimum = 30000
$SmokePortMaximum = 45000

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
$PackageDir = [System.IO.Path]::GetFullPath($PackageDir)
if (-not (Test-Path -LiteralPath $PackageDir -PathType Container)) {
    throw "Loom package directory is missing: $PackageDir"
}
$VersionId = Split-Path -Leaf $PackageDir
$resolvedApps = @("Loom")
$ExpectedLoomCapabilityIds = "brain.plan,tea.ticket.decompose.v1,tea.ticket.execute.v1,tea.ticket.review.v1"
$script:SmokeEvidenceRunId = ""
$script:SmokeEvidenceRunDir = ""
if ([string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    $EvidenceRoot = Join-Path $repoRoot "target\runtime-smoke"
}
$EvidenceRoot = [System.IO.Path]::GetFullPath($EvidenceRoot)
function Assert-Equal {
    param(
        [object]$Expected,
        [object]$Actual,
        [string]$Message
    )

    if ($Expected -ne $Actual) {
        throw "$Message Expected=[$Expected] Actual=[$Actual]"
    }
}

function Assert-Contains {
    param(
        [string]$Needle,
        [string]$Haystack,
        [string]$Message
    )

    if (-not $Haystack.Contains($Needle)) {
        throw "$Message Missing=[$Needle]"
    }
}

function Assert-PathExists {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing release smoke path: $Path"
    }
}

function Get-JsonPropertyOrNull {
    param(
        [object]$Object,
        [string]$Name
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    return $property.Value
}

function Get-ReleaseExePath {
    param(
        [string]$App,
        [string]$ExeName
    )

    return [System.IO.Path]::GetFullPath((Join-Path $PackageDir $ExeName))
}

function Get-FreePort {
    for ($attempt = 0; $attempt -lt 64; $attempt++) {
        $port = Get-Random -Minimum $SmokePortMinimum -Maximum ($SmokePortMaximum + 1)
        $listener = [System.Net.Sockets.TcpListener]::new(
            [System.Net.IPAddress]::Parse("127.0.0.1"),
            $port
        )
        $listener.ExclusiveAddressUse = $true
        try {
            $listener.Start()
            return [int]$port
        }
        catch { }
        finally {
            $listener.Stop()
        }
    }
    throw "Unable to allocate an isolated Loom release smoke port between $SmokePortMinimum and $SmokePortMaximum."
}

function New-SmokeTempRoot {
    param([string]$Prefix)

    $suffix = [System.Guid]::NewGuid().ToString("N")
    $path = Join-Path $env:TEMP "$Prefix-$PID-$suffix"
    New-Item -ItemType Directory -Force -Path $path | Out-Null
    return $path
}

function Wait-ForFileJson {
    param(
        [string]$Path,
        [int]$TimeoutSeconds = 20
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastJsonError = $null
    do {
        try {
            if (Test-Path -LiteralPath $Path) {
                $raw = Get-Content -LiteralPath $Path -Raw
                if (-not [string]::IsNullOrWhiteSpace($raw)) {
                    return $raw | ConvertFrom-Json
                }
            }
        } catch {
            $lastJsonError = $_
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    if ($null -ne $lastJsonError) {
        throw "Timed out waiting for complete JSON file: $Path LastError=$($lastJsonError.Exception.Message)"
    }
    throw "Timed out waiting for JSON file: $Path"
}

function Invoke-JsonGet {
    param(
        [string]$Uri,
        [hashtable]$Headers = @{}
    )

    return Invoke-RestMethod -Uri $Uri -Method Get -Headers $Headers -TimeoutSec 10
}

function Invoke-JsonPost {
    param(
        [string]$Uri,
        [object]$Body,
        [hashtable]$Headers = @{}
    )

    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $Uri -Method Post -Headers $Headers -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Invoke-JsonPut {
    param(
        [string]$Uri,
        [object]$Body,
        [hashtable]$Headers = @{}
    )

    $json = $Body | ConvertTo-Json -Depth 20
    return Invoke-RestMethod -Uri $Uri -Method Put -Headers $Headers -ContentType "application/json" -Body $json -TimeoutSec 20
}

function Invoke-JsonDelete {
    param(
        [string]$Uri,
        [hashtable]$Headers = @{}
    )

    return Invoke-RestMethod -Uri $Uri -Method Delete -Headers $Headers -TimeoutSec 20
}

function New-LoomHookBridgeWebSocket {
    param(
        [int]$Port
    )

    $client = [System.Net.WebSockets.ClientWebSocket]::new()
    $uri = [Uri]::new("ws://127.0.0.1:$Port")
    $connectCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        [void]$client.ConnectAsync($uri, $connectCts.Token).GetAwaiter().GetResult()
    } finally {
        $connectCts.Dispose()
    }

    return $client
}

function Send-LoomHookBridgeWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client,
        [string]$Json
    )

    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Json)
    $sendCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
    try {
        [void]$Client.SendAsync(
            [ArraySegment[byte]]::new($bytes),
            [System.Net.WebSockets.WebSocketMessageType]::Text,
            $true,
            $sendCts.Token
        ).GetAwaiter().GetResult()
    } finally {
        $sendCts.Dispose()
    }
}

function Receive-LoomHookBridgeWebSocketJson {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client
    )

    $buffer = New-Object byte[] 4096
    $builder = [System.Text.StringBuilder]::new()
    do {
        $receiveCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
        try {
            $result = $Client.ReceiveAsync(
                [ArraySegment[byte]]::new($buffer),
                $receiveCts.Token
            ).GetAwaiter().GetResult()
        } finally {
            $receiveCts.Dispose()
        }

        if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
            throw "Loom Hook Bridge WebSocket closed before sending JSON response."
        }

        [void]$builder.Append([System.Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count))
    } while (-not $result.EndOfMessage)

    return $builder.ToString() | ConvertFrom-Json
}

function Close-LoomHookBridgeWebSocket {
    param(
        [System.Net.WebSockets.ClientWebSocket]$Client
    )

    if ($null -eq $Client) {
        return
    }

    try {
        if ($Client.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
            $closeCts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(10))
            try {
                [void]$Client.CloseAsync(
                    [System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure,
                    "done",
                    $closeCts.Token
                ).GetAwaiter().GetResult()
            } finally {
                $closeCts.Dispose()
            }
        }
    } finally {
        $Client.Dispose()
    }
}

function Connect-LoomHookBridgeWebSocket {
    param(
        [int]$Port
    )

    $client = New-LoomHookBridgeWebSocket -Port $Port
    try {
        $request = '{"method":"handshake","params":{"client_version":"release-smoke"}}'
        Send-LoomHookBridgeWebSocketJson -Client $client -Json $request
        return Receive-LoomHookBridgeWebSocketJson -Client $client
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeSettingsCompatibility {
    param(
        [int]$Port,
        [string]$TranslateFixtureDir
    )

    $client = $null
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"get_settings"}'
        $settings = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "settings" ([string]$settings.type) "Loom Hook Bridge get_settings response type mismatch."
        Assert-Equal "system" ([string]$settings.data.general.theme) "Loom Hook Bridge get_settings theme mismatch."
        Assert-Equal "python.exe" ([string]$settings.data.engine.python_interpreter) "Loom Hook Bridge get_settings Python interpreter mismatch."

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"get_shortcuts"}'
        $shortcuts = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "shortcuts" ([string]$shortcuts.type) "Loom Hook Bridge get_shortcuts response type mismatch."
        $shortcutIds = @($shortcuts.data) | ForEach-Object { [string]$_.id }
        Assert-Contains "capture" ($shortcutIds -join ",") "Loom Hook Bridge get_shortcuts must include capture."
        Assert-Equal 7 (@($shortcuts.data).Count) "Loom Hook Bridge get_shortcuts Hook shortcut count mismatch."
        Assert-Contains "toggle_translation" ($shortcutIds -join ",") "Loom Hook Bridge get_shortcuts must include toggle_translation."

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"art_loom/translate_text","params":{"text":"release loom translate","target_lang":"zh"}}'
        $translated = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "success" ([string]$translated.type) "Loom Hook Bridge translate_text response type mismatch."
        Assert-Equal "translated:release loom translate:zh" ([string]$translated.data.translated_text) "Loom Hook Bridge translate_text provider translation mismatch."
        Assert-Equal "loom-translate-provider" ([string]$translated.data.source) "Loom Hook Bridge translate_text source mismatch."
        if (-not [string]::IsNullOrWhiteSpace($TranslateFixtureDir)) {
            $translateRequestPath = Join-Path $TranslateFixtureDir "translate-request.json"
            Wait-ForPath -Path $translateRequestPath -TimeoutSeconds 20
            $translateRequest = Get-Content -Raw -LiteralPath $translateRequestPath | ConvertFrom-Json
            Assert-Equal "release loom translate" ([string]$translateRequest.text) "Loom translate fixture request text mismatch."
            Assert-Equal "zh" ([string]$translateRequest.target_lang) "Loom translate fixture target_lang mismatch."
            Assert-Equal "auto" ([string]$translateRequest.source_lang) "Loom translate fixture source_lang mismatch."
        }

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"update_art_param","params":{"art_id":"fixture-artloom-compat","param_id":"strength","value":0.5}}'
        $updated = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "success" ([string]$updated.type) "Loom Hook Bridge update_art_param response type mismatch."
        Assert-Equal "update_art_param" ([string]$updated.data.compatCommand) "Loom Hook Bridge update_art_param command mismatch."
        Assert-Equal "fixture-artloom-compat" ([string]$updated.data.art_id) "Loom Hook Bridge update_art_param art id mismatch."

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"list_arts"}'
        $artsAfterParamUpdate = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "arts" ([string]$artsAfterParamUpdate.type) "Loom Hook Bridge list_arts after update_art_param response type mismatch."
        $updatedArt = @($artsAfterParamUpdate.data) | Where-Object { [string]$_.art_id -eq "fixture-artloom-compat" } | Select-Object -First 1
        if ($null -eq $updatedArt) {
            throw "Loom Hook Bridge list_arts after update_art_param did not include fixture-artloom-compat."
        }
        Assert-Equal 0.5 ([double]$updatedArt.defaults.strength) "Loom Hook Bridge update_art_param did not persist default."

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"sync_shortcuts"}'
        $synced = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "shortcuts" ([string]$synced.type) "Loom Hook Bridge sync_shortcuts response type mismatch."
        Assert-Equal 7 (@($synced.data).Count) "Loom Hook Bridge sync_shortcuts Hook shortcut count mismatch."

        return [ordered]@{
            settingsTheme = [string]$settings.data.general.theme
            shortcutCount = @($shortcuts.data).Count
            translatedText = [string]$translated.data.translated_text
            translationSource = [string]$translated.data.source
            updatedArtId = [string]$updated.data.art_id
            updatedStrength = [double]$updatedArt.defaults.strength
            syncedShortcutCount = @($synced.data).Count
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeWebSocketBroadcast {
    param(
        [int]$Port
    )

    $subscriber = $null
    $publisher = $null
    try {
        $subscriber = New-LoomHookBridgeWebSocket -Port $Port
        Send-LoomHookBridgeWebSocketJson `
            -Client $subscriber `
            -Json '{"method":"subscribe","params":{"channels":["art_hook"]}}'
        $subscribe = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "success" ([string]$subscribe.type) "Loom Hook Bridge WebSocket subscribe response type mismatch."
        Assert-Equal $true ([bool]$subscribe.data.subscribed) "Loom Hook Bridge WebSocket subscribe flag mismatch."

        $publisher = New-LoomHookBridgeWebSocket -Port $Port
        Send-LoomHookBridgeWebSocketJson `
            -Client $publisher `
            -Json '{"method":"art_loom/instantiate_workflow","params":{"nodes":[{"id":"release-node"}],"edges":[{"source":"release-node","target":"release-output"}],"mode":"reference","workflow_id":"wf-release-broadcast"}}'
        $publishResponse = Receive-LoomHookBridgeWebSocketJson -Client $publisher
        Assert-Equal "success" ([string]$publishResponse.type) "Loom Hook Bridge WebSocket instantiate response type mismatch."

        $broadcast = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "art_hook/instantiate" ([string]$broadcast.method) "Loom Hook Bridge WebSocket broadcast method mismatch."
        Assert-Equal "wf-release-broadcast" ([string]$broadcast.params.workflow_id) "Loom Hook Bridge WebSocket broadcast workflow id mismatch."
        Assert-Equal "release-node" ([string]$broadcast.params.nodes[0].id) "Loom Hook Bridge WebSocket broadcast node id mismatch."
        Assert-Equal "release-output" ([string]$broadcast.params.edges[0].target) "Loom Hook Bridge WebSocket broadcast edge target mismatch."

        return [ordered]@{
            method = [string]$broadcast.method
            workflowId = [string]$broadcast.params.workflow_id
            nodeId = [string]$broadcast.params.nodes[0].id
            edgeTarget = [string]$broadcast.params.edges[0].target
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $publisher
        Close-LoomHookBridgeWebSocket -Client $subscriber
    }
}

function Test-LoomArtLoomIpcWorkflowCompat {
    param(
        [string]$BaseUrl,
        [int]$Port
    )

    $subscriber = $null
    try {
        $subscriber = New-LoomHookBridgeWebSocket -Port $Port
        Send-LoomHookBridgeWebSocketJson `
            -Client $subscriber `
            -Json '{"method":"subscribe","params":{"channels":["art_hook"]}}'
        $subscribe = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "success" ([string]$subscribe.type) "Loom instantiate_workflow HTTP alias subscribe response type mismatch."

        $instantiated = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/ipc/instantiate-workflow" -Body @{
            nodes = @(
                @{
                    id = "release-http-node"
                }
            )
            edges = @(
                @{
                    source = "release-http-node"
                    target = "release-http-output"
                }
            )
            mode = "reference"
            workflowId = "wf-release-http-alias"
        }
        Assert-Equal "instantiate_workflow" ([string]$instantiated.compatCommand) "Loom instantiate_workflow compat command mismatch."
        Assert-Equal "success" ([string]$instantiated.type) "Loom instantiate_workflow response type mismatch."
        Assert-Equal "art_hook/instantiate" ([string]$instantiated.method) "Loom instantiate_workflow broadcast method mismatch."

        $broadcast = Receive-LoomHookBridgeWebSocketJson -Client $subscriber
        Assert-Equal "art_hook/instantiate" ([string]$broadcast.method) "Loom instantiate_workflow HTTP alias broadcast method mismatch."
        Assert-Equal "wf-release-http-alias" ([string]$broadcast.params.workflow_id) "Loom instantiate_workflow HTTP alias workflow id mismatch."
        Assert-Equal "release-http-node" ([string]$broadcast.params.nodes[0].id) "Loom instantiate_workflow HTTP alias node id mismatch."

        $executed = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/ipc/execute-art-node" -Body @{
            nodeId = "release-http-execute-node"
            artId = "fixture-echo"
            inputBase64 = "data:text/plain;base64,cmVsZWFzZQ=="
            params = @{
                text = "release execute art node http alias"
            }
        }
        Assert-Equal "execute_art_node" ([string]$executed.compatCommand) "Loom execute_art_node compat command mismatch."
        Assert-Equal "success" ([string]$executed.type) "Loom execute_art_node HTTP alias response type mismatch."
        Assert-Equal $true ([bool]$executed.data.success) "Loom execute_art_node HTTP alias success mismatch."
        Assert-Equal "release-http-execute-node" ([string]$executed.data.node_id) "Loom execute_art_node HTTP alias node id mismatch."
        Assert-Equal "release execute art node http alias" ([string]$executed.data.output_text) "Loom execute_art_node HTTP alias output mismatch."

        return [ordered]@{
            instantiateCommand = [string]$instantiated.compatCommand
            instantiateType = [string]$instantiated.type
            broadcastMethod = [string]$broadcast.method
            workflowId = [string]$broadcast.params.workflow_id
            executeCommand = [string]$executed.compatCommand
            executeType = [string]$executed.type
            executeNodeId = [string]$executed.data.node_id
            executeOutputText = [string]$executed.data.output_text
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $subscriber
    }
}

function Test-LoomHookLiveWorkflowPersistence {
    param(
        [int]$Port,
        [string]$BaseUrl
    )

    $client = $null
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"art_loom/instantiate_workflow","params":{"nodes":[{"id":"hook-live-release-node","type":"artNode","data":{"artId":"hook.capture","label":"Hook Screenshot"}},{"id":"hook-live-release-output","type":"artNode","data":{"artId":"hook.output","label":"Hook Output"}}],"edges":[{"source":"hook-live-release-node","target":"hook-live-release-output","sourceHandle":"screenshot","targetHandle":"image"}],"mode":"reference","workflow_id":"wf-release-hook-live"}}'
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "success" ([string]$response.type) "Loom Hook Bridge live workflow instantiate response type mismatch."

        $workflows = Invoke-JsonGet -Uri "$BaseUrl/v1/workflows"
        $hookLive = @($workflows.workflows | Where-Object { [string]$_.id -eq "hook-live" })
        Assert-Equal 1 $hookLive.Count "Loom workflow list must include canonical Hook live workflow."
        Assert-Equal "Hook 实时工作流" ([string]$hookLive[0].name) "Loom Hook live workflow list label mismatch."

        $loaded = Invoke-JsonGet -Uri "$BaseUrl/v1/workflows/hook-live"
        Assert-Equal "hook-live" ([string]$loaded.workflow.id) "Loom Hook live workflow load id mismatch."
        Assert-Contains "hook-live-release-node" ([string]$loaded.workflow.data) "Loom Hook live workflow load data missing node."
        Assert-Contains "hook-live-release-output" ([string]$loaded.workflow.data) "Loom Hook live workflow load data missing target node."
        Assert-Contains "nodes.hook-live-release-node.outputs.screenshot" ([string]$loaded.workflow.data) "Loom Hook live workflow load data missing edge binding."

        return [ordered]@{
            workflowId = [string]$loaded.workflow.id
            listName = [string]$hookLive[0].name
            nodePersisted = ([string]$loaded.workflow.data).Contains("hook-live-release-node")
            targetNodePersisted = ([string]$loaded.workflow.data).Contains("hook-live-release-output")
            edgePersisted = ([string]$loaded.workflow.data).Contains("nodes.hook-live-release-node.outputs.screenshot")
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeExecuteArtNode {
    param(
        [int]$Port
    )

    $client = $null
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json '{"method":"art_loom/execute_art_node","params":{"node_id":"release-node-mcp","art_id":"fixture-echo","input_base64":"data:text/plain;base64,cmVsZWFzZQ==","params":{"text":"release execute art node"}}}'
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "success" ([string]$response.type) "Loom Hook Bridge execute_art_node response type mismatch."
        Assert-Equal $true ([bool]$response.data.success) "Loom Hook Bridge execute_art_node success flag mismatch."
        Assert-Equal "release-node-mcp" ([string]$response.data.node_id) "Loom Hook Bridge execute_art_node node id mismatch."
        Assert-Equal "release execute art node" ([string]$response.data.output_text) "Loom Hook Bridge execute_art_node output text mismatch."

        return [ordered]@{
            type = [string]$response.type
            success = [bool]$response.data.success
            nodeId = [string]$response.data.node_id
            outputText = [string]$response.data.output_text
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeAhrpProcess {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art/process"
            params = [ordered]@{
                request_id = "release-ahrp-process"
                art_id = "fixture-echo"
                input = [ordered]@{
                    type = "base64"
                    data = $imageData
                    width = 1
                    height = 1
                    format = "rgba8"
                }
                params = [ordered]@{
                    text = $imageData
                    ignored = "remove me"
                }
                disabled_params = @("ignored")
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "release-ahrp-process" ([string]$response.request_id) "Loom Hook Bridge AHRP process request id mismatch."
        Assert-Equal "Success" ([string]$response.status) "Loom Hook Bridge AHRP process status mismatch."
        Assert-Equal "result" ([string]$response.data.type) "Loom Hook Bridge AHRP process data type mismatch."
        Assert-Equal "base64" ([string]$response.data.output.type) "Loom Hook Bridge AHRP process output type mismatch."
        Assert-Equal $imageData ([string]$response.data.output.data) "Loom Hook Bridge AHRP process output data mismatch."
        Assert-Equal 1 ([int]$response.data.output.width) "Loom Hook Bridge AHRP process output width mismatch."
        Assert-Equal 1 ([int]$response.data.output.height) "Loom Hook Bridge AHRP process output height mismatch."

        return [ordered]@{
            requestId = [string]$response.request_id
            status = [string]$response.status
            outputType = [string]$response.data.output.type
            width = [int]$response.data.output.width
            height = [int]$response.data.output.height
            outputData = [string]$response.data.output.data
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function New-LoomNativeImageSmokePngDataUrl {
    Add-Type -AssemblyName System.Drawing

    $bitmap = $null
    $stream = $null
    try {
        $bitmap = [System.Drawing.Bitmap]::new(1, 1)
        $bitmap.SetPixel(0, 0, [System.Drawing.Color]::FromArgb(255, 10, 20, 30))
        $stream = [System.IO.MemoryStream]::new()
        $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
        return "data:image/png;base64,$([Convert]::ToBase64String($stream.ToArray()))"
    } finally {
        if ($null -ne $stream) {
            $stream.Dispose()
        }
        if ($null -ne $bitmap) {
            $bitmap.Dispose()
        }
    }
}

function Test-LoomArtLoomNativeProcessArtCompat {
    param(
        [string]$BaseUrl
    )

    $imageData = New-LoomNativeImageSmokePngDataUrl
    $response = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/native/process-art" -Body @{
        artId = "core.image.invert"
        inputBase64 = $imageData
        params = @{}
    }

    Assert-Equal "native_process_art" ([string]$response.compatCommand) "Loom native_process_art compat command mismatch."
    Assert-Equal $true ([bool]$response.success) "Loom native_process_art success mismatch."
    $outputData = [string]$response.output_base64
    if (-not $outputData.StartsWith("data:image/png;base64,", [System.StringComparison]::Ordinal)) {
        throw "Loom native_process_art output must be a PNG data URL."
    }
    if ($outputData -eq $imageData) {
        throw "Loom native_process_art output should differ from input after invert."
    }
    if ($null -ne (Get-JsonPropertyOrNull -Object $response -Name "error")) {
        throw "Loom native_process_art should return null error for successful native processing."
    }

    return [ordered]@{
        command = [string]$response.compatCommand
        artId = "core.image.invert"
        success = [bool]$response.success
        inputLength = [int]$imageData.Length
        outputLength = [int]$outputData.Length
        outputChanged = $true
    }
}

function Test-LoomMcpDirectCompat {
    param(
        [string]$BaseUrl,
        [string]$FixtureMcpScript
    )

    $called = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/mcp/call-tool" -Body @{
        command = "powershell.exe"
        args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $FixtureMcpScript)
        env = @{}
        toolName = "echo"
        toolArgs = @{
            text = "release direct mcp runtime"
        }
    }

    Assert-Equal "call_mcp_tool" ([string]$called.compatCommand) "Loom call_mcp_tool compat command mismatch."
    Assert-Equal "succeeded" ([string]$called.status) "Loom call_mcp_tool status mismatch."
    Assert-Equal "2.0" ([string]$called.jsonrpc) "Loom call_mcp_tool JSON-RPC version mismatch."
    Assert-Equal "release direct mcp runtime" ([string]$called.result.content[0].text) "Loom call_mcp_tool content mismatch."

    return [ordered]@{
        command = [string]$called.compatCommand
        status = [string]$called.status
        jsonrpc = [string]$called.jsonrpc
        resultText = [string]$called.result.content[0].text
    }
}

function Test-LoomArtLoomMcpServerStoreCompat {
    param(
        [string]$BaseUrl
    )

    $empty = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/mcp/servers"
    Assert-Equal "get_mcp_servers" ([string]$empty.compatCommand) "Loom get_mcp_servers compat command mismatch."
    if ($null -eq $empty.servers) {
        throw "Loom get_mcp_servers must return a servers array."
    }

    $saved = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/mcp/servers" -Body @{
        id = "release-artloom-mcp"
        name = "Release ArtLoom MCP"
        description = "Release smoke old MCP server store compatibility"
        command = "powershell.exe"
        args = @("-NoProfile")
        env = @{
            RELEASE_ARTLOOM_MCP = "1"
        }
        enabled = $true
    }
    Assert-Equal "save_mcp_server" ([string]$saved.compatCommand) "Loom save_mcp_server compat command mismatch."
    Assert-Equal "Saved successfully" ([string]$saved.message) "Loom save_mcp_server message mismatch."
    Assert-Equal "release-artloom-mcp" ([string]$saved.server.id) "Loom save_mcp_server id mismatch."

    $listed = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/mcp/servers"
    Assert-Equal "get_mcp_servers" ([string]$listed.compatCommand) "Loom get_mcp_servers after save command mismatch."
    $serverIds = @($listed.servers | ForEach-Object { [string]$_.id })
    Assert-Contains "release-artloom-mcp" ($serverIds -join ",") "Loom get_mcp_servers did not include saved fixture."

    $registry = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/mcp/registry?search=fixture&limit=250&cursor=cursor-1"
    Assert-Equal "fetch_mcp_registry" ([string]$registry.compatCommand) "Loom fetch_mcp_registry compat command mismatch."
    Assert-Equal "io.modelcontextprotocol/fixture" ([string]$registry.servers[0].server.name) "Loom fetch_mcp_registry server name mismatch."

    $deleted = Invoke-JsonDelete -Uri "$BaseUrl/v1/artloom-compat/mcp/servers/release-artloom-mcp"
    Assert-Equal "delete_mcp_server" ([string]$deleted.compatCommand) "Loom delete_mcp_server compat command mismatch."
    Assert-Equal "Deleted successfully" ([string]$deleted.message) "Loom delete_mcp_server message mismatch."
    Assert-Equal $true ([bool]$deleted.deleted) "Loom delete_mcp_server deleted flag mismatch."

    return [ordered]@{
        listCommand = [string]$empty.compatCommand
        saveCommand = [string]$saved.compatCommand
        registryCommand = [string]$registry.compatCommand
        deleteCommand = [string]$deleted.compatCommand
        savedServerId = [string]$saved.server.id
        registryServerName = [string]$registry.servers[0].server.name
        deleted = [bool]$deleted.deleted
    }
}

function Test-LoomHookBridgeNativeImageFilter {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art/process"
            params = [ordered]@{
                request_id = "release-native-image-filter"
                art_id = "core.image.invert"
                input = [ordered]@{
                    type = "base64"
                    data = $imageData
                    width = 1
                    height = 1
                    format = "rgba8"
                }
                params = [ordered]@{}
                disabled_params = @()
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "release-native-image-filter" ([string]$response.request_id) "Loom native image filter request id mismatch."
        Assert-Equal "Success" ([string]$response.status) "Loom native image filter status mismatch."
        Assert-Equal "result" ([string]$response.data.type) "Loom native image filter data type mismatch."
        Assert-Equal "base64" ([string]$response.data.output.type) "Loom native image filter output type mismatch."
        $outputData = [string]$response.data.output.data
        if (-not $outputData.StartsWith("data:image/png;base64,", [System.StringComparison]::Ordinal)) {
            throw "Loom native image filter output must be a PNG data URL."
        }
        if ($outputData -eq $imageData) {
            throw "Loom native image filter output should differ from input after invert."
        }
        Assert-Equal 1 ([int]$response.data.output.width) "Loom native image filter output width mismatch."
        Assert-Equal 1 ([int]$response.data.output.height) "Loom native image filter output height mismatch."

        return [ordered]@{
            requestId = [string]$response.request_id
            status = [string]$response.status
            artId = "core.image.invert"
            outputType = [string]$response.data.output.type
            width = [int]$response.data.output.width
            height = [int]$response.data.output.height
            outputChanged = $true
            inputLength = [int]$imageData.Length
            outputLength = [int]$outputData.Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeSharedImageAhrpProcess {
    param(
        [int]$Port,
        [string]$BaseUrl
    )

    $created = Invoke-JsonPost -Uri "$BaseUrl/v1/shared-images" -Body @{
        width = 1
        height = 1
        format = "rgba8"
        data = @(10, 20, 30, 255)
    }
    Assert-Equal 1 ([int]$created.image.width) "Loom shared image create width mismatch."
    Assert-Equal 1 ([int]$created.image.height) "Loom shared image create height mismatch."
    Assert-Equal 4 ([int]$created.image.size) "Loom shared image create size mismatch."
    Assert-Equal "rgba8" ([string]$created.image.format) "Loom shared image create format mismatch."
    $inputHandle = [string]$created.image.handle
    if ([string]::IsNullOrWhiteSpace($inputHandle)) {
        throw "Loom shared image create did not return a handle."
    }

    $listed = Invoke-JsonGet -Uri "$BaseUrl/v1/shared-images"
    if (@($listed.images).Count -lt 1) {
        throw "Loom shared image list did not include the created image."
    }

    $client = $null
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art/process"
            params = [ordered]@{
                request_id = "release-shared-image-ahrp-process"
                art_id = "core.image.invert"
                input = [ordered]@{
                    type = "shared_memory"
                    handle = $inputHandle
                    size = 4
                    width = 1
                    height = 1
                    format = "rgba8"
                }
                params = [ordered]@{}
                disabled_params = @()
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "release-shared-image-ahrp-process" ([string]$response.request_id) "Loom shared image AHRP request id mismatch."
        Assert-Equal "Success" ([string]$response.status) "Loom shared image AHRP status mismatch."
        Assert-Equal "result" ([string]$response.data.type) "Loom shared image AHRP data type mismatch."
        Assert-Equal "shared_memory" ([string]$response.data.output.type) "Loom shared image AHRP output type mismatch."
        Assert-Equal 1 ([int]$response.data.output.width) "Loom shared image AHRP output width mismatch."
        Assert-Equal 1 ([int]$response.data.output.height) "Loom shared image AHRP output height mismatch."
        Assert-Equal 4 ([int]$response.data.output.size) "Loom shared image AHRP output size mismatch."
        Assert-Equal "rgba8" ([string]$response.data.output.format) "Loom shared image AHRP output format mismatch."
        $outputHandle = [string]$response.data.output.handle
        if ([string]::IsNullOrWhiteSpace($outputHandle)) {
            throw "Loom shared image AHRP output handle missing."
        }
        if ($outputHandle -eq $inputHandle) {
            throw "Loom shared image AHRP output should use a distinct shared image handle."
        }

        $output = Invoke-JsonGet -Uri "$BaseUrl/v1/shared-images/$outputHandle"
        $outputBytes = @($output.data | ForEach-Object { [int]$_ })
        Assert-Equal "245,235,225,255" ($outputBytes -join ",") "Loom shared image AHRP output RGBA mismatch."

        return [ordered]@{
            requestId = [string]$response.request_id
            status = [string]$response.status
            artId = "core.image.invert"
            inputHandle = $inputHandle
            outputHandle = $outputHandle
            outputType = [string]$response.data.output.type
            width = [int]$response.data.output.width
            height = [int]$response.data.output.height
            size = [int]$response.data.output.size
            format = [string]$response.data.output.format
            outputRgba = ($outputBytes -join ",")
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomImageHelperConvert {
    param(
        [string]$BaseUrl
    )

    $imageData = New-LoomNativeImageSmokePngDataUrl
    $buffer = Invoke-JsonPost -Uri "$BaseUrl/v1/image-helpers/convert" -Body @{
        sourceType = "image_base64"
        targetType = "image_buffer"
        data = $imageData
    }
    Assert-Equal 1 ([int]$buffer.image.width) "Loom image helper buffer width mismatch."
    Assert-Equal 1 ([int]$buffer.image.height) "Loom image helper buffer height mismatch."
    Assert-Equal "rgba8" ([string]$buffer.image.format) "Loom image helper buffer format mismatch."
    Assert-Equal 4 ([int]$buffer.image.size) "Loom image helper buffer size mismatch."
    $rgba = @($buffer.data | ForEach-Object { [int]$_ })
    Assert-Equal "10,20,30,255" ($rgba -join ",") "Loom image helper RGBA output mismatch."

    $base64 = Invoke-JsonPost -Uri "$BaseUrl/v1/image-helpers/convert" -Body @{
        sourceType = "image_buffer"
        targetType = "image_base64"
        width = 1
        height = 1
        data = @(10, 20, 30, 255)
    }
    $dataBase64 = [string]$base64.dataBase64
    if (-not $dataBase64.StartsWith("data:image/png;base64,", [System.StringComparison]::Ordinal)) {
        throw "Loom image helper image_buffer to image_base64 did not return a PNG data URL."
    }

    return [ordered]@{
        inputType = "image_base64"
        outputType = "image_buffer"
        width = [int]$buffer.image.width
        height = [int]$buffer.image.height
        format = [string]$buffer.image.format
        size = [int]$buffer.image.size
        outputRgba = ($rgba -join ",")
        roundtripType = "image_base64"
        roundtripLength = [int]$dataBase64.Length
    }
}

function Test-LoomHookBridgeOcrImage {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json (@{
                method = "art_loom/get_capabilities"
            } | ConvertTo-Json -Depth 20 -Compress)
        $capabilities = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "success" ([string]$capabilities.type) "Loom OCR capabilities response type mismatch."
        Assert-Equal $true ([bool]$capabilities.data.ocr) "Loom OCR fixture capability should be available in release smoke."

        $request = [ordered]@{
            method = "art_loom/ocr_image"
            params = [ordered]@{
                image_base64 = $imageData
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom OCR response type mismatch."
        Assert-Equal "release loom ocr" ([string]$response.data.fullText) "Loom OCR fixture text mismatch."
        Assert-Equal 1 ([int]$response.data.width) "Loom OCR image width mismatch."
        Assert-Equal 1 ([int]$response.data.height) "Loom OCR image height mismatch."
        Assert-Equal "release loom ocr" ([string]$response.data.textBlocks[0].text) "Loom OCR first text block mismatch."

        return [ordered]@{
            type = [string]$response.type
            method = "art_loom/ocr_image"
            ocrAvailable = [bool]$capabilities.data.ocr
            fullText = [string]$response.data.fullText
            width = [int]$response.data.width
            height = [int]$response.data.height
            blockCount = @($response.data.textBlocks).Count
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeRealOcrImage {
    param(
        [int]$Port,
        [string]$PackageDir
    )

    $fixturePath = Join-Path $PackageDir "runtime\resources\ocr\fixtures\test_1.png"
    Assert-PathExists $fixturePath
    $imageBytes = [System.IO.File]::ReadAllBytes($fixturePath)
    $imageData = "data:image/png;base64,$([Convert]::ToBase64String($imageBytes))"

    $client = $null
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port

        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json (@{
                method = "art_loom/get_capabilities"
            } | ConvertTo-Json -Depth 20 -Compress)
        $capabilities = Receive-LoomHookBridgeWebSocketJson -Client $client
        Assert-Equal "success" ([string]$capabilities.type) "Loom real OCR capabilities response type mismatch."
        Assert-Equal $true ([bool]$capabilities.data.ocr) "Loom real OCR capability should be available from packaged resources."

        $request = [ordered]@{
            method = "art_loom/ocr_image"
            params = [ordered]@{
                image_base64 = $imageData
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom real OCR response type mismatch."
        $fullText = ([string]$response.data.fullText).Trim()
        if ([string]::IsNullOrWhiteSpace($fullText)) {
            throw "Loom real OCR fullText should not be empty."
        }
        $textBlocks = @($response.data.textBlocks)
        if ($textBlocks.Count -lt 1) {
            throw "Loom real OCR should return at least one text block."
        }
        Assert-Equal 678 ([int]$response.data.width) "Loom real OCR fixture width mismatch."
        Assert-Equal 108 ([int]$response.data.height) "Loom real OCR fixture height mismatch."

        return [ordered]@{
            type = [string]$response.type
            method = "art_loom/ocr_image"
            ocrAvailable = [bool]$capabilities.data.ocr
            fullTextLength = [int]$fullText.Length
            width = [int]$response.data.width
            height = [int]$response.data.height
            blockCount = [int]$textBlocks.Count
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomReleaseRealOcr {
    param(
        [string]$LoomDaemonExe,
        [string]$PackageDir,
        [string]$TempRoot
    )

    $ocrDir = Join-Path $PackageDir "runtime\resources\ocr"
    Assert-PathExists (Join-Path $ocrDir "ch_PP-OCRv4_det_infer.onnx")
    Assert-PathExists (Join-Path $ocrDir "ch_ppocr_mobile_v2.0_cls_infer.onnx")
    Assert-PathExists (Join-Path $ocrDir "ch_PP-OCRv4_rec_infer.onnx")
    Assert-PathExists (Join-Path $ocrDir "onnxruntime.dll")
    Assert-PathExists (Join-Path $ocrDir "onnxruntime_providers_shared.dll")

    $realOcrProcess = $null
    $port = Get-FreePort
    $manifestDir = Join-Path $TempRoot "real-ocr-capabilities"
    New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
    $stdout = Join-Path $TempRoot "loom-daemon-real-ocr.stdout.log"
    $stderr = Join-Path $TempRoot "loom-daemon-real-ocr.stderr.log"
    $controlPlaneRoot = Join-Path $TempRoot "loom-real-ocr-control-plane"

    $oldHost = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_HOST", "Process")
    $oldPort = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_PORT", "Process")
    $oldToken = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_TOKEN", "Process")
    $oldControlPlaneRoot = [Environment]::GetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", "Process")
    $oldOcrFixtureText = [Environment]::GetEnvironmentVariable("LOOM_OCR_FIXTURE_TEXT", "Process")
    $oldOcrModelDir = [Environment]::GetEnvironmentVariable("LOOM_OCR_MODEL_DIR", "Process")
    [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", "127.0.0.1", "Process")
    [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", [string]$port, "Process")
    [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $null, "Process")
    [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $controlPlaneRoot, "Process")
    [Environment]::SetEnvironmentVariable("LOOM_OCR_FIXTURE_TEXT", $null, "Process")
    [Environment]::SetEnvironmentVariable("LOOM_OCR_MODEL_DIR", $null, "Process")
    try {
        $realOcrProcess = Start-SmokeProcess `
            -FilePath $LoomDaemonExe `
            -ArgumentList @("--manifest-dir", $manifestDir) `
            -StdoutPath $stdout `
            -StderrPath $stderr
    } finally {
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", $oldHost, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", $oldPort, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $oldToken, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $oldControlPlaneRoot, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_OCR_FIXTURE_TEXT", $oldOcrFixtureText, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_OCR_MODEL_DIR", $oldOcrModelDir, "Process")
    }

    try {
        $manifest = Wait-ForFileJson -Path (Join-Path $manifestDir "loom.json")
        $baseUrl = [string]$manifest.transport.baseUrl
        Assert-Equal "http://127.0.0.1:$port" $baseUrl "Real OCR Loom manifest baseUrl mismatch."
        Wait-LoomDaemonHealth -BaseUrl $baseUrl -Message "Timed out waiting for real OCR Loom daemon" | Out-Null

        $hookBridgeStarted = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/start" -Body @{
            port = 0
        }
        Assert-Equal $true ([bool]$hookBridgeStarted.running) "Real OCR Hook Bridge start state mismatch."
        if ([int]$hookBridgeStarted.port -le 0) {
            throw "Real OCR Hook Bridge start should allocate a port."
        }

        $result = Test-LoomHookBridgeRealOcrImage -Port ([int]$hookBridgeStarted.port) -PackageDir $PackageDir
        $hookBridgeStopped = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/stop" -Body @{}
        Assert-Equal $false ([bool]$hookBridgeStopped.running) "Real OCR Hook Bridge stop state mismatch."
        return $result
    } finally {
        Stop-SpawnedProcess $realOcrProcess
    }
}

function Test-LoomHookBridgeScriptArtNode {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art_loom/execute_art_node"
            params = [ordered]@{
                node_id = "release-node-script"
                art_id = "fixture-script-art"
                input_base64 = $imageData
                params = [ordered]@{}
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom script Art node response type mismatch."
        Assert-Equal $true ([bool]$response.data.success) "Loom script Art node success flag mismatch."
        Assert-Equal "release-node-script" ([string]$response.data.node_id) "Loom script Art node id mismatch."
        $outputData = [string]$response.data.output_base64
        Assert-Equal $imageData $outputData "Loom script Art node output data mismatch."

        return [ordered]@{
            type = [string]$response.type
            success = [bool]$response.data.success
            nodeId = [string]$response.data.node_id
            outputType = "base64"
            outputLength = [int]$outputData.Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomPackagedPythonScriptTool {
    param(
        [string]$BaseUrl,
        [string]$PackageDir,
        [string]$TempRoot
    )

    $packagedPython = Join-Path $PackageDir "runtime\bin\python-embed\python.exe"
    $packagedLauncher = Join-Path $PackageDir "runtime\python\Launcher.py"
    Assert-PathExists $packagedPython
    Assert-PathExists $packagedLauncher

    $fixturePythonScript = Join-Path $TempRoot "fixture-python-script.py"
    $fixturePythonSource = @'
import json
import sys

payload = json.loads(sys.argv[1])
arguments = payload.get("arguments", {})
response = {
    "content": [
        {
            "type": "text",
            "text": "python saw " + str(arguments.get("text", "")),
        }
    ],
    "pythonExecutable": sys.executable,
}
print(json.dumps(response, ensure_ascii=False))
'@
    [System.IO.File]::WriteAllText($fixturePythonScript, $fixturePythonSource, [System.Text.UTF8Encoding]::new($false))

    $savedPythonTool = Invoke-JsonPut -Uri "$BaseUrl/v1/tools/fixture-python-script" -Body @{
        id = "fixture-python-script"
        name = "Fixture Python Script"
        description = "Release smoke Python script backed by packaged Python"
        enabled = $true
        execution = @{
            type = "script"
            path = $fixturePythonScript
        }
    }
    Assert-Equal "fixture-python-script" $savedPythonTool.tool.id "Loom Python script tool save id mismatch."
    Assert-Equal "script" $savedPythonTool.tool.execution.type "Loom Python script execution type mismatch."

    $executedPythonTool = Invoke-JsonPost -Uri "$BaseUrl/v1/tools/fixture-python-script/execute" -Body @{
        arguments = @{
            text = "release embedded python"
        }
    }
    Assert-Equal "succeeded" $executedPythonTool.status "Loom Python script tool execution status mismatch."
    Assert-Equal "python saw release embedded python" ([string]$executedPythonTool.result.content[0].text) "Loom Python script tool content mismatch."

    $actualPython = [System.IO.Path]::GetFullPath([string]$executedPythonTool.result.pythonExecutable)
    $expectedPython = [System.IO.Path]::GetFullPath($packagedPython)
    Assert-Equal $expectedPython $actualPython "Loom Python script did not use packaged embedded Python."

    return [ordered]@{
        text = [string]$executedPythonTool.result.content[0].text
        pythonExecutable = $actualPython
        packagedPython = $true
    }
}

function Test-LoomPythonArtCatalog {
    param(
        [string]$BaseUrl,
        [string]$PackageDir
    )

    $packagedPython = Join-Path $PackageDir "runtime\bin\python-embed\python.exe"
    $packagedArtJson = Join-Path $PackageDir "runtime\python\Arts\Art_LoomEcho\art.json"
    $packagedArtMain = Join-Path $PackageDir "runtime\python\Arts\Art_LoomEcho\main.py"
    Assert-PathExists $packagedPython
    Assert-PathExists $packagedArtJson
    Assert-PathExists $packagedArtMain

    $catalog = Invoke-JsonGet -Uri "$BaseUrl/v1/python-arts"
    $catalogArts = @($catalog.arts)
    $loomEcho = $catalogArts | Where-Object { [string]$_.art_id -eq "loom_echo" } | Select-Object -First 1
    if ($null -eq $loomEcho) {
        throw "Loom Python Art catalog did not include loom_echo."
    }
    Assert-Equal "Loom Echo" ([string]$loomEcho.label) "Loom Python Art catalog label mismatch."

    $savedPythonArtTool = Invoke-JsonPut -Uri "$BaseUrl/v1/tools/fixture-python-art" -Body @{
        id = "fixture-python-art"
        name = "Fixture Python Art"
        description = "Release smoke installed Python Art"
        enabled = $true
        execution = @{
            type = "python_art"
            artId = "loom_echo"
            artPath = [string]$loomEcho.path
        }
    }
    Assert-Equal "fixture-python-art" $savedPythonArtTool.tool.id "Loom Python Art tool save id mismatch."
    Assert-Equal "python_art" $savedPythonArtTool.tool.execution.type "Loom Python Art execution type mismatch."

    $executedPythonArtTool = Invoke-JsonPost -Uri "$BaseUrl/v1/tools/fixture-python-art/execute" -Body @{
        arguments = @{
            text = "release installed python art"
        }
    }
    Assert-Equal "succeeded" $executedPythonArtTool.status "Loom Python Art tool execution status mismatch."
    Assert-Equal "python art saw release installed python art" ([string]$executedPythonArtTool.result.content[0].text) "Loom Python Art tool content mismatch."

    $actualPython = [System.IO.Path]::GetFullPath([string]$executedPythonArtTool.result.pythonExecutable)
    $expectedPython = [System.IO.Path]::GetFullPath($packagedPython)
    Assert-Equal $expectedPython $actualPython "Loom Python Art did not use packaged embedded Python."

    return [ordered]@{
        artId = [string]$loomEcho.art_id
        label = [string]$loomEcho.label
        path = [string]$loomEcho.path
        count = [int]$catalogArts.Count
    }
}

function Test-LoomArtLoomRegistryCompat {
    param(
        [string]$BaseUrl
    )

    $native = Invoke-JsonPut -Uri "$BaseUrl/v1/tools/fixture-native-tool" -Body @{
        id = "fixture-native-tool"
        name = "Fixture Native Tool"
        description = "Release smoke native Loom tool preserved across ArtLoom sync"
        enabled = $true
        execution = @{
            type = "cli_wrapper"
            command = "echo"
            args = @("native")
        }
    }
    Assert-Equal "fixture-native-tool" ([string]$native.tool.id) "Loom native tool save mismatch before ArtLoom sync."

    $saved = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/arts/sync" -Body @{
        arts = @(
            @{
                id = "fixture-artloom-compat"
                name = "Fixture ArtLoom Compat"
                description = "Release smoke old art_registry compatibility aliases"
                iconColor = "#52c41a"
                enabled = $true
                autoProcess = $true
                defaults = @{
                    seed = 1234
                }
                execution_type = "cli_wrapper"
                execution = @{
                    command = "echo"
                    args = "{{inputs.image.path}} --out {{outputs.result.path}}"
                    outputs = @(
                        @{
                            name = "result"
                            type = "image"
                        }
                    )
                }
                inputs = @(
                    @{
                        name = "image"
                        type = "image"
                    }
                )
                params = @(
                    @{
                        id = "strength"
                        default = 0.25
                    }
                )
            }
        )
    }
    Assert-Equal "sync_user_arts" ([string]$saved.compatCommand) "Loom ArtLoom registry compat import command mismatch."
    Assert-Equal $true ([bool]$saved.sideEffect) "Loom ArtLoom registry compat import sideEffect mismatch."
    Assert-Equal 1 ([int]$saved.syncedCount) "Loom ArtLoom registry compat import count mismatch."

    $toolsAfterCompatImport = Invoke-JsonGet -Uri "$BaseUrl/v1/tools"
    $toolIdsAfterCompatImport = @($toolsAfterCompatImport.tools | ForEach-Object { [string]$_.id })
    Assert-Contains "fixture-native-tool" ($toolIdsAfterCompatImport -join ",") "Loom native tool was cleared by ArtLoom sync."

    $listed = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/arts"
    Assert-Equal "list_arts" ([string]$listed.compatCommand) "Loom ArtLoom list_arts compat command mismatch."
    $listedIds = @($listed.arts | ForEach-Object { [string]$_.id })
    Assert-Contains "fixture-artloom-compat" ($listedIds -join ",") "Loom ArtLoom list_arts did not include fixture."
    $listedArt = @($listed.arts)[0]
    Assert-Equal "cli_wrapper" ([string]$listedArt.execution_type) "Loom ArtLoom list_arts execution_type mismatch."
    Assert-Equal "{{inputs.image.path}} --out {{outputs.result.path}}" ([string]$listedArt.execution.args) "Loom ArtLoom list_arts legacy execution args mismatch."
    Assert-Equal "result" ([string]@($listedArt.execution.outputs)[0].name) "Loom ArtLoom list_arts legacy execution outputs mismatch."
    Assert-Equal $true ([bool]$listedArt.auto_process) "Loom ArtLoom list_arts auto_process mismatch."
    Assert-Equal 1234 ([int]$listedArt.defaults.seed) "Loom ArtLoom list_arts independent defaults mismatch."

    $enabledArts = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/arts/enabled"
    Assert-Equal "get_enabled_arts" ([string]$enabledArts.compatCommand) "Loom ArtLoom get_enabled_arts compat command mismatch."
    Assert-Equal "arts" ([string]$enabledArts.type) "Loom ArtLoom get_enabled_arts type mismatch."
    Assert-Equal 1 (@($enabledArts.arts).Count) "Loom ArtLoom get_enabled_arts enabled count mismatch."
    Assert-Equal "fixture-artloom-compat" ([string]@($enabledArts.arts)[0].id) "Loom ArtLoom get_enabled_arts id mismatch."

    $userArts = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/user-arts"
    Assert-Equal "get_user_arts" ([string]$userArts.compatCommand) "Loom ArtLoom get_user_arts compat command mismatch."
    $userArt = @($userArts.arts)[0]
    Assert-Equal "fixture-artloom-compat" ([string]$userArt.id) "Loom ArtLoom get_user_arts id mismatch."
    Assert-Equal "Fixture ArtLoom Compat" ([string]$userArt.name) "Loom ArtLoom get_user_arts name mismatch."
    Assert-Equal "Adapter" ([string]$userArt.category) "Loom ArtLoom get_user_arts category mismatch."
    Assert-Equal "1.0.0" ([string]$userArt.version) "Loom ArtLoom get_user_arts version mismatch."
    Assert-Equal "User" ([string]$userArt.author) "Loom ArtLoom get_user_arts author mismatch."
    Assert-Equal "active" ([string]$userArt.status) "Loom ArtLoom get_user_arts status mismatch."
    Assert-Equal "#52c41a" ([string]$userArt.iconColor) "Loom ArtLoom get_user_arts iconColor mismatch."
    Assert-Equal 0 ([int]$userArt.downloads) "Loom ArtLoom get_user_arts downloads mismatch."
    Assert-Equal $true ([bool]$userArt.owned) "Loom ArtLoom get_user_arts owned mismatch."
    Assert-Equal "cli_wrapper" ([string]$userArt.executionType) "Loom ArtLoom get_user_arts executionType mismatch."
    Assert-Equal "{{inputs.image.path}} --out {{outputs.result.path}}" ([string]$userArt.execution.args) "Loom ArtLoom get_user_arts legacy execution args mismatch."
    $autoProcess = Get-JsonPropertyOrNull -Object $userArt -Name "autoProcess"
    if ($null -eq $autoProcess) {
        throw "Loom ArtLoom get_user_arts autoProcess field missing."
    }
    Assert-Equal $true ([bool]$autoProcess) "Loom ArtLoom get_user_arts autoProcess mismatch."
    $userArtInputs = @($userArt.inputs)
    $userArtOutputs = @($userArt.outputs)
    Assert-Equal "image" ([string]$userArtInputs[0].name) "Loom ArtLoom get_user_arts inputs mismatch."
    Assert-Equal "result" ([string]$userArtOutputs[0].name) "Loom ArtLoom get_user_arts outputs mismatch."

    $loaded = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/arts/fixture-artloom-compat"
    Assert-Equal "get_art" ([string]$loaded.compatCommand) "Loom ArtLoom get_art compat command mismatch."
    Assert-Equal "fixture-artloom-compat" ([string]$loaded.art.id) "Loom ArtLoom get_art id mismatch."
    Assert-Equal $true ([bool]$loaded.art.auto_process) "Loom ArtLoom get_art auto_process mismatch."
    Assert-Equal 1234 ([int]$loaded.art.defaults.seed) "Loom ArtLoom get_art independent defaults mismatch."

    $disabled = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/arts/fixture-artloom-compat/disable" -Body @{}
    Assert-Equal "disable_art" ([string]$disabled.compatCommand) "Loom ArtLoom disable_art compat command mismatch."
    Assert-Equal $false ([bool]$disabled.enabled) "Loom ArtLoom disable_art enabled flag mismatch."

    $enabledArtsAfterDisable = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/arts/enabled"
    Assert-Equal "get_enabled_arts" ([string]$enabledArtsAfterDisable.compatCommand) "Loom ArtLoom get_enabled_arts after disable command mismatch."
    Assert-Equal 0 (@($enabledArtsAfterDisable.arts).Count) "Loom ArtLoom get_enabled_arts after disable count mismatch."

    $enabled = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/arts/fixture-artloom-compat/enable" -Body @{}
    Assert-Equal "enable_art" ([string]$enabled.compatCommand) "Loom ArtLoom enable_art compat command mismatch."
    Assert-Equal $true ([bool]$enabled.enabled) "Loom ArtLoom enable_art enabled flag mismatch."

    $defaults = Invoke-JsonPut -Uri "$BaseUrl/v1/artloom-compat/arts/fixture-artloom-compat/defaults" -Body @{
        defaults = @{
            strength = 0.75
        }
    }
    Assert-Equal "update_art_defaults" ([string]$defaults.compatCommand) "Loom ArtLoom update_art_defaults compat command mismatch."
    Assert-Equal 0.75 ([double]$defaults.tool.params[0].default) "Loom ArtLoom update_art_defaults value mismatch."

    $synced = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/arts/sync" -Body @{}
    Assert-Equal "sync_user_arts" ([string]$synced.compatCommand) "Loom ArtLoom sync_user_arts compat command mismatch."
    Assert-Equal $true ([bool]$synced.synced) "Loom ArtLoom sync_user_arts synced flag mismatch."

    $broadcasted = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/arts/broadcast-updated" -Body @{}
    Assert-Equal "broadcast_arts_updated" ([string]$broadcasted.compatCommand) "Loom ArtLoom broadcast_arts_updated compat command mismatch."
    Assert-Equal $true ([bool]$broadcasted.broadcasted) "Loom ArtLoom broadcast_arts_updated flag mismatch."

    $ipc = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/ipc/status"
    Assert-Equal "get_ipc_status" ([string]$ipc.compatCommand) "Loom ArtLoom get_ipc_status compat command mismatch."
    Assert-Equal "artloom-compat" ([string]$ipc.protocol) "Loom ArtLoom get_ipc_status protocol mismatch."

    return [ordered]@{
        listCommand = [string]$listed.compatCommand
        listExecutionType = [string]$listedArt.execution_type
        listLegacyArgs = [string]$listedArt.execution.args
        listAutoProcess = [bool]$listedArt.auto_process
        listDefaultSeed = [int]$listedArt.defaults.seed
        getCommand = [string]$loaded.compatCommand
        userArtsCommand = [string]$userArts.compatCommand
        userArtsCategory = [string]$userArt.category
        userArtsExecutionType = [string]$userArt.executionType
        userArtsLegacyArgs = [string]$userArt.execution.args
        userArtsAutoProcess = [bool]$autoProcess
        userArtsInput = [string]$userArtInputs[0].name
        userArtsOutput = [string]$userArtOutputs[0].name
        enabledArtsCommand = [string]$enabledArts.compatCommand
        enabledArtsCount = @($enabledArts.arts).Count
        enabledArtsAfterDisableCount = @($enabledArtsAfterDisable.arts).Count
        disableCommand = [string]$disabled.compatCommand
        enableCommand = [string]$enabled.compatCommand
        defaultsCommand = [string]$defaults.compatCommand
        syncCommand = [string]$synced.compatCommand
        broadcastCommand = [string]$broadcasted.compatCommand
        ipcCommand = [string]$ipc.compatCommand
        count = @($listed.arts).Count
    }
}

function Test-LoomArtLoomSystemCompat {
    param(
        [string]$BaseUrl
    )

    $initialAutostart = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/system/autostart"
    Assert-Equal "is_autostart_enabled" ([string]$initialAutostart.compatCommand) "Loom ArtLoom is_autostart_enabled compat command mismatch."
    Assert-Equal $false ([bool]$initialAutostart.sideEffect) "Loom ArtLoom is_autostart_enabled sideEffect mismatch."

    $setAutostart = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/system/autostart" -Body @{
        enabled = $true
    }
    Assert-Equal "set_autostart" ([string]$setAutostart.compatCommand) "Loom ArtLoom set_autostart compat command mismatch."
    Assert-Equal $true ([bool]$setAutostart.enabled) "Loom ArtLoom set_autostart enabled mismatch."
    Assert-Equal $false ([bool]$setAutostart.sideEffect) "Loom ArtLoom set_autostart sideEffect mismatch."

    $updatedAutostart = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/system/autostart"
    Assert-Equal "is_autostart_enabled" ([string]$updatedAutostart.compatCommand) "Loom ArtLoom is_autostart_enabled after set command mismatch."
    Assert-Equal $true ([bool]$updatedAutostart.enabled) "Loom ArtLoom is_autostart_enabled after set mismatch."
    Assert-Equal $false ([bool]$updatedAutostart.sideEffect) "Loom ArtLoom is_autostart_enabled after set sideEffect mismatch."

    $disabledAutostart = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/system/autostart/disable" -Body @{}
    Assert-Equal "disable_autostart" ([string]$disabledAutostart.compatCommand) "Loom ArtLoom disable_autostart compat command mismatch."
    Assert-Equal $false ([bool]$disabledAutostart.enabled) "Loom ArtLoom disable_autostart enabled mismatch."
    Assert-Equal $false ([bool]$disabledAutostart.sideEffect) "Loom ArtLoom disable_autostart sideEffect mismatch."

    $enabledAutostart = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/system/autostart/enable" -Body @{}
    Assert-Equal "enable_autostart" ([string]$enabledAutostart.compatCommand) "Loom ArtLoom enable_autostart compat command mismatch."
    Assert-Equal $true ([bool]$enabledAutostart.enabled) "Loom ArtLoom enable_autostart enabled mismatch."
    Assert-Equal $false ([bool]$enabledAutostart.sideEffect) "Loom ArtLoom enable_autostart sideEffect mismatch."

    $tray = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/system/minimize-to-tray" -Body @{
        enabled = $false
    }
    Assert-Equal "set_minimize_to_tray" ([string]$tray.compatCommand) "Loom ArtLoom set_minimize_to_tray compat command mismatch."
    Assert-Equal $false ([bool]$tray.enabled) "Loom ArtLoom set_minimize_to_tray enabled mismatch."
    Assert-Equal $false ([bool]$tray.sideEffect) "Loom ArtLoom set_minimize_to_tray sideEffect mismatch."

    return [ordered]@{
        queryCommand = [string]$initialAutostart.compatCommand
        initialEnabled = [bool]$initialAutostart.enabled
        setCommand = [string]$setAutostart.compatCommand
        updatedEnabled = [bool]$updatedAutostart.enabled
        disableCommand = [string]$disabledAutostart.compatCommand
        enableCommand = [string]$enabledAutostart.compatCommand
        trayCommand = [string]$tray.compatCommand
        sideEffect = [bool]$setAutostart.sideEffect
    }
}

function Test-LoomArtLoomWorkflowStoreCompat {
    param(
        [string]$BaseUrl
    )

    $workflowId = "release-artloom-workflow"
    $metadata = Invoke-JsonPut -Uri "$BaseUrl/v1/artloom-compat/workflows/$workflowId/metadata" -Body @{
        id = $workflowId
        name = "Release ArtLoom Workflow"
        description = "Old workflow_store metadata"
        created_at = "1"
        updated_at = ""
        status = "draft"
        node_count = 0
        tags = @("release", "compat")
    }
    Assert-Equal "save_workflow_metadata" ([string]$metadata.compatCommand) "Loom save_workflow_metadata compat command mismatch."
    Assert-Equal $workflowId ([string]$metadata.workflow.id) "Loom save_workflow_metadata id mismatch."
    Assert-Equal "Release ArtLoom Workflow" ([string]$metadata.workflow.name) "Loom save_workflow_metadata name mismatch."
    Assert-Equal "Old workflow_store metadata" ([string]$metadata.workflow.description) "Loom save_workflow_metadata description mismatch."
    Assert-Equal "draft" ([string]$metadata.workflow.status) "Loom save_workflow_metadata status mismatch."
    Assert-Equal "release" ([string]@($metadata.workflow.tags)[0]) "Loom save_workflow_metadata tags mismatch."

    $workflowYaml = @"
name: Release ArtLoom Workflow
nodes:
  - id: prompt
    uses: text.prompt
"@
    $saved = Invoke-JsonPut -Uri "$BaseUrl/v1/artloom-compat/workflows/$workflowId/data" -Body @{
        data = $workflowYaml
    }
    Assert-Equal "save_workflow_data" ([string]$saved.compatCommand) "Loom save_workflow_data compat command mismatch."
    Assert-Equal $workflowId ([string]$saved.workflowId) "Loom save_workflow_data id mismatch."
    Assert-Equal $true ([bool]$saved.saved) "Loom save_workflow_data saved flag mismatch."

    $listed = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/workflows"
    Assert-Equal "list_workflows" ([string]$listed.compatCommand) "Loom list_workflows compat command mismatch."
    $listedWorkflow = @($listed.workflows) | Where-Object { [string]$_.id -eq $workflowId } | Select-Object -First 1
    if ($null -eq $listedWorkflow) {
        throw "Loom list_workflows did not include $workflowId."
    }
    Assert-Equal "Release ArtLoom Workflow" ([string]$listedWorkflow.name) "Loom list_workflows name mismatch."
    Assert-Equal 1 ([int]$listedWorkflow.node_count) "Loom list_workflows node_count mismatch."
    Assert-Equal 1 ([int]$listedWorkflow.nodeCount) "Loom list_workflows nodeCount mismatch."

    $loaded = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/workflows/$workflowId/data"
    Assert-Equal "load_workflow_data" ([string]$loaded.compatCommand) "Loom load_workflow_data compat command mismatch."
    Assert-Equal $workflowId ([string]$loaded.workflowId) "Loom load_workflow_data id mismatch."
    Assert-Contains "uses: text.prompt" ([string]$loaded.data) "Loom load_workflow_data YAML mismatch."

    $deleted = Invoke-JsonDelete -Uri "$BaseUrl/v1/artloom-compat/workflows/$workflowId/data"
    Assert-Equal "delete_workflow_data" ([string]$deleted.compatCommand) "Loom delete_workflow_data compat command mismatch."
    Assert-Equal $workflowId ([string]$deleted.workflowId) "Loom delete_workflow_data id mismatch."
    Assert-Equal $true ([bool]$deleted.deleted) "Loom delete_workflow_data deleted flag mismatch."

    return [ordered]@{
        metadataCommand = [string]$metadata.compatCommand
        saveDataCommand = [string]$saved.compatCommand
        listCommand = [string]$listed.compatCommand
        loadDataCommand = [string]$loaded.compatCommand
        deleteDataCommand = [string]$deleted.compatCommand
        listedNodeCount = [int]$listedWorkflow.node_count
        loadedContainsPrompt = ([string]$loaded.data).Contains("uses: text.prompt")
    }
}

function Test-LoomArtLoomSharedMemoryCompat {
    param(
        [string]$BaseUrl
    )

    $created = Invoke-JsonPost -Uri "$BaseUrl/v1/shared-memory/buffers" -Body @{
        width = 1
        height = 1
        channels = 4
    }
    Assert-Equal "shm_create_buffer" ([string]$created.compatCommand) "Loom shm_create_buffer compat command mismatch."
    $handle = [string]$created.handle
    if ([string]::IsNullOrWhiteSpace($handle)) {
        throw "Loom shm_create_buffer did not return a handle."
    }

    $listed = Invoke-JsonGet -Uri "$BaseUrl/v1/shared-memory/buffers"
    Assert-Equal "shm_list_buffers" ([string]$listed.compatCommand) "Loom shm_list_buffers compat command mismatch."
    Assert-Contains $handle ((@($listed.buffers) | ForEach-Object { [string]$_.handle_name }) -join ",") "Loom shm_list_buffers did not include created handle."

    $info = Invoke-JsonGet -Uri "$BaseUrl/v1/shared-memory/buffers/$handle"
    Assert-Equal "shm_get_buffer_info" ([string]$info.compatCommand) "Loom shm_get_buffer_info compat command mismatch."
    Assert-Equal "rgba8" ([string]$info.buffer.format) "Loom shm_get_buffer_info format mismatch."
    Assert-Equal 1 ([int]$info.buffer.ref_count) "Loom shm_get_buffer_info ref_count mismatch."

    $released = Invoke-JsonDelete -Uri "$BaseUrl/v1/shared-memory/buffers/$handle"
    Assert-Equal "shm_release_buffer" ([string]$released.compatCommand) "Loom shm_release_buffer compat command mismatch."
    Assert-Equal $true ([bool]$released.released) "Loom shm_release_buffer released flag mismatch."

    return [ordered]@{
        createCommand = [string]$created.compatCommand
        listCommand = [string]$listed.compatCommand
        infoCommand = [string]$info.compatCommand
        releaseCommand = [string]$released.compatCommand
        handle = $handle
        format = [string]$info.buffer.format
    }
}

function Test-LoomPythonEngineCompat {
    param(
        [string]$BaseUrl
    )

    $status = Invoke-JsonGet -Uri "$BaseUrl/v1/python-arts/engine/status"
    Assert-Equal "python_engine_status" ([string]$status.compatCommand) "Loom python_engine_status compat command mismatch."
    Assert-Equal $true ([bool]$status.available) "Loom python_engine_status should be available in packaged release."

    $prefetch = Invoke-JsonPost -Uri "$BaseUrl/v1/python-arts/shader/prefetch" -Body @{
        artId = "loom_echo"
        params = @{
            output_mode = "shader"
            mode = "shader"
        }
    }
    Assert-Equal "prefetch_shader" ([string]$prefetch.compatCommand) "Loom prefetch_shader compat command mismatch."
    Assert-Equal "text" ([string]$prefetch.result.content[0].type) "Loom prefetch_shader result content type mismatch."

    return [ordered]@{
        statusCommand = [string]$status.compatCommand
        prefetchCommand = [string]$prefetch.compatCommand
        available = [bool]$status.available
        installedArtCount = [int]$status.installedArtCount
        resultType = [string]$prefetch.result.content[0].type
    }
}

function Test-LoomPythonDirectCompat {
    param(
        [string]$BaseUrl,
        [string]$TempRoot
    )

    $executed = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/python/execute-art" -Body @{
        artId = "loom_echo"
        params = @{
            text = "release direct python art"
        }
    }
    Assert-Equal "execute_python_art" ([string]$executed.compatCommand) "Loom execute_python_art compat command mismatch."
    Assert-Equal 200 ([int]$executed.status) "Loom execute_python_art status mismatch."
    Assert-Equal "python art saw release direct python art" ([string]$executed.data.content[0].text) "Loom execute_python_art content mismatch."

    $imageArtDir = Join-Path $TempRoot "python-process-image-art"
    New-Item -ItemType Directory -Force -Path $imageArtDir | Out-Null
    $imageArtSource = @'
import shutil

def main(args):
    shutil.copyfile(args["input_path"], args["output_path"])
    return {"copied": True}
'@
    [System.IO.File]::WriteAllText(
        (Join-Path $imageArtDir "main.py"),
        $imageArtSource,
        [System.Text.UTF8Encoding]::new($false)
    )

    $imageData = New-LoomNativeImageSmokePngDataUrl
    $processed = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/python/process-image" -Body @{
        artId = "copy_image"
        artPath = $imageArtDir
        inputBase64 = $imageData
        params = @{}
    }
    Assert-Equal "python_process_image" ([string]$processed.compatCommand) "Loom python_process_image compat command mismatch."
    Assert-Equal $true ([bool]$processed.success) "Loom python_process_image success mismatch."
    $outputData = [string]$processed.output_base64
    if (-not $outputData.StartsWith("data:image/png;base64,", [System.StringComparison]::Ordinal)) {
        throw "Loom python_process_image output must be a PNG data URL."
    }
    if ([string]::IsNullOrWhiteSpace([string]$processed.output_path)) {
        throw "Loom python_process_image output_path missing."
    }

    return [ordered]@{
        executeCommand = [string]$executed.compatCommand
        executeStatus = [int]$executed.status
        executeText = [string]$executed.data.content[0].text
        processCommand = [string]$processed.compatCommand
        processSuccess = [bool]$processed.success
        processOutputLength = [int]$outputData.Length
    }
}

function Test-LoomArtLoomPythonSourceCompat {
    param(
        [string]$BaseUrl,
        [string]$TempRoot
    )

    $sourceDir = Join-Path $TempRoot "artloom-python-source-compat"
    New-Item -ItemType Directory -Force -Path $sourceDir | Out-Null
    $sourcePath = Join-Path $sourceDir "source_alias_fixture.py"
    $artJsonPath = Join-Path $sourceDir "art.json"
    $source = @'
def main(args):
    return {"text": args.get("text", "compat")}
'@
    [System.IO.File]::WriteAllText($sourcePath, $source, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText($artJsonPath, @'
{
  "art_id": "source_alias_fixture",
  "label": "Source Alias Fixture",
  "description": "Release smoke ArtLoom command-name source alias fixture"
}
'@, [System.Text.UTF8Encoding]::new($false))

    $installed = Invoke-JsonGet -Uri "$BaseUrl/v1/artloom-compat/python/installed-arts"
    Assert-Equal "list_installed_arts" ([string]$installed.compatCommand) "Loom list_installed_arts compat command mismatch."
    if ($null -eq $installed.arts) {
        throw "Loom list_installed_arts must return an arts array."
    }

    $read = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/python/read-python-file" -Body @{
        filePath = $sourcePath
    }
    Assert-Equal "read_python_file" ([string]$read.compatCommand) "Loom read_python_file compat command mismatch."
    Assert-Contains 'args.get("text"' ([string]$read.content) "Loom read_python_file did not return fixture code."
    Assert-Equal $sourcePath ([string]$read.filePath) "Loom read_python_file filePath mismatch."

    $nearby = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/python/check-art-json-nearby" -Body @{
        pythonPath = $sourcePath
    }
    Assert-Equal "check_art_json_nearby" ([string]$nearby.compatCommand) "Loom check_art_json_nearby compat command mismatch."
    Assert-Equal $true ([bool]$nearby.found) "Loom check_art_json_nearby found mismatch."
    Assert-Equal "source_alias_fixture" ([string]$nearby.artJson.art_id) "Loom check_art_json_nearby art id mismatch."

    $artJson = Invoke-JsonPost -Uri "$BaseUrl/v1/artloom-compat/python/read-art-json" -Body @{
        artPath = $sourceDir
    }
    Assert-Equal "read_art_json" ([string]$artJson.compatCommand) "Loom read_art_json compat command mismatch."
    Assert-Equal "Source Alias Fixture" ([string]$artJson.artJson.label) "Loom read_art_json label mismatch."

    return [ordered]@{
        listCommand = [string]$installed.compatCommand
        installedCount = @($installed.arts).Count
        readCommand = [string]$read.compatCommand
        readBytes = [int]$read.bytes
        nearbyCommand = [string]$nearby.compatCommand
        nearbyFound = [bool]$nearby.found
        readArtJsonCommand = [string]$artJson.compatCommand
        artId = [string]$artJson.artJson.art_id
    }
}

function Test-LoomPythonArtSourceImport {
    param(
        [string]$BaseUrl,
        [string]$TempRoot
    )

    $sourceDir = Join-Path $TempRoot "python-source-import"
    New-Item -ItemType Directory -Force -Path $sourceDir | Out-Null
    $sourcePath = Join-Path $sourceDir "source_import_fixture.py"
    $artJsonPath = Join-Path $sourceDir "art.json"
    $source = @'
import json
import sys

def run(args):
    text = args.get("text", "")
    strength = args["strength"]
    return {"text": text, "confidence": strength}

payload = json.loads(sys.argv[1])
arguments = payload.get("arguments", {})
response = {
    "content": [
        {
            "type": "text",
            "text": "source import saw " + str(arguments.get("text", "")),
        }
    ],
    "pythonExecutable": sys.executable,
}
print(json.dumps(response, ensure_ascii=False))
'@
    [System.IO.File]::WriteAllText($sourcePath, $source, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText($artJsonPath, @'
{
  "art_id": "source_import_fixture",
  "label": "Source Import Fixture",
  "description": "Release smoke nearby art.json fixture",
  "signature": {
    "inputs": [
      { "id": "text", "label": "Text", "type": "String" }
    ],
    "outputs": [
      { "id": "text", "label": "Text", "type": "String" }
    ]
  },
  "variables": [
    { "id": "strength", "label": "Strength", "widget": "slider", "default": 0.5 }
  ]
}
'@, [System.Text.UTF8Encoding]::new($false))

    # POST /v1/python-arts/source/read
    $read = Invoke-JsonPost -Uri "$BaseUrl/v1/python-arts/source/read" -Body @{
        path = $sourcePath
    }
    Assert-Contains 'args.get("text"' ([string]$read.content) "Loom Python source read did not return fixture code."

    # POST /v1/python-arts/source/check-art-json
    $nearby = Invoke-JsonPost -Uri "$BaseUrl/v1/python-arts/source/check-art-json" -Body @{
        pythonPath = $sourcePath
    }
    Assert-Equal $true ([bool]$nearby.found) "Loom nearby art.json detection failed."
    Assert-Equal "Source Import Fixture" ([string]$nearby.artJson.label) "Loom nearby art.json label mismatch."

    # POST /v1/python-arts/source/read-art-json
    $artJson = Invoke-JsonPost -Uri "$BaseUrl/v1/python-arts/source/read-art-json" -Body @{
        artPath = $sourceDir
    }
    Assert-Equal "source_import_fixture" ([string]$artJson.artJson.art_id) "Loom read-art-json art id mismatch."

    # POST /v1/python-arts/source/infer-ports
    $inferred = Invoke-JsonPost -Uri "$BaseUrl/v1/python-arts/source/infer-ports" -Body @{
        path = $sourcePath
    }
    Assert-Equal "text" ([string]$inferred.inputs[0].name) "Loom Python source inferred input mismatch."
    Assert-Equal "strength" ([string]$inferred.inputs[1].name) "Loom Python source inferred variable input mismatch."
    Assert-Equal "text" ([string]$inferred.outputs[0].name) "Loom Python source inferred output mismatch."

    $savedSourceTool = Invoke-JsonPut -Uri "$BaseUrl/v1/tools/fixture-python-source-import" -Body @{
        id = "fixture-python-source-import"
        name = "Fixture Python Source Import"
        description = "Release smoke source-imported Python script tool"
        enabled = $true
        execution = @{
            type = "script"
            path = $sourcePath
        }
        inputs = @($inferred.inputs)
        outputs = @($inferred.outputs)
    }
    Assert-Equal "fixture-python-source-import" $savedSourceTool.tool.id "Loom source-imported tool save id mismatch."
    Assert-Equal "script" $savedSourceTool.tool.execution.type "Loom source-imported tool execution type mismatch."

    $executedSourceTool = Invoke-JsonPost -Uri "$BaseUrl/v1/tools/fixture-python-source-import/execute" -Body @{
        arguments = @{
            text = "release source helper"
            strength = 0.5
        }
    }
    Assert-Equal "succeeded" $executedSourceTool.status "Loom source-imported script execution status mismatch."
    Assert-Equal "source import saw release source helper" ([string]$executedSourceTool.result.content[0].text) "Loom source-imported script execution content mismatch."

    return [ordered]@{
        sourcePath = [string]$read.path
        nearbyArtJsonFound = [bool]$nearby.found
        nearbyArtJsonLabel = [string]$nearby.artJson.label
        inferredInputs = @($inferred.inputs).Count
        inferredOutputs = @($inferred.outputs).Count
        scriptToolExecution = [string]$executedSourceTool.result.content[0].text
    }
}

function Test-LoomHookBridgeScriptAhrpProcess {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art/process"
            params = [ordered]@{
                request_id = "release-script-ahrp-process"
                art_id = "fixture-script-art"
                input = [ordered]@{
                    type = "base64"
                    data = $imageData
                    width = 1
                    height = 1
                    format = "rgba8"
                }
                params = [ordered]@{}
                disabled_params = @()
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "release-script-ahrp-process" ([string]$response.request_id) "Loom script AHRP request id mismatch."
        Assert-Equal "Success" ([string]$response.status) "Loom script AHRP status mismatch."
        Assert-Equal "result" ([string]$response.data.type) "Loom script AHRP data type mismatch."
        Assert-Equal "base64" ([string]$response.data.output.type) "Loom script AHRP output type mismatch."
        Assert-Equal $imageData ([string]$response.data.output.data) "Loom script AHRP output data mismatch."
        Assert-Equal 1 ([int]$response.data.output.width) "Loom script AHRP output width mismatch."
        Assert-Equal 1 ([int]$response.data.output.height) "Loom script AHRP output height mismatch."

        return [ordered]@{
            requestId = [string]$response.request_id
            status = [string]$response.status
            outputType = [string]$response.data.output.type
            width = [int]$response.data.output.width
            height = [int]$response.data.output.height
            outputLength = [int]([string]$response.data.output.data).Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeScriptShaderArt {
    param(
        [int]$Port
    )

    $client = $null
    $expectedShader = "void fragment() { COLOR = vec4(1.0); }"
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art_loom/execute_art_node"
            params = [ordered]@{
                node_id = "release-node-shader"
                art_id = "fixture-script-shader"
                params = [ordered]@{
                    mode = "shader"
                }
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom script shader response type mismatch."
        Assert-Equal $true ([bool]$response.data.success) "Loom script shader success flag mismatch."
        Assert-Equal "release-node-shader" ([string]$response.data.node_id) "Loom script shader node id mismatch."
        Assert-Equal $expectedShader ([string]$response.data.output_text) "Loom script shader output text mismatch."

        return [ordered]@{
            type = [string]$response.type
            success = [bool]$response.data.success
            nodeId = [string]$response.data.node_id
            outputText = [string]$response.data.output_text
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeCloudArtNode {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art_loom/execute_art_node"
            params = [ordered]@{
                node_id = "release-node-cloud"
                art_id = "fixture-cloud-art"
                input_base64 = $imageData
                params = [ordered]@{}
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom cloud API Art node response type mismatch."
        Assert-Equal $true ([bool]$response.data.success) "Loom cloud API Art node success flag mismatch."
        Assert-Equal "release-node-cloud" ([string]$response.data.node_id) "Loom cloud API Art node id mismatch."
        $outputData = [string]$response.data.output_base64
        Assert-Equal $imageData $outputData "Loom cloud API Art node output data mismatch."

        return [ordered]@{
            type = [string]$response.type
            success = [bool]$response.data.success
            nodeId = [string]$response.data.node_id
            outputType = "base64"
            outputLength = [int]$outputData.Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeCloudMultipartArtNode {
    param(
        [int]$Port,
        [string]$FixtureOutputDir
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art_loom/execute_art_node"
            params = [ordered]@{
                node_id = "release-node-cloud-multipart"
                art_id = "fixture-cloud-multipart-art"
                input_base64 = $imageData
                params = [ordered]@{
                    route = "image"
                    trace = "release-trace"
                    prompt = "release cloud multipart"
                }
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom cloud multipart Art node response type mismatch."
        Assert-Equal $true ([bool]$response.data.success) "Loom cloud multipart Art node success flag mismatch."
        Assert-Equal "release-node-cloud-multipart" ([string]$response.data.node_id) "Loom cloud multipart Art node id mismatch."
        $outputData = [string]$response.data.output_base64
        if ([string]::IsNullOrWhiteSpace($outputData)) {
            throw "Loom cloud multipart Art node output data missing."
        }

        $evidencePath = Join-Path $FixtureOutputDir "multipart-request.json"
        Wait-ForPath -Path $evidencePath -TimeoutSeconds 20
        $evidence = Get-Content -Raw -LiteralPath $evidencePath | ConvertFrom-Json
        Assert-Equal "/multipart/image" ([string]$evidence.path) "Loom cloud multipart request path mismatch."
        Assert-Equal $true ([bool]$evidence.multipartSeen) "Loom cloud multipart request content type missing."
        Assert-Equal $true ([bool]$evidence.fileFieldSeen) "Loom cloud multipart request file field missing."
        Assert-Equal $true ([bool]$evidence.tempFilenameSeen) "Loom cloud multipart request temp filename missing."
        Assert-Equal $true ([bool]$evidence.promptSeen) "Loom cloud multipart request prompt template missing."
        Assert-Equal $true ([bool]$evidence.traceSeen) "Loom cloud multipart request header template missing."
        Assert-Equal $false ([bool]$evidence.unresolvedTemplateSeen) "Loom cloud multipart request still contains unresolved templates."

        return [ordered]@{
            type = [string]$response.type
            success = [bool]$response.data.success
            nodeId = [string]$response.data.node_id
            outputType = "base64"
            outputLength = [int]$outputData.Length
            multipartSeen = [bool]$evidence.multipartSeen
            fileFieldSeen = [bool]$evidence.fileFieldSeen
            tempFilenameSeen = [bool]$evidence.tempFilenameSeen
            promptSeen = [bool]$evidence.promptSeen
            traceSeen = [bool]$evidence.traceSeen
            unresolvedTemplateSeen = [bool]$evidence.unresolvedTemplateSeen
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeCloudAhrpProcess {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art/process"
            params = [ordered]@{
                request_id = "release-cloud-ahrp-process"
                art_id = "fixture-cloud-art"
                input = [ordered]@{
                    type = "base64"
                    data = $imageData
                    width = 1
                    height = 1
                    format = "rgba8"
                }
                params = [ordered]@{}
                disabled_params = @()
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "release-cloud-ahrp-process" ([string]$response.request_id) "Loom cloud API AHRP request id mismatch."
        Assert-Equal "Success" ([string]$response.status) "Loom cloud API AHRP status mismatch."
        Assert-Equal "result" ([string]$response.data.type) "Loom cloud API AHRP data type mismatch."
        Assert-Equal "base64" ([string]$response.data.output.type) "Loom cloud API AHRP output type mismatch."
        Assert-Equal $imageData ([string]$response.data.output.data) "Loom cloud API AHRP output data mismatch."
        Assert-Equal 1 ([int]$response.data.output.width) "Loom cloud API AHRP output width mismatch."
        Assert-Equal 1 ([int]$response.data.output.height) "Loom cloud API AHRP output height mismatch."

        return [ordered]@{
            requestId = [string]$response.request_id
            status = [string]$response.status
            outputType = [string]$response.data.output.type
            width = [int]$response.data.output.width
            height = [int]$response.data.output.height
            outputLength = [int]([string]$response.data.output.data).Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeWorkflowArtNode {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art_loom/execute_art_node"
            params = [ordered]@{
                node_id = "release-node-workflow"
                art_id = "release-workflow-tool"
                input_base64 = $imageData
                params = [ordered]@{}
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "success" ([string]$response.type) "Loom workflow Art node response type mismatch."
        Assert-Equal $true ([bool]$response.data.success) "Loom workflow Art node success flag mismatch."
        Assert-Equal "release-node-workflow" ([string]$response.data.node_id) "Loom workflow Art node id mismatch."
        $outputData = [string]$response.data.output_base64
        Assert-Equal $imageData $outputData "Loom workflow Art node output data mismatch."

        return [ordered]@{
            type = [string]$response.type
            success = [bool]$response.data.success
            nodeId = [string]$response.data.node_id
            outputType = "base64"
            outputLength = [int]$outputData.Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Test-LoomHookBridgeWorkflowAhrpProcess {
    param(
        [int]$Port
    )

    $client = $null
    $imageData = New-LoomNativeImageSmokePngDataUrl
    try {
        $client = New-LoomHookBridgeWebSocket -Port $Port
        $request = [ordered]@{
            method = "art/process"
            params = [ordered]@{
                request_id = "release-workflow-ahrp-process"
                art_id = "release-workflow-tool"
                input = [ordered]@{
                    type = "base64"
                    data = $imageData
                    width = 1
                    height = 1
                    format = "rgba8"
                }
                params = [ordered]@{}
                disabled_params = @()
            }
        }
        Send-LoomHookBridgeWebSocketJson `
            -Client $client `
            -Json ($request | ConvertTo-Json -Depth 20 -Compress)
        $response = Receive-LoomHookBridgeWebSocketJson -Client $client

        Assert-Equal "release-workflow-ahrp-process" ([string]$response.request_id) "Loom workflow AHRP request id mismatch."
        Assert-Equal "Success" ([string]$response.status) "Loom workflow AHRP status mismatch."
        Assert-Equal "result" ([string]$response.data.type) "Loom workflow AHRP data type mismatch."
        Assert-Equal "base64" ([string]$response.data.output.type) "Loom workflow AHRP output type mismatch."
        Assert-Equal $imageData ([string]$response.data.output.data) "Loom workflow AHRP output data mismatch."
        Assert-Equal 1 ([int]$response.data.output.width) "Loom workflow AHRP output width mismatch."
        Assert-Equal 1 ([int]$response.data.output.height) "Loom workflow AHRP output height mismatch."

        return [ordered]@{
            requestId = [string]$response.request_id
            status = [string]$response.status
            outputType = [string]$response.data.output.type
            width = [int]$response.data.output.width
            height = [int]$response.data.output.height
            outputLength = [int]([string]$response.data.output.data).Length
        }
    } finally {
        Close-LoomHookBridgeWebSocket -Client $client
    }
}

function Assert-HttpStatus {
    param(
        [string]$Uri,
        [string]$Method = "Get",
        [int]$ExpectedStatus,
        [object]$Body = $null,
        [hashtable]$Headers = @{}
    )

    $statusCode = $null
    try {
        if ($null -eq $Body) {
            Invoke-WebRequest -Uri $Uri -Method $Method -Headers $Headers -TimeoutSec 10 -UseBasicParsing | Out-Null
        } else {
            $json = $Body | ConvertTo-Json -Depth 20
            Invoke-WebRequest -Uri $Uri -Method $Method -Headers $Headers -ContentType "application/json" -Body $json -TimeoutSec 10 -UseBasicParsing | Out-Null
        }
        $statusCode = 200
    } catch {
        if ($null -eq $_.Exception.Response) {
            throw
        }
        $statusCode = [int]$_.Exception.Response.StatusCode
    }

    Assert-Equal $ExpectedStatus $statusCode "HTTP status mismatch for $Method $Uri."
}

function Stop-SpawnedProcess {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutMilliseconds = 5000
    )

    if ($null -eq $Process) {
        return
    }

    try {
        $descendantProcessIds = @(Get-SmokeDescendantProcessIds -ProcessId $Process.Id)
        for ($index = $descendantProcessIds.Count - 1; $index -ge 0; $index--) {
            Stop-Process -Id $descendantProcessIds[$index] -Force -ErrorAction SilentlyContinue
        }

        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        }
        if (-not $Process.WaitForExit($TimeoutMilliseconds)) {
            Write-Warning "Timed out waiting for process $($Process.Id) to exit after Stop-Process."
        }

        foreach ($descendantProcessId in $descendantProcessIds) {
            $descendant = Get-Process -Id $descendantProcessId -ErrorAction SilentlyContinue
            if ($null -ne $descendant) {
                Write-Warning "Descendant process $descendantProcessId was still running after cleanup."
            }
        }
    } catch {
        Write-Warning "Failed to stop spawned process $($Process.Id): $($_.Exception.Message)"
    }
}

function Get-SmokeDescendantProcessIds {
    param([int]$ProcessId)

    $pending = New-Object System.Collections.ArrayList
    $descendants = New-Object System.Collections.ArrayList
    $seen = @{}
    [void]$pending.Add($ProcessId)

    while ($pending.Count -gt 0) {
        $parentProcessId = [int]$pending[0]
        $pending.RemoveAt(0)

        foreach ($childProcessId in @(Get-SmokeChildProcessIds -ParentProcessId $parentProcessId)) {
            if (-not $seen.ContainsKey($childProcessId)) {
                $seen[$childProcessId] = $true
                [void]$descendants.Add($childProcessId)
                [void]$pending.Add($childProcessId)
            }
        }
    }

    return @($descendants | ForEach-Object { [int]$_ })
}

function Get-SmokeChildProcessIds {
    param([int]$ParentProcessId)

    $filter = "ParentProcessId=$ParentProcessId"
    try {
        return @(Get-CimInstance -ClassName Win32_Process -Filter $filter -ErrorAction Stop | ForEach-Object { [int]$_.ProcessId })
    } catch {
        return @(Get-WmiObject -Class Win32_Process -Filter $filter -ErrorAction SilentlyContinue | ForEach-Object { [int]$_.ProcessId })
    }
}

function Remove-SmokeTempRoot {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }

    try {
        Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop
    } catch {
        Write-Warning "Failed to remove smoke temp root '$Path': $($_.Exception.Message)"
    }
}

function Save-SmokeFailureEvidence {
    param(
        [string]$TempRoot,
        [string]$Label
    )

    if ([string]::IsNullOrWhiteSpace($TempRoot) -or -not (Test-Path -LiteralPath $TempRoot)) {
        return $null
    }

    $safeVersion = $VersionId -replace "[^A-Za-z0-9._-]", "_"
    $safeLabel = $Label -replace "[^A-Za-z0-9._-]", "_"
    $failureEvidencePath = Join-Path (Get-SmokeEvidenceDir) "failures\$safeVersion-$safeLabel-$PID-$([System.Guid]::NewGuid().ToString("N"))"
    New-Item -ItemType Directory -Force -Path $failureEvidencePath | Out-Null

    Get-ChildItem -LiteralPath $TempRoot -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
        $relativePath = $_.FullName.Substring($TempRoot.Length).TrimStart([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
        $destination = Join-Path $failureEvidencePath $relativePath
        $destinationDir = Split-Path -Parent $destination
        New-Item -ItemType Directory -Force -Path $destinationDir | Out-Null
        $extension = $_.Extension.ToLowerInvariant()
        if ($extension -in @(".json", ".log", ".txt", ".toml")) {
            $content = Get-Content -LiteralPath $_.FullName -Raw
            Write-Utf8NoBomFile -Path $destination -Content (Redact-SmokeJsonContent -Content $content)
        } else {
            Copy-Item -LiteralPath $_.FullName -Destination $destination -Force
        }
    }

    Write-Warning "Saved smoke failure diagnostics to failureEvidencePath=$failureEvidencePath"
    return $failureEvidencePath
}

function Write-Utf8NoBomFile {
    param(
        [string]$Path,
        [string]$Content
    )

    $encoding = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText($Path, $Content, $encoding)
}

function Get-SmokeEvidenceDir {
    $evidenceDir = $EvidenceRoot
    New-Item -ItemType Directory -Force -Path $evidenceDir | Out-Null
    return $evidenceDir
}

function New-SmokeEvidenceRunId {
    $safeApps = (@($resolvedApps) -join "-") -replace "[^A-Za-z0-9._-]", "_"
    return "$(Get-Date -Format 'yyyyMMdd-HHmmss')-$safeApps-$PID-$([System.Guid]::NewGuid().ToString('N'))"
}

function Initialize-SmokeEvidenceRun {
    if ([string]::IsNullOrWhiteSpace($script:SmokeEvidenceRunId)) {
        $script:SmokeEvidenceRunId = New-SmokeEvidenceRunId
    }

    if ([string]::IsNullOrWhiteSpace($script:SmokeEvidenceRunDir)) {
        $script:SmokeEvidenceRunDir = Join-Path (Get-SmokeEvidenceDir) "runs\$($script:SmokeEvidenceRunId)"
        New-Item -ItemType Directory -Force -Path $script:SmokeEvidenceRunDir | Out-Null
    }
}

function Get-SmokeRunEvidenceDir {
    Initialize-SmokeEvidenceRun
    return $script:SmokeEvidenceRunDir
}

function Write-SmokeJsonEvidence {
    param(
        [string]$FileName,
        [object]$Value,
        [switch]$Latest
    )

    $root = Get-SmokeRunEvidenceDir
    if ($Latest) {
        $root = Join-Path (Get-SmokeEvidenceDir) "latest"
        New-Item -ItemType Directory -Force -Path $root | Out-Null
    }
    $evidencePath = Join-Path $root $FileName
    $json = Redact-SmokeJsonContent -Content ($Value | ConvertTo-Json -Depth 40)
    Write-Utf8NoBomFile -Path $evidencePath -Content $json
    return $evidencePath
}

function Redact-SmokeJsonContent {
    param([AllowEmptyString()][string]$Content)

    $redacted = $Content -replace '("authToken"\s*:\s*")[^"]*(")', '$1<redacted>$2'
    $redacted = $redacted -replace '("Authorization"\s*:\s*")[^"]*(")', '$1<redacted>$2'
    $redacted = $redacted -replace '(Bearer\s+)[A-Za-z0-9._~+/\-=]+', '$1<redacted>'
    return $redacted
}

function Redact-SmokeFailureText {
    param([AllowEmptyString()][string]$Text)

    return Redact-SmokeJsonContent -Content $Text
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)

    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) {
        return $Argument
    }

    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashCount = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq [char]0x5c) {
            $backslashCount += 1
            continue
        }

        if ($character -eq [char]0x22) {
            if ($backslashCount -gt 0) {
                [void]$builder.Append("\" * (($backslashCount * 2) + 1))
            } else {
                [void]$builder.Append("\")
            }
            [void]$builder.Append('"')
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            [void]$builder.Append("\" * $backslashCount)
            $backslashCount = 0
        }
        [void]$builder.Append($character)
    }

    if ($backslashCount -gt 0) {
        [void]$builder.Append("\" * ($backslashCount * 2))
    }
    [void]$builder.Append('"')

    return $builder.ToString()
}

function Start-SmokeProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$StdoutPath = "",
        [string]$StderrPath = "",
        [switch]$Wait,
        [int]$TimeoutSeconds = 0
    )

    $argumentLine = (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument -Argument $_ }) -join " "
    $parameters = @{
        FilePath = $FilePath
        ArgumentList = $argumentLine
        PassThru = $true
        WindowStyle = "Hidden"
    }

    if (-not [string]::IsNullOrWhiteSpace($StdoutPath)) {
        $parameters.RedirectStandardOutput = $StdoutPath
    }
    if (-not [string]::IsNullOrWhiteSpace($StderrPath)) {
        $parameters.RedirectStandardError = $StderrPath
    }

    $process = Start-Process @parameters

    if ($Wait) {
        if ($TimeoutSeconds -gt 0) {
            if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
                Stop-SpawnedProcess -Process $process
                throw "Timed out waiting for process '$FilePath' after $TimeoutSeconds seconds."
            }
        } else {
            $process.WaitForExit()
        }
    }

    return $process
}

function Get-SmokeCmdExePath {
    $cmdPath = Join-Path $env:WINDIR "System32\cmd.exe"
    Assert-PathExists $cmdPath
    return $cmdPath
}

function Start-SmokeCapturedProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$StdoutPath,
        [string]$StderrPath,
        [string]$WrapperPath
    )

    $exeAndArguments = @((ConvertTo-ProcessArgument -Argument $FilePath)) + (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument -Argument $_ })
    $commandLine = ($exeAndArguments -join " ")
    $wrapperText = @"
@echo off
$commandLine 1>$(ConvertTo-ProcessArgument -Argument $StdoutPath) 2>$(ConvertTo-ProcessArgument -Argument $StderrPath)
exit /b %ERRORLEVEL%
"@
    Write-Utf8NoBomFile -Path $WrapperPath -Content $wrapperText

    return Start-SmokeProcess `
        -FilePath (Get-SmokeCmdExePath) `
        -ArgumentList @("/d", "/s", "/c", $WrapperPath)
}

function Invoke-ProcessCapture {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [int]$TimeoutSeconds = 60
    )

    $captureRoot = New-SmokeTempRoot -Prefix "neuro-process-capture"
    try {
        $stdoutPath = Join-Path $captureRoot "stdout.log"
        $stderrPath = Join-Path $captureRoot "stderr.log"
        $wrapperPath = Join-Path $captureRoot "run.cmd"
        $process = Start-SmokeCapturedProcess `
            -FilePath $FilePath `
            -ArgumentList $ArgumentList `
            -StdoutPath $stdoutPath `
            -StderrPath $stderrPath `
            -WrapperPath $wrapperPath

        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            Stop-SpawnedProcess -Process $process
            $timeoutOutput = (((Read-SmokeTextFile -Path $stdoutPath), (Read-SmokeTextFile -Path $stderrPath)) -join "`n").Trim()
            throw "Timed out waiting for process '$FilePath' after $TimeoutSeconds seconds. output=$(Redact-SmokeFailureText -Text $timeoutOutput)"
        }

        $stdout = Read-SmokeTextFile -Path $stdoutPath
        $stderr = Read-SmokeTextFile -Path $stderrPath

        return [ordered]@{
            exitCode = [int]$process.ExitCode
            stdout = [string]$stdout
            stderr = [string]$stderr
            output = (($stdout, $stderr) -join "`n").Trim()
        }
    } finally {
        Remove-SmokeTempRoot -Path $captureRoot
    }
}

function Read-SmokeTextFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return ""
    }

    return [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
}

function Wait-ForPath {
    param(
        [string]$Path,
        [int]$TimeoutSeconds = 20
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        if (Test-Path -LiteralPath $Path) {
            return
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    throw "Timed out waiting for path: $Path"
}

function Start-LoomCloudApiFixtureJob {
    param(
        [int]$Port,
        [string]$OutputDir,
        [int]$MaxRequests = 3
    )

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $readyPath = Join-Path $OutputDir "ready.txt"

    return Start-Job -ArgumentList $Port, $OutputDir, $readyPath, $MaxRequests -ScriptBlock {
        param(
            [int]$FixturePort,
            [string]$FixtureOutputDir,
            [string]$FixtureReadyPath,
            [int]$FixtureMaxRequests
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"

        function Read-LoomCloudRequest {
            param([System.Net.Sockets.NetworkStream]$Stream)

            $buffer = New-Object byte[] 1024
            $bytes = New-Object System.Collections.Generic.List[byte]
            while ($true) {
                $read = $Stream.Read($buffer, 0, $buffer.Length)
                if ($read -le 0) {
                    break
                }
                for ($i = 0; $i -lt $read; $i++) {
                    $bytes.Add($buffer[$i])
                }

                $raw = [System.Text.Encoding]::UTF8.GetString($bytes.ToArray())
                $headerEnd = $raw.IndexOf("`r`n`r`n")
                if ($headerEnd -lt 0) {
                    continue
                }

                $headers = $raw.Substring(0, $headerEnd)
                $contentLength = 0
                foreach ($line in ($headers -split "`r`n")) {
                    $parts = $line.Split(":", 2)
                    if ($parts.Count -eq 2 -and $parts[0].Trim().Equals("content-length", [System.StringComparison]::OrdinalIgnoreCase)) {
                        $contentLength = [int]$parts[1].Trim()
                    }
                }

                $body = $raw.Substring($headerEnd + 4)
                if (($bytes.Count - ($headerEnd + 4)) -ge $contentLength) {
                    $requestLine = (($headers -split "`r`n") | Select-Object -First 1)
                    $requestParts = $requestLine.Split(" ")
                    return [ordered]@{
                        raw = $raw
                        path = [string]$requestParts[1]
                        body = [string]$body
                    }
                }
            }

            return [ordered]@{
                raw = ""
                path = "/"
                body = ""
            }
        }

        function Get-LoomJsonProperty {
            param(
                [object]$Object,
                [string]$Name
            )

            if ($null -eq $Object) {
                return $null
            }
            $property = $Object.PSObject.Properties[$Name]
            if ($null -eq $property) {
                return $null
            }
            return $property.Value
        }

        function Write-LoomCloudJsonResponse {
            param(
                [System.Net.Sockets.NetworkStream]$Stream,
                [string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $header = "HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
            $Stream.Write($headerBytes, 0, $headerBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), $FixturePort)
        $listener.Start()
        try {
            [System.IO.File]::WriteAllText($FixtureReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
            for ($index = 0; $index -lt $FixtureMaxRequests; $index++) {
                $client = $listener.AcceptTcpClient()
                try {
                    $stream = $client.GetStream()
                    $stream.ReadTimeout = 30000
                    $request = Read-LoomCloudRequest -Stream $stream
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request-$index.txt"),
                        [string]$request["raw"],
                        [System.Text.UTF8Encoding]::new($false)
                    )
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request-$index.json"),
                        [string]$request["body"],
                        [System.Text.UTF8Encoding]::new($false)
                    )

                    $payload = $null
                    $bodyText = [string]$request["body"]
                    $bodyTrimmed = $bodyText.TrimStart()
                    if (
                        -not [string]::IsNullOrWhiteSpace($bodyText) -and
                        ($bodyTrimmed.StartsWith("{") -or $bodyTrimmed.StartsWith("["))
                    ) {
                        $payload = $bodyText | ConvertFrom-Json
                    }

                    if ([string]$request["path"] -like "/multipart/*") {
                        $raw = [string]$request["raw"]
                        $rawLower = $raw.ToLowerInvariant()
                        $evidence = [ordered]@{
                            path = [string]$request["path"]
                            multipartSeen = $rawLower.Contains("content-type: multipart/form-data; boundary=")
                            fileFieldSeen = $raw.Contains('name="file"')
                            tempFilenameSeen = $raw.Contains('filename="loom-cloud-input-')
                            promptSeen = $raw.Contains("release cloud multipart")
                            traceSeen = $rawLower.Contains("x-trace: release-trace")
                            unresolvedTemplateSeen = $raw.Contains("{{")
                        }
                        [System.IO.File]::WriteAllText(
                            (Join-Path $FixtureOutputDir "multipart-request.json"),
                            ($evidence | ConvertTo-Json -Depth 20 -Compress),
                            [System.Text.UTF8Encoding]::new($false)
                        )
                        $response = [ordered]@{
                            content = @(
                                [ordered]@{
                                    type = "image"
                                    data = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
                                    mimeType = "image/png"
                                }
                            )
                        }
                    } elseif ([string]$request["path"] -eq "/text") {
                        $prompt = [string](Get-LoomJsonProperty -Object $payload -Name "prompt")
                        $response = [ordered]@{
                            content = @(
                                [ordered]@{
                                    type = "text"
                                    text = "cloud saw $prompt"
                                }
                            )
                        }
                    } else {
                        $imageData = Get-LoomJsonProperty -Object $payload -Name "input_base64"
                        $inputValue = Get-LoomJsonProperty -Object $payload -Name "input"
                        if ([string]::IsNullOrWhiteSpace([string]$imageData) -and $null -ne $inputValue -and -not ($inputValue -is [string])) {
                            $imageData = Get-LoomJsonProperty -Object $inputValue -Name "data"
                        }
                        if ([string]::IsNullOrWhiteSpace([string]$imageData)) {
                            $imageData = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
                        }
                        $response = [ordered]@{
                            content = @(
                                [ordered]@{
                                    type = "image"
                                    data = [string]$imageData
                                    mimeType = "image/png"
                                }
                            )
                        }
                    }

                    Write-LoomCloudJsonResponse -Stream $stream -Body ($response | ConvertTo-Json -Depth 20 -Compress)
                } finally {
                    $client.Close()
                }
            }
        } finally {
            $listener.Stop()
        }
    }
}

function Start-LoomTranslateFixtureJob {
    param(
        [int]$Port,
        [string]$OutputDir
    )

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $readyPath = Join-Path $OutputDir "ready.txt"

    return Start-Job -ArgumentList $Port, $OutputDir, $readyPath -ScriptBlock {
        param(
            [int]$FixturePort,
            [string]$FixtureOutputDir,
            [string]$FixtureReadyPath
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"

        function Read-LoomTranslateRequest {
            param([System.Net.Sockets.NetworkStream]$Stream)

            $buffer = New-Object byte[] 2048
            $bytes = New-Object System.Collections.Generic.List[byte]
            while ($true) {
                $read = $Stream.Read($buffer, 0, $buffer.Length)
                if ($read -le 0) {
                    break
                }
                for ($i = 0; $i -lt $read; $i++) {
                    $bytes.Add($buffer[$i])
                }
                $raw = [System.Text.Encoding]::UTF8.GetString($bytes.ToArray())
                $headerEnd = $raw.IndexOf("`r`n`r`n")
                if ($headerEnd -lt 0) {
                    continue
                }
                $headers = $raw.Substring(0, $headerEnd)
                $contentLength = 0
                foreach ($line in ($headers -split "`r`n")) {
                    $parts = $line.Split(":", 2)
                    if ($parts.Count -eq 2 -and $parts[0].Trim().Equals("content-length", [System.StringComparison]::OrdinalIgnoreCase)) {
                        $contentLength = [int]$parts[1].Trim()
                    }
                }
                $body = $raw.Substring($headerEnd + 4)
                if (($bytes.Count - ($headerEnd + 4)) -ge $contentLength) {
                    return [ordered]@{
                        raw = $raw
                        body = [string]$body
                    }
                }
            }
            return [ordered]@{ raw = ""; body = "" }
        }

        function Write-LoomTranslateJsonResponse {
            param(
                [System.Net.Sockets.NetworkStream]$Stream,
                [string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $header = "HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
            $Stream.Write($headerBytes, 0, $headerBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), $FixturePort)
        $listener.Start()
        try {
            [System.IO.File]::WriteAllText($FixtureReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
            $client = $listener.AcceptTcpClient()
            try {
                $stream = $client.GetStream()
                $stream.ReadTimeout = 30000
                $request = Read-LoomTranslateRequest -Stream $stream
                [System.IO.File]::WriteAllText(
                    (Join-Path $FixtureOutputDir "translate-request.txt"),
                    [string]$request.raw,
                    [System.Text.UTF8Encoding]::new($false)
                )
                [System.IO.File]::WriteAllText(
                    (Join-Path $FixtureOutputDir "translate-request.json"),
                    [string]$request.body,
                    [System.Text.UTF8Encoding]::new($false)
                )
                $payload = [string]$request.body | ConvertFrom-Json
                $text = [string]$payload.text
                $targetLang = [string]$payload.target_lang
                $response = [ordered]@{
                    code = 200
                    data = "translated:${text}:${targetLang}"
                }
                Write-LoomTranslateJsonResponse -Stream $stream -Body ($response | ConvertTo-Json -Depth 10 -Compress)
            } finally {
                $client.Close()
            }
        } finally {
            $listener.Stop()
        }
    }
}

function Start-LoomMcpRegistryFixtureJob {
    param(
        [int]$Port,
        [string]$OutputDir
    )

    New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
    $readyPath = Join-Path $OutputDir "ready.txt"

    return Start-Job -ArgumentList $Port, $OutputDir, $readyPath -ScriptBlock {
        param(
            [int]$FixturePort,
            [string]$FixtureOutputDir,
            [string]$FixtureReadyPath
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = "Stop"

        function Read-LoomMcpRegistryRequest {
            param([System.Net.Sockets.NetworkStream]$Stream)

            $buffer = New-Object byte[] 2048
            $bytes = New-Object System.Collections.Generic.List[byte]
            while ($true) {
                $read = $Stream.Read($buffer, 0, $buffer.Length)
                if ($read -le 0) {
                    break
                }
                for ($i = 0; $i -lt $read; $i++) {
                    $bytes.Add($buffer[$i])
                }
                $raw = [System.Text.Encoding]::UTF8.GetString($bytes.ToArray())
                if ($raw.Contains("`r`n`r`n")) {
                    return $raw
                }
            }
            return ""
        }

        function Write-LoomMcpRegistryJsonResponse {
            param(
                [System.Net.Sockets.NetworkStream]$Stream,
                [string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $header = "HTTP/1.1 200 OK`r`nContent-Type: application/json`r`nContent-Length: $($bodyBytes.Length)`r`nConnection: close`r`n`r`n"
            $headerBytes = [System.Text.Encoding]::ASCII.GetBytes($header)
            $Stream.Write($headerBytes, 0, $headerBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), $FixturePort)
        $listener.Start()
        try {
            [System.IO.File]::WriteAllText($FixtureReadyPath, "ready", [System.Text.UTF8Encoding]::new($false))
            for ($requestIndex = 1; $requestIndex -le 2; $requestIndex++) {
                $client = $listener.AcceptTcpClient()
                try {
                    $stream = $client.GetStream()
                    $stream.ReadTimeout = 30000
                    $request = Read-LoomMcpRegistryRequest -Stream $stream
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request.txt"),
                        $request,
                        [System.Text.UTF8Encoding]::new($false)
                    )
                    [System.IO.File]::WriteAllText(
                        (Join-Path $FixtureOutputDir "request-$requestIndex.txt"),
                        $request,
                        [System.Text.UTF8Encoding]::new($false)
                    )
                    $body = '{"servers":[{"server":{"name":"io.modelcontextprotocol/fixture","title":"Fixture MCP","description":"Fixture registry server","packages":[{"registryType":"npm","identifier":"@fixture/mcp","version":"1.0.0","transport":{"type":"stdio"},"runtimeArguments":[{"value":"-y"}],"environmentVariables":[{"name":"FIXTURE_API_KEY","isRequired":true}]}]},"_meta":{"io.modelcontextprotocol.registry/official":{"status":"active","isLatest":true,"updatedAt":"2026-06-12T00:00:00Z"}}}],"metadata":{"count":1}}'
                    Write-LoomMcpRegistryJsonResponse -Stream $stream -Body $body
                } finally {
                    $client.Close()
                }
            }
        } finally {
            $listener.Stop()
        }
    }
}

function Test-LoomRelease {
    $loomExe = $null
    $loomDaemonExe = $null
    $loomDesktopExe = $null
    $process = $null
    $tokenProcess = $null
    $cloudJob = $null
    $mcpRegistryJob = $null
    $translateJob = $null
    $realOcrImage = $null
    $tempRoot = New-SmokeTempRoot -Prefix "loom-release-smoke"
    try {
        $cliExtractRoot = Join-Path $tempRoot "cli"
        $layout = Get-LoomReleaseLayout -PackageDir $PackageDir -CliExtractRoot $cliExtractRoot
        $loomExe = $layout.cliExe
        $loomDaemonExe = $layout.daemonExe
        $loomDesktopExe = $layout.desktopExe
        Assert-PathExists $loomExe
        Assert-PathExists $loomDaemonExe
        Assert-PathExists $loomDesktopExe

        $helpRun = Invoke-ProcessCapture -FilePath $loomExe -ArgumentList @("--help") -TimeoutSeconds 30
        $helpText = [string]$helpRun.output
        if ($helpRun.exitCode -ne 0) {
            throw "loom CLI --help failed with exit code $($helpRun.exitCode) output=$(Redact-SmokeFailureText -Text $helpText)"
        }
        Assert-Contains "Usage: loom" $helpText "Loom CLI help output mismatch."

        $versionRun = Invoke-ProcessCapture -FilePath $loomExe -ArgumentList @("--version") -TimeoutSeconds 30
        $versionText = [string]$versionRun.output
        if ($versionRun.exitCode -ne 0) {
            throw "loom CLI --version failed with exit code $($versionRun.exitCode) output=$(Redact-SmokeFailureText -Text $versionText)"
        }
        Assert-Contains "loom " $versionText "Loom CLI version output mismatch."

        $daemonVersionRun = Invoke-ProcessCapture -FilePath $loomDaemonExe -ArgumentList @("--version") -TimeoutSeconds 30
        $daemonVersionText = [string]$daemonVersionRun.output
        if ($daemonVersionRun.exitCode -ne 0) {
            throw "loom-daemon.exe --version failed with exit code $($daemonVersionRun.exitCode) output=$(Redact-SmokeFailureText -Text $daemonVersionText)"
        }
        Assert-Contains "loom-daemon " $daemonVersionText "Loom daemon version output mismatch."

        $port = Get-FreePort
        $manifestDir = Join-Path $tempRoot "capabilities"
        New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
        $stdout = Join-Path $tempRoot "loom-daemon.stdout.log"
        $stderr = Join-Path $tempRoot "loom-daemon.stderr.log"
        $controlPlaneRoot = Join-Path $tempRoot "loom-control-plane"
        $oldHost = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_HOST", "Process")
        $oldPort = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_PORT", "Process")
        $oldControlPlaneRoot = [Environment]::GetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", "Process")
        $oldOcrFixtureText = [Environment]::GetEnvironmentVariable("LOOM_OCR_FIXTURE_TEXT", "Process")
        $oldLoomPython = [Environment]::GetEnvironmentVariable("LOOM_PYTHON", "Process")
        $oldMcpRegistryEndpoint = [Environment]::GetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", "Process")
        $oldTranslateEndpoint = [Environment]::GetEnvironmentVariable("LOOM_TRANSLATE_ENDPOINT", "Process")
        $mcpRegistryFixtureDir = Join-Path $tempRoot "mcp-registry-fixture"
        $mcpRegistryPort = Get-FreePort
        $mcpRegistryJob = Start-LoomMcpRegistryFixtureJob -Port $mcpRegistryPort -OutputDir $mcpRegistryFixtureDir
        Wait-ForPath -Path (Join-Path $mcpRegistryFixtureDir "ready.txt") -TimeoutSeconds 20
        $translateFixtureDir = Join-Path $tempRoot "translate-fixture"
        $translatePort = Get-FreePort
        $translateJob = Start-LoomTranslateFixtureJob -Port $translatePort -OutputDir $translateFixtureDir
        Wait-ForPath -Path (Join-Path $translateFixtureDir "ready.txt") -TimeoutSeconds 20
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", "127.0.0.1", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", [string]$port, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $controlPlaneRoot, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_OCR_FIXTURE_TEXT", "release loom ocr", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_PYTHON", $null, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", "http://127.0.0.1:$mcpRegistryPort/v0/servers", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_TRANSLATE_ENDPOINT", "http://127.0.0.1:$translatePort/translate", "Process")
        try {
            $process = Start-SmokeProcess `
                -FilePath $loomDaemonExe `
                -ArgumentList @("--manifest-dir", $manifestDir) `
                -StdoutPath $stdout `
                -StderrPath $stderr
        } finally {
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", $oldHost, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", $oldPort, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $oldControlPlaneRoot, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_OCR_FIXTURE_TEXT", $oldOcrFixtureText, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_PYTHON", $oldLoomPython, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", $oldMcpRegistryEndpoint, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_TRANSLATE_ENDPOINT", $oldTranslateEndpoint, "Process")
        }

        $manifest = Wait-ForFileJson -Path (Join-Path $manifestDir "loom.json")
        Assert-Equal "loom" $manifest.appId "Loom manifest appId mismatch."
        Assert-Equal "Loom" $manifest.displayName "Loom manifest displayName mismatch."
        Assert-Equal "http" $manifest.transport.type "Loom manifest transport type mismatch."
        Assert-Equal "none" $manifest.transport.auth "Loom manifest auth mismatch."
        $manifestCapabilityIds = @($manifest.capabilities) -join ","
        Assert-Equal $ExpectedLoomCapabilityIds $manifestCapabilityIds "Loom manifest capability list mismatch."

        $baseUrl = [string]$manifest.transport.baseUrl
        Assert-Equal "http://127.0.0.1:$port" $baseUrl "Loom manifest baseUrl mismatch."
        $health = $null
        $deadline = (Get-Date).AddSeconds(20)
        do {
            try {
                $health = Invoke-JsonGet -Uri "$baseUrl/health"
                break
            } catch {
                Start-Sleep -Milliseconds 150
            }
        } while ((Get-Date) -lt $deadline)

        if ($null -eq $health) {
            throw "Timed out waiting for Loom daemon on $baseUrl"
        }

        Assert-Equal "ok" $health.status "Loom health status mismatch."

        $status = Invoke-JsonGet -Uri "$baseUrl/status"
        Assert-Equal "ready" $status.status "Loom daemon status mismatch."

        $capabilities = Invoke-JsonGet -Uri "$baseUrl/v1/capabilities"
        $capabilityIds = @($capabilities.capabilities | ForEach-Object { $_.id }) -join ","
        Assert-Equal $ExpectedLoomCapabilityIds $capabilityIds "Loom capability list mismatch."

        $mcpServers = Invoke-JsonGet -Uri "$baseUrl/v1/mcp/servers"
        Assert-Equal 0 (@($mcpServers.servers).Count) "Loom MCP server list should start empty in isolated smoke root."
        $savedMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/release-mcp" -Body @{
            id = "release-mcp"
            name = "Release MCP"
            command = "npx"
            args = @("-y", "@example/release-mcp")
            env = @{}
            enabled = $true
        }
        Assert-Equal "release-mcp" $savedMcpServer.server.id "Loom MCP server save id mismatch."
        $mcpServersAfterSave = Invoke-JsonGet -Uri "$baseUrl/v1/mcp/servers"
        Assert-Equal "release-mcp" ([string]$mcpServersAfterSave.servers[0].id) "Loom MCP server list id mismatch."

        $fixtureMcpScript = Join-Path $tempRoot "fixture-mcp-server.ps1"
        $fixtureMcpSource = @'
$ErrorActionPreference = "Stop"
while ($null -ne ($line = [Console]::In.ReadLine())) {
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }
    $request = $line | ConvertFrom-Json
    if ($request.method -eq "initialize") {
        $response = [ordered]@{
            jsonrpc = "2.0"
            id = $request.id
            result = [ordered]@{
                protocolVersion = "2024-11-05"
                capabilities = @{ tools = @{} }
                serverInfo = [ordered]@{
                    name = "release-fixture"
                    version = "0.1.0"
                }
            }
        }
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
        [Console]::Out.Flush()
        continue
    }
    if ($request.method -eq "notifications/initialized") {
        continue
    }
    if ($request.method -eq "tools/list") {
        $response = [ordered]@{
            jsonrpc = "2.0"
            id = $request.id
            result = [ordered]@{
                tools = @(
                    [ordered]@{
                        name = "echo"
                        description = "Echo arguments"
                        inputSchema = [ordered]@{
                            type = "object"
                            properties = [ordered]@{
                                text = [ordered]@{ type = "string" }
                            }
                        }
                    }
                )
            }
        }
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
        [Console]::Out.Flush()
        continue
    }
    if ($request.method -eq "tools/call") {
        $response = [ordered]@{
            jsonrpc = "2.0"
            id = $request.id
            result = [ordered]@{
                content = @(
                    [ordered]@{
                        type = "text"
                        text = [string]$request.params.arguments.text
                    }
                )
            }
        }
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
        [Console]::Out.Flush()
        continue
    }
    $errorResponse = [ordered]@{
        jsonrpc = "2.0"
        id = $request.id
        error = [ordered]@{
            code = -32601
            message = "unknown method $($request.method)"
        }
    }
    [Console]::Out.WriteLine(($errorResponse | ConvertTo-Json -Depth 20 -Compress))
    [Console]::Out.Flush()
}
'@
        [System.IO.File]::WriteAllText($fixtureMcpScript, $fixtureMcpSource, [System.Text.UTF8Encoding]::new($false))

        # GET /v1/mcp/registry?search=fixture
        $mcpRegistry = Invoke-JsonGet -Uri "$baseUrl/v1/mcp/registry?search=fixture&limit=250&cursor=cursor-1"
        Assert-Equal "io.modelcontextprotocol/fixture" ([string]$mcpRegistry.servers[0].server.name) "Loom MCP Registry server name mismatch."
        $mcpRegistryRequestPath = Join-Path $mcpRegistryFixtureDir "request.txt"
        Assert-PathExists $mcpRegistryRequestPath
        $mcpRegistryRequest = Get-Content -Raw -LiteralPath $mcpRegistryRequestPath
        Assert-Contains "GET /v0/servers?limit=100&search=fixture&cursor=cursor-1" $mcpRegistryRequest "Loom MCP Registry request URL mismatch."

        # POST /v1/mcp/test
        $mcpConnectionTest = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/test" -Body @{
            id = "fixture-test"
            name = "Fixture Test MCP"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal $true ([bool]$mcpConnectionTest.success) "Loom MCP connection test success mismatch."
        Assert-Equal "test_mcp_connection" ([string]$mcpConnectionTest.compatCommand) "Loom MCP connection test compat command mismatch."
        Assert-Equal "echo" ([string]$mcpConnectionTest.tools[0].name) "Loom MCP connection test tool name mismatch."
        Assert-Equal "release-fixture" ([string]$mcpConnectionTest.server_info.serverInfo.name) "Loom MCP connection test server info mismatch."
        $mcpDirectCompat = Test-LoomMcpDirectCompat `
            -BaseUrl $baseUrl `
            -FixtureMcpScript $fixtureMcpScript
        $artLoomMcpServerStoreCompat = Test-LoomArtLoomMcpServerStoreCompat -BaseUrl $baseUrl

        # POST /v1/mcp/package/check and POST /v1/mcp/package/install-plan
        $mcpPackageCheck = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/package/check" -Body @{
            moduleName = "json"
        }
        Assert-Equal "check_mcp_package_installed" ([string]$mcpPackageCheck.compatCommand) "Loom MCP package check compat command mismatch."
        Assert-Equal "json" ([string]$mcpPackageCheck.module) "Loom MCP package check module mismatch."
        $mcpPackageInstallPlan = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/package/install-plan" -Body @{
            packageName = "mcp-server-demo"
        }
        Assert-Equal "install_mcp_package" ([string]$mcpPackageInstallPlan.compatCommand) "Loom MCP package install plan compat command mismatch."
        Assert-Equal $false ([bool]$mcpPackageInstallPlan.sideEffect) "Loom MCP package install plan must be side-effect free."
        Assert-Contains "pip" (($mcpPackageInstallPlan.command | ForEach-Object { [string]$_ }) -join " ") "Loom MCP package install plan should include pip command."

        $fixtureScriptArt = Join-Path $tempRoot "fixture-script-art.ps1"
        $fixtureScriptSource = @'
$ErrorActionPreference = "Stop"
$payload = $args[0] | ConvertFrom-Json
$arguments = $payload.arguments
if ($arguments.mode -eq "shader") {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "text"
                text = "void fragment() { COLOR = vec4(1.0); }"
            }
        )
    }
} elseif ($arguments.input_base64) {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = [string]$arguments.input_base64
                mimeType = "image/png"
            }
        )
    }
} elseif ($arguments.input -and $arguments.input.data) {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = [string]$arguments.input.data
                mimeType = "image/png"
            }
        )
    }
} else {
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "text"
                text = "script saw $($arguments.text)"
            }
        )
    }
}
        [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
'@
        [System.IO.File]::WriteAllText($fixtureScriptArt, $fixtureScriptSource, [System.Text.UTF8Encoding]::new($false))

        $cloudFixtureDir = Join-Path $tempRoot "cloud-api-fixture"
        $cloudPort = Get-FreePort
        $cloudJob = Start-LoomCloudApiFixtureJob -Port $cloudPort -OutputDir $cloudFixtureDir -MaxRequests 4
        Wait-ForPath -Path (Join-Path $cloudFixtureDir "ready.txt") -TimeoutSeconds 20

        $savedFixtureMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/fixture" -Body @{
            id = "fixture"
            name = "Fixture MCP"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal "fixture" $savedFixtureMcpServer.server.id "Loom fixture MCP server save id mismatch."

        $savedDeleteMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/fixture-delete" -Body @{
            id = "fixture-delete"
            name = "Fixture Delete MCP"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal "fixture-delete" $savedDeleteMcpServer.server.id "Loom delete MCP server save id mismatch."
        # DELETE /v1/mcp/servers/fixture-delete
        $deletedMcpServer = Invoke-JsonDelete -Uri "$baseUrl/v1/mcp/servers/fixture-delete"
        Assert-Equal $true ([bool]$deletedMcpServer.deleted) "Loom MCP server deletion mismatch."

        $savedTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/release-workflow-tool" -Body @{
            id = "release-workflow-tool"
            name = "Release Workflow Tool"
            description = "Release smoke workflow-backed tool"
            enabled = $true
            execution = @{
                type = "workflow"
                workflowId = "release-workflow"
            }
        }
        Assert-Equal "release-workflow-tool" $savedTool.tool.id "Loom registry save id mismatch."
        Assert-Equal "workflow" $savedTool.tool.execution.type "Loom registry execution type mismatch."
        $tools = Invoke-JsonGet -Uri "$baseUrl/v1/tools"
        Assert-Equal "release-workflow-tool" ([string]$tools.tools[0].id) "Loom registry list id mismatch."

        $savedDeleteTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-delete-tool" -Body @{
            id = "fixture-delete-tool"
            name = "Fixture Delete Tool"
            description = "Release smoke deletable registry tool"
            enabled = $true
            execution = @{
                type = "script"
                path = $fixtureScriptArt
            }
        }
        Assert-Equal "fixture-delete-tool" $savedDeleteTool.tool.id "Loom delete tool save id mismatch."
        # DELETE /v1/tools/fixture-delete-tool
        $deletedTool = Invoke-JsonDelete -Uri "$baseUrl/v1/tools/fixture-delete-tool"
        Assert-Equal $true ([bool]$deletedTool.deleted) "Loom registry tool deletion mismatch."

        $savedMcpTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-echo" -Body @{
            id = "fixture-echo"
            name = "Fixture Echo"
            description = "Release smoke MCP-backed tool"
            enabled = $true
            execution = @{
                type = "mcp"
                serverId = "fixture"
                toolName = "echo"
            }
        }
        Assert-Equal "fixture-echo" $savedMcpTool.tool.id "Loom MCP-backed tool save id mismatch."
        $executedMcpTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/fixture-echo/execute" -Body @{
            arguments = @{
                text = "release mcp runtime"
            }
        }
        Assert-Equal "succeeded" $executedMcpTool.status "Loom MCP-backed tool execution status mismatch."
        Assert-Equal "release mcp runtime" ([string]$executedMcpTool.result.content[0].text) "Loom MCP-backed tool execution content mismatch."

        $savedScriptTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-script-art" -Body @{
            id = "fixture-script-art"
            name = "Fixture Script Art"
            description = "Release smoke script-backed image Art"
            enabled = $true
            execution = @{
                type = "script"
                path = $fixtureScriptArt
            }
        }
        Assert-Equal "fixture-script-art" $savedScriptTool.tool.id "Loom script-backed tool save id mismatch."
        Assert-Equal "script" $savedScriptTool.tool.execution.type "Loom script-backed execution type mismatch."
        $executedScriptTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/fixture-script-art/execute" -Body @{
            arguments = @{
                text = "release script runtime"
            }
        }
        Assert-Equal "succeeded" $executedScriptTool.status "Loom script-backed tool execution status mismatch."
        Assert-Equal "script saw release script runtime" ([string]$executedScriptTool.result.content[0].text) "Loom script-backed tool execution content mismatch."

        $pythonToolExecution = Test-LoomPackagedPythonScriptTool `
            -BaseUrl $baseUrl `
            -PackageDir $PackageDir `
            -TempRoot $tempRoot
        $pythonArtCatalog = Test-LoomPythonArtCatalog `
            -BaseUrl $baseUrl `
            -PackageDir $PackageDir
        $artLoomRegistryCompat = Test-LoomArtLoomRegistryCompat -BaseUrl $baseUrl
        $artLoomNativeProcessArt = Test-LoomArtLoomNativeProcessArtCompat -BaseUrl $baseUrl
        $artLoomSystemCompat = Test-LoomArtLoomSystemCompat -BaseUrl $baseUrl
        $pythonEngineCompat = Test-LoomPythonEngineCompat -BaseUrl $baseUrl
        $pythonDirectCompat = Test-LoomPythonDirectCompat `
            -BaseUrl $baseUrl `
            -TempRoot $tempRoot
        $pythonArtSourceImport = Test-LoomPythonArtSourceImport `
            -BaseUrl $baseUrl `
            -TempRoot $tempRoot
        $artLoomPythonSourceCompat = Test-LoomArtLoomPythonSourceCompat `
            -BaseUrl $baseUrl `
            -TempRoot $tempRoot
        $pythonArtToolExecution = Invoke-JsonPost -Uri "$baseUrl/v1/tools/fixture-python-art/execute" -Body @{
            arguments = @{
                text = "release installed python art"
            }
        }
        Assert-Equal "succeeded" $pythonArtToolExecution.status "Loom Python Art repeated execution status mismatch."
        Assert-Equal "python art saw release installed python art" ([string]$pythonArtToolExecution.result.content[0].text) "Loom Python Art repeated execution content mismatch."

        $savedCloudTextTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-cloud-text" -Body @{
            id = "fixture-cloud-text"
            name = "Fixture Cloud Text"
            description = "Release smoke cloud API text tool"
            enabled = $true
            execution = @{
                type = "cloud_api"
                endpoint = "http://127.0.0.1:$cloudPort/text"
                method = "POST"
            }
        }
        Assert-Equal "fixture-cloud-text" $savedCloudTextTool.tool.id "Loom cloud API text tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudTextTool.tool.execution.type "Loom cloud API text execution type mismatch."
        $executedCloudTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/fixture-cloud-text/execute" -Body @{
            arguments = @{
                prompt = "release cloud runtime"
            }
        }
        Assert-Equal "succeeded" $executedCloudTool.status "Loom cloud API tool execution status mismatch."
        Assert-Equal "cloud saw release cloud runtime" ([string]$executedCloudTool.result.content[0].text) "Loom cloud API tool execution content mismatch."

        $savedCloudArtTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-cloud-art" -Body @{
            id = "fixture-cloud-art"
            name = "Fixture Cloud Art"
            description = "Release smoke cloud API image Art"
            enabled = $true
            execution = @{
                type = "cloud_api"
                endpoint = "http://127.0.0.1:$cloudPort/process"
                method = "POST"
            }
        }
        Assert-Equal "fixture-cloud-art" $savedCloudArtTool.tool.id "Loom cloud API Art tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudArtTool.tool.execution.type "Loom cloud API Art execution type mismatch."

        $savedCloudMultipartArtTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-cloud-multipart-art" -Body @{
            id = "fixture-cloud-multipart-art"
            name = "Fixture Cloud Multipart Art"
            description = "Release smoke old ArtLoom multipart cloud API image Art"
            enabled = $true
            execution = @{
                type = "cloud_api"
                url = "http://127.0.0.1:$cloudPort/multipart/{{inputs.route.value}}"
                method = "POST"
                contentType = "multipart/form-data"
                headers = '{"X-Trace":"{{inputs.trace.value}}"}'
                body = '{"file":"{{inputs.input.path}}","prompt":"{{inputs.prompt.value}}"}'
            }
        }
        Assert-Equal "fixture-cloud-multipart-art" $savedCloudMultipartArtTool.tool.id "Loom cloud multipart Art tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudMultipartArtTool.tool.execution.type "Loom cloud multipart Art execution type mismatch."
        Assert-Equal "multipart/form-data" ([string]$savedCloudMultipartArtTool.tool.execution.contentType) "Loom cloud multipart contentType save mismatch."

        $savedScriptShaderTool = Invoke-JsonPut -Uri "$baseUrl/v1/tools/fixture-script-shader" -Body @{
            id = "fixture-script-shader"
            name = "Fixture Script Shader"
            description = "Release smoke script-backed shader Art"
            enabled = $true
            execution = @{
                type = "script"
                path = $fixtureScriptArt
            }
        }
        Assert-Equal "fixture-script-shader" $savedScriptShaderTool.tool.id "Loom script shader tool save id mismatch."

        $workflowYaml = @"
name: Release Workflow Runtime
nodes:
  - id: image
    uses: fixture-script-art
    with:
      text: release workflow runtime
"@
        $savedWorkflow = Invoke-JsonPut -Uri "$baseUrl/v1/workflows/release-workflow" -Body @{
            data = $workflowYaml
        }
        Assert-Equal "release-workflow" $savedWorkflow.workflow.id "Loom workflow save id mismatch."
        Assert-Equal 1 ([int]$savedWorkflow.workflow.nodeCount) "Loom workflow node count mismatch."
        $workflows = Invoke-JsonGet -Uri "$baseUrl/v1/workflows"
        Assert-Equal "release-workflow" ([string]$workflows.workflows[0].id) "Loom workflow list id mismatch."

        # GET /v1/workflows/release-workflow
        $loadedWorkflow = Invoke-JsonGet -Uri "$baseUrl/v1/workflows/release-workflow"
        Assert-Equal "release-workflow" ([string]$loadedWorkflow.workflow.id) "Loom workflow load id mismatch."
        Assert-Contains "Release Workflow Runtime" ([string]$loadedWorkflow.workflow.data) "Loom workflow load data mismatch."

        $deleteWorkflowYaml = @"
name: Delete Workflow Runtime
nodes:
  - id: image
    uses: fixture-script-art
    with:
      text: delete workflow runtime
"@
        $savedDeleteWorkflow = Invoke-JsonPut -Uri "$baseUrl/v1/workflows/fixture-delete-workflow" -Body @{
            data = $deleteWorkflowYaml
        }
        Assert-Equal "fixture-delete-workflow" ([string]$savedDeleteWorkflow.workflow.id) "Loom delete workflow save id mismatch."
        # DELETE /v1/workflows/fixture-delete-workflow
        $deletedWorkflow = Invoke-JsonDelete -Uri "$baseUrl/v1/workflows/fixture-delete-workflow"
        Assert-Equal $true ([bool]$deletedWorkflow.deleted) "Loom workflow deletion mismatch."
        $artLoomWorkflowStoreCompat = Test-LoomArtLoomWorkflowStoreCompat -BaseUrl $baseUrl

        $executedWorkflowTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/release-workflow-tool/execute" -Body @{
            arguments = @{}
        }
        Assert-Equal "succeeded" $executedWorkflowTool.status "Loom workflow-backed tool execution status mismatch."
        Assert-Equal "text" ([string]$executedWorkflowTool.result.content[0].type) "Loom workflow-backed tool content type mismatch."
        Assert-Equal "script saw release workflow runtime" ([string]$executedWorkflowTool.result.content[0].text) "Loom workflow-backed tool execution content mismatch."

        $imageHelperConvert = Test-LoomImageHelperConvert -BaseUrl $baseUrl
        $sharedMemoryCompat = Test-LoomArtLoomSharedMemoryCompat -BaseUrl $baseUrl

        $hookBridge = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/status"
        Assert-Equal 19820 ([int]$hookBridge.port) "Loom Hook Bridge compatibility port mismatch."
        Assert-Equal $false ([bool]$hookBridge.running) "Loom Hook Bridge protocol-only smoke state mismatch."
        $hookBridgeMethods = @($hookBridge.methods | ForEach-Object { [string]$_ })
        $legacyHookSessionMethod = "read_art" + "hook_session"
        Assert-Contains "art_loom/update_workflow_node" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing workflow node update."
        Assert-Contains "art_hook/instantiate" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing legacy instantiate."
        Assert-Contains $legacyHookSessionMethod ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing legacy session reader."
        $hookSessionCompat = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/session"
        Assert-Equal $legacyHookSessionMethod ([string]$hookSessionCompat.method) "Loom Hook session compatibility method mismatch."
        Assert-Equal "artloom-compat" ([string]$hookSessionCompat.protocol) "Loom Hook session protocol mismatch."
        $hookSessionStickerCount = @($hookSessionCompat.session.stickers).Count
        $hookSessionLinkCount = @($hookSessionCompat.session.links).Count
        if ($hookSessionStickerCount -lt 0 -or $hookSessionLinkCount -lt 0) {
            throw "Loom Hook session counts should be readable."
        }

        $hookBridgeStarted = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/start" -Body @{
            port = 0
        }
        Assert-Equal $true ([bool]$hookBridgeStarted.running) "Loom Hook Bridge start state mismatch."
        if ([int]$hookBridgeStarted.port -le 0) {
            throw "Loom Hook Bridge start should allocate a port."
        }
        $hookBridgeRunning = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/status"
        Assert-Equal $true ([bool]$hookBridgeRunning.running) "Loom Hook Bridge running status mismatch."
        Assert-Equal ([int]$hookBridgeStarted.port) ([int]$hookBridgeRunning.port) "Loom Hook Bridge running port mismatch."
        $websocketHandshake = Connect-LoomHookBridgeWebSocket -Port ([int]$hookBridgeStarted.port)
        Assert-Equal "handshake" ([string]$websocketHandshake.type) "Loom Hook Bridge WebSocket handshake response type mismatch."
        Assert-Equal "0.1.0" ([string]$websocketHandshake.data.server_version) "Loom Hook Bridge WebSocket server version mismatch."
        if ([string]::IsNullOrWhiteSpace([string]$websocketHandshake.data.session_id)) {
            throw "Loom Hook Bridge WebSocket handshake session_id missing."
        }
        $hookBridgeSettings = Test-LoomHookBridgeSettingsCompatibility `
            -Port ([int]$hookBridgeStarted.port) `
            -TranslateFixtureDir $translateFixtureDir
        $websocketBroadcast = Test-LoomHookBridgeWebSocketBroadcast -Port ([int]$hookBridgeStarted.port)
        $artLoomIpcWorkflowCompat = Test-LoomArtLoomIpcWorkflowCompat `
            -BaseUrl $baseUrl `
            -Port ([int]$hookBridgeStarted.port)
        $hookLiveWorkflow = Test-LoomHookLiveWorkflowPersistence -Port ([int]$hookBridgeStarted.port) -BaseUrl $baseUrl
        $executeArtNode = Test-LoomHookBridgeExecuteArtNode -Port ([int]$hookBridgeStarted.port)
        $ahrpProcess = Test-LoomHookBridgeAhrpProcess -Port ([int]$hookBridgeStarted.port)
        $nativeImageFilter = Test-LoomHookBridgeNativeImageFilter -Port ([int]$hookBridgeStarted.port)
        $sharedImageAhrpProcess = Test-LoomHookBridgeSharedImageAhrpProcess -Port ([int]$hookBridgeStarted.port) -BaseUrl $baseUrl
        $ocrImage = Test-LoomHookBridgeOcrImage -Port ([int]$hookBridgeStarted.port)
        $scriptArtNode = Test-LoomHookBridgeScriptArtNode -Port ([int]$hookBridgeStarted.port)
        $scriptAhrpProcess = Test-LoomHookBridgeScriptAhrpProcess -Port ([int]$hookBridgeStarted.port)
        $scriptShaderArt = Test-LoomHookBridgeScriptShaderArt -Port ([int]$hookBridgeStarted.port)
        $cloudArtNode = Test-LoomHookBridgeCloudArtNode -Port ([int]$hookBridgeStarted.port)
        $cloudAhrpProcess = Test-LoomHookBridgeCloudAhrpProcess -Port ([int]$hookBridgeStarted.port)
        $cloudMultipartArtNode = Test-LoomHookBridgeCloudMultipartArtNode -Port ([int]$hookBridgeStarted.port) -FixtureOutputDir $cloudFixtureDir
        $workflowArtNode = Test-LoomHookBridgeWorkflowArtNode -Port ([int]$hookBridgeStarted.port)
        $workflowAhrpProcess = Test-LoomHookBridgeWorkflowAhrpProcess -Port ([int]$hookBridgeStarted.port)
        $hookBridgeStopped = Invoke-JsonPost -Uri "$baseUrl/v1/hook-bridge/stop" -Body @{}
        Assert-Equal $false ([bool]$hookBridgeStopped.running) "Loom Hook Bridge stop state mismatch."

        $completedCloudJob = Wait-Job -Job $cloudJob -Timeout 20
        if ($null -eq $completedCloudJob) {
            throw "Timed out waiting for Loom cloud API fixture job."
        }
        try {
            Receive-Job -Job $completedCloudJob -ErrorAction Stop | Out-Null
        } catch {
            throw "Loom cloud API fixture job reported errors: $($_.Exception.Message)"
        }
        Remove-Job -Job $cloudJob -Force -ErrorAction SilentlyContinue
        $cloudJob = $null

        $invoke = Invoke-JsonPost -Uri "$baseUrl/v1/invoke" -Body @{
            requestId = "release-loom-1"
            caller = "hook"
            capability = "brain.plan"
            input = @{
                goal = "release smoke"
                constraints = @("Hook Talk Loom")
            }
        }
        Assert-Equal "succeeded" $invoke.status "Loom invoke status mismatch."

        $runId = [string]$invoke.output.runId
        $run = Invoke-JsonGet -Uri "$baseUrl/v1/runs/$runId"
        Assert-Equal "brain.plan" $run.capability "Loom run capability mismatch."

        $events = Invoke-JsonGet -Uri "$baseUrl/v1/runs/$runId/events"
        $eventKinds = @($events.events | ForEach-Object { $_.kind }) -join ","
        Assert-Equal "run_started,capability_completed" $eventKinds "Loom run event kinds mismatch."

        Stop-SpawnedProcess $process
        $process = $null

        $realOcrImage = Test-LoomReleaseRealOcr `
            -LoomDaemonExe $loomDaemonExe `
            -PackageDir $PackageDir `
            -TempRoot $tempRoot

        $tokenPort = Get-FreePort
        $tokenValue = "release-smoke-token-$PID"
        $tokenManifestDir = Join-Path $tempRoot "tokenized-capabilities"
        New-Item -ItemType Directory -Force -Path $tokenManifestDir | Out-Null
        $tokenStdout = Join-Path $tempRoot "loom-daemon-token.stdout.log"
        $tokenStderr = Join-Path $tempRoot "loom-daemon-token.stderr.log"
        $tokenControlPlaneRoot = Join-Path $tempRoot "loom-token-control-plane"
        $oldHost = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_HOST", "Process")
        $oldPort = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_PORT", "Process")
        $oldToken = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_TOKEN", "Process")
        $oldControlPlaneRoot = [Environment]::GetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", "127.0.0.1", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", [string]$tokenPort, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $tokenValue, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $tokenControlPlaneRoot, "Process")
        try {
            $tokenProcess = Start-SmokeProcess `
                -FilePath $loomDaemonExe `
                -ArgumentList @("--manifest-dir", $tokenManifestDir) `
                -StdoutPath $tokenStdout `
                -StderrPath $tokenStderr
        } finally {
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", $oldHost, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", $oldPort, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_DAEMON_TOKEN", $oldToken, "Process")
            [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $oldControlPlaneRoot, "Process")
        }

        $tokenManifest = Wait-ForFileJson -Path (Join-Path $tokenManifestDir "loom.json")
        Assert-Equal "loom" $tokenManifest.appId "Tokenized Loom manifest appId mismatch."
        Assert-Equal "bearer" $tokenManifest.transport.auth "Tokenized Loom manifest auth mismatch."
        Assert-Equal $tokenValue $tokenManifest.transport.authToken "Tokenized Loom manifest authToken mismatch."
        $tokenBaseUrl = [string]$tokenManifest.transport.baseUrl
        Assert-Equal "http://127.0.0.1:$tokenPort" $tokenBaseUrl "Tokenized Loom manifest baseUrl mismatch."

        $tokenHealth = $null
        $tokenDeadline = (Get-Date).AddSeconds(20)
        do {
            try {
                $tokenHealth = Invoke-JsonGet -Uri "$tokenBaseUrl/health"
                break
            } catch {
                Start-Sleep -Milliseconds 150
            }
        } while ((Get-Date) -lt $tokenDeadline)
        if ($null -eq $tokenHealth) {
            throw "Timed out waiting for tokenized Loom daemon on $tokenBaseUrl"
        }
        Assert-Equal "ok" $tokenHealth.status "Tokenized Loom health status mismatch."

        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/capabilities" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/mcp/servers" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/mcp/registry?search=fixture" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/mcp/test" -Method "Post" -Body @{ id = "fixture-test" } -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/mcp/servers/fixture-delete" -Method "Delete" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/tools/fixture-delete-tool" -Method "Delete" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/python-arts" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/workflows" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/workflows/fixture-delete-workflow" -Method "Delete" -ExpectedStatus 401
        Assert-HttpStatus -Uri "$tokenBaseUrl/v1/hook-bridge/status" -ExpectedStatus 401
        Assert-HttpStatus `
            -Uri "$tokenBaseUrl/v1/hook-bridge/start" `
            -Method "Post" `
            -ExpectedStatus 401 `
            -Body @{
                port = 0
            }
        Assert-HttpStatus `
            -Uri "$tokenBaseUrl/v1/invoke" `
            -Method "Post" `
            -ExpectedStatus 401 `
            -Body @{
                requestId = "release-loom-token-unauthorized"
                caller = "hook"
                capability = "brain.plan"
                input = @{
                    goal = "tokenized release smoke unauthorized"
                }
            }

        $tokenHeaders = @{ Authorization = "Bearer $tokenValue" }
        $tokenCapabilities = Invoke-JsonGet -Uri "$tokenBaseUrl/v1/capabilities" -Headers $tokenHeaders
        $tokenCapabilityIds = @($tokenCapabilities.capabilities | ForEach-Object { $_.id }) -join ","
        Assert-Equal $ExpectedLoomCapabilityIds $tokenCapabilityIds "Tokenized Loom capability list mismatch."
        $tokenHookBridge = Invoke-JsonGet -Uri "$tokenBaseUrl/v1/hook-bridge/status" -Headers $tokenHeaders
        Assert-Equal 19820 ([int]$tokenHookBridge.port) "Tokenized Loom Hook Bridge status port mismatch."
        $tokenHookBridgeStarted = Invoke-JsonPost -Uri "$tokenBaseUrl/v1/hook-bridge/start" -Headers $tokenHeaders -Body @{
            port = 0
        }
        Assert-Equal $true ([bool]$tokenHookBridgeStarted.running) "Tokenized Loom Hook Bridge start mismatch."
        $tokenHookBridgeStopped = Invoke-JsonPost -Uri "$tokenBaseUrl/v1/hook-bridge/stop" -Headers $tokenHeaders -Body @{}
        Assert-Equal $false ([bool]$tokenHookBridgeStopped.running) "Tokenized Loom Hook Bridge stop mismatch."

        $tokenInvoke = Invoke-JsonPost -Uri "$tokenBaseUrl/v1/invoke" -Headers $tokenHeaders -Body @{
            requestId = "release-loom-token-1"
            caller = "hook"
            capability = "brain.plan"
            input = @{
                goal = "tokenized release smoke"
            }
        }
        Assert-Equal "succeeded" $tokenInvoke.status "Tokenized Loom invoke status mismatch."

        return [ordered]@{
            app = "Loom"
            exes = @($loomDesktopExe, $loomDaemonExe)
            cliExe = $loomExe
            desktopExe = $loomDesktopExe
            version = $versionText.Trim()
            daemonVersion = $daemonVersionText.Trim()
            manifestAppId = $manifest.appId
            health = $health.status
            status = $status.status
            capabilities = $capabilityIds
            controlPlane = [ordered]@{
                mcpServerId = [string]$mcpServersAfterSave.servers[0].id
                toolId = [string]$tools.tools[0].id
                workflowId = [string]$workflows.workflows[0].id
                hookBridgePort = [int]$hookBridge.port
                hookBridgeRuntimePort = [int]$hookBridgeStarted.port
                hookBridgeMethods = $hookBridgeMethods
                mcpToolExecution = [string]$executedMcpTool.result.content[0].text
                scriptToolExecution = [string]$executedScriptTool.result.content[0].text
                pythonToolExecution = $pythonToolExecution
                pythonArtCatalog = $pythonArtCatalog
                pythonEngineCompat = $pythonEngineCompat
                pythonDirectCompat = $pythonDirectCompat
                mcpDirectCompat = $mcpDirectCompat
                artLoomMcpServerStoreCompat = $artLoomMcpServerStoreCompat
                artLoomRegistryCompat = $artLoomRegistryCompat
                artLoomNativeProcessArt = $artLoomNativeProcessArt
                artLoomSystemCompat = $artLoomSystemCompat
                artLoomWorkflowStoreCompat = $artLoomWorkflowStoreCompat
                pythonArtSourceImport = $pythonArtSourceImport
                artLoomPythonSourceCompat = $artLoomPythonSourceCompat
                pythonArtToolExecution = [string]$pythonArtToolExecution.result.content[0].text
                managementCrud = [ordered]@{
                    mcpServerDeleted = [bool]$deletedMcpServer.deleted
                    toolDeleted = [bool]$deletedTool.deleted
                    workflowLoaded = [string]$loadedWorkflow.workflow.id
                    workflowDeleted = [bool]$deletedWorkflow.deleted
                }
                mcpMarketplace = [ordered]@{
                    registryServerCount = @($mcpRegistry.servers).Count
                    registryServerName = [string]$mcpRegistry.servers[0].server.name
                    connectionTestCommand = [string]$mcpConnectionTest.compatCommand
                    connectionTestSuccess = [bool]$mcpConnectionTest.success
                    connectionTestTool = [string]$mcpConnectionTest.tools[0].name
                    connectionTestServer = [string]$mcpConnectionTest.server_info.serverInfo.name
                    packageCheckCommand = [string]$mcpPackageCheck.compatCommand
                    packageCheckModule = [string]$mcpPackageCheck.module
                    packageInstallCommand = [string]$mcpPackageInstallPlan.compatCommand
                    packageInstallSideEffect = [bool]$mcpPackageInstallPlan.sideEffect
                }
                cloudToolExecution = [string]$executedCloudTool.result.content[0].text
                workflowToolExecution = [string]$executedWorkflowTool.result.content[0].text
                websocketHandshake = [ordered]@{
                    type = [string]$websocketHandshake.type
                    serverVersion = [string]$websocketHandshake.data.server_version
                    hasSessionId = -not [string]::IsNullOrWhiteSpace([string]$websocketHandshake.data.session_id)
                }
                hookBridgeSettings = $hookBridgeSettings
                hookSessionCompat = [ordered]@{
                    method = [string]$hookSessionCompat.method
                    protocol = [string]$hookSessionCompat.protocol
                    available = [bool]$hookSessionCompat.available
                    stickerCount = $hookSessionStickerCount
                    linkCount = $hookSessionLinkCount
                }
                websocketBroadcast = $websocketBroadcast
                artLoomIpcWorkflowCompat = $artLoomIpcWorkflowCompat
                hookLiveWorkflow = $hookLiveWorkflow
                executeArtNode = $executeArtNode
                ahrpProcess = $ahrpProcess
                nativeImageFilter = $nativeImageFilter
                sharedMemoryCompat = $sharedMemoryCompat
                sharedImageAhrpProcess = $sharedImageAhrpProcess
                imageHelperConvert = $imageHelperConvert
                ocrImage = $ocrImage
                realOcrImage = $realOcrImage
                scriptArtNode = $scriptArtNode
                scriptAhrpProcess = $scriptAhrpProcess
                scriptShaderArt = $scriptShaderArt
                cloudArtNode = $cloudArtNode
                cloudAhrpProcess = $cloudAhrpProcess
                cloudMultipartArtNode = $cloudMultipartArtNode
                workflowArtNode = $workflowArtNode
                workflowAhrpProcess = $workflowAhrpProcess
            }
            invoke = $invoke.status
            runCapability = $run.capability
            eventKinds = $eventKinds
            tokenizedAuth = $tokenManifest.transport.auth
            tokenizedCapabilities = $tokenCapabilityIds
            tokenizedHookBridgePort = [int]$tokenHookBridge.port
            tokenizedHookBridgeRuntimePort = [int]$tokenHookBridgeStarted.port
            tokenizedInvoke = $tokenInvoke.status
        }
    } catch {
        Save-SmokeFailureEvidence -TempRoot $tempRoot -Label "loom-release" | Out-Null
        throw
    } finally {
        Stop-SpawnedProcess $process
        Stop-SpawnedProcess $tokenProcess
        if ($null -ne $cloudJob) {
            Stop-Job -Job $cloudJob -ErrorAction SilentlyContinue
            Remove-Job -Job $cloudJob -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $mcpRegistryJob) {
            Stop-Job -Job $mcpRegistryJob -ErrorAction SilentlyContinue
            Remove-Job -Job $mcpRegistryJob -Force -ErrorAction SilentlyContinue
        }
        if ($null -ne $translateJob) {
            Stop-Job -Job $translateJob -ErrorAction SilentlyContinue
            Remove-Job -Job $translateJob -Force -ErrorAction SilentlyContinue
        }
        Start-Sleep -Milliseconds 300
        Remove-SmokeTempRoot -Path $tempRoot
    }
}

function Wait-LoomDaemonHealth {
    param(
        [string]$BaseUrl,
        [string]$Message
    )

    $deadline = (Get-Date).AddSeconds(20)
    $health = $null
    do {
        try {
            $health = Invoke-JsonGet -Uri "$BaseUrl/health"
            break
        } catch {
            Start-Sleep -Milliseconds 150
        }
    } while ((Get-Date) -lt $deadline)

    if ($null -eq $health) {
        throw "$Message on $BaseUrl"
    }

    Assert-Equal "ok" $health.status "$Message status mismatch."
    return $health
}

function Invoke-FocusedLoomSmoke {
    param(
        [string]$ScriptName,
        [string]$EvidenceSubdirectory
    )

    $scriptPath = Join-Path $PSScriptRoot $ScriptName
    Assert-PathExists $scriptPath
    $focusedEvidenceRoot = Join-Path $EvidenceRoot $EvidenceSubdirectory
    $output = & powershell.exe -NoProfile -ExecutionPolicy Bypass `
        -File $scriptPath `
        -PackageDir $PackageDir `
        -EvidenceRoot $focusedEvidenceRoot 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Focused Loom smoke failed ($ScriptName): $(Redact-SmokeFailureText -Text ($output -join [Environment]::NewLine))"
    }
    return [ordered]@{
        script = $ScriptName
        status = "passed"
        evidenceRoot = $focusedEvidenceRoot
    }
}

Initialize-SmokeEvidenceRun
$localResult = Test-LoomRelease
$focusedResults = @(
    Invoke-FocusedLoomSmoke `
        -ScriptName "Invoke-LoomGatewayBrainPlanSmoke.ps1" `
        -EvidenceSubdirectory "gateway"
    Invoke-FocusedLoomSmoke `
        -ScriptName "Invoke-LoomRunPersistenceSmoke.ps1" `
        -EvidenceSubdirectory "persistence"
    Invoke-FocusedLoomSmoke `
        -ScriptName "Invoke-LoomDaemonConcurrencySmoke.ps1" `
        -EvidenceSubdirectory "concurrency"
)

$safeVersion = $VersionId -replace "[^A-Za-z0-9._-]", "_"
$summaryFileName = "loom-release-$safeVersion-summary.json"
$summary = [ordered]@{
    schemaVersion = 1
    mode = "loom-release-smoke"
    status = "passed"
    versionId = $VersionId
    packageDir = $PackageDir
    evidenceRunId = $script:SmokeEvidenceRunId
    evidenceRunDir = $script:SmokeEvidenceRunDir
    summaryEvidencePath = $null
    summaryLatestEvidencePath = $null
    local = $localResult
    focused = $focusedResults
}
$summary.summaryEvidencePath = Write-SmokeJsonEvidence -FileName $summaryFileName -Value $summary
$summary.summaryLatestEvidencePath = Write-SmokeJsonEvidence -FileName $summaryFileName -Value $summary -Latest
Write-SmokeJsonEvidence -FileName $summaryFileName -Value $summary | Out-Null
Write-SmokeJsonEvidence -FileName $summaryFileName -Value $summary -Latest | Out-Null
$summary | ConvertTo-Json -Depth 40
