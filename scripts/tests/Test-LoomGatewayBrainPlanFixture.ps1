[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$entryPath = Join-Path $repoRoot "scripts\Invoke-LoomGatewayBrainPlanSmoke.ps1"
$fixturePath = Join-Path $repoRoot "scripts\LoomGatewayBrainPlanFixture.ps1"
. $fixturePath
. (Join-Path $repoRoot "scripts\LoomSmokePorts.ps1")

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -ne $Actual) { throw "$Message Expected=[$Expected] Actual=[$Actual]" }
}

foreach ($path in @($entryPath, $fixturePath, $PSCommandPath)) {
    $tokens = $null
    $parseErrors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($path, [ref]$tokens, [ref]$parseErrors)
    Assert-Equal 0 @($parseErrors).Count "PowerShell parse errors in $path."
    $bytes = [System.IO.File]::ReadAllBytes($path)
    Assert-True (-not ($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191)) "Unexpected UTF-8 BOM in $path."
    if ($path -eq $entryPath) {
        Assert-Equal "PackageDir,EvidenceRoot" (@($ast.ParamBlock.Parameters | ForEach-Object { $_.Name.VariablePath.UserPath }) -join ",") "Public smoke parameters changed."
    }
}

function Wait-FixtureReady {
    param([string]$Path, $Job)
    $deadline = (Get-Date).AddSeconds(15)
    while (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        Assert-True ($Job.State -ne "Failed") "Gateway fixture failed before ready."
        Assert-True ((Get-Date) -lt $deadline) "Gateway fixture did not become ready."
        Start-Sleep -Milliseconds 100
    }
}

function Send-FixtureRequest {
    param([int]$Port, [string]$Request)
    $client = [System.Net.Sockets.TcpClient]::new()
    $stream = $null
    $response = [System.IO.MemoryStream]::new()
    try {
        $client.Connect("127.0.0.1", $Port)
        $stream = $client.GetStream()
        $stream.ReadTimeout = 10000
        $stream.WriteTimeout = 10000
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Request)
        $stream.Write($bytes, 0, $bytes.Length)
        $client.Client.Shutdown([System.Net.Sockets.SocketShutdown]::Send)
        $buffer = New-Object byte[] 4096
        while (($count = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
            $response.Write($buffer, 0, $count)
        }
        return [System.Text.Encoding]::UTF8.GetString($response.ToArray())
    }
    finally {
        $response.Dispose()
        if ($null -ne $stream) { $stream.Dispose() }
        $client.Dispose()
    }
}

function Assert-FixtureStopped {
    param($Job, [int]$Port)
    Assert-Equal 0 @(Stop-LoomGatewayBrainPlanFixture -Job $Job -Port $Port).Count "Fixture cleanup emitted unexpected output."
    Assert-Equal 0 @(Get-Job | Where-Object { $_.Id -eq $Job.Id }).Count "Fixture job remained registered."
    $client = [System.Net.Sockets.TcpClient]::new()
    $connected = $false
    try {
        $client.Connect("127.0.0.1", $Port)
        $connected = $true
    }
    catch {
    }
    finally {
        $client.Dispose()
    }
    Assert-True (-not $connected) "Fixture listener remained open after cleanup."
}

