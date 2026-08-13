[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
. (Join-Path $PSScriptRoot "LoomReleaseLayout.ps1")
. (Join-Path $PSScriptRoot "LoomSmokePorts.ps1")
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

        $port = Get-LoomSmokePort
        $manifestDir = Join-Path $tempRoot "capabilities"
        New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
        $stdout = Join-Path $tempRoot "loom-daemon.stdout.log"
        $stderr = Join-Path $tempRoot "loom-daemon.stderr.log"
        $controlPlaneRoot = Join-Path $tempRoot "loom-control-plane"
        $oldHost = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_HOST", "Process")
        $oldPort = [Environment]::GetEnvironmentVariable("LOOM_DAEMON_PORT", "Process")
        $oldControlPlaneRoot = [Environment]::GetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", "Process")
        $oldMcpRegistryEndpoint = [Environment]::GetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", "Process")
        $mcpRegistryFixtureDir = Join-Path $tempRoot "mcp-registry-fixture"
        $mcpRegistryPort = Get-LoomSmokePort
        $mcpRegistryJob = Start-LoomMcpRegistryFixtureJob -Port $mcpRegistryPort -OutputDir $mcpRegistryFixtureDir
        Wait-ForPath -Path (Join-Path $mcpRegistryFixtureDir "ready.txt") -TimeoutSeconds 20
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_HOST", "127.0.0.1", "Process")
        [Environment]::SetEnvironmentVariable("LOOM_DAEMON_PORT", [string]$port, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_CONTROL_PLANE_ROOT", $controlPlaneRoot, "Process")
        [Environment]::SetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", "http://127.0.0.1:$mcpRegistryPort/v0/servers", "Process")
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
            [Environment]::SetEnvironmentVariable("LOOM_MCP_REGISTRY_ENDPOINT", $oldMcpRegistryEndpoint, "Process")
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
            transport = "stdio"
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
            transport = "stdio"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal $true ([bool]$mcpConnectionTest.success) "Loom MCP connection test success mismatch."
        Assert-Equal "echo" ([string]$mcpConnectionTest.tools[0].name) "Loom MCP connection test tool name mismatch."
        Assert-Equal "release-fixture" ([string]$mcpConnectionTest.server_info.serverInfo.name) "Loom MCP connection test server info mismatch."

        # POST /v1/mcp/package/check and POST /v1/mcp/package/install-plan
        $mcpPackageCheck = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/package/check" -Body @{
            moduleName = "json"
        }
        Assert-Equal "json" ([string]$mcpPackageCheck.module) "Loom MCP package check module mismatch."
        $mcpPackageInstallPlan = Invoke-JsonPost -Uri "$baseUrl/v1/mcp/package/install-plan" -Body @{
            packageName = "mcp-server-demo"
        }
        Assert-Equal $false ([bool]$mcpPackageInstallPlan.sideEffect) "Loom MCP package install plan must be side-effect free."
        Assert-Contains "pip" (($mcpPackageInstallPlan.command | ForEach-Object { [string]$_ }) -join " ") "Loom MCP package install plan should include pip command."


        $cloudFixtureDir = Join-Path $tempRoot "cloud-api-fixture"
        $cloudPort = Get-LoomSmokePort
    $cloudJob = Start-LoomCloudApiFixtureJob -Port $cloudPort -OutputDir $cloudFixtureDir -MaxRequests 1
        Wait-ForPath -Path (Join-Path $cloudFixtureDir "ready.txt") -TimeoutSeconds 20

        $savedFixtureMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/fixture" -Body @{
            id = "fixture"
            name = "Fixture MCP"
            transport = "stdio"
            command = "powershell.exe"
            args = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureMcpScript)
            env = @{}
            enabled = $true
        }
        Assert-Equal "fixture" $savedFixtureMcpServer.server.id "Loom fixture MCP server save id mismatch."

        $savedDeleteMcpServer = Invoke-JsonPut -Uri "$baseUrl/v1/mcp/servers/fixture-delete" -Body @{
            id = "fixture-delete"
            name = "Fixture Delete MCP"
            transport = "stdio"
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
                type = "workflow"
                workflowId = "fixture-delete-workflow"
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
            description = "Release smoke multipart cloud API image Art"
            enabled = $true
            execution = @{
                type = "cloud_api"
                endpoint = "http://127.0.0.1:$cloudPort/multipart/{{inputs.route.value}}"
                method = "POST"
                contentType = "multipart/form-data"
                headers = '{"X-Trace":"{{inputs.trace.value}}"}'
                body = '{"file":"{{inputs.input.path}}","prompt":"{{inputs.prompt.value}}"}'
            }
        }
        Assert-Equal "fixture-cloud-multipart-art" $savedCloudMultipartArtTool.tool.id "Loom cloud multipart Art tool save id mismatch."
        Assert-Equal "cloud_api" $savedCloudMultipartArtTool.tool.execution.type "Loom cloud multipart Art execution type mismatch."
        Assert-Equal "multipart/form-data" ([string]$savedCloudMultipartArtTool.tool.execution.contentType) "Loom cloud multipart contentType save mismatch."

        $workflowYaml = @"
