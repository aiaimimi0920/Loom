[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PublisherId,
    [Parameter(Mandatory = $true)][string]$PackageId,
    [string]$DaemonExecutable = ".\target\debug\loom-daemon.exe",
    [string]$PluginCliExecutable = ".\target\debug\loom-plugin.exe",
    [string]$EvidenceRoot = ".\target\capability-plugin-conformance",
    [switch]$AuditSourceIsolation,
    [string]$LoomRepository = "",
    [string]$HookRepository = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))

function Resolve-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-SafeCapabilityIdentifier {
    param([string]$Label, [string]$Value, [bool]$AllowDots)
    $alphabet = if ($AllowDots) { '^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$' } else { '^[a-z0-9](?:[a-z0-9_-]*[a-z0-9])?$' }
    $base = @($Value -split '\.', 2)[0].ToUpperInvariant()
    $reserved = $base -match '^(CON|PRN|AUX|NUL)$' -or $base -match '^(COM|LPT)0*[1-9]$'
    $safe = -not [string]::IsNullOrWhiteSpace($Value) -and $Value.Length -le 128 -and
        $Value -match $alphabet -and -not $Value.Contains("..") -and -not $reserved
    Assert-True $safe "Unsafe capability ${Label}: $Value"
}

function Write-Utf8NoBomFile {
    param([string]$Path, [string]$Content)
    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function Invoke-NativeChecked {
    param([string]$Executable, [string[]]$Arguments)
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Executable failed with exit code $LASTEXITCODE"
    }
}

function Get-GitStateFingerprint {
    param([string]$Repository)
    $head = (& git -C $Repository rev-parse HEAD 2>$null | Out-String).Trim()
    $status = (& git -C $Repository status --porcelain=v1 --untracked-files=all 2>$null | Out-String).Trim()
    $sourceFiles = @(& git -C $Repository ls-files --cached --others --exclude-standard 2>$null |
        Where-Object { $_ } | Sort-Object -Unique)
    $hashes = foreach ($relative in $sourceFiles) {
        $path = Join-Path $Repository $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            "$relative|$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash)"
        }
    }
    $bytes = [System.Text.Encoding]::UTF8.GetBytes(($hashes -join "`n"))
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $source = ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "") }
    finally { $sha.Dispose() }
    return "$head|$status|$source"
}

function Invoke-JsonRequest {
    param(
        [string]$Method,
        [string]$BaseUrl,
        [string]$Path,
        [string]$Token,
        [AllowNull()][object]$Body = $null
    )
    $parameters = @{
        Uri = "$BaseUrl$Path"
        Method = $Method
        Headers = @{ Authorization = "Bearer $Token"; Accept = "application/json" }
        TimeoutSec = 120
        UseBasicParsing = $true
    }
    if ($null -ne $Body) {
        $parameters.ContentType = "application/json"
        $parameters.Body = $Body | ConvertTo-Json -Depth 64 -Compress
    }
    $response = Invoke-WebRequest @parameters
    if ([string]::IsNullOrWhiteSpace($response.Content)) { return $null }
    return $response.Content | ConvertFrom-Json
}

function Invoke-JsonRequestWithRuntimeProbe {
    param(
        [string]$BaseUrl,
        [string]$Path,
        [string]$Token,
        [object]$Body,
        [string]$MarkerPath,
        [string]$ControlRoot
    )
    $bodyJson = $Body | ConvertTo-Json -Depth 64 -Compress
    $job = Start-Job -ScriptBlock {
        param($Uri, $AuthToken, $Json)
        $response = Invoke-WebRequest -Uri $Uri -Method POST -Headers @{
            Authorization = "Bearer $AuthToken"
            Accept = "application/json"
        } -ContentType "application/json" -Body $Json -TimeoutSec 120 -UseBasicParsing
        $response.Content
    } -ArgumentList "$BaseUrl$Path", $Token, $bodyJson
    try {
        $runtime = Wait-RuntimeLaunchEvidence -MarkerPath $MarkerPath -ControlRoot $ControlRoot
        $completed = Wait-Job -Job $job -Timeout 120
        Assert-True ($null -ne $completed -and $job.State -eq "Completed") "Capability invocation job did not complete"
        $content = (Receive-Job -Job $job -ErrorAction Stop | Out-String).Trim()
        return [ordered]@{ response = ($content | ConvertFrom-Json); runtime = $runtime }
    }
    finally {
        Stop-Job -Job $job -ErrorAction SilentlyContinue
        Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
    }
}