$temporaryParent = Join-Path $repoRoot ".tmp"
New-Item -ItemType Directory -Force -Path $temporaryParent | Out-Null
Assert-True (((Get-Item -LiteralPath $temporaryParent).Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) "Test temporary parent must not be a reparse point."
$testRoot = Join-Path $temporaryParent "gateway-brain-plan-fixture-$PID-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $testRoot | Out-Null
$job = $null
$port = 0
try {
    foreach ($case in @("valid", "unicode", "invalid")) {
        $valid = $case -ne "invalid"
        $port = Get-LoomSmokePort
        $readyPath = Join-Path $testRoot "ready-$case"
        $capturePath = Join-Path $testRoot "capture-$case.json"
        $job = Start-LoomGatewayBrainPlanFixture -Port $port -ReadyPath $readyPath -CapturePath $capturePath
        Wait-FixtureReady -Path $readyPath -Job $job
        $userContent = '{"goal":"Verify packaged Gateway planning","context":{"source":"fixture-contract"}}'
        if ($case -eq "unicode") {
            $userContent = $userContent.Replace("planning", "planning $([char]0x2603)")
        }
        $body = [ordered]@{
            model = "smoke-planner"
            messages = @(
                @{ role = "system"; content = "Return a plan." },
                @{ role = "user"; content = $userContent }
            )
            stream = $false
        } | ConvertTo-Json -Depth 10 -Compress
        $token = if ($valid) { "smoke-token" } else { "wrong-token" }
        $bodyLength = [System.Text.Encoding]::UTF8.GetByteCount($body)
        $request = "POST /v1/chat/completions HTTP/1.1`r`nHost: 127.0.0.1`r`nAuthorization: Bearer $token`r`nContent-Length: $bodyLength`r`n`r`n$body"
        $response = Send-FixtureRequest -Port $port -Request $request
        Assert-True ($null -ne (Wait-Job -Job $job -Timeout 10)) "Fixture did not finish its single request."
        Assert-Equal "Completed" ([string]$job.State) "Fixture job failed."
        $captureText = [System.IO.File]::ReadAllText($capturePath)
        $capture = $captureText | ConvertFrom-Json
        Assert-Equal $valid ([bool]$capture.valid) "Fixture validity changed."
        Assert-Equal $valid ([bool]$capture.authReceived) "Fixture authorization evidence changed."
        Assert-Equal "POST" ([string]$capture.method) "Fixture method changed."
        Assert-Equal "/v1/chat/completions" ([string]$capture.path) "Fixture request path changed."
        Assert-Equal "smoke-planner" ([string]$capture.model) "Fixture model changed."
        Assert-Equal "system,user" (@($capture.messageRoles) -join ",") "Fixture message roles changed."
        Assert-Equal $userContent ([string]$capture.userContent) "Fixture user payload changed."
        Assert-True (-not $captureText.Contains($token)) "Fixture capture leaked the bearer token."
        $responseBody = ($response -split "`r`n`r`n", 2)[1] | ConvertFrom-Json
        if ($valid) {
            Assert-True $response.StartsWith("HTTP/1.1 200 OK`r`n") "Valid request did not receive HTTP 200."
            Assert-Equal "smoke-resolved-model" ([string]$responseBody.model) "Resolved model changed."
            $plan = $responseBody.choices[0].message.content | ConvertFrom-Json
            Assert-Equal "Packaged Gateway plan" ([string]$plan.summary) "Plan summary changed."
            Assert-Equal "inspect packaged request,execute packaged plan" (@($plan.steps) -join ",") "Plan steps changed."
        }
        else {
            Assert-True $response.StartsWith("HTTP/1.1 400 Bad Request`r`n") "Invalid request did not receive HTTP 400."
            Assert-Equal "invalid_smoke_request" ([string]$responseBody.error.code) "Invalid request error changed."
        }
        Assert-FixtureStopped -Job $job -Port $port
        $job = $null
    }

    $port = Get-LoomSmokePort
    $readyPath = Join-Path $testRoot "ready-idle"
    $capturePath = Join-Path $testRoot "capture-idle.json"
    $job = Start-LoomGatewayBrainPlanFixture -Port $port -ReadyPath $readyPath -CapturePath $capturePath
    Wait-FixtureReady -Path $readyPath -Job $job
    Assert-FixtureStopped -Job $job -Port $port
    $job = $null
    $capture = [System.IO.File]::ReadAllText($capturePath) | ConvertFrom-Json
    Assert-Equal $false ([bool]$capture.valid) "Incomplete request did not retain failure evidence."
    Assert-Equal "Gateway fixture received an incomplete HTTP request" ([string]$capture.error) "Incomplete request error changed."
    Assert-Equal 0 @(Stop-LoomGatewayBrainPlanFixture -Job $null -Port $port).Count "Null fixture cleanup changed."
}
finally {
    Stop-LoomGatewayBrainPlanFixture -Job $job -Port $port
    $resolvedRoot = (Resolve-Path -LiteralPath $repoRoot).Path
    $resolvedTarget = (Resolve-Path -LiteralPath $testRoot).Path
    Assert-True $resolvedTarget.StartsWith("$resolvedRoot\", [System.StringComparison]::OrdinalIgnoreCase) "Test cleanup escaped the repository."
    $ancestor = Get-Item -LiteralPath $resolvedTarget
    while ($ancestor.FullName -ne $resolvedRoot) {
        Assert-True (($ancestor.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -eq 0) "Test cleanup encountered a reparse point."
        $ancestor = $ancestor.Parent
    }
    Remove-Item -LiteralPath $resolvedTarget -Recurse -Force
}

Write-Output "Gateway ai.chat fixture protocol and cleanup contract passed."
