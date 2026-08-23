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
# Deliberately outside every per-case Art directory, so it is outside every root a request grants.
$outsideImage = Join-Path $workRoot "outside\outside.png"
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
$nestedImageSearchFrameworkData = @{
    mcp = @{
        serverId = "fixture"
        toolName = "brave_image_search"
        arguments = @{ query = "nested loom smoke"; count = 1 }
        result = @{
            structuredContent = @{
                items = @(
                    @{
                        title = "Nested fixture image"
                        url = "https://example.invalid/nested-source"
                        image = @{ url = $imageSecond }
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
# A real listener on a loopback port, so the SSRF case below names a service that would answer.
# Nothing ever accepts the connection: the assertion is that the Art never opens one, and the
# backlog records the attempt even without an `AcceptTcpClient` call.
$loopbackListener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$loopbackListener.Start()
$loopbackPort = ([System.Net.IPEndPoint]$loopbackListener.LocalEndpoint).Port
$loopbackImageSearchFrameworkData = @{
    mcp = @{
        serverId = "fixture"
        toolName = "brave_image_search"
        arguments = @{ query = "loom smoke"; count = 1 }
        result = @{
            structuredContent = @{
                type = "object"
                items = @(
                    @{
                        title = "Loopback candidate"
                        url = "https://example.invalid/source"
                        properties = @{
                            url = "http://127.0.0.1:$loopbackPort/candidate.png"
                            width = 1
                            height = 1
                        }
                    }
                )
            }
        }
    }
}

# A loopback image server the seam case below is allowed to read, on a port chosen the same way the
# rest of these smokes choose one. The fixture runs as its own process because the Art runtime blocks
# this script while it downloads.
$seamPortProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$seamPortProbe.Start()
$seamImagePort = ([System.Net.IPEndPoint]$seamPortProbe.LocalEndpoint).Port
$seamPortProbe.Stop()
$seamImageUrl = "http://127.0.0.1:$seamImagePort/fixture.png"
$seamImageReadyPath = Join-Path $workRoot "seam-image-server.ready"
$seamImageRequestPath = Join-Path $workRoot "seam-image-server.requests"
$seamImageStdoutPath = Join-Path $workRoot "seam-image-server.out"
$seamImageStderrPath = Join-Path $workRoot "seam-image-server.err"
$seamImageServer = $null

function Start-SeamImageServer {
    $fixtureScript = Join-Path $scriptRoot "fixtures\LoopbackImageFixture.ps1"
    Assert-True (Test-Path -LiteralPath $fixtureScript -PathType Leaf) "Loopback image fixture is missing: $fixtureScript"
    Remove-Item -LiteralPath $seamImageReadyPath -Force -ErrorAction SilentlyContinue
    $imageBase64 = ($image -split ',', 2)[1]
    $script:seamImageServer = Start-Process -FilePath "powershell.exe" -ArgumentList (
        "-NoProfile -ExecutionPolicy Bypass -File `"$fixtureScript`" -Port $seamImagePort " +
        "-ReadyPath `"$seamImageReadyPath`" -RequestPath `"$seamImageRequestPath`" -ImageBase64 $imageBase64"
    ) -WindowStyle Hidden -PassThru -RedirectStandardOutput $seamImageStdoutPath -RedirectStandardError $seamImageStderrPath
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $seamImageReadyPath -PathType Leaf)) {
        if ($script:seamImageServer.HasExited) {
            throw "Loopback image fixture exited early: $([IO.File]::ReadAllText($seamImageStderrPath))"
        }
        if ([DateTime]::UtcNow -ge $deadline) { throw "Timed out waiting for the loopback image fixture." }
        Start-Sleep -Milliseconds 50
    }
}

function Stop-SeamImageServer {
    if ($null -eq $script:seamImageServer) { return }
    if (-not $script:seamImageServer.HasExited) {
        Stop-Process -Id $script:seamImageServer.Id -Force -ErrorAction SilentlyContinue
    }
    $script:seamImageServer = $null
}

$seamImageSearchFrameworkData = @{
    mcp = @{
        serverId = "fixture"
        toolName = "brave_image_search"
        arguments = @{ query = "loom smoke"; count = 1 }
        result = @{
            structuredContent = @{
                type = "object"
                items = @(
                    @{
                        title = "Seam candidate"
                        url = "https://example.invalid/source"
                        properties = @{ url = $seamImageUrl; width = 1; height = 1 }
                    }
                )
            }
        }
    }
}
$cases = @(
    @{ id = "custom-1770146354922"; framework = "process"; params = @{ quality_num = 90; lossless = $true }; inputs = @{ input = $image } },
    @{ id = "custom-remove-bg-cloud"; framework = "cloud_api"; params = @{}; inputs = @{ input = $image } },
    @{ id = "custom-image-search"; framework = "mcp"; params = @{ query = "loom smoke"; count = 3; result_index = 1 }; inputs = @{}; frameworkData = $imageSearchFrameworkData; expectSelectedSecondCandidate = $true; expectUrlProvenance = $true },
    @{ id = "custom-image-search"; label = "a negative result_index"; framework = "mcp"; params = @{ query = "loom smoke"; count = 3; result_index = -1 }; inputs = @{}; frameworkData = $imageSearchFrameworkData; expectError = $true; expectErrorMessage = "result_index must be non-negative" },
    @{ id = "custom-image-search"; label = "a non-numeric result_index"; framework = "mcp"; params = @{ query = "loom smoke"; count = 3; result_index = "second" }; inputs = @{}; frameworkData = $imageSearchFrameworkData; expectError = $true; expectErrorMessage = "result_index must be an integer" },
    @{ id = "custom-image-search"; label = "an out-of-range result_index"; framework = "mcp"; params = @{ query = "loom smoke"; count = 3; result_index = 2 }; inputs = @{}; frameworkData = $imageSearchFrameworkData; expectError = $true; expectErrorMessage = "result_index 2 is out of range for 2 downloadable candidates" },
    @{ id = "custom-image-search"; framework = "mcp"; params = @{ query = "nested loom smoke"; count = 1 }; inputs = @{}; frameworkData = $nestedImageSearchFrameworkData; expectNestedImageUrl = $true },
    @{ id = "custom-1770131241684"; framework = "process"; params = $colorTransferParams; inputs = @{ input = $image; reference = $image } },
    @{ id = "custom-1770131241684"; framework = "process"; params = $colorTransferParams; inputs = @{ input = $image; reference = $image; output_mode = "shader" }; expectShader = $true },
    @{ id = "custom-image-blend-script"; framework = "process"; params = @{ mix_ratio = 50 }; inputs = @{ input = $image; reference = $image } },
    # The image inputs below all name a readable file, or a host that would answer, and every one of
    # them sits outside the roots the request granted. A runtime that resolves them reads images it
    # was never given access to, and the UNC spellings turn the read into an outbound SMB
    # connection that offers the caller's Windows credentials to whoever answers.
    @{ id = "custom-1770146354922"; label = "an absolute path outside the granted roots"; framework = "process"; params = @{}; inputs = @{ input = $outsideImage }; expectError = $true },
    @{ id = "custom-1770146354922"; label = "a file:// URL outside the granted roots"; framework = "process"; params = @{}; inputs = @{ input = ("file:///" + $outsideImage.Replace('\', '/')) }; expectError = $true },
    @{ id = "custom-1770146354922"; label = "a UNC path naming a remote share"; framework = "process"; params = @{}; inputs = @{ input = "\\attacker.invalid\share\input.png" }; expectError = $true },
    @{ id = "custom-1770146354922"; label = "a file:// URL carrying a remote host"; framework = "process"; params = @{}; inputs = @{ input = "file://attacker.invalid/share/input.png" }; expectError = $true },
    # The image-search Art downloads whatever URL the MCP server names, so a server can point it at a
    # service that is only reachable from the user's machine and read the response back as an
    # "image". The candidate below satisfies the Art's extension check and names a port that is
    # genuinely listening, so a runtime without an outbound address policy connects to it.
    @{ id = "custom-image-search"; label = "an MCP candidate naming a loopback service"; framework = "mcp"; params = @{ query = "loom smoke"; count = 1 }; inputs = @{}; frameworkData = $loopbackImageSearchFrameworkData; expectError = $true; assertNoConnection = $true },
    # The same candidate shape, with the loopback test seam set. It is the only way an installed
    # package can be tested against a fixture image server, and it must stay narrow: the seam exempts
    # a loopback address written literally in the URL and nothing else, which the Rust-side tests and
    # the case above cover from the other direction.
    @{ id = "custom-image-search"; label = "a literal loopback candidate with the download seam enabled"; framework = "mcp"; params = @{ query = "loom smoke"; count = 1 }; inputs = @{}; frameworkData = $seamImageSearchFrameworkData; environment = @{ LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES = "1" }; requiresSeamImageServer = $true }
)

function Get-ResponseImageDataUrl {
    param([Parameter(Mandatory = $true)][object]$Output)

    # The same order the host reads an Art's image in: the `content` block first, then a
    # self-declared `output_base64` for the Arts that still carry one. A PowerShell Art that writes
    # its image to a file no longer duplicates it under `output_base64`, so an assertion keyed on
    # that property alone would only be testing which of the two spellings the Art happened to pick.
    $contentProperty = $Output.PSObject.Properties["content"]
    if ($null -ne $contentProperty) {
        foreach ($entry in @($contentProperty.Value)) {
            if ([string]$entry.type -eq "image" -and -not [string]::IsNullOrWhiteSpace([string]$entry.data)) {
                return [string]$entry.data
            }
        }
    }
    $outputBase64Property = $Output.PSObject.Properties["output_base64"]
    if ($null -ne $outputBase64Property) {
        return [string]$outputBase64Property.Value
    }
    return ""
}

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
    if ($Case.ContainsKey("environment")) {
        foreach ($entry in $Case.environment.GetEnumerator()) {
            $psi.Environment[$entry.Key] = [string]$entry.Value
        }
    }
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
    if ($Case.ContainsKey("expectError") -and [bool]$Case.expectError) {
        Assert-True ([string]$response.status -eq "error") "$($Case.id) accepted $($Case.label): $stdout"
        if ($Case.ContainsKey("expectErrorMessage")) {
            Assert-True ([string]$response.error.code -eq "image_search_failed") "$($Case.id) returned the wrong error code: $stdout"
            Assert-True ([string]$response.error.message -eq [string]$Case.expectErrorMessage) "$($Case.id) returned the wrong error message: $stdout"
        }
        return $response
    }
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
        Assert-True (-not [string]::IsNullOrWhiteSpace((Get-ResponseImageDataUrl -Output $response.output))) "$($Case.id) output image is missing: $stdout"
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
    if ($Case.id -eq "custom-image-search" -and $Case.ContainsKey("expectSelectedSecondCandidate")) {
        Assert-True ([string]$response.output.selectedCandidate -eq "brave-search-2") "Image Search did not select result_index=1: $stdout"
        Assert-True ((Get-ResponseImageDataUrl -Output $response.output) -eq $imageSecond) "Image Search selected output did not match the second candidate: $stdout"
        $searchOutputBase64Property = $response.output.PSObject.Properties["output_base64"]
        Assert-True ($null -eq $searchOutputBase64Property) "Image Search must not duplicate the selected image beside its content block: $stdout"
    }
    if ($Case.id -eq "custom-image-search" -and $Case.ContainsKey("expectUrlProvenance")) {
        $secondCandidate = @($response.candidates)[1]
        Assert-True ([string]$secondCandidate.sourceUrl -eq "https://example.invalid/source-two") "Image Search confused the source page with the downloadable image: $stdout"
        Assert-True ([string]$secondCandidate.imageUrlSource -eq "result.structuredContent.items[1].properties.url") "Image Search did not preserve downloadable-image provenance: $stdout"
        Assert-True ([string]$secondCandidate.sourceUrlSource -eq "result.structuredContent.items[1].url") "Image Search did not preserve source-page provenance: $stdout"
    }
    if ($Case.id -eq "custom-image-search" -and $Case.ContainsKey("expectNestedImageUrl")) {
        $nestedCandidate = @($response.candidates)[0]
        Assert-True ((Get-ResponseImageDataUrl -Output $response.output) -eq $imageSecond) "Image Search did not normalize the nested image URL: $stdout"
        Assert-True ([string]$nestedCandidate.imageUrlSource -eq "result.structuredContent.items[0].image.url") "Image Search did not preserve nested downloadable-image provenance: $stdout"
        Assert-True ([string]$nestedCandidate.sourceUrl -eq "https://example.invalid/nested-source") "Image Search did not preserve the nested image's parent page URL: $stdout"
        Assert-True ([string]$nestedCandidate.sourceUrlSource -eq "result.structuredContent.items[0].url") "Image Search did not preserve nested source-page provenance: $stdout"
    }
    return $response
}

New-Item -ItemType Directory -Force -Path $workRoot | Out-Null
try {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $outsideImage) | Out-Null
    [System.IO.File]::WriteAllBytes(
        $outsideImage,
        [Convert]::FromBase64String(($image -split ',', 2)[1])
    )
    foreach ($case in $cases) {
        $zipPath = Join-Path $artifactRootPath "$($case.id).zip"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Missing Art ZIP for runtime smoke: $zipPath"
        $artDirectory = Join-Path $workRoot $case.id
        Expand-Archive -LiteralPath $zipPath -DestinationPath $artDirectory -Force
        if ($case.ContainsKey("requiresSeamImageServer") -and [bool]$case.requiresSeamImageServer) {
            Start-SeamImageServer
        }
        $response = Invoke-Runtime -ArtDirectory $artDirectory -Case $case
        if ($case.ContainsKey("requiresSeamImageServer") -and [bool]$case.requiresSeamImageServer) {
            $servedRequests = @(Get-Content -LiteralPath $seamImageRequestPath -ErrorAction SilentlyContinue)
            Stop-SeamImageServer
            Assert-True (
                @($servedRequests | Where-Object { $_ -eq "GET /fixture.png" }).Count -ge 1
            ) "$($case.id) did not download the loopback candidate with the seam enabled: $($servedRequests -join '; ')"
        }
        if ($case.ContainsKey("expectError") -and [bool]$case.expectError) {
            if ($case.ContainsKey("assertNoConnection") -and [bool]$case.assertNoConnection) {
                Assert-True (-not $loopbackListener.Pending()) "$($case.id) opened a connection to the loopback service named by $($case.label)"
            }
            Write-Host ("PASS {0}: rejected {1}" -f $case.id, $case.label)
            continue
        }
        $candidateProperty = $response.PSObject.Properties["candidates"]
        $candidateCount = if ($null -ne $candidateProperty) { @($candidateProperty.Value).Count } else { 0 }
        $outputLength = if ($case.ContainsKey("expectShader") -and [bool]$case.expectShader) {
            ([string]$response.output.textures.lut).Length
        }
        elseif ($case.id -eq "custom-1770146354922") {
            (Get-Item -LiteralPath ([string]$response.output.output_path)).Length
        }
        else {
            (Get-ResponseImageDataUrl -Output $response.output).Length
        }
        $caseLabel = if ($case.ContainsKey("label")) { " (" + $case.label + ")" } else { "" }
        Write-Host ("PASS {0}{1}: output={2} candidates={3}" -f $case.id, $caseLabel, $outputLength, $candidateCount)
    }
}
finally {
    Stop-SeamImageServer
    $loopbackListener.Stop()
    Remove-Item -LiteralPath $workRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Curated Art runtime smoke passed for $($cases.Count) execution and rejection cases."
