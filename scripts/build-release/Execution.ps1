<# Owns one release-script responsibility. #>

function Copy-LoomLockedFile {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    $sourceFullPath = [System.IO.Path]::GetFullPath($Source)
    Assert-LoomPathHasNoReparsePoints -RootPath (Split-Path -Parent $sourceFullPath) -Path $sourceFullPath
    $input = [System.IO.FileStream]::new($sourceFullPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::Read)
    try {
        $output = [System.IO.FileStream]::new($Destination, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
        try {
            $input.CopyTo($output)
            $output.Flush($true)
        }
        finally {
            $output.Dispose()
        }
    }
    finally {
        $input.Dispose()
    }
}

function ConvertTo-LoomGitHubCommandValue {
    param([string]$Value)
    return $Value.Replace("%", "%25").Replace("`r", "%0D").Replace("`n", "%0A")
}

function Invoke-CommandToLog {
    param(
        [System.Collections.Specialized.OrderedDictionary]$Command,
        [string]$LogPath
    )

    $redactedArguments = [System.Collections.Generic.List[string]]::new()
    $redactNextArgument = $false
    foreach ($argument in @($Command.arguments)) {
        $argumentText = [string]$argument
        if ($redactNextArgument) {
            $redactedArguments.Add("[REDACTED]")
            $redactNextArgument = $false
        }
        elseif ($argumentText -match '^(?i:--?(?:token|secret|password|api[_-]?key))$') {
            $redactedArguments.Add($argumentText)
            $redactNextArgument = $true
        }
        else {
            $redactedArguments.Add([regex]::Replace(
                $argumentText,
                '(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*\S+',
                '$1=[REDACTED]'
            ))
        }
    }
    $header = @(
        "Command: $($Command.display)"
        "Executable: $($Command.executable)"
        "Arguments: $($redactedArguments.ToArray() -join ' ')"
        "Working directory: $($Command.workingDirectory)"
        "Started at: $(Get-Date -Format o)"
        ""
    ) -join [Environment]::NewLine
    Write-Utf8NoBom -Path $LogPath -Value $header
    $encoding = [System.Text.UTF8Encoding]::new($false)
    $logStream = [System.IO.FileStream]::new(
        $LogPath,
        [System.IO.FileMode]::Append,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::Read
    )
    $writer = [System.IO.StreamWriter]::new($logStream, $encoding, 4096, $true)
    $writtenBytes = [int64]$logStream.Length

    $locationPushed = $false
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        Push-Location -LiteralPath $Command.workingDirectory
        $locationPushed = $true
        $ErrorActionPreference = "Continue"
        & $Command.executable @($Command.arguments) 2>&1 | ForEach-Object {
            $line = $_.ToString()
            $line = [regex]::Replace($line, '(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*\S+', '$1=[REDACTED]')
            $lineBytes = $encoding.GetByteCount($line + [Environment]::NewLine)
            if ($writtenBytes + $lineBytes -le 16MB) {
                $writer.WriteLine($line)
                $writtenBytes += $lineBytes
            }
        }
        $exitCode = $LASTEXITCODE
    }
    catch {
        $line = [regex]::Replace($_.Exception.ToString(), '(?i)(token|secret|password|api[_-]?key)\s*[:=]\s*\S+', '$1=[REDACTED]')
        $line = $line.Substring(0, [Math]::Min($line.Length, 64KB))
        $lineBytes = $encoding.GetByteCount($line + [Environment]::NewLine)
        if ($writtenBytes + $lineBytes -le 16MB) {
            $writer.WriteLine($line)
        }
        $exitCode = 1
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
        if ($locationPushed) {
            Pop-Location
        }
        $writer.Flush()
        $writer.Dispose()
        $logStream.Dispose()
    }

    if ($exitCode -ne 0) {
        if ([string]::Equals($env:GITHUB_ACTIONS, "true", [System.StringComparison]::OrdinalIgnoreCase)) {
            $logTail = @(Get-Content -LiteralPath $LogPath -Encoding UTF8 -Tail 40)
            $diagnostic = @($logTail | Select-Object -Last 20) -join " | "
            if ($diagnostic.Length -gt 2000) {
                $diagnostic = $diagnostic.Substring($diagnostic.Length - 2000)
            }
            $detail = ConvertTo-LoomGitHubCommandValue -Value $diagnostic
            Write-Output "::error title=Loom release build command failed::$detail"
        }
        throw "Build command failed with exit code ${exitCode}: $($Command.display). See $LogPath"
    }
}

function Copy-PayloadFile {
    param(
        [string]$PackageRoot,
        [string]$Source,
        [string]$Destination,
        [string]$RelativePath,
        [string]$Kind
    )

    if (-not (Test-Path -LiteralPath $Source -PathType Leaf)) {
        throw "Required Loom build input is missing: $Source"
    }
    $sourceFullPath = [System.IO.Path]::GetFullPath($Source)
    Assert-LoomPathHasNoReparsePoints -RootPath (Split-Path -Parent $sourceFullPath) -Path $sourceFullPath
    $resolvedDestination = Resolve-LoomPackageRelativePath -PackageDir $PackageRoot -RelativePath $RelativePath
    if (-not [string]::Equals($resolvedDestination, [System.IO.Path]::GetFullPath($Destination), [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Loom payload destination does not match its package-relative path: $RelativePath"
    }
    $parent = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    Assert-LoomPathHasNoReparsePoints -RootPath $PackageRoot -Path $parent
    Copy-LoomLockedFile -Source $sourceFullPath -Destination $Destination
    Assert-LoomPathHasNoReparsePoints -RootPath $PackageRoot -Path $Destination
    $file = Get-Item -LiteralPath $Destination
    $digest = Get-LoomFileDigest -Path $Destination
    return [ordered]@{
        kind = $Kind
        name = $file.Name
        path = $RelativePath.Replace("/", "\")
        bytes = [int64]$digest.bytes
        sha256 = $digest.sha256
    }
}