function Start-IsolatedDaemon {
    param([string]$ControlRoot, [string]$ManifestRoot, [int]$Port, [int]$Sequence)
    Remove-Item -LiteralPath (Join-Path $ManifestRoot "loom.json") -Force -ErrorAction SilentlyContinue
    $saved = @{}
    foreach ($name in @("LOOM_CONTROL_PLANE_ROOT", "LOOM_CONFIGURATION_ROOT", "LOOM_DAEMON_PORT", "LOOM_CAPABILITY_MANIFEST_DIR")) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
    }
    try {
        $env:LOOM_CONTROL_PLANE_ROOT = $ControlRoot
        $env:LOOM_CONFIGURATION_ROOT = Join-Path $ControlRoot "configuration"
        $env:LOOM_DAEMON_PORT = [string]$Port
        $env:LOOM_CAPABILITY_MANIFEST_DIR = $ManifestRoot
        $process = Start-Process -FilePath $daemonPath -ArgumentList @("--manifest-dir", $ManifestRoot) `
            -PassThru -WindowStyle Hidden `
            -RedirectStandardOutput (Join-Path $evidencePath "daemon-$Sequence.stdout.log") `
            -RedirectStandardError (Join-Path $evidencePath "daemon-$Sequence.stderr.log")
    }
    finally {
        foreach ($name in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($name, $saved[$name], "Process")
        }
    }
    $manifestPath = Join-Path $ManifestRoot "loom.json"
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        if ($process.HasExited) { throw "Loom daemon exited during startup with $($process.ExitCode)" }
        if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
            try {
                $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
                if ($manifest.pid -eq $process.Id) {
                    return @{ Process = $process; BaseUrl = $manifest.transport.baseUrl; Token = $manifest.transport.authToken }
                }
            }
            catch { }
        }
        Start-Sleep -Milliseconds 100
        $process.Refresh()
    }
    throw "Timed out waiting for isolated Loom daemon manifest"
}

function Stop-IsolatedDaemon {
    param([AllowNull()][System.Diagnostics.Process]$Process)
    if ($null -eq $Process) { return }
    $Process.Refresh()
    if (-not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force
        $Process.WaitForExit(10000) | Out-Null
    }
}

function Test-PathWithinRoot {
    param([string]$Path, [string]$Root)
    $rootPrefix = [System.IO.Path]::GetFullPath($Root).TrimEnd([char[]]@('\', '/')) +
        [System.IO.Path]::DirectorySeparatorChar
    return [System.IO.Path]::GetFullPath($Path).StartsWith(
        $rootPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

function Get-OwnedPluginProcesses {
    param([string]$ControlRoot)
    return @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        try {
            $processPath = $_.Path
            $null -ne $processPath -and (Test-PathWithinRoot -Path $processPath -Root $ControlRoot)
        }
        catch { $false }
    })
}

function Wait-RuntimeLaunchEvidence {
    param([string]$MarkerPath, [string]$ControlRoot, [int]$TimeoutMilliseconds = 5000)
    $attempts = [Math]::Max(1, [Math]::Ceiling($TimeoutMilliseconds / 100.0))
    for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
        if (Test-Path -LiteralPath $MarkerPath -PathType Leaf) {
            $parts = (Get-Content -LiteralPath $MarkerPath -Raw -Encoding UTF8) -split '\|', 2
            if ($parts.Count -eq 2) {
                $runtimeProcessId = 0
                Assert-True ([int]::TryParse($parts[0], [ref]$runtimeProcessId)) "Invalid runtime PID evidence"
                Assert-True (Test-PathWithinRoot -Path $parts[1] -Root $ControlRoot) "Runtime launched outside the isolated control root"
                $runtimeProcess = Get-Process -Id $runtimeProcessId -ErrorAction SilentlyContinue
                if ($null -ne $runtimeProcess) {
                    $actualPath = $runtimeProcess.Path
                    Assert-True ($null -ne $actualPath) "Runtime process path is unavailable"
                    Assert-True (
                        [System.IO.Path]::GetFullPath($actualPath) -eq [System.IO.Path]::GetFullPath($parts[1])
                    ) "Runtime PID does not match its launch evidence"
                    return [ordered]@{ pid = $runtimeProcessId; path = $parts[1] }
                }
            }
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Capability runtime did not produce launch evidence"
}

function Assert-ProcessExited {
    param([int]$ProcessId, [string]$ExpectedPath, [int]$TimeoutMilliseconds = 5000)
    $attempts = [Math]::Max(1, [Math]::Ceiling($TimeoutMilliseconds / 100.0))
    for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
        $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        if ($null -eq $process) { return }
        $actualPath = $process.Path
        if ($null -eq $actualPath -or
            [System.IO.Path]::GetFullPath($actualPath) -ne [System.IO.Path]::GetFullPath($ExpectedPath)) {
            return
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Capability runtime process $ProcessId did not exit"
}

function Assert-NoPluginProcesses {
    param([string]$ControlRoot, [int]$TimeoutMilliseconds = 5000)
    $attempts = [Math]::Max(1, [Math]::Ceiling($TimeoutMilliseconds / 100.0))
    for ($attempt = 0; $attempt -lt $attempts; $attempt++) {
        $remaining = @(Get-OwnedPluginProcesses -ControlRoot $ControlRoot)
        if ($remaining.Count -eq 0) { return }
        Start-Sleep -Milliseconds 100
    }
    $remaining = @(Get-OwnedPluginProcesses -ControlRoot $ControlRoot)
    $remainingIds = @($remaining | ForEach-Object { $_.Id }) -join ", "
    throw "Capability runtime process remained active: $remainingIds"
}

function Write-CapabilityRuntime {
    param(
        [string]$Path,
        [string]$CommandId,
        [string]$TypeId,
        [string]$RendererId,
        [string]$Version,
        [string]$MarkerPath
    )
    $source = @'
use std::io::{Read, Write};

fn field(input: &str, key: &str) -> String {
    let marker = format!("\"{}\":\"", key);
    let rest = &input[input.find(&marker).unwrap() + marker.len()..];
    rest[..rest.find('\"').unwrap()].to_owned()
}

fn main() {
    let executable = std::env::current_exe().unwrap();
    let launch = format!("{}|{}", std::process::id(), executable.display());
    std::fs::write(r#"__MARKER__"#, launch).unwrap();
    let mut input = std::io::stdin();
    let mut output = std::io::stdout();
    loop {
        let mut length = [0u8; 4];
        if input.read_exact(&mut length).is_err() { break; }
        let mut bytes = vec![0u8; u32::from_be_bytes(length) as usize];
        input.read_exact(&mut bytes).unwrap();
        let request = String::from_utf8(bytes).unwrap();
        let request_id = field(&request, "requestId");
        let method = field(&request, "method");
        if method == "command" {
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
        let effects = if method == "command" {
            r#"[{"type":"attachment.upsert","payload":{"attachmentId":"__COMMAND__.result","typeId":"__TYPE__","schemaVersion":"1.0","priorRevision":0,"revision":1,"rendererId":"__RENDERER__","payload":{"version":"__VERSION__"},"resourceRefs":[]}},{"type":"notice.show","payload":{"title":"External capability","message":"Command completed"}}]"#
        } else { "[]" };
        let response = format!(
            "{{\"type\":\"response\",\"protocol\":\"loom.capability.runtime.v1\",\"apiVersion\":\"1.0\",\"requestId\":\"{}\",\"status\":\"succeeded\",\"payload\":{{\"output\":{{\"version\":\"__VERSION__\"}},\"effects\":{}}}}}",
            request_id, effects
        );
        output.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
        output.write_all(response.as_bytes()).unwrap();
        output.flush().unwrap();
        if method == "deactivate" { break; }
    }
}
'@
    $source = $source.Replace("__COMMAND__", $CommandId).Replace("__TYPE__", $TypeId).
        Replace("__RENDERER__", $RendererId).Replace("__VERSION__", $Version).
        Replace("__MARKER__", $MarkerPath)
    $sourcePath = [System.IO.Path]::ChangeExtension($Path, ".rs")
    Write-Utf8NoBomFile -Path $sourcePath -Content $source
    Invoke-NativeChecked -Executable "rustc" -Arguments @($sourcePath, "-O", "-o", $Path)
    Remove-Item -LiteralPath $sourcePath -Force
    Remove-Item -LiteralPath ([System.IO.Path]::ChangeExtension($Path, ".pdb")) -Force -ErrorAction SilentlyContinue
}

function New-CapabilityPackage {
    param([string]$Directory, [string]$Version, [string]$Archive, [string]$MarkerPath)
    Invoke-NativeChecked -Executable $pluginCliPath -Arguments @("init", "capability", $Directory, $PackageId, $PublisherId)
    $directoryRoot = [System.IO.Path]::GetFullPath($Directory)
    $directoryPrefix = $directoryRoot + [System.IO.Path]::DirectorySeparatorChar
    $runtimePath = [System.IO.Path]::GetFullPath((Join-Path $directoryRoot "runtime\$PackageId.exe"))
    Assert-True ($runtimePath.StartsWith($directoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) "Capability runtime escaped its package directory"
    Write-CapabilityRuntime -Path $runtimePath -CommandId $commandId -TypeId $typeId `
        -RendererId $rendererId -Version $Version -MarkerPath $MarkerPath
    $manifestPath = Join-Path $Directory "capability.manifest.json"
    $manifest = Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $manifest.version = $Version
    $manifest.permissions = @("hook.unit.attachments.write", "hook.notice.show")
    $manifest.activationEvents = @("onCommand:$commandId")
    $manifest.contributes = [ordered]@{
        commands = @([ordered]@{ id = $commandId; title = "Run external capability"; permissions = $manifest.permissions })
        shortcuts = @([ordered]@{ id = "$qualifiedId.default-shortcut"; command = $commandId; title = "Run external capability"; payload = @{ keys = "Ctrl+Alt+A" } })
        menus = @([ordered]@{ id = "$qualifiedId.unit-toolbar"; command = $commandId; title = "External capability"; placement = "hook.unit.toolbar"; order = 20 })
        settings = @([ordered]@{ id = $settingId; title = "Result style"; payload = @{ type = "enum"; options = @("balanced", "compact"); default = "balanced"; description = "Controls result presentation" } })
        dataTypes = @([ordered]@{ id = $typeId; payload = @{ jsonSchema = @{ type = "object" } } })
        renderers = @([ordered]@{ id = $rendererId; payload = @{ typeId = $typeId; bounds = @{ x = 8; y = 8; width = 180; height = 44 }; scene = @{ id = "result"; type = "text"; props = @{ text = "External result" } } } })
        unitOverlays = @([ordered]@{ id = "$qualifiedId.run-overlay"; command = $commandId; payload = @{ typeId = $typeId; bounds = @{ x = 8; y = 56; width = 140; height = 36 }; scene = @{ id = "run"; type = "button"; props = @{ label = "Run" }; events = @{ click = $commandId } } } })
    }
    Write-Utf8NoBomFile -Path $manifestPath -Content (($manifest | ConvertTo-Json -Depth 64) + "`n")
    Invoke-NativeChecked -Executable $pluginCliPath -Arguments @("sign", $Directory, $keyPath, $PublisherId)
    Invoke-NativeChecked -Executable $pluginCliPath -Arguments @("pack", $Directory, $Archive)
}

