<# Owns daemon health readiness and bounded focused-smoke child execution. #>

function Wait-LoomDaemonHealth {
    param(
        [string]$BaseUrl,
        [string]$Message
    )

    Assert-SmokeLoopbackUri -Uri "$BaseUrl/health"
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

    if (-not [string]::Equals([System.IO.Path]::GetFileName($ScriptName), $ScriptName, [System.StringComparison]::Ordinal) -or
        [System.IO.Path]::GetExtension($ScriptName) -ne ".ps1") {
        throw "Focused smoke script name must be a single PowerShell path segment: $ScriptName"
    }
    if ($EvidenceSubdirectory -notmatch '^[A-Za-z0-9._-]+$') {
        throw "Focused smoke evidence subdirectory must be a safe path segment: $EvidenceSubdirectory"
    }

    $scriptPath = Join-Path $PSScriptRoot $ScriptName
    Assert-PathExists $scriptPath
    $focusedEvidenceRoot = Join-Path $EvidenceRoot $EvidenceSubdirectory
    $powerShellExe = Join-Path $PSHOME "powershell.exe"
    $run = Invoke-ProcessCapture `
        -FilePath $powerShellExe `
        -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $scriptPath, "-PackageDir", $PackageDir, "-EvidenceRoot", $focusedEvidenceRoot) `
        -TimeoutSeconds 300
    if ($run.exitCode -ne 0) {
        throw "Focused Loom smoke failed ($ScriptName): $(Redact-SmokeFailureText -Text ([string]$run.output))"
    }
    return [ordered]@{
        script = $ScriptName
        status = "passed"
        evidenceRoot = $focusedEvidenceRoot
    }
}
