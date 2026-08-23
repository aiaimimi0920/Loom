[CmdletBinding()]
param(
    [string]$PackageDir = "",
    [string]$EvidenceRoot = ".\target\surface-prototype-smoke",
    [ValidateSet("Debug", "Release")][string]$Configuration = "Debug",
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$script:DaemonRequestHeaders = @{}
$script:DaemonToken = ""

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $repoRoot "scripts\LoomSmokePorts.ps1")

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-Equal {
    param([object]$Expected, [object]$Actual, [string]$Message)
    if ($Expected -is [string] -or $Actual -is [string]) {
        $same = [string]::Equals([string]$Expected, [string]$Actual, [StringComparison]::Ordinal)
    } else {
        $same = $Expected -eq $Actual
    }
    if (-not $same) { throw "$Message Expected=[$Expected] Actual=[$Actual]" }
}

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)
    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) { return $Argument }
    $builder = [Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq [char]0x5c) { $slashes++; continue }
        if ($character -eq [char]0x22) {
            if ($slashes -gt 0) { [void]$builder.Append(('\' * (($slashes * 2) + 1))) }
            [void]$builder.Append('"')
            $slashes = 0
            continue
        }
        if ($slashes -gt 0) { [void]$builder.Append(('\' * $slashes)); $slashes = 0 }
        [void]$builder.Append($character)
    }
    if ($slashes -gt 0) { [void]$builder.Append(('\' * ($slashes * 2))) }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Start-SmokeProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = "",
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )
    $argumentLine = (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument $_ }) -join " "
    $parameters = @{ FilePath = $FilePath; PassThru = $true; WindowStyle = "Hidden" }
    if ($argumentLine) { $parameters.ArgumentList = $argumentLine }
    if ($WorkingDirectory) { $parameters.WorkingDirectory = $WorkingDirectory }
    if ($StdoutPath) { $parameters.RedirectStandardOutput = $StdoutPath }
    if ($StderrPath) { $parameters.RedirectStandardError = $StderrPath }
    return Start-Process @parameters
}

function Start-InheritedEnvProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$WorkingDirectory = "",
        [hashtable]$Environment = @{},
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )
    $previous = @{}
    foreach ($entry in $Environment.GetEnumerator()) {
        $previous[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key, "Process")
        [Environment]::SetEnvironmentVariable($entry.Key, [string]$entry.Value, "Process")
    }
    try {
        return Start-SmokeProcess -FilePath $FilePath -ArgumentList $ArgumentList `
            -WorkingDirectory $WorkingDirectory -StdoutPath $StdoutPath -StderrPath $StderrPath
    } finally {
        foreach ($entry in $previous.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, "Process")
        }
    }
}

function Stop-SmokeProcess {
    param([System.Diagnostics.Process]$Process)
    if ($null -eq $Process) { return }
    try {
        if (-not $Process.HasExited) {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
            [void]$Process.WaitForExit(5000)
        }
    } finally { $Process.Dispose() }
}

function Start-SurfaceDaemon {
    param(
        [string]$FilePath,
        [string]$WorkingDirectory,
        [int]$Port,
        [string]$ControlPlaneRoot,
        [string]$ArtStoreUrl,
        [string]$AppDataRoot,
        [string]$LocalAppDataRoot,
        [string]$LogsRoot,
        [string]$LogStem
    )
    return Start-InheritedEnvProcess -FilePath $FilePath -WorkingDirectory $WorkingDirectory -Environment @{
        LOOM_DAEMON_HOST = "127.0.0.1"; LOOM_DAEMON_PORT = "$Port"; LOOM_CONTROL_PLANE_ROOT = $ControlPlaneRoot
        LOOM_DAEMON_TOKEN = $script:DaemonToken
        LOOM_ART_STORE_URL = $ArtStoreUrl; APPDATA = $AppDataRoot; LOCALAPPDATA = $LocalAppDataRoot
    } -StdoutPath (Join-Path $LogsRoot "$LogStem.stdout.log") -StderrPath (Join-Path $LogsRoot "$LogStem.stderr.log")
}

function Invoke-JsonGet {
    param([string]$Uri)
    return Invoke-RestMethod -Uri $Uri -Method Get -Headers $script:DaemonRequestHeaders -TimeoutSec 20
}

function Invoke-JsonPost {
    param([string]$Uri, [object]$Body)
    $json = $Body | ConvertTo-Json -Depth 80 -Compress
    return Invoke-RestMethod -Uri $Uri -Method Post -Headers $script:DaemonRequestHeaders -ContentType "application/json" -Body $json -TimeoutSec 30
}

function Invoke-SurfaceResourceGet {
    param(
        [string]$BaseUrl,
        [object]$Lease,
        [string]$OutputPath
    )
    $resourceId = [string]$Lease.resource.resourceId
    Assert-True ($resourceId.StartsWith("sha256:", [StringComparison]::Ordinal)) "Surface resource id is not a SHA-256 digest"
    $digest = $resourceId.Substring("sha256:".Length)
    $response = Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/v1/surfaces/resources/$digest" -Method Get `
        -Headers @{ Authorization = [string]$script:DaemonRequestHeaders.Authorization; "X-Loom-Surface-Lease" = [string]$Lease.leaseId } -OutFile $OutputPath -PassThru -TimeoutSec 20
    $bytes = [IO.File]::ReadAllBytes($OutputPath)
    Assert-Equal 200 ([int]$response.StatusCode) "Surface resource GET status mismatch"
    Assert-Equal ([int64]$Lease.resource.size) ([int64]$bytes.Length) "Surface resource byte length mismatch"
    Assert-Equal $digest ((Get-FileHash -LiteralPath $OutputPath -Algorithm SHA256).Hash.ToLowerInvariant()) "Surface resource digest mismatch"
    $contentType = [string]$response.Headers["Content-Type"]
    Assert-True ($contentType.StartsWith([string]$Lease.resource.mime, [StringComparison]::OrdinalIgnoreCase)) "Surface resource MIME mismatch"
    return [pscustomobject]@{ digest = $digest; bytes = $bytes.Length; mime = [string]$Lease.resource.mime; path = $OutputPath }
}

function Assert-SurfaceResourceRejectsWrongLease {
    param([string]$BaseUrl, [object]$Lease)
    $digest = ([string]$Lease.resource.resourceId).Substring("sha256:".Length)
    $rejected = $false
    try {
        [void](Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/v1/surfaces/resources/$digest" -Method Get `
            -Headers @{ Authorization = [string]$script:DaemonRequestHeaders.Authorization; "X-Loom-Surface-Lease" = "lease:wrong" } -TimeoutSec 20)
    } catch {
        if ($null -ne $_.Exception.Response) {
            $rejected = ([int]$_.Exception.Response.StatusCode) -eq 403
        }
    }
    Assert-True $rejected "Surface resource GET accepted a wrong lease"
}

function Wait-Http {
    param([string]$Uri, [int]$TimeoutSeconds = 30)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        try { return Invoke-JsonGet $Uri } catch { Start-Sleep -Milliseconds 200 }
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for $Uri"
}

function Get-PropertyValue {
    param([object]$Object, [string]$Name)
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Get-SurfaceAttachment {
    param([object]$Record, [string]$AttachmentId)
    return Get-PropertyValue (Get-PropertyValue $Record "attachments") $AttachmentId
}

function Wait-SurfaceAction {
    param(
        [string]$BaseUrl,
        [string]$InstanceId,
        [string]$EventId,
        [string[]]$TerminalStatuses = @("succeeded", "failed", "cancelled"),
        [int]$TimeoutSeconds = 20
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $record = Invoke-JsonGet "$BaseUrl/v1/surfaces/instances/$InstanceId"
        $acks = Get-PropertyValue $record "eventAcks"
        $ack = Get-PropertyValue $acks $EventId
        if ($null -ne $ack -and ([string]$ack.status) -in $TerminalStatuses) {
            return [pscustomobject]@{ record = $record; ack = $ack }
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for Surface action $EventId"
}

function Wait-SurfaceStatus {
    param([string]$BaseUrl, [string]$InstanceId, [string]$EventId, [string]$Status, [int]$TimeoutSeconds = 10)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        $record = Invoke-JsonGet "$BaseUrl/v1/surfaces/instances/$InstanceId"
        $ack = Get-PropertyValue (Get-PropertyValue $record "eventAcks") $EventId
        if ($null -ne $ack -and [string]$ack.status -eq $Status) { return $ack }
        Start-Sleep -Milliseconds 25
    } while ((Get-Date) -lt $deadline)
    throw "Timed out waiting for Surface action $EventId to reach $Status"
}

function Get-CurrentSurfaceRecord {
    param([string]$BaseUrl, [string]$InstanceId)
    return Invoke-JsonGet "$BaseUrl/v1/surfaces/instances/$InstanceId"
}

function Invoke-SurfaceAction {
    param(
        [string]$BaseUrl,
        [string]$InstanceId,
        [string]$AttachmentId,
        [string]$DeviceId,
        [string]$NodeId,
        [string]$EventName,
        [string]$ActionId,
        [string]$EventClass = "discrete",
        [object]$Payload = @{}
    )
    $record = Get-CurrentSurfaceRecord $BaseUrl $InstanceId
    $attachment = Get-SurfaceAttachment $record $AttachmentId
    $snapshot = Get-PropertyValue $attachment "snapshot"
    Assert-True ($null -ne $snapshot) "Surface attachment is not mounted: $AttachmentId"
    $eventId = "event:$ActionId-$([Guid]::NewGuid().ToString('N'))"
    $body = [ordered]@{
        protocolVersion = "loom.surface.v1"
        instanceId = $InstanceId
        attachmentId = $AttachmentId
        eventId = $eventId
        nodeId = $NodeId
        event = $EventName
        action = $ActionId
        class = $EventClass
        generation = [int64]$record.descriptor.generation
        baseRevision = [int64]$snapshot.revision
        payload = $Payload
    }
    $ack = Invoke-JsonPost "$BaseUrl/v1/surfaces/instances/$InstanceId/events" $body
    return [pscustomobject]@{ eventId = $eventId; ack = $ack; record = $record }
}

function New-SurfaceHostCapabilities {
    param([switch]$RemoteResources)
    $capabilities = @()
    if ($RemoteResources) { $capabilities += "remote_resources" }
    return @{
        apiVersion = "1.0"
        runtimes = @("declarative")
        nodes = @("view", "row", "column", "stack", "scroll", "text", "image", "icon", "button", "input", "textarea", "number", "slider", "switch", "select", "progress", "divider", "spacer")
        transports = @("loom_resource", "shared_memory")
        capabilities = $capabilities
        input = @{ pointer = $true; hover = $true; touch = $true; keyboard = $true }
    }
}

$targetSubdir = if ($Configuration -eq "Release") { "release" } else { "debug" }
$buildArgs = if ($Configuration -eq "Release") { @("build", "--release", "-p", "loom-daemon", "-p", "loom-art-store") } else { @("build", "-p", "loom-daemon", "-p", "loom-art-store") }
$daemonExe = Join-Path $repoRoot "target\$targetSubdir\loom-daemon.exe"
$artStoreExe = Join-Path $repoRoot "target\$targetSubdir\loom-art-store.exe"
$daemonWorkingDirectory = $repoRoot
$packageFullPath = $null
if ($PackageDir) {
    $packageFullPath = if ([IO.Path]::IsPathRooted($PackageDir)) { [IO.Path]::GetFullPath($PackageDir) } else { [IO.Path]::GetFullPath((Join-Path $repoRoot $PackageDir)) }
    $daemonExe = Join-Path $packageFullPath "runtime\loom-daemon.exe"
    $daemonWorkingDirectory = $packageFullPath
}

$EvidenceRoot = if ([IO.Path]::IsPathRooted($EvidenceRoot)) { [IO.Path]::GetFullPath($EvidenceRoot) } else { [IO.Path]::GetFullPath((Join-Path $repoRoot $EvidenceRoot)) }
$runId = "$(Get-Date -Format 'yyyyMMdd-HHmmss')-surface-prototypes-$PID-$([Guid]::NewGuid().ToString('N'))"
$runRoot = Join-Path $EvidenceRoot $runId
$storeRoot = Join-Path $runRoot "store"
$controlPlaneRoot = Join-Path $runRoot "control-plane"
$appDataRoot = Join-Path $runRoot "appdata"
$localAppDataRoot = Join-Path $runRoot "localappdata"
$logsRoot = Join-Path $runRoot "logs"
$frameworkBuildRoot = Join-Path $runRoot "framework-artifacts"
$artBuildRoot = Join-Path $runRoot "art-artifacts"
$summaryPath = Join-Path $runRoot "summary.json"
New-Item -ItemType Directory -Force -Path $storeRoot, $controlPlaneRoot, $appDataRoot, $localAppDataRoot, $logsRoot, $frameworkBuildRoot, $artBuildRoot, (Join-Path $storeRoot "frameworks"), (Join-Path $storeRoot "arts") | Out-Null
$script:DaemonToken = [Guid]::NewGuid().ToString("N") + [Guid]::NewGuid().ToString("N")
$script:DaemonRequestHeaders = @{ Authorization = "Bearer $script:DaemonToken" }

try {
    if (-not $SkipBuild) {
        if (-not (Test-Path -LiteralPath $daemonExe -PathType Leaf) -or -not (Test-Path -LiteralPath $artStoreExe -PathType Leaf)) {
            Push-Location $repoRoot
            try {
                & cargo @buildArgs
                if ($LASTEXITCODE -ne 0) { throw "cargo build failed: $LASTEXITCODE" }
            } finally { Pop-Location }
        }
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\Build-LoomArtFrameworkPackages.ps1") -OutputRoot $frameworkBuildRoot
        if ($LASTEXITCODE -ne 0) { throw "framework package build failed: $LASTEXITCODE" }
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $repoRoot "scripts\build-surface-prototypes.ps1") -OutputDir $artBuildRoot
        if ($LASTEXITCODE -ne 0) { throw "Surface prototype package build failed: $LASTEXITCODE" }
    }
    Assert-True (Test-Path -LiteralPath $daemonExe -PathType Leaf) "Missing daemon executable: $daemonExe"
    Assert-True (Test-Path -LiteralPath $artStoreExe -PathType Leaf) "Missing Art store executable: $artStoreExe"

    $frameworkSourceRoot = if (Test-Path -LiteralPath (Join-Path $frameworkBuildRoot "process.zip") -PathType Leaf) { $frameworkBuildRoot } else { Join-Path $repoRoot "target\surface-smoke-frameworks" }
    $artSourceRoot = if (Test-Path -LiteralPath (Join-Path $artBuildRoot "surface-prototype-stock-card.zip") -PathType Leaf) { $artBuildRoot } else { Join-Path $repoRoot "target\surface-smoke-arts" }
    Copy-Item -LiteralPath (Join-Path $frameworkSourceRoot "process.zip") -Destination (Join-Path $storeRoot "frameworks\process.zip") -Force
    $artNames = @{
        stock = @{ source = "surface-prototype-stock-card.zip"; id = "surface-stock-card" }
        dashboard = @{ source = "surface-prototype-dashboard.zip"; id = "surface-device-dashboard" }
        form = @{ source = "surface-prototype-form.zip"; id = "surface-project-form" }
    }
    foreach ($entry in $artNames.Values) {
        $versionDirectory = Join-Path $storeRoot "arts\$($entry.id)"
        New-Item -ItemType Directory -Force -Path $versionDirectory | Out-Null
        $versionZip = Join-Path $versionDirectory "1.0.0.zip"
        Copy-Item -LiteralPath (Join-Path $artSourceRoot $entry.source) -Destination $versionZip -Force
        $digest = (Get-FileHash -LiteralPath $versionZip -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText("$versionZip.sha256", "$digest  1.0.0.zip`n", [Text.UTF8Encoding]::new($false))
    }

    $storePort = Get-LoomSmokePort
    $daemonPort = Get-LoomSmokePort
    $storeProcess = $null
    $daemonProcess = $null
    try {
        $storeProcess = Start-InheritedEnvProcess -FilePath $artStoreExe -WorkingDirectory $repoRoot -Environment @{
            LOOM_ART_STORE_HOST = "127.0.0.1"; LOOM_ART_STORE_PORT = "$storePort"; LOOM_ART_STORE_ROOT = $storeRoot
        } -StdoutPath (Join-Path $logsRoot "store.stdout.log") -StderrPath (Join-Path $logsRoot "store.stderr.log")
        [void](Wait-Http "http://127.0.0.1:$storePort/health")
        $daemonProcess = Start-SurfaceDaemon -FilePath $daemonExe -WorkingDirectory $daemonWorkingDirectory -Port $daemonPort `
            -ControlPlaneRoot $controlPlaneRoot -ArtStoreUrl "http://127.0.0.1:$storePort" -AppDataRoot $appDataRoot `
            -LocalAppDataRoot $localAppDataRoot -LogsRoot $logsRoot -LogStem "daemon"
        [void](Wait-Http "http://127.0.0.1:$daemonPort/health")
        $baseUrl = "http://127.0.0.1:$daemonPort"

        $frameworkInstall = Invoke-JsonPost "$baseUrl/v1/frameworks/process/install" @{}
        Assert-Equal "process" ([string]$frameworkInstall.framework.id) "process framework install id mismatch"
        Assert-Equal $true ([bool]$frameworkInstall.framework.ready) "process framework must be ready"
        $artInstall = @{}
        foreach ($entry in $artNames.GetEnumerator()) {
            $installed = Invoke-JsonPost "$baseUrl/v1/arts/store/install" @{ artId = $entry.Value.id }
            $artInstall[$entry.Key] = $installed.reports
        }

        $localDevice = "device-000-local"
        $attach = @{}
        $firstDashboard = Invoke-JsonPost "$baseUrl/v1/surfaces/attach" @{
            artId = $artNames.dashboard.id; hookNodeId = "hook-node:dashboard-one"; deviceId = $localDevice
            capabilities = New-SurfaceHostCapabilities -RemoteResources
        }
        $secondDashboard = Invoke-JsonPost "$baseUrl/v1/surfaces/attach" @{
            artId = $artNames.dashboard.id; hookNodeId = "hook-node:dashboard-two"; deviceId = $localDevice
            capabilities = New-SurfaceHostCapabilities -RemoteResources
        }
        $dashboardInstanceId = [string]$firstDashboard.instance.descriptor.instanceId
        Assert-Equal $dashboardInstanceId ([string]$secondDashboard.instance.descriptor.instanceId) "shared dashboard instance was not reused"
        Assert-Equal "shared" ([string]$secondDashboard.instance.descriptor.instanceMode) "dashboard instance mode mismatch"
        $firstDashboardAttachmentId = [string](@($firstDashboard.instance.attachments.PSObject.Properties)[0].Name)
        $dashboardAttachmentProperties = @($secondDashboard.instance.attachments.PSObject.Properties)
        $secondDashboardAttachmentId = [string]($dashboardAttachmentProperties | Where-Object { $_.Name -ne $firstDashboardAttachmentId } | Select-Object -First 1).Name
        $attach.dashboard = @{
            instanceId = $dashboardInstanceId
            firstAttachmentId = $firstDashboardAttachmentId
            secondAttachmentId = $secondDashboardAttachmentId
        }

        $stock = Invoke-JsonPost "$baseUrl/v1/surfaces/attach" @{
            artId = $artNames.stock.id; hookNodeId = "hook-node:stock"; deviceId = $localDevice
            capabilities = New-SurfaceHostCapabilities
        }
        $form = Invoke-JsonPost "$baseUrl/v1/surfaces/attach" @{
            artId = $artNames.form.id; hookNodeId = "hook-node:form"; deviceId = $localDevice
            capabilities = New-SurfaceHostCapabilities
        }
        $attach.stock = @{ instanceId = [string]$stock.instance.descriptor.instanceId; attachmentId = [string](@($stock.instance.attachments.PSObject.Properties)[0].Name) }
        $attach.form = @{ instanceId = [string]$form.instance.descriptor.instanceId; attachmentId = [string](@($form.instance.attachments.PSObject.Properties)[0].Name) }

        $generation = @{}
        foreach ($surface in @($attach.stock, $attach.dashboard, $attach.form)) {
            $generation[$surface.instanceId] = Invoke-JsonPost "$baseUrl/v1/surfaces/instances/$($surface.instanceId)/generation" @{}
        }

        $stockSymbol = Invoke-SurfaceAction $baseUrl $attach.stock.instanceId $attach.stock.attachmentId $localDevice "symbol" "input" "stock_symbol_input" "continuous" @{ value = "NVDA" }
        $stockSymbolResult = Wait-SurfaceAction $baseUrl $attach.stock.instanceId $stockSymbol.eventId
        Assert-Equal "succeeded" ([string]$stockSymbolResult.ack.status) "stock symbol input failed"
        $stockCommit = Invoke-SurfaceAction $baseUrl $attach.stock.instanceId $attach.stock.attachmentId $localDevice "symbol" "change" "stock_symbol_commit" "commit" @{ value = "NVDA" }
        [void](Wait-SurfaceAction $baseUrl $attach.stock.instanceId $stockCommit.eventId)
        $stockRefresh = Invoke-SurfaceAction $baseUrl $attach.stock.instanceId $attach.stock.attachmentId $localDevice "refresh" "click" "stock_refresh"
        $stockFinal = Wait-SurfaceAction $baseUrl $attach.stock.instanceId $stockRefresh.eventId
        Assert-Equal "succeeded" ([string]$stockFinal.ack.status) "stock refresh failed"
        Assert-Equal "NVDA" ([string]$stockFinal.record.latestResult.outputs.quote.value.symbol) "stock formal result symbol mismatch"

        $dashboardRefresh = Invoke-SurfaceAction $baseUrl $attach.dashboard.instanceId $attach.dashboard.firstAttachmentId $localDevice "refresh" "click" "dashboard_refresh"
        $dashboardFinal = Wait-SurfaceAction $baseUrl $attach.dashboard.instanceId $dashboardRefresh.eventId
        Assert-Equal "succeeded" ([string]$dashboardFinal.ack.status) "dashboard refresh failed"
        $dashboardRecord = $dashboardFinal.record
        $dashboardAttachments = @($dashboardRecord.attachments.PSObject.Properties | ForEach-Object { $_.Value })
        Assert-Equal 2 $dashboardAttachments.Count "dashboard attachment count mismatch"
        Assert-Equal 4 ([int64]$dashboardAttachments[0].snapshot.revision) "dashboard first attachment did not receive fanout patches"
        Assert-Equal 4 ([int64]$dashboardAttachments[1].snapshot.revision) "dashboard second attachment did not receive fanout patches"
        Assert-Equal "ready" ([string]$dashboardRecord.latestResult.outputs.dashboard.value.status) "dashboard formal result mismatch"
        $leases = @($dashboardAttachments | ForEach-Object { $_.snapshot.resourceLeases[0].leaseId })
        Assert-True ($leases.Count -eq 2 -and $leases[0] -ne $leases[1]) "shared resource leases must be attachment-scoped"

        $formValidate = Invoke-SurfaceAction $baseUrl $attach.form.instanceId $attach.form.attachmentId $localDevice "project_name" "input" "form_validate" "continuous" @{ value = "Neuro Surface" }
        [void](Wait-SurfaceAction $baseUrl $attach.form.instanceId $formValidate.eventId)
        $formSubmit = Invoke-SurfaceAction $baseUrl $attach.form.instanceId $attach.form.attachmentId $localDevice "submit" "click" "form_submit"
        Assert-Equal "awaiting_confirmation" ([string]$formSubmit.ack.status) "form submit must require confirmation"
        $pending = Wait-SurfaceStatus $baseUrl $attach.form.instanceId $formSubmit.eventId "awaiting_confirmation"
        $formRecord = Get-CurrentSurfaceRecord $baseUrl $attach.form.instanceId
        $pendingConfirmation = @($formRecord.pendingConfirmations.PSObject.Properties | ForEach-Object { $_.Value } | Where-Object { $_.request.eventId -eq $formSubmit.eventId })[0]
        $confirmation = $pendingConfirmation.request
        Assert-True ($null -ne $confirmation) "form confirmation request missing"
        [void](Invoke-JsonPost "$baseUrl/v1/surfaces/confirmations/decision" @{
            protocolVersion = "loom.surface.v1"; confirmationId = $confirmation.confirmationId; instanceId = $attach.form.instanceId
            attachmentId = $attach.form.attachmentId; deviceId = $localDevice; approved = $true
        })
        $formFinal = Wait-SurfaceAction $baseUrl $attach.form.instanceId $formSubmit.eventId -TimeoutSeconds 20
        Assert-Equal "succeeded" ([string]$formFinal.ack.status) "confirmed form submit failed"

        $formReset = Invoke-SurfaceAction $baseUrl $attach.form.instanceId $attach.form.attachmentId $localDevice "cancel" "click" "form_cancel"
        [void](Wait-SurfaceAction $baseUrl $attach.form.instanceId $formReset.eventId)
        $formValidate2 = Invoke-SurfaceAction $baseUrl $attach.form.instanceId $attach.form.attachmentId $localDevice "project_name" "input" "form_validate" "continuous" @{ value = "Cancellation test" }
        [void](Wait-SurfaceAction $baseUrl $attach.form.instanceId $formValidate2.eventId)
        $cancelEvent = Invoke-SurfaceAction $baseUrl $attach.form.instanceId $attach.form.attachmentId $localDevice "submit" "click" "form_submit"
        $formRecord = Get-CurrentSurfaceRecord $baseUrl $attach.form.instanceId
        $pendingConfirmation = @($formRecord.pendingConfirmations.PSObject.Properties | ForEach-Object { $_.Value } | Where-Object { $_.request.eventId -eq $cancelEvent.eventId })[0]
        $confirmation = $pendingConfirmation.request
        Assert-True ($null -ne $confirmation) "cancellation form confirmation request missing"
        [void](Invoke-JsonPost "$baseUrl/v1/surfaces/confirmations/decision" @{
            protocolVersion = "loom.surface.v1"; confirmationId = $confirmation.confirmationId; instanceId = $attach.form.instanceId
            attachmentId = $attach.form.attachmentId; deviceId = $localDevice; approved = $true
        })
        [void](Wait-SurfaceStatus $baseUrl $attach.form.instanceId $cancelEvent.eventId "running" -TimeoutSeconds 5)
        $formRecord = Get-CurrentSurfaceRecord $baseUrl $attach.form.instanceId
        $cancelAck = Get-PropertyValue (Get-PropertyValue $formRecord "eventAcks") $cancelEvent.eventId
        [void](Invoke-JsonPost "$baseUrl/v1/surfaces/actions/cancel" @{
            protocolVersion = "loom.surface.v1"; instanceId = $attach.form.instanceId; requestId = $cancelAck.requestId; deviceId = $localDevice
        })
        $cancelFinal = Wait-SurfaceAction $baseUrl $attach.form.instanceId $cancelEvent.eventId
        Assert-Equal "cancelled" ([string]$cancelFinal.ack.status) "form cancellation did not terminate action"

        $preRestartDashboard = Get-CurrentSurfaceRecord $baseUrl $attach.dashboard.instanceId
        $preRestartFirstAttachment = Get-SurfaceAttachment $preRestartDashboard $attach.dashboard.firstAttachmentId
        $preRestartRevision = [int64]$preRestartFirstAttachment.snapshot.revision
        $temporary = Invoke-JsonPost "$baseUrl/v1/surfaces/attach" @{
            artId = $artNames.stock.id; hookNodeId = "hook-node:temporary"; deviceId = $localDevice
            capabilities = New-SurfaceHostCapabilities; persistence = "temporary"
        }
        $temporaryInstanceId = [string]$temporary.instance.descriptor.instanceId

        Stop-SmokeProcess $daemonProcess
        $daemonProcess = $null
        Start-Sleep -Milliseconds 300
        $daemonProcess = Start-SurfaceDaemon -FilePath $daemonExe -WorkingDirectory $daemonWorkingDirectory -Port $daemonPort `
            -ControlPlaneRoot $controlPlaneRoot -ArtStoreUrl "http://127.0.0.1:$storePort" -AppDataRoot $appDataRoot `
            -LocalAppDataRoot $localAppDataRoot -LogsRoot $logsRoot -LogStem "daemon-restart"
        [void](Wait-Http "$baseUrl/health")

        $instancesAfterRestart = Invoke-JsonGet "$baseUrl/v1/surfaces/instances"
        $persistentIds = @($instancesAfterRestart.instances | ForEach-Object { [string]$_.descriptor.instanceId })
        Assert-True ($persistentIds -contains $attach.dashboard.instanceId) "persistent dashboard instance was lost after daemon restart"
        Assert-True ($persistentIds -contains $attach.stock.instanceId) "persistent stock instance was lost after daemon restart"
        Assert-True ($persistentIds -contains $attach.form.instanceId) "persistent form instance was lost after daemon restart"
        Assert-True ($persistentIds -notcontains $temporaryInstanceId) "temporary Surface instance survived daemon restart"

        $streamRecovery = Invoke-JsonGet "$baseUrl/v1/surfaces/stream?after=0&timeoutMs=1"
        $dashboardRecovery = @($streamRecovery.messages | Where-Object {
            [string]$_.method -eq "loom.surface.snapshot" -and [string]$_.params.hookNodeId -eq "hook-node:dashboard-one"
        } | Select-Object -First 1)
        Assert-Equal 1 $dashboardRecovery.Count "Surface stream did not replay the dashboard snapshot after restart"
        Assert-Equal $preRestartRevision ([int64]$dashboardRecovery[0].params.snapshot.revision) "replayed Surface snapshot revision mismatch"

        $remountedDashboard = Invoke-JsonPost "$baseUrl/v1/surfaces/instances/$($attach.dashboard.instanceId)/mount" @{
            attachmentId = $attach.dashboard.firstAttachmentId
        }
        $remountedAttachment = Get-SurfaceAttachment $remountedDashboard.instance $attach.dashboard.firstAttachmentId
        Assert-Equal ($preRestartRevision + 1) ([int64]$remountedAttachment.snapshot.revision) "remount did not publish the next full Surface snapshot revision"
        Assert-Equal "ready" ([string]$remountedAttachment.snapshot.authoritativeState.status) "remount lost authoritative Surface state"

        $remountedLease = @($remountedAttachment.snapshot.resourceLeases)[0]
        Assert-True ($null -ne $remountedLease) "remounted dashboard resource lease is missing"
        $resourceEvidence = Invoke-SurfaceResourceGet $baseUrl $remountedLease (Join-Path $runRoot "dashboard-chart.bin")
        Assert-SurfaceResourceRejectsWrongLease $baseUrl $remountedLease

        $postRestartToggle = Invoke-SurfaceAction $baseUrl $attach.dashboard.instanceId $attach.dashboard.firstAttachmentId `
            $localDevice "auto_sync" "change" "dashboard_toggle" "discrete" @{ value = $false }
        $postRestartToggleFinal = Wait-SurfaceAction $baseUrl $attach.dashboard.instanceId $postRestartToggle.eventId
        Assert-Equal "succeeded" ([string]$postRestartToggleFinal.ack.status) "Surface action failed after daemon restart"
        $postRestartFirstAttachment = Get-SurfaceAttachment $postRestartToggleFinal.record $attach.dashboard.firstAttachmentId
        Assert-Equal $false ([bool]$postRestartFirstAttachment.snapshot.authoritativeState.autoSync) "post-restart Surface action did not update authoritative state"

        $summary = [ordered]@{
            schemaVersion = 2; protocolVersion = "loom.surface.v1"; runId = $runId; configuration = $Configuration
            packageDir = $packageFullPath; daemonPath = $daemonExe; isolatedControlPlane = $true; runRoot = $runRoot
            frameworkInstalled = [bool]$frameworkInstall.framework.ready; artInstall = $artInstall; attachments = $attach
            stock = @{ action = $stockRefresh.eventId; formalResult = $stockFinal.record.latestResult }
            dashboard = @{ instanceReused = ($firstDashboard.instance.descriptor.instanceId -eq $secondDashboard.instance.descriptor.instanceId); revisions = @($dashboardAttachments | ForEach-Object { $_.snapshot.revision }); leaseIds = $leases; formalResult = $dashboardRecord.latestResult; resource = $resourceEvidence }
            form = @{ confirmed = $formFinal.ack.status -eq "succeeded"; cancelled = $cancelFinal.ack.status -eq "cancelled"; cancellationAck = $cancelFinal.ack }
            restartRecovery = @{ persistentInstanceCount = $persistentIds.Count; temporaryInstanceRemoved = $persistentIds -notcontains $temporaryInstanceId; replayedRevision = [int64]$dashboardRecovery[0].params.snapshot.revision; remountedRevision = [int64]$remountedAttachment.snapshot.revision; postRestartAction = [string]$postRestartToggleFinal.ack.status }
        }
        [IO.File]::WriteAllText($summaryPath, ($summary | ConvertTo-Json -Depth 80) + "`n", [Text.UTF8Encoding]::new($false))
        Write-Output ($summary | ConvertTo-Json -Depth 20)
    } finally {
        Stop-SmokeProcess $daemonProcess
        Stop-SmokeProcess $storeProcess
    }
} catch {
    if (-not (Test-Path -LiteralPath $summaryPath)) {
        [IO.File]::WriteAllText($summaryPath, (([ordered]@{ schemaVersion = 1; runId = $runId; error = $_.Exception.Message } | ConvertTo-Json -Depth 20) + "`n"), [Text.UTF8Encoding]::new($false))
    }
    throw
}