function Install-CapabilityArchive {
    param([hashtable]$Daemon, [string]$Archive)
    $bytes = [System.IO.File]::ReadAllBytes($Archive)
    return Invoke-JsonRequest -Method "POST" -BaseUrl $Daemon.BaseUrl -Path "/v1/capability-plugins/install" -Token $Daemon.Token -Body @{
        zipBase64 = "data:application/zip;base64,$([Convert]::ToBase64String($bytes))"
    }
}

Assert-SafeCapabilityIdentifier -Label "publisher id" -Value $PublisherId -AllowDots $true
Assert-SafeCapabilityIdentifier -Label "package id" -Value $PackageId -AllowDots $false
$daemonPath = Resolve-RepoPath $DaemonExecutable
$pluginCliPath = Resolve-RepoPath $PluginCliExecutable
$evidencePath = Resolve-RepoPath $EvidenceRoot
Assert-True (Test-Path -LiteralPath $daemonPath -PathType Leaf) "Loom daemon executable not found: $daemonPath"
Assert-True (Test-Path -LiteralPath $pluginCliPath -PathType Leaf) "loom-plugin executable not found: $pluginCliPath"
New-Item -ItemType Directory -Force -Path $evidencePath | Out-Null

$qualifiedId = "$PublisherId/$PackageId"
$commandId = "$qualifiedId.run"
$settingId = "$qualifiedId.result-style"
$typeId = "$qualifiedId.result.v1"
$rendererId = "$qualifiedId.result-card"
$encodedId = [Uri]::EscapeDataString($qualifiedId)
$pluginPath = "/v1/capability-plugins/$encodedId"
$loomSourcePath = $null
$hookSourcePath = $null
$loomBefore = $null
$hookBefore = $null
if ($AuditSourceIsolation) {
    Assert-True (-not [string]::IsNullOrWhiteSpace($LoomRepository)) "-LoomRepository is required with -AuditSourceIsolation"
    Assert-True (-not [string]::IsNullOrWhiteSpace($HookRepository)) "-HookRepository is required with -AuditSourceIsolation"
    $loomSourcePath = Resolve-RepoPath $LoomRepository
    $hookSourcePath = Resolve-RepoPath $HookRepository
    Assert-True (Test-Path -LiteralPath $loomSourcePath -PathType Container) "Loom repository not found: $loomSourcePath"
    Assert-True (Test-Path -LiteralPath $hookSourcePath -PathType Container) "Hook repository not found: $hookSourcePath"
    $sourceLiteral = [regex]::Escape($qualifiedId)
    $loomIdMatch = Get-ChildItem -LiteralPath (Join-Path $loomSourcePath "apps"), (Join-Path $loomSourcePath "crates") -Recurse -File |
        Select-String -Pattern $sourceLiteral | Select-Object -First 1
    $hookIdMatch = Get-ChildItem -LiteralPath (Join-Path $hookSourcePath "src") -Recurse -File |
        Select-String -Pattern $sourceLiteral | Select-Object -First 1
    Assert-True ($null -eq $loomIdMatch) "Loom core contains the external capability id"
    Assert-True ($null -eq $hookIdMatch) "Hook core contains the external capability id"
    $loomBefore = Get-GitStateFingerprint $loomSourcePath
    $hookBefore = Get-GitStateFingerprint $hookSourcePath
}
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-capability-conformance-" + [Guid]::NewGuid().ToString("N"))
$controlRoot = Join-Path $tempRoot "control"
$manifestRoot = Join-Path $tempRoot "manifest"
$packageV1 = Join-Path $tempRoot "package-v1"
$packageV2 = Join-Path $tempRoot "package-v2"
$archiveV1 = Join-Path $tempRoot "package-v1.zip"
$archiveV2 = Join-Path $tempRoot "package-v2.zip"
$keyPath = Join-Path $tempRoot "publisher-key.json"
$runtimeMarkerPath = Join-Path $tempRoot "runtime-started.txt"
$daemon = $null
$evidence = [ordered]@{ qualifiedId = $qualifiedId; lifecycle = @(); startedAt = [DateTime]::UtcNow.ToString("o") }

