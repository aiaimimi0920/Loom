<# Owns shared IO, process, HTTP, and cleanup primitives for the sample Art install smoke. #>

function Assert-True {
    param([bool]$Condition, [string]$Message)

    if (-not $Condition) {
        throw $Message
    }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    }
    finally {
        $listener.Stop()
    }
}

function Get-PropertyValue {
    param([AllowNull()][object]$Value, [string]$Name)

    if ($null -eq $Value) { return $null }
    $property = $Value.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Read-BoundedFileBytes {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int64]$MaximumBytes = (128MB)
    )

    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
    try {
        $length = $stream.Length
        if ($length -gt $MaximumBytes -or $length -gt [int]::MaxValue) {
            throw "Package exceeds the $MaximumBytes byte smoke-test limit: $Path"
        }
        $bytes = New-Object byte[] ([int]$length)
        $offset = 0
        while ($offset -lt $bytes.Length) {
            $read = $stream.Read($bytes, $offset, $bytes.Length - $offset)
            if ($read -eq 0) {
                throw "Package changed or ended while being read: $Path"
            }
            $offset += $read
        }
        if ($stream.ReadByte() -ne -1) {
            throw "Package grew while being read: $Path"
        }
        return ,$bytes
    }
    finally {
        $stream.Dispose()
    }
}

function Install-Zip {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [Parameter(Mandatory = $true)][string]$Prefix
    )

    $bytes = Read-BoundedFileBytes -Path $ZipPath
    $encoded = "data:application/zip;base64,$([Convert]::ToBase64String($bytes))"
    return Invoke-LoomJson -Method Post -Url ($Url.TrimEnd('/') + $Prefix + "/install") -Body @{ zipBase64 = $encoded }
}

function Install-McpZip {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [Parameter(Mandatory = $true)][string]$ZipPath
    )

    $bytes = Read-BoundedFileBytes -Path $ZipPath
    return Invoke-LoomJson -Method Post -Url ($Url.TrimEnd('/') + "/v1/mcp/servers/install") -Body @{
        zipBase64 = [Convert]::ToBase64String($bytes)
    }
}

function Write-Utf8NoBomFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    [System.IO.File]::WriteAllText($Path, $Content, [System.Text.UTF8Encoding]::new($false))
}

function ConvertTo-WindowsCommandLineArgument {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Argument)

    if ($Argument.Length -eq 0) { return '""' }
    if ($Argument -notmatch '[\s"]') { return $Argument }
    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashes = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq '\') {
            $backslashes++
            continue
        }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * (($backslashes * 2) + 1)))
            [void]$builder.Append('"')
            $backslashes = 0
            continue
        }
        if ($backslashes -gt 0) {
            [void]$builder.Append(('\' * $backslashes))
            $backslashes = 0
        }
        [void]$builder.Append($character)
    }
    if ($backslashes -gt 0) {
        [void]$builder.Append(('\' * ($backslashes * 2)))
    }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Join-WindowsCommandLine {
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments)

    return (@($Arguments | ForEach-Object { ConvertTo-WindowsCommandLineArgument -Argument $_ }) -join " ")
}

function Start-PowerShellFixtureProcess {
    param(
        [Parameter(Mandatory = $true)][string]$ScriptPath,
        [Parameter(Mandatory = $true)][hashtable]$Parameters,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )

    $invocationId = [Guid]::NewGuid().ToString("N")
    $invocationRoot = Split-Path -Parent $StdoutPath
    $payloadPath = Join-Path $invocationRoot "fixture-invocation-$invocationId.json"
    $launcherPath = Join-Path $invocationRoot "fixture-launcher-$invocationId.ps1"
    $payload = [ordered]@{
        scriptPath = [System.IO.Path]::GetFullPath($ScriptPath)
        parameters = $Parameters
    }
    Write-Utf8NoBomFile -Path $payloadPath -Content (($payload | ConvertTo-Json -Depth 5 -Compress) + "`n")
    $launcherSource = @'
param(
    [string]$InvocationPath,
    [string]$StdoutPath,
    [string]$StderrPath
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
try {
    $payload = Get-Content -Raw -Encoding UTF8 -LiteralPath $InvocationPath | ConvertFrom-Json
    $parameters = @{}
    foreach ($property in $payload.parameters.PSObject.Properties) {
        $parameters[$property.Name] = $property.Value
    }
    & ([string]$payload.scriptPath) @parameters 1> $StdoutPath 2> $StderrPath
    if (Test-Path Variable:LASTEXITCODE) { exit [int]$LASTEXITCODE }
    exit 0
}
catch {
    [System.IO.File]::AppendAllText($StderrPath, ($_ | Out-String), [System.Text.UTF8Encoding]::new($false))
    exit 1
}
'@
    Write-Utf8NoBomFile -Path $launcherPath -Content ($launcherSource + "`n")
    $tokens = @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $launcherPath,
        "-InvocationPath", $payloadPath, "-StdoutPath", $StdoutPath, "-StderrPath", $StderrPath
    )
    $processInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $processInfo.FileName = Join-Path $PSHOME "powershell.exe"
    $processInfo.Arguments = Join-WindowsCommandLine -Arguments $tokens
    $processInfo.UseShellExecute = $false
    $processInfo.CreateNoWindow = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $processInfo
    Assert-True $process.Start() "Failed to start PowerShell fixture: $ScriptPath"
    return $process
}

function Redact-SensitiveText {
    param([AllowNull()][string]$Text)

    if ($null -eq $Text) { return "" }
    $redacted = $Text -replace '(?i)(authorization\s*:\s*bearer\s+)[^\s\r\n]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)([?&](?:access[_-]?token|auth(?:orization)?[_-]?token|api[_-]?key|password|secret|cookie|token)=)[^&\s#]+', '$1<redacted>'
    $redacted = $redacted -replace '(?i)("(?:access[_-]?token|auth(?:orization)?[_-]?token|api[_-]?key|password|secret|cookie|token)"\s*:\s*")[^"]*"', '$1<redacted>"'
    $redacted = $redacted -replace '(?i)((?:access[_-]?token|auth(?:orization)?[_-]?token|api[_-]?key|password|secret|cookie|token)\s*[=:]\s*)[^\s,;}\r\n&]+', '$1<redacted>'
    return $redacted.Replace("loom-package-smoke-key", "<redacted>")
}

function Read-BoundedUtf8Text {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int]$MaximumBytes = (1MB)
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return "" }
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $readLength = [int][Math]::Min($stream.Length, [int64]$MaximumBytes)
        $buffer = New-Object byte[] $readLength
        $offset = 0
        while ($offset -lt $buffer.Length) {
            $read = $stream.Read($buffer, $offset, $buffer.Length - $offset)
            if ($read -eq 0) { break }
            $offset += $read
        }
        $text = [System.Text.Encoding]::UTF8.GetString($buffer, 0, $offset)
        if ($stream.Length -gt $MaximumBytes) {
            $text += "`r`n<loom-log-truncated totalBytes=$($stream.Length)>"
        }
        return $text
    }
    finally {
        $stream.Dispose()
    }
}

