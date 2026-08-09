$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-PathExists {
    param([string]$Path)
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "Missing required file: $Path"
}

function Assert-Contains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True $Haystack.Contains($Needle) $Message
}

function Assert-NotContains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True (-not $Haystack.Contains($Needle)) $Message
}

function ConvertFrom-UnicodeCodePoints {
    param([int[]]$CodePoints)

    $characters = foreach ($codePoint in $CodePoints) {
        [char]$codePoint
    }
    return -join $characters
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$appPath = Join-Path $repoRoot "apps\desktop\src\App.tsx"
$thumbnailPath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasThumbnail.tsx"
$nodePath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasNode.tsx"
$desktopRustPath = Join-Path $repoRoot "apps\desktop\src-tauri\src\lib.rs"
$smokePath = Join-Path $repoRoot "scripts\Invoke-LoomHookCanvasUiSmoke.ps1"
$inspectorPath = Join-Path $repoRoot "scripts\Inspect-LoomWebView.mjs"

Assert-PathExists $thumbnailPath
Assert-PathExists $nodePath
Assert-PathExists $desktopRustPath

$app = Get-Content -Raw -Encoding UTF8 $appPath
$thumbnail = Get-Content -Raw -Encoding UTF8 $thumbnailPath
$node = Get-Content -Raw -Encoding UTF8 $nodePath
$desktopRust = Get-Content -Raw -Encoding UTF8 $desktopRustPath
$visualWorkflowLabel = ConvertFrom-UnicodeCodePoints @(0x6253, 0x5F00, 0x53EF, 0x89C6, 0x5316, 0x5DE5, 0x4F5C, 0x6D41)
$saveWorkflowLabel = ConvertFrom-UnicodeCodePoints @(0x4FDD, 0x5B58, 0x5DE5, 0x4F5C, 0x6D41)
$executionFailureLabel = ConvertFrom-UnicodeCodePoints @(0x6267, 0x884C, 0x5931, 0x8D25)
$quotaExceededErrorMessage = ConvertFrom-UnicodeCodePoints @(
    0x989D, 0x5EA6, 0x4E0D, 0x8DB3, 0xFF08, 0x0048, 0x0054, 0x0054, 0x0050,
    0x0020, 0x0034, 0x0030, 0x0032, 0xFF09
)

Assert-Contains 'data-testid=' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'nav-hook-bridge' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'data-testid="hook-canvas-thumbnail"' $thumbnail "Screenshot Sync must render a real Hook canvas thumbnail."
Assert-NotContains 'data-testid="hook-canvas-open-workflow"' $thumbnail "The removed visual-workflow entry must not return."
Assert-Contains 'data-testid="hook-canvas-node"' $node "Hook canvas nodes need stable smoke targets."
Assert-NotContains $visualWorkflowLabel $thumbnail "The removed visual-workflow label must not return."
Assert-Contains $saveWorkflowLabel $thumbnail "Hook Sync must expose the compact save-workflow action."
Assert-Contains 'const nextSnapshot = await waitForLoomOnline(refreshSnapshot);' $app "Daemon start must always poll the complete snapshot until online."
Assert-NotContains '? await waitForLoomOnline' $app "Daemon readiness polling must not depend on whether a new process was spawned."

Assert-PathExists $smokePath
Assert-PathExists $inspectorPath
$smoke = Get-Content -Raw -Encoding UTF8 $smokePath
$inspector = Get-Content -Raw -Encoding UTF8 $inspectorPath
Assert-Contains 'LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT' $smoke "Smoke must pass an isolated CDP port through Loom runtime configuration."
Assert-NotContains 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS' $smoke "Smoke must not rely on the WebView2 environment variable ignored by hosted runners."
Assert-Contains 'LOOM_WEBVIEW2_REMOTE_DEBUGGING_PORT_ENV' $desktopRust "Desktop must define the scoped WebView2 debugging port variable."
Assert-Contains 'configured_webview2_browser_args' $desktopRust "Desktop must validate and construct WebView2 browser arguments."
Assert-Contains 'context.config_mut().app.windows' $desktopRust "Desktop must inject browser arguments before Tauri creates its configured windows."
Assert-Contains 'additional_browser_args' $desktopRust "Desktop must pass explicit browser arguments through Tauri/Wry."
Assert-Contains 'LOOM_CONTROL_PLANE_ROOT' $smoke "Smoke must isolate Loom data."
Assert-Contains 'APPDATA' $smoke "Smoke must isolate the Hook session."
Assert-Contains 'ExpectedExecutablePath' $smoke "Smoke cleanup must validate exact process paths."
Assert-Contains 'SmokePortMinimum = 30000' $smoke "Smoke listeners must stay below the Windows dynamic client-port range."
Assert-Contains 'SmokePortMaximum = 45000' $smoke "Smoke listeners must stay below the Windows dynamic client-port range."
Assert-Contains 'id = "failed-art"' $smoke "Hook canvas smoke fixture must include a failed Art node."
Assert-Contains 'status = "error"' $smoke "Hook canvas smoke fixture must preserve the failed Art node status."
Assert-Contains '$quotaExceededErrorMessage = ConvertFrom-UnicodeCodePoints' $smoke "Hook canvas smoke must define the failed Art error message via ASCII-safe Unicode code points."
Assert-Contains 'errorMessage = $quotaExceededErrorMessage' $smoke "Hook canvas smoke fixture must preserve the failed Art node error reason."
Assert-Contains 'Wait-ForHookCanvasUi' $smoke "Smoke must wait on Hook canvas DOM conditions."
Assert-Contains '[int]$TimeoutSeconds = 90' $smoke "Hosted WebView2 startup needs a bounded 90-second Hook canvas wait budget."
Assert-Contains 'Inspector diagnostic:' $smoke "Hook canvas timeouts must include the latest Inspector diagnostic."
Assert-Contains 'Get-CimInstance Win32_Process' $smoke "Hook canvas failures must report the desktop WebView2 process tree."
Assert-Contains '-OperationTimeoutSec 2' $smoke "Hook canvas process diagnostics must have a bounded CIM operation timeout."
Assert-Contains 'GetActiveTcpListeners()' $smoke "Hook canvas failures must use a direct TCP listener snapshot."
Assert-NotContains 'Get-NetTCPConnection' $smoke "Hook canvas failure diagnostics must not block on the NetTCP CIM provider."
Assert-Contains 'Desktop stderr:' $smoke "Hook canvas failures must include desktop stderr."
Assert-Contains '-MaxLength 1800' $smoke "Hook canvas process-tree diagnostics must have a reserved section budget."
Assert-Contains '-MaxLength 500' $smoke "Hook canvas listener diagnostics must have a reserved section budget."
Assert-Contains '-MaxLength 900' $smoke "Hook canvas desktop stderr must have a reserved section budget."
Assert-Contains 'Runtime diagnostic:' $smoke "Hook canvas failures must label the reserved runtime diagnostic budget."
Assert-Contains 'Primary failure:' $smoke "Hook canvas failures must label the bounded primary exception."
$runtimeDiagnosticIndex = $smoke.IndexOf('"Runtime diagnostic: "')
$primaryFailureIndex = $smoke.IndexOf('"Primary failure: "')
Assert-True ($runtimeDiagnosticIndex -ge 0 -and $primaryFailureIndex -gt $runtimeDiagnosticIndex) "Runtime diagnostics must precede the primary exception so truncation cannot remove them."
Assert-Contains 'function Limit-SmokeText' $smoke "Hook canvas smoke output must have a bounded diagnostic size."
Assert-Contains '[Math]::Min(20' $smoke "Each Inspector invocation must be capped at 20 seconds within the remaining budget."
Assert-Contains '-TimeoutSeconds $inspectorTimeoutSeconds' $smoke "Inspector child-process timeout must use the remaining Hook canvas budget."
Assert-Contains '[System.Diagnostics.ProcessStartInfo]::new()' $smoke "Inspector must use the .NET process API for reliable Windows PowerShell exit codes."
Assert-Contains 'ReadToEndAsync()' $smoke "Inspector stdout and stderr must be drained asynchronously."
Assert-Contains '$exitCode = $process.ExitCode' $smoke "Inspector must capture a concrete .NET process exit code."
Assert-Contains '$process.WaitForExit($TimeoutSeconds * 1000)' $smoke "Hook canvas smoke must wait on the Inspector process with a hard timeout."
Assert-NotContains '$process.WaitForExit()' $smoke "Inspector cleanup must not contain an unbounded process wait."
Assert-Contains 'function Read-BoundedTaskText' $smoke "Inspector stream draining must use a bounded task wait."
Assert-Contains '$Task.Wait($TimeoutMilliseconds)' $smoke "Inspector stream draining must not block indefinitely."
Assert-NotContains 'Start-Sleep -Seconds 2' $smoke "Smoke must not use a fixed Hook canvas refresh delay."
Assert-Contains 'failedArtThumbnailFailureVisible' $smoke "Hook canvas smoke must assert the failed Art thumbnail presentation."
Assert-Contains 'min-nodes' $inspector "Inspector must wait for the expected Hook canvas node count."
Assert-Contains 'data-revision' $inspector "Inspector must wait for a non-empty Hook canvas revision."
Assert-NotContains 'hook-canvas-open-workflow' $inspector "Inspector must not target the removed visual-workflow entry."
Assert-Contains ('placeholderTitle === "' + $executionFailureLabel + '"') $inspector "Inspector must detect the failed Art execution-failure placeholder."
Assert-Contains 'thumbnailNodes' $inspector "Inspector must persist thumbnail node presentation evidence."
Assert-Contains 'placeholderDetailText' $inspector "Inspector must persist failed-node detail text evidence."
Assert-Contains "hasImage: Boolean(node.querySelector('img'))" $inspector "Inspector must distinguish image previews from placeholder rendering."
Assert-Contains 'let diagnostic = {};' $inspector "Inspector must capture diagnostics before the initial canvas wait."
Assert-Contains 'let client = null;' $inspector "Inspector must capture failures before a CDP client exists."
Assert-Contains 'AbortController' $inspector "Inspector HTTP probes must have a timeout."
Assert-Contains 'command(method, params = {}, timeoutMs = 10000)' $inspector "Inspector CDP commands must have a timeout."
Assert-Contains 'setTimeout' $inspector "Inspector WebSocket operations must have a timeout."
Assert-Contains 'if (client) client.close();' $inspector "Inspector must close a partially initialized CDP client."
Assert-Contains 'this.socket.readyState === WebSocket.OPEN' $inspector "Inspector socket cleanup must only close an open WebSocket."
Assert-Contains 'try {' $inspector "Inspector socket cleanup must not replace a completed result with a close error."
Assert-Contains 'await fs.writeFile(args.output, `${JSON.stringify(failure, null, 2)}\n`, "utf8");' $inspector "Inspector failures must persist diagnostic JSON."
Assert-Contains 'await new Promise((resolve, reject) => {' $inspector "Inspector must flush successful stdout before exiting."
Assert-Contains '.then(() => process.exit(0))' $inspector "Inspector CLI must terminate after successful evidence writes."
Assert-Contains 'process.stderr.write(message, () => {' $inspector "Inspector CLI must flush stderr before failure exit."
Assert-Contains 'setTimeout(() => process.exit(1), 1000);' $inspector "Inspector CLI failure flush must retain a bounded exit fallback."
Assert-NotContains 'setTimeout(() => process.exit(1), 50);' $inspector "Inspector CLI must not use a fixed delay as a stderr flush proxy."

function Get-SmokeFunctionDefinition {
    param(
        [System.Management.Automation.Language.ScriptBlockAst]$Ast,
        [string]$Name
    )

    $definition = $Ast.Find({
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $Name
    }, $true)
    Assert-True ($null -ne $definition) "Missing smoke function for runtime contract: $Name"
    return [scriptblock]::Create($definition.Extent.Text)
}

$tokens = $null
$parseErrors = $null
$smokeAst = [System.Management.Automation.Language.Parser]::ParseFile($smokePath, [ref]$tokens, [ref]$parseErrors)
Assert-True (@($parseErrors).Count -eq 0) "Hook canvas smoke must parse before runtime process tests."
. (Get-SmokeFunctionDefinition -Ast $smokeAst -Name "Limit-SmokeText")
. (Get-SmokeFunctionDefinition -Ast $smokeAst -Name "Write-Utf8NoBom")
. (Get-SmokeFunctionDefinition -Ast $smokeAst -Name "Read-BoundedTaskText")
. (Get-SmokeFunctionDefinition -Ast $smokeAst -Name "Get-HookCanvasFailureDiagnostic")
. (Get-SmokeFunctionDefinition -Ast $smokeAst -Name "Invoke-Inspector")

$fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-inspector-contract-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null
try {
    $exitFixturePath = Join-Path $fixtureRoot "exit-fixture.mjs"
    $hangFixturePath = Join-Path $fixtureRoot "hang-fixture.mjs"
    $fixtureEncoding = New-Object System.Text.UTF8Encoding($false)
    $diagnosticStderrPath = Join-Path $fixtureRoot "desktop.stderr.log"
    [System.IO.File]::WriteAllText($diagnosticStderrPath, "fixture desktop stderr", $fixtureEncoding)
    $diagnosticWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $runtimeDiagnostic = Get-HookCanvasFailureDiagnostic -DesktopPid $PID -CdpPort 1 -DesktopStderrPath $diagnosticStderrPath
    $diagnosticWatch.Stop()
    Assert-True ($diagnosticWatch.Elapsed.TotalSeconds -lt 5) "Hook canvas runtime diagnostic exceeded its bounded collection allowance."
    Assert-Contains 'Desktop process tree:' $runtimeDiagnostic "Runtime diagnostic must include the desktop process tree."
    Assert-Contains 'CDP listeners:' $runtimeDiagnostic "Runtime diagnostic must include the CDP listener snapshot."
    Assert-Contains 'fixture desktop stderr' $runtimeDiagnostic "Runtime diagnostic must include desktop stderr."
    [System.IO.File]::WriteAllText($exitFixturePath, @'
import fs from "node:fs";
const args = process.argv.slice(2);
const value = (name) => args[args.indexOf(name) + 1];
fs.writeFileSync(value("--output"), `${JSON.stringify({ pid: process.pid })}\n`, "utf8");
fs.writeSync(1, "fixture stdout\n");
fs.writeSync(2, "fixture stderr\n");
process.exit(7);
'@, $fixtureEncoding)
    [System.IO.File]::WriteAllText($hangFixturePath, @'
import fs from "node:fs";
const args = process.argv.slice(2);
const value = (name) => args[args.indexOf(name) + 1];
fs.writeFileSync(value("--output"), `${JSON.stringify({ pid: process.pid })}\n`, "utf8");
fs.writeSync(1, "hanging stdout\n");
fs.writeSync(2, "hanging stderr\n");
setInterval(() => {}, 1000);
'@, $fixtureEncoding)

    $exitOutputPath = Join-Path $fixtureRoot "exit.json"
    $exitScreenshotPath = Join-Path $fixtureRoot "exit.png"
    $exitFailure = $null
    try {
        Invoke-Inspector -InspectorPath $exitFixturePath -DebugPort 1 -OutputPath $exitOutputPath -ScreenshotPath $exitScreenshotPath -TimeoutSeconds 10 | Out-Null
    }
    catch {
        $exitFailure = $_.Exception.Message
    }
    Assert-True (-not [string]::IsNullOrWhiteSpace($exitFailure)) "Nonzero Inspector fixture unexpectedly succeeded."
    Assert-Contains 'exit code 7' $exitFailure "Inspector must report the concrete nonzero exit code. Actual=[$exitFailure]"
    Assert-Contains 'fixture stdout' $exitFailure "Inspector failure must include redirected stdout."
    Assert-Contains 'fixture stderr' $exitFailure "Inspector failure must include redirected stderr."

    $hangOutputPath = Join-Path $fixtureRoot "hang.json"
    $hangScreenshotPath = Join-Path $fixtureRoot "hang.png"
    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $hangFailure = $null
    try {
        Invoke-Inspector -InspectorPath $hangFixturePath -DebugPort 1 -OutputPath $hangOutputPath -ScreenshotPath $hangScreenshotPath -TimeoutSeconds 1 | Out-Null
    }
    catch {
        $hangFailure = $_.Exception.Message
    }
    $watch.Stop()
    Assert-True (-not [string]::IsNullOrWhiteSpace($hangFailure)) "Hanging Inspector fixture unexpectedly succeeded."
    Assert-Contains 'timed out after 1 seconds' $hangFailure "Hanging Inspector fixture must report its timeout."
    Assert-True ($watch.Elapsed.TotalSeconds -lt 6) "Inspector timeout path exceeded its bounded cleanup allowance."
    $hangPid = [int]((Get-Content -Raw -Encoding UTF8 -LiteralPath $hangOutputPath | ConvertFrom-Json).pid)
    Start-Sleep -Milliseconds 200
    Assert-True ($null -eq (Get-Process -Id $hangPid -ErrorAction SilentlyContinue)) "Timed-out Inspector process remained alive."
}
finally {
    Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Hook canvas UI contract passed."