name: Release Workflow Runtime
nodes:
  - id: image
    uses: core.image.invert
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
    uses: core.image.invert
"@
        $savedDeleteWorkflow = Invoke-JsonPut -Uri "$baseUrl/v1/workflows/fixture-delete-workflow" -Body @{
            data = $deleteWorkflowYaml
        }
        Assert-Equal "fixture-delete-workflow" ([string]$savedDeleteWorkflow.workflow.id) "Loom delete workflow save id mismatch."
        # DELETE /v1/workflows/fixture-delete-workflow
        $deletedWorkflow = Invoke-JsonDelete -Uri "$baseUrl/v1/workflows/fixture-delete-workflow"
        Assert-Equal $true ([bool]$deletedWorkflow.deleted) "Loom workflow deletion mismatch."
        $workflowInput = New-LoomNativeImageSmokePngDataUrl
        $executedWorkflowTool = Invoke-JsonPost -Uri "$baseUrl/v1/tools/release-workflow-tool/execute" -Body @{
            arguments = @{ input_base64 = $workflowInput }
        }
        Assert-Equal "succeeded" $executedWorkflowTool.status "Loom workflow-backed tool execution status mismatch."
        Assert-Equal "image" ([string]$executedWorkflowTool.result.content[0].type) "Loom workflow-backed tool content type mismatch."
        if ([string]::IsNullOrWhiteSpace([string]$executedWorkflowTool.result.content[0].data)) {
            throw "Loom workflow-backed image output missing."
        }

        $imageHelperConvert = Test-LoomImageHelperConvert -BaseUrl $baseUrl

        $hookBridge = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/status"
        Assert-Equal 19820 ([int]$hookBridge.port) "Loom Hook Bridge default port mismatch."
        Assert-Equal $false ([bool]$hookBridge.running) "Loom Hook Bridge protocol-only smoke state mismatch."
        Assert-Equal "loom.hook.v1" ([string]$hookBridge.protocol) "Loom Hook Bridge protocol mismatch."
        $hookBridgeMethods = @($hookBridge.methods | ForEach-Object { [string]$_ })
        Assert-Contains "loom.hook.handshake" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing handshake."
        Assert-Contains "loom.hook.workflow.node.update" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing workflow node update."
        Assert-Contains "loom.hook.workflow.instantiate" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing workflow instantiate."
        Assert-Contains "loom.hook.art.execute" ($hookBridgeMethods -join ",") "Loom Hook Bridge method catalog missing Art execution."
        $hookSession = Invoke-JsonGet -Uri "$baseUrl/v1/hook-bridge/session"
        Assert-Equal "loom.hook.v1" ([string]$hookSession.protocolVersion) "Loom Hook session protocol mismatch."

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

        $tokenPort = Get-LoomSmokePort
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
                hookProtocol = [string]$hookBridge.protocol
                hookSessionAvailable = [bool]$hookSession.available
                mcpToolExecution = [string]$executedMcpTool.result.content[0].text
                managementCrud = [ordered]@{
                    mcpServerDeleted = [bool]$deletedMcpServer.deleted
                    toolDeleted = [bool]$deletedTool.deleted
                    workflowLoaded = [string]$loadedWorkflow.workflow.id
                    workflowDeleted = [bool]$deletedWorkflow.deleted
                }
                mcpMarketplace = [ordered]@{
                    registryServerCount = @($mcpRegistry.servers).Count
                    registryServerName = [string]$mcpRegistry.servers[0].server.name
                    connectionTestSuccess = [bool]$mcpConnectionTest.success
                    connectionTestTool = [string]$mcpConnectionTest.tools[0].name
                    connectionTestServer = [string]$mcpConnectionTest.server_info.serverInfo.name
                    packageCheckModule = [string]$mcpPackageCheck.module
                    packageInstallSideEffect = [bool]$mcpPackageInstallPlan.sideEffect
                }
                cloudToolExecution = [string]$executedCloudTool.result.content[0].text
                workflowToolExecution = [string]$executedWorkflowTool.result.content[0].type
                imageHelperConvert = $imageHelperConvert
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