try {
    New-Item -ItemType Directory -Force -Path $controlRoot, $manifestRoot | Out-Null
    Invoke-NativeChecked -Executable $pluginCliPath -Arguments @("keygen", $keyPath, "conformance-key")
    Invoke-NativeChecked -Executable $pluginCliPath -Arguments @("trust", "add", (Join-Path $controlRoot "plugin-trust.json"), $PublisherId, $keyPath)
    New-CapabilityPackage -Directory $packageV1 -Version "1.0.0" -Archive $archiveV1 -MarkerPath $runtimeMarkerPath
    New-CapabilityPackage -Directory $packageV2 -Version "2.0.0" -Archive $archiveV2 -MarkerPath $runtimeMarkerPath

    $port = Get-LoomSmokePort
    $daemon = Start-IsolatedDaemon -ControlRoot $controlRoot -ManifestRoot $manifestRoot -Port $port -Sequence 1
    $installedV1 = Install-CapabilityArchive -Daemon $daemon -Archive $archiveV1
    $digestV1 = [string]$installedV1.package.digest
    Assert-True ($installedV1.package.qualifiedId -eq $qualifiedId) "Installed package identity mismatch"
    Assert-True ($digestV1.Length -eq 64) "Installed package digest is missing"
    $evidence.lifecycle += "install-v1"
    $approvedV1 = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/approve" -Token $daemon.Token -Body @{ digest = $digestV1; permissions = @("hook.unit.attachments.write", "hook.notice.show") }
    Assert-True ($approvedV1.approved -eq $true) "Permission approval was not committed"
    $enabledV1 = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/enable" -Token $daemon.Token -Body @{ digest = $digestV1 }
    Assert-True ($enabledV1.plugin.activeDigest -eq $digestV1) "Enable did not activate v1"
    $evidence.lifecycle += "enable-v1"

    $snapshot = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins/extensions" -Token $daemon.Token
    Assert-True ($snapshot.snapshot.contributions.commands.Count -eq 1) "Unknown command was not published"
    Assert-True ($snapshot.snapshot.contributions.shortcuts.Count -eq 1) "Default shortcut was not published"
    Assert-True ($snapshot.snapshot.contributions.menus[0].placement -eq "hook.unit.toolbar") "Toolbar menu was not published"
    Assert-True ($snapshot.snapshot.contributions.settings.Count -eq 1) "Settings field was not published"
    Assert-True ($snapshot.snapshot.contributions.dataTypes.Count -eq 1) "Namespaced data type was not published"
    Assert-True ($snapshot.snapshot.contributions.unitOverlays.Count -eq 1) "Clickable overlay was not published"

    $settings = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/settings" -Token $daemon.Token
    Assert-True ($settings.fields[0].id -eq $settingId) "Manifest-driven setting field was not returned"
    Invoke-JsonRequest -Method "PUT" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/settings" -Token $daemon.Token -Body @{ expectedRevision = 0; packageDigest = $settings.packageDigest; values = @{ $settingId = "compact" } } | Out-Null
    $invokeProbe = Invoke-JsonRequestWithRuntimeProbe -BaseUrl $daemon.BaseUrl -Path "/v1/invoke" `
        -Token $daemon.Token -MarkerPath $runtimeMarkerPath -ControlRoot $controlRoot `
        -Body @{ requestId = "external-v1"; caller = "hook"; capability = $commandId; input = @{} }
    $invoked = $invokeProbe.response
    Assert-True ($invoked.status -eq "succeeded") "External command invocation failed"
    Assert-True (@($invoked.output.effects | Where-Object { $_.type -eq "attachment.upsert" }).Count -eq 1) "Attachment effect was not returned"
    Assert-True (@($invoked.output.effects | Where-Object { $_.type -eq "notice.show" }).Count -eq 1) "Notice effect was not returned"
    $runtimeLaunch = $invokeProbe.runtime
    $evidence.runtimeProcess = $runtimeLaunch
    $evidence.lifecycle += "invoke-v1"

    Stop-IsolatedDaemon -Process $daemon.Process
    $daemon = $null
    Assert-ProcessExited -ProcessId $runtimeLaunch.pid -ExpectedPath $runtimeLaunch.path
    Assert-NoPluginProcesses -ControlRoot $controlRoot
    $daemon = Start-IsolatedDaemon -ControlRoot $controlRoot -ManifestRoot $manifestRoot -Port $port -Sequence 2
    $restored = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins/extensions" -Token $daemon.Token
    Assert-True ($restored.snapshot.plugins[0].version -eq "1.0.0") "Active capability was not restored after restart"
    $evidence.lifecycle += "restart-restored"

    $disabledPlugin = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/disable" -Token $daemon.Token -Body @{}
    Assert-True ($disabledPlugin.plugin.enabledIntent -eq $false) "Disable intent was not committed"
    Assert-ProcessExited -ProcessId $runtimeLaunch.pid -ExpectedPath $runtimeLaunch.path
    Assert-NoPluginProcesses -ControlRoot $controlRoot
    $disabled = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins/extensions" -Token $daemon.Token
    Assert-True ($disabled.snapshot.plugins.Count -eq 0) "Disabled capability retained active objects"
    $evidence.lifecycle += "disable"
    $reEnabled = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/enable" -Token $daemon.Token -Body @{ digest = $digestV1 }
    Assert-True ($reEnabled.plugin.activeDigest -eq $digestV1) "Re-enable did not reactivate v1"
    $evidence.lifecycle += "re-enable"

    $installedV2 = Install-CapabilityArchive -Daemon $daemon -Archive $archiveV2
    $digestV2 = [string]$installedV2.package.digest
    Assert-True ($installedV2.package.qualifiedId -eq $qualifiedId) "Installed v2 package identity mismatch"
    Assert-True ($digestV2.Length -eq 64) "Installed v2 package digest is missing"
    $approvedV2 = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/approve" -Token $daemon.Token -Body @{ digest = $digestV2; permissions = @("hook.unit.attachments.write", "hook.notice.show") }
    Assert-True ($approvedV2.approved -eq $true) "V2 permission approval was not committed"
    $upgrade = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/upgrade" -Token $daemon.Token -Body @{ digest = $digestV2 }
    Assert-True ($upgrade.plugin.activeDigest -eq $digestV2) "Upgrade response did not activate v2"
    $upgraded = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins/extensions" -Token $daemon.Token
    Assert-True ($upgraded.snapshot.plugins[0].version -eq "2.0.0") "Upgrade did not activate v2"
    $evidence.lifecycle += "upgrade-v2"
    $rollback = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/rollback" -Token $daemon.Token -Body @{}
    Assert-True ($rollback.plugin.activeDigest -eq $digestV1) "Rollback response did not restore v1"
    $rolledBack = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins/extensions" -Token $daemon.Token
    Assert-True ($rolledBack.snapshot.plugins[0].version -eq "1.0.0") "Rollback did not restore v1"
    $evidence.lifecycle += "rollback-v1"
    $uninstall = Invoke-JsonRequest -Method "POST" -BaseUrl $daemon.BaseUrl -Path "$pluginPath/uninstall" -Token $daemon.Token -Body @{}
    Assert-True ($uninstall.uninstalled -eq $true) "Uninstall response was not committed"
    Assert-ProcessExited -ProcessId $runtimeLaunch.pid -ExpectedPath $runtimeLaunch.path
    Assert-NoPluginProcesses -ControlRoot $controlRoot
    $uninstalled = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins/extensions" -Token $daemon.Token
    Assert-True ($uninstalled.snapshot.plugins.Count -eq 0) "Uninstalled capability retained active objects"
    $listed = Invoke-JsonRequest -Method "GET" -BaseUrl $daemon.BaseUrl -Path "/v1/capability-plugins" -Token $daemon.Token
    Assert-True ($listed.plugins.Count -eq 0) "Uninstalled capability remained in registry"
    $evidence.lifecycle += "uninstall"
}
finally {
    $cleanupErrors = @()
    try {
        if ($null -ne $daemon) { Stop-IsolatedDaemon -Process $daemon.Process }
    }
    catch { $cleanupErrors += "daemon cleanup failed: $($_.Exception.Message)" }
    try {
        $owned = @(Get-OwnedPluginProcesses -ControlRoot $controlRoot)
        foreach ($process in $owned) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
        }
        Assert-NoPluginProcesses -ControlRoot $controlRoot
    }
    catch { $cleanupErrors += "runtime cleanup failed: $($_.Exception.Message)" }
    try {
        if (Test-Path -LiteralPath $tempRoot) {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }
    catch { $cleanupErrors += "temporary file cleanup failed: $($_.Exception.Message)" }
    if ($cleanupErrors.Count -gt 0) { throw ($cleanupErrors -join "; ") }
}

if ($AuditSourceIsolation) {
    $loomAfter = Get-GitStateFingerprint $loomSourcePath
    $hookAfter = Get-GitStateFingerprint $hookSourcePath
    Assert-True ($loomBefore -eq $loomAfter) "Loom source fingerprint changed during external plugin conformance"
    Assert-True ($hookBefore -eq $hookAfter) "Hook source fingerprint changed during external plugin conformance"
    $evidence.sourceFingerprints = @{ loom = $loomAfter; hook = $hookAfter }
}
$evidence.sourceIsolationAudit = [bool]$AuditSourceIsolation
$evidence.completedAt = [DateTime]::UtcNow.ToString("o")
$evidencePathJson = Join-Path $evidencePath "capability-plugin-conformance.json"
Write-Utf8NoBomFile -Path $evidencePathJson -Content (($evidence | ConvertTo-Json -Depth 16) + "`n")
Write-Host "Capability Plugin conformance passed: $qualifiedId"
Write-Host "Evidence: $evidencePathJson"
