[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$moduleRoot = Join-Path $repoRoot "scripts\smoke-release"
$VersionId = "smoke-module-contract"
$resolvedApps = @("Loom")
$ExpectedLoomCapabilityIds = "brain.plan,tea.ticket.decompose.v1,tea.ticket.execute.v1,tea.ticket.review.v1"
$script:SmokeEvidenceRunId = ""
$script:SmokeEvidenceRunDir = ""
$script:DaemonAuthHeaders = @{}
$EvidenceRoot = Join-Path $env:TEMP "loom-smoke-module-evidence-$PID-$([Guid]::NewGuid().ToString('N'))"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -ne $Actual) { throw "$Message Expected=[$Expected] Actual=[$Actual]" }
}

foreach ($moduleName in @(
    "Assertions.ps1",
    "Image.ps1",
    "HttpStatus.ps1",
    "ProcessTree.ps1",
    "Evidence.ps1",
    "Process.ps1",
    "CloudFixture.ps1",
    "McpRegistryFixture.ps1",
    "ReleasePhases.ps1",
    "Release.ps1",
    "Focused.ps1"
)) {
    . (Join-Path $moduleRoot $moduleName)
}

try {
foreach ($uri in @("http://127.0.0.1:4200/health", "http://localhost:4200/health", "http://[::1]:4200/health")) {
    Assert-SmokeLoopbackUri -Uri $uri
}
foreach ($uri in @("https://example.com/", "file:///C:/Windows/win.ini", "http://user@127.0.0.1:4200/")) {
    $rejected = $false
    try { Assert-SmokeLoopbackUri -Uri $uri } catch { $rejected = $true }
    Assert-True $rejected "Non-loopback or ambiguous smoke URI was accepted: $uri"
}

$secret = "module-secret-$([Guid]::NewGuid().ToString('N'))"
$sensitiveText = @"
{"authToken":"$secret","apiKey":"$secret","password":"$secret","cookie":"$secret","message":"Bearer $secret"}
token = "$secret"
Authorization: $secret
http://127.0.0.1:4200/status?access_token=$secret
"@
$redactedText = Redact-SmokeJsonContent -Content $sensitiveText
Assert-True (-not $redactedText.Contains($secret)) "Smoke redaction retained a secret value."
Assert-True $redactedText.Contains("<redacted>") "Smoke redaction did not emit its marker."
Assert-True (Test-SmokeSensitiveEvidencePath -RelativePath "control-plane\daemon-token") "Daemon token path was not classified as sensitive."
Assert-True (Test-SmokeSensitiveEvidencePath -RelativePath "credentials\state.json") "Credential directory was not classified as sensitive."
Assert-True (-not (Test-SmokeSensitiveEvidencePath -RelativePath "logs\tokenizer.log")) "Unrelated tokenizer log was classified as a secret."

$invalidEvidenceNameRejected = $false
try { Write-SmokeJsonEvidence -FileName "..\escape.json" -Value @{} | Out-Null } catch { $invalidEvidenceNameRejected = $true }
Assert-True $invalidEvidenceNameRejected "Smoke evidence writer accepted a path traversal file name."

$failureRoot = New-SmokeTempRoot -Prefix "loom-module-failure"
try {
    $controlPlaneRoot = Join-Path $failureRoot "control-plane"
    New-Item -ItemType Directory -Path $controlPlaneRoot | Out-Null
    [System.IO.File]::WriteAllText((Join-Path $controlPlaneRoot "daemon-token"), $secret, [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $failureRoot "runtime.log"), "Authorization: Bearer $secret", [System.Text.UTF8Encoding]::new($false))
    $failureEvidencePath = Save-SmokeFailureEvidence -TempRoot $failureRoot -Label "module" 3>$null
    Assert-Equal 0 @(Get-ChildItem -LiteralPath $failureEvidencePath -Recurse -File | Where-Object { $_.Name -eq "daemon-token" }).Count "Failure evidence copied the daemon token file."
    $savedLog = [System.IO.File]::ReadAllText((Join-Path $failureEvidencePath "runtime.log"), [System.Text.Encoding]::UTF8)
    Assert-True (-not $savedLog.Contains($secret)) "Failure evidence retained a bearer token."
} finally {
    Remove-SmokeTempRoot -Path $failureRoot
}

$protectedTempRoot = [System.IO.Path]::GetFullPath($env:TEMP)
Remove-SmokeTempRoot -Path $protectedTempRoot 3>$null
Assert-True (Test-Path -LiteralPath $protectedTempRoot -PathType Container) "Smoke cleanup removed the TEMP root."

$powerShellExe = Join-Path $PSHOME "powershell.exe"
$captureFixtureRoot = New-SmokeTempRoot -Prefix "loom-module-capture"
try {
    $captureFixturePath = Join-Path $captureFixtureRoot "capture-fixture.ps1"
    [System.IO.File]::WriteAllText(
        $captureFixturePath,
        '[Console]::Out.WriteLine("fixture stdout"); [Console]::Error.WriteLine("fixture stderr"); exit 7',
        [System.Text.UTF8Encoding]::new($false)
    )
    $capture = Invoke-ProcessCapture `
        -FilePath $powerShellExe `
        -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $captureFixturePath) `
        -TimeoutSeconds 20
    Assert-Equal 7 ([int]$capture.exitCode) "Direct smoke process capture lost the child exit code."
    Assert-True ([string]$capture.stdout).Contains("fixture stdout") "Direct smoke process capture lost stdout."
    Assert-True ([string]$capture.stderr).Contains("fixture stderr") "Direct smoke process capture lost stderr."
} finally {
    Remove-SmokeTempRoot -Path $captureFixtureRoot
}

$PackageDir = $repoRoot
$originalInvokeProcessCapture = ${function:Invoke-ProcessCapture}
try {
    $script:FocusedCapture = $null
    function Invoke-ProcessCapture {
        param(
            [string]$FilePath,
            [string[]]$ArgumentList,
            [int]$TimeoutSeconds
        )
        $script:FocusedCapture = [ordered]@{
            filePath = $FilePath
            argumentList = @($ArgumentList)
            timeoutSeconds = $TimeoutSeconds
        }
        return [pscustomobject]@{ exitCode = 0; output = ""; stdout = ""; stderr = "" }
    }

    foreach ($focusedName in @(
        "Invoke-LoomGatewayBrainPlanSmoke.ps1",
        "Invoke-LoomRunPersistenceSmoke.ps1",
        "Invoke-LoomDaemonConcurrencySmoke.ps1"
    )) {
        $expectedFocusedPath = Join-Path (Join-Path $repoRoot "scripts") $focusedName
        $null = Invoke-FocusedLoomSmoke -ScriptName $focusedName -EvidenceSubdirectory "contract"
        Assert-Equal $expectedFocusedPath ([string]$script:FocusedCapture.argumentList[4]) "Focused smoke resolved outside the repository scripts root."
        Assert-Equal 300 ([int]$script:FocusedCapture.timeoutSeconds) "Focused smoke timeout contract changed."
    }
}
finally {
    Set-Item -Path Function:\Invoke-ProcessCapture -Value $originalInvokeProcessCapture
}

$processSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $moduleRoot "Process.ps1")
foreach ($forbidden in @("Get-SmokeCmdExePath", "Start-SmokeCapturedProcess", "run.cmd", "cmd.exe")) {
    Assert-True (-not $processSource.Contains($forbidden)) "Smoke process capture retained a command wrapper: $forbidden"
}
$cloudFixtureSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $moduleRoot "CloudFixture.ps1")
$mcpFixtureSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $moduleRoot "McpRegistryFixture.ps1")
Assert-True $cloudFixtureSource.Contains("16 * 1024 * 1024") "Cloud fixture request cap is missing."
Assert-True $mcpFixtureSource.Contains('$maxRequestBytes = 1MB') "MCP Registry fixture request cap is missing."
Assert-True (-not $cloudFixtureSource.Contains('GetString($bytes.ToArray())')) "Cloud fixture restored repeated full-buffer decoding."
Assert-True (-not $mcpFixtureSource.Contains('GetString($bytes.ToArray())')) "MCP Registry fixture restored repeated full-buffer decoding."
Assert-True $cloudFixtureSource.Contains('GetString($requestBytes, $headerEnd + 4, $contentLength)') "Cloud fixture does not slice its UTF-8 body by byte Content-Length."
Assert-True (-not $cloudFixtureSource.Contains('Substring($headerEnd + 4)')) "Cloud fixture mixed a byte offset with a string offset."
$assertionsSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $moduleRoot "Assertions.ps1")
$httpStatusSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $moduleRoot "HttpStatus.ps1")
Assert-True $assertionsSource.Contains('-MaximumRedirection 0') "JSON HTTP helpers allow redirects beyond loopback."
Assert-True $assertionsSource.Contains('Smoke POST failed for ${Uri} (timeoutSec=$TimeoutSec)') "JSON POST diagnostics omit the endpoint or timeout."
Assert-True $httpStatusSource.Contains('-MaximumRedirection 0') "HTTP status helper allows redirects beyond loopback."
} finally {
    if (Test-Path -LiteralPath $EvidenceRoot) {
        Remove-Item -LiteralPath $EvidenceRoot -Recurse -Force
    }
}
Write-Output "Loom smoke release module contract passed."
