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
$imageSecond = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
$imageSearchFrameworkData = @{
    mcp = @{
        serverId = "fixture"
        toolName = "brave_image_search"
        arguments = @{ query = "loom smoke"; count = 3 }
        result = @{
            structuredContent = @{
                type = "object"
                items = @(
                    @{
                        title = "Fixture image one"
                        url = "https://example.invalid/source"
                        properties = @{ url = $image; width = 1; height = 1 }
                    },
                    @{
                        title = "Fixture image two"
                        url = "https://example.invalid/source-two"
                        properties = @{ url = $imageSecond; width = 1; height = 1 }
                    }
                )
            }
        }
    }
}
$colorTransferParams = @{
    strength = 80
    gamma = 1.2
    exposure = -0.2
    contrast = 10
    highlights = -5
    shadows = 5
    whites = 3
    blacks = -3
    temperature = 10
    tint = -5
    saturation = 12
    vibrance = 8
    hue = 15
    split_h_hue = 35
    split_h_sat = 10
    split_s_hue = 215
    split_s_sat = 8
    split_balance = -10
    skin_protection = $true
}
$cases = @(
    @{ id = "custom-1770146354922"; framework = "process"; params = @{ quality_num = 90; lossless = $true }; inputs = @{ input = $image } },
    @{ id = "custom-remove-bg-cloud"; framework = "cloud_api"; params = @{}; inputs = @{ input = $image } },
    @{ id = "custom-image-search"; framework = "mcp"; params = @{ query = "loom smoke"; count = 3; result_index = 1 }; inputs = @{}; frameworkData = $imageSearchFrameworkData },
    @{ id = "custom-1770131241684"; framework = "process"; params = $colorTransferParams; inputs = @{ input = $image; reference = $image } },
    @{ id = "custom-1770131241684"; framework = "process"; params = $colorTransferParams; inputs = @{ input = $image; reference = $image; output_mode = "shader" }; expectShader = $true },
    @{ id = "custom-image-blend-script"; framework = "process"; params = @{ mix_ratio = 50 }; inputs = @{ input = $image; reference = $image } }
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
    if ($Case.ContainsKey("frameworkData")) {
        $request["frameworkData"] = $Case.frameworkData
    }
    $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $ArtDirectory "art.runtime.json") | ConvertFrom-Json
    $command = [string]$runtime.entry.command
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = if ($command -ieq "python.exe") {
        Join-Path $repoRoot "resources\python-embed\python.exe"
    }
    else {
        $command
    }
    $psi.Arguments = @($runtime.entry.args | ForEach-Object {
        '"' + ([string]$_).Replace('"', '\"') + '"'
    }) -join ' '
    $psi.WorkingDirectory = $ArtDirectory
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.Environment["PYTHONDONTWRITEBYTECODE"] = "1"
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
    if ($Case.ContainsKey("expectShader") -and [bool]$Case.expectShader) {
        Assert-True ([string]$response.output.type -eq "shader") "Color Transfer shader response type is invalid: $stdout"
        Assert-True (-not [string]::IsNullOrWhiteSpace([string]$response.output.vertex_shader)) "Color Transfer vertex shader is missing: $stdout"
        Assert-True (-not [string]::IsNullOrWhiteSpace([string]$response.output.fragment_shader)) "Color Transfer fragment shader is missing: $stdout"
        Assert-True ([string]$response.output.textures.lut -like "data:image/png;base64,*") "Color Transfer LUT texture is missing: $stdout"
        Assert-True ([double]$response.output.uniforms.gamma -eq 1.2) "Color Transfer shader gamma uniform was not preserved: $stdout"
        Assert-True ([double]$response.output.uniforms.skin_protection -eq 1.0) "Color Transfer shader skin protection uniform was not preserved: $stdout"
        return $response
    }
    if ($Case.id -eq "custom-1770146354922") {
        $outputPath = [string]$response.output.output_path
        Assert-True (-not [string]::IsNullOrWhiteSpace($outputPath)) "Image Compress output path is missing: $stdout"
        Assert-True (Test-Path -LiteralPath $outputPath -PathType Leaf) "Image Compress output file is missing: $outputPath"
        $outputBase64Property = $response.output.PSObject.Properties["output_base64"]
        $contentProperty = $response.output.PSObject.Properties["content"]
        Assert-True ($null -eq $outputBase64Property -or [string]::IsNullOrWhiteSpace([string]$outputBase64Property.Value)) "Image Compress must not duplicate the image through stdout: $stdout"
        Assert-True ($null -eq $contentProperty) "Image Compress must not embed image content in stdout: $stdout"
        Assert-True ($stdout.Length -lt 16384) "Image Compress stdout unexpectedly contains a large image payload: $($stdout.Length) bytes"
    }
    else {
        Assert-True (-not [string]::IsNullOrWhiteSpace([string]$response.output.output_base64)) "$($Case.id) output image is missing: $stdout"
    }
    if ($Case.id -eq "custom-1770131241684") {
        Assert-True ([string]$response.output.algorithm -eq "oklab-statistical-transfer") "Color Transfer did not execute the restored algorithm: $stdout"
        $appliedParameterIds = @($response.output.applied_params.PSObject.Properties.Name)
        Assert-True ($appliedParameterIds.Count -eq 19) "Color Transfer runtime did not receive all 19 parameters: $stdout"
        foreach ($key in $Case.params.Keys) {
            Assert-True ($appliedParameterIds -contains $key) "Color Transfer runtime did not apply parameter: $key"
        }
        Assert-True ([double]$response.output.applied_params.gamma -eq 1.2) "Color Transfer gamma was not preserved: $stdout"
        Assert-True ([bool]$response.output.applied_params.skin_protection) "Color Transfer skin protection was not preserved: $stdout"
    }
    if ($Case.id -eq "custom-image-search") {
        Assert-True ([string]$response.output.selectedCandidate -eq "brave-search-2") "Image Search did not select result_index=1: $stdout"
        Assert-True ([string]$response.output.output_base64 -eq $imageSecond) "Image Search selected output did not match the second candidate: $stdout"
    }
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
        $outputLength = if ($case.ContainsKey("expectShader") -and [bool]$case.expectShader) {
            ([string]$response.output.textures.lut).Length
        }
        elseif ($case.id -eq "custom-1770146354922") {
            (Get-Item -LiteralPath ([string]$response.output.output_path)).Length
        }
        else {
            ([string]$response.output.output_base64).Length
        }
        Write-Host ("PASS {0}: output={1} candidates={2}" -f $case.id, $outputLength, $candidateCount)
    }
}
finally {
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Curated Art runtime smoke passed for $($cases.Count) execution cases."