function Read-BoundedRedactedText {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int]$MaximumBytes = (1MB)
    )

    return Redact-SensitiveText (Read-BoundedUtf8Text -Path $Path -MaximumBytes $MaximumBytes)
}

function Stop-ProcessTree {
    param([AllowNull()][System.Diagnostics.Process]$Process)

    if ($null -eq $Process) { return }
    try {
        if ($Process.HasExited) { return }
        $rootId = $Process.Id
        $descendants = @()
        $pendingParents = @($rootId)
        $allProcesses = @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue)
        while ($pendingParents.Count -gt 0) {
            $parentId = [int]$pendingParents[0]
            $pendingParents = @($pendingParents | Select-Object -Skip 1)
            foreach ($child in @($allProcesses | Where-Object { [int]$_.ParentProcessId -eq $parentId })) {
                $childId = [int]$child.ProcessId
                $descendants += $childId
                $pendingParents += $childId
            }
        }
        [array]::Reverse($descendants)
        foreach ($processId in $descendants) {
            Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
        }
        Stop-Process -Id $rootId -Force -ErrorAction SilentlyContinue
        $null = $Process.WaitForExit(5000)
    }
    catch {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    }
}

function Remove-VerifiedTemporaryTree {
    param([Parameter(Mandatory = $true)][string]$Path)

    $separator = [System.IO.Path]::DirectorySeparatorChar
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd([char[]]@('\', '/')) + $separator
    $resolved = [System.IO.Path]::GetFullPath($Path)
    $leaf = [System.IO.Path]::GetFileName($resolved.TrimEnd([char[]]@('\', '/')))
    if (-not $resolved.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or $leaf -notmatch '^loom-sample-art-install-[0-9a-f]{32}$') {
        throw "Refusing to remove an unexpected temporary path: $resolved"
    }
    if (-not (Test-Path -LiteralPath $resolved)) { return }
    $pending = @($resolved)
    $entries = @()
    while ($pending.Count -gt 0) {
        $current = [string]$pending[0]
        $pending = @($pending | Select-Object -Skip 1)
        $attributes = [System.IO.File]::GetAttributes($current)
        if (($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Refusing to traverse a reparse point during cleanup: $current"
        }
        if (($attributes -band [System.IO.FileAttributes]::Directory) -ne 0) {
            foreach ($entry in [System.IO.Directory]::EnumerateFileSystemEntries($current)) {
                $entries += $entry
                $pending += $entry
            }
        }
    }
    foreach ($entry in @($entries | Sort-Object { $_.Length } -Descending)) {
        $attributes = [System.IO.File]::GetAttributes($entry)
        if (($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Refusing to remove a reparse point during cleanup: $entry"
        }
        if (($attributes -band [System.IO.FileAttributes]::ReadOnly) -ne 0) {
            $writableAttributes = $attributes -band (-bnot [System.IO.FileAttributes]::ReadOnly)
            [System.IO.File]::SetAttributes($entry, [System.IO.FileAttributes]$writableAttributes)
        }
        if (($attributes -band [System.IO.FileAttributes]::Directory) -ne 0) {
            [System.IO.Directory]::Delete($entry, $false)
        }
        else {
            [System.IO.File]::Delete($entry)
        }
    }
    [System.IO.Directory]::Delete($resolved, $false)
}
