param(
    [string]$ArtifactRoot = ".loom-art-store-data\arts"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$artifactRootPath = if ([System.IO.Path]::IsPathRooted($ArtifactRoot)) {
    [System.IO.Path]::GetFullPath($ArtifactRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
}
$workRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-sample-art-runtime-" + [guid]::NewGuid().ToString("N"))
$image = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII="
$cases = @(
    @{ id = "custom-1770146354922"; framework = "cli_wrapper"; params = @{ quality_num = 90; lossless = $true }; inputs = @{ input = $image } },
    @{ id = "custom-remove-bg-cloud"; framework = "cloud_api"; params = @{}; inputs = @{ input = $image } },
    @{ id = "custom-image-search"; framework = "mcp"; params = @{ query = "loom smoke"; count = 3 }; inputs = @{} },
    @{ id = "custom-1770131241684"; framework = "python_art"; params = @{ strength = 50 }; inputs = @{ input = $image; reference = $image } },
    @{ id = "custom-image-blend-script"; framework = "script"; params = @{ mix_ratio = 50 }; inputs = @{ input = $image; reference = $image } },
    @{ id = "custom-image-blend-compress-workflow"; framework = "workflow"; params = @{ mix_ratio = 50; quality_num = 90 }; inputs = @{ input = $image; reference = $image } }
)

function Invoke-Runtime {
    param(
        [Parameter(Mandatory = $true)][string]$ArtDirectory,
        [Parameter(Mandatory = $true)][hashtable]$Case
    )

    $request = [ordered]@{
        protocolVersion = "loom.framework.v1"
        frameworkId = $Case.framework
        artId = $Case.id
        artDir = $ArtDirectory
        inputs = $Case.inputs
        params = $Case.params
        disabledParams = @()
        context = @{
            requestId = "smoke-$($Case.id)"
            cacheDir = (Join-Path $ArtDirectory ".cache")
            tempDir = (Join-Path $ArtDirectory ".temp")
        }
    }
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    if ($Case.framework -eq "python_art") {
        $psi.FileName = "python.exe"
        $psi.Arguments = "`"runtime\main.py`""
    }
    else {
        $psi.FileName = "powershell.exe"
        $psi.Arguments = "-NoProfile -ExecutionPolicy Bypass -File `"runtime\main.ps1`""
    }
    $psi.WorkingDirectory = $ArtDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $psi
    Assert-True $process.Start() "Failed to start Art runtime: $($Case.id)"
    $process.StandardInput.WriteLine(($request | ConvertTo-Json -Depth 20 -Compress))
    $process.StandardInput.Close()
    $stdout = $process.StandardOutput.ReadToEnd()
    $stderr = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    Assert-True ($process.ExitCode -eq 0) "$($Case.id) runtime exited with $($process.ExitCode): $stderr"
    Assert-True (-not [string]::IsNullOrWhiteSpace($stdout)) "$($Case.id) runtime returned no stdout: $stderr"
    $response = $stdout.Trim() | ConvertFrom-Json
    Assert-True ([string]$response.status -eq "success") "$($Case.id) runtime failed: $stdout $stderr"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$response.output.output_base64)) "$($Case.id) output image is missing: $stdout"
    return $response
}

New-Item -ItemType Directory -Force -Path $workRoot | Out-Null
try {
    foreach ($case in $cases) {
        $zipPath = Join-Path $artifactRootPath "$($case.id).zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Missing Art ZIP for runtime smoke: $zipPath"
        $artDirectory = Join-Path $workRoot $case.id
        Expand-Archive -LiteralPath $zipPath -DestinationPath $artDirectory -Force
        $response = Invoke-Runtime -ArtDirectory $artDirectory -Case $case
        $candidateProperty = $response.PSObject.Properties["candidates"]
        $candidateCount = if ($null -ne $candidateProperty) { @($candidateProperty.Value).Count } else { 0 }
        Write-Host ("PASS {0}: output={1} candidates={2}" -f $case.id, ([string]$response.output.output_base64).Length, $candidateCount)
    }
}
finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Sample Art runtime smoke passed for $($cases.Count) packages."
