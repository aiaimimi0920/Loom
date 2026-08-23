param(
    [string]$ArtifactRoot
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$automaticRefreshLabel = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("6Ieq5YqoIA=="))

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-ArtIdentityMetadata {
    param(
        [object]$Manifest,
        [string]$Context
    )

    $publisher = $Manifest.metadata.packageSecurity.publisher
    $publisherId = [string]$publisher.id
    $publisherName = [string]$publisher.name
    $publisherIcon = [string]$publisher.icon
    $art = $Manifest.metadata.art
    $localization = $Manifest.metadata.localization

    Assert-True (-not [string]::IsNullOrWhiteSpace($publisherId)) "Art publisher id is required: $Context"
    Assert-True (-not [string]::IsNullOrWhiteSpace($publisherName)) "Art publisher name is required: $Context"
    Assert-True (-not [string]::IsNullOrWhiteSpace($publisherIcon)) "Art publisher icon is required: $Context"
    Assert-True ([string]$art.qualifiedId -eq "$publisherId/$($Manifest.id)") "Art qualified id must equal publisher id plus Art id: $Context"
    Assert-True ([string]$art.englishName -eq [string]$Manifest.id) "Art published English name must equal its technical Art id: $Context"
    Assert-True ([string]$art.globalId -match '^NA\d{11}$') "Art global id must use NA followed by 11 digits: $Context"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$localization.names.'zh-CN')) "Art zh-CN name is required: $Context"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$localization.names.'en-US')) "Art en-US name is required: $Context"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$localization.descriptions.'zh-CN')) "Art zh-CN description is required: $Context"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$localization.descriptions.'en-US')) "Art en-US description is required: $Context"
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent (Split-Path -Parent $scriptRoot)
$packagesRoot = Join-Path $repoRoot "art-packages\samples"
$buildScript = Join-Path $repoRoot "scripts\Build-LoomSampleArtPackages.ps1"
$expected = [ordered]@{
    "image-compress" = [ordered]@{ id = "custom-1770146354922"; framework = "process"; executionType = "framework_art"; globalId = "NA20260802001"; version = "0.1.2" }
    "remove-bg" = [ordered]@{ id = "custom-remove-bg-cloud"; framework = "cloud_api"; executionType = "framework_art"; globalId = "NA20260802002"; version = "0.1.1" }
    "image-search" = [ordered]@{ id = "custom-image-search"; framework = "mcp"; executionType = "framework_art"; globalId = "NA20260802003"; version = "0.4.0" }
    "color-transfer" = [ordered]@{ id = "custom-1770131241684"; framework = "process"; executionType = "framework_art"; globalId = "NA20260802004"; version = "0.1.4" }
    "image-blend" = [ordered]@{ id = "custom-image-blend-script"; framework = "process"; executionType = "framework_art"; globalId = "NA20260802005"; version = "0.1.0" }
    "image-blend-compress" = [ordered]@{ id = "custom-image-blend-compress-workflow"; framework = "workflow"; executionType = "workflow"; globalId = "NA20260802006"; version = "0.1.0" }
    "stock-monitor" = [ordered]@{ id = "custom-stock-monitor"; framework = "mcp"; executionType = "framework_art"; globalId = "NA20260802007"; version = "1.6.0" }
}

Assert-True (Test-Path -LiteralPath $packagesRoot -PathType Container) "Sample Art package source directory is required: $packagesRoot"
Assert-True (Test-Path -LiteralPath $buildScript -PathType Leaf) "Independent sample Art package build script is required: $buildScript"

$sourceDirectories = @(Get-ChildItem -LiteralPath $packagesRoot -Directory)
Assert-True ($sourceDirectories.Count -eq $expected.Count) "Expected exactly $($expected.Count) sample Art source directories, found $($sourceDirectories.Count)."
$seenSourceGlobalIds = @{}

foreach ($entry in $expected.GetEnumerator()) {
    $sourceDirectory = Join-Path $packagesRoot $entry.Key
    Assert-True (Test-Path -LiteralPath $sourceDirectory -PathType Container) "Missing sample Art source directory: $sourceDirectory"

    $manifestPath = Join-Path $sourceDirectory "manifest.json"
    $runtimePath = Join-Path $sourceDirectory "art.runtime.json"
    $workflowPath = Join-Path $sourceDirectory "workflow.yaml"
    Assert-True (Test-Path -LiteralPath $manifestPath -PathType Leaf) "Sample Art manifest is required: $manifestPath"

    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$manifest.id)) "Sample Art id is required: $manifestPath"
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$manifest.name)) "Sample Art name is required: $manifestPath"
    Assert-True ([bool]$manifest.enabled) "Sample Art must be enabled by default in its package manifest: $manifestPath"
    Assert-True ([string]$manifest.execution.type -eq $entry.Value.executionType) "Sample Art execution type mismatch: $manifestPath"
    Assert-True ([string]$manifest.id -eq $entry.Value.id) "Sample Art id mismatch: $manifestPath"
    if ($entry.Value.executionType -eq "framework_art") {
        Assert-True ([string]$manifest.execution.framework -eq $entry.Value.framework) "Sample Art framework mismatch: $manifestPath"
        Assert-True (Test-Path -LiteralPath $runtimePath -PathType Leaf) "Sample Art runtime manifest is required: $runtimePath"
    }
    else {
        Assert-True ([string]$manifest.execution.workflowId -eq "image-blend-compress-workflow") "Workflow sample Art id mismatch: $manifestPath"
        Assert-True (Test-Path -LiteralPath $workflowPath -PathType Leaf) "Workflow sample Art definition is required: $workflowPath"
        Assert-True (-not (Test-Path -LiteralPath $runtimePath)) "Workflow sample Art must not bundle a local reimplementation runtime: $runtimePath"
    }
    Assert-True ([string]$manifest.metadata.dependencies.framework -eq $entry.Value.framework) "Sample Art framework dependency mismatch: $manifestPath"
    Assert-True ([string]$manifest.metadata.packageSecurity.version -eq $entry.Value.version) "Sample Art package version mismatch: $manifestPath"
    Assert-ArtIdentityMetadata -Manifest $manifest -Context $manifestPath
    $sourceGlobalId = [string]$manifest.metadata.art.globalId
    Assert-True ($sourceGlobalId -eq $entry.Value.globalId) "Sample Art global id mismatch: $manifestPath"
    Assert-True (-not $seenSourceGlobalIds.ContainsKey($sourceGlobalId)) "Duplicate sample Art global id: $sourceGlobalId"
    $seenSourceGlobalIds[$sourceGlobalId] = $true
    Assert-True (($null -ne $manifest.inputs -and @($manifest.inputs).Count -gt 0) -or ($null -ne $manifest.params -and @($manifest.params).Count -gt 0)) "Sample Art inputs or params are required: $manifestPath"
    Assert-True ($null -ne $manifest.outputs -and @($manifest.outputs).Count -gt 0) "Sample Art outputs are required: $manifestPath"
    if ($entry.Key -eq "image-search") {
        $secret = @($manifest.params | Where-Object { [string]$_.id -eq "brave_api_key" }) | Select-Object -First 1
        Assert-True ($null -eq $secret) "Image search Art must not own the MCP service credential."
        Assert-True ([string]$manifest.metadata.mcp.serverId -eq "neuro-image-search") "Image search MCP server id is invalid."
        Assert-True ([string]$manifest.metadata.mcp.packageId -eq "neuro.official/neuro-image-search") "Image search MCP package id is invalid."
        Assert-True ([string]$manifest.metadata.mcp.version -eq "^0.1") "Image search MCP package version requirement is invalid."
        Assert-True ([string]$manifest.metadata.mcp.toolName -eq "brave_image_search") "Image search must call brave_image_search."
        foreach ($forbidden in @("command", "args", "env", "url", "headers", "credentialEnv", "credentialHeaders")) {
            Assert-True ($null -eq $manifest.metadata.mcp.PSObject.Properties[$forbidden]) "Image search Art must not own MCP runtime field '$forbidden'."
        }
        $mcpDependencies = @($manifest.metadata.dependencies.mcpServers)
        Assert-True ($mcpDependencies.Count -eq 1) "Image search must declare exactly one MCP server dependency."
        Assert-True ([string]$mcpDependencies[0].id -eq "neuro.official/neuro-image-search") "Image search dependency package id is invalid."
        Assert-True ([string]$mcpDependencies[0].version -eq "^0.1") "Image search dependency version is invalid."
        $mcpServerPath = Join-Path $sourceDirectory "runtime\image-search-mcp.ps1"
        Assert-True (-not (Test-Path -LiteralPath $mcpServerPath)) "Image search Art must not bundle the independent MCP server runtime."
        $runtimeSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $sourceDirectory "runtime\main.ps1")
        Assert-True ($runtimeSource -match 'frameworkData\.mcp\.result') "Image search runtime must consume the real MCP result."
        Assert-True ($runtimeSource -notmatch 'New-PlaceholderImage') "Image search runtime must not generate placeholder results."
    }
    if ($entry.Key -eq "color-transfer") {
        $expectedParameterIds = @(
            "strength", "gamma", "exposure", "contrast", "highlights", "shadows", "whites", "blacks",
            "temperature", "tint", "saturation", "vibrance", "hue", "split_h_hue", "split_h_sat",
            "split_s_hue", "split_s_sat", "split_balance", "skin_protection"
        )
        $actualParameterIds = @($manifest.params | ForEach-Object { [string]$_.id })
        Assert-True ($actualParameterIds.Count -eq $expectedParameterIds.Count) "Color Transfer must expose all 19 RBF-era parameters."
        foreach ($parameterId in $expectedParameterIds) {
            Assert-True ($actualParameterIds -contains $parameterId) "Color Transfer parameter is missing: $parameterId"
        }
        $strength = @($manifest.params | Where-Object { [string]$_.id -eq "strength" })[0]
        $gamma = @($manifest.params | Where-Object { [string]$_.id -eq "gamma" })[0]
        $splitHighlightHue = @($manifest.params | Where-Object { [string]$_.id -eq "split_h_hue" })[0]
        $splitHighlightSaturation = @($manifest.params | Where-Object { [string]$_.id -eq "split_h_sat" })[0]
        $splitShadowHue = @($manifest.params | Where-Object { [string]$_.id -eq "split_s_hue" })[0]
        $skinProtection = @($manifest.params | Where-Object { [string]$_.id -eq "skin_protection" })[0]
        Assert-True ([double]$strength.default -eq 100 -and [double]$strength.min -eq 0 -and [double]$strength.max -eq 100) "Color Transfer strength contract regressed."
        Assert-True ([double]$gamma.default -eq 1 -and [double]$gamma.min -eq 0.1 -and [double]$gamma.max -eq 3 -and [double]$gamma.step -eq 0.1) "Color Transfer gamma contract regressed."
        Assert-True ([double]$splitHighlightHue.max -eq 360 -and [double]$splitHighlightHue.default -eq 30) "Color Transfer highlight hue contract regressed."
        Assert-True ([double]$splitHighlightSaturation.max -eq 100) "Color Transfer highlight saturation contract regressed."
        Assert-True ([double]$splitShadowHue.default -eq 210) "Color Transfer shadow hue default regressed."
        Assert-True ([string]$skinProtection.widget -eq "checkbox" -and [string]$skinProtection.data_type -eq "boolean") "Color Transfer skin protection contract regressed."
        Assert-True ([string]$manifest.metadata.capabilities.preview -eq "shader") "Color Transfer must advertise shader preview."
        Assert-True ([bool]$manifest.metadata.capabilities.requiresLiveInputs) "Color Transfer shader must require live input/reference images."
        Assert-True ([string]$manifest.metadata.capabilities.parameters -eq "dynamic") "Color Transfer parameters must be declared as dynamic shader uniforms."
        $runtimeSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $sourceDirectory "runtime\main.py")
        Assert-True ($runtimeSource -match 'oklab-statistical-transfer') "Color Transfer runtime must use the restored OkLab transfer pipeline."
        Assert-True ($runtimeSource -match 'shader_output') "Color Transfer runtime must expose the LUT shader output path."
        Assert-True ($runtimeSource -notmatch '"mix_ratio"') "Color Transfer must not retain the obsolete mix_ratio parameter alias."
        $fragmentShader = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $sourceDirectory "runtime\color_transfer.frag")
        Assert-True ($fragmentShader -match 'uniform sampler2D u_lut') "Color Transfer fragment shader must consume the generated LUT texture."
        foreach ($parameterId in $expectedParameterIds) {
            Assert-True ($runtimeSource -match [regex]::Escape($parameterId)) "Color Transfer runtime does not consume parameter: $parameterId"
            Assert-True ($fragmentShader -match [regex]::Escape("u_$parameterId")) "Color Transfer shader does not expose uniform: $parameterId"
        }
    }
    if ($entry.Key -eq "image-blend-compress") {
        $dependencyArts = @($manifest.metadata.dependencies.arts | ForEach-Object { [string]$_ })
        Assert-True ($dependencyArts.Count -eq 2) "Workflow sample Art must declare exactly two child Art dependencies."
        Assert-True ($dependencyArts -contains "neuro.official/custom-image-blend-script") "Workflow sample Art must depend on publisher-qualified image blend."
        Assert-True ($dependencyArts -contains "neuro.official/custom-1770146354922") "Workflow sample Art must depend on publisher-qualified image compression."
        $workflowSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $workflowPath
        Assert-True ($workflowSource -match 'uses:\s*neuro\.official/custom-image-blend-script') "Workflow sample Art must execute the publisher-qualified image-blend child Art."
        Assert-True ($workflowSource -match 'uses:\s*neuro\.official/custom-1770146354922') "Workflow sample Art must execute the publisher-qualified image-compress child Art."
        Assert-True ($workflowSource -notmatch 'Blend-Bitmaps|Save-Png') "Workflow sample Art must not reimplement child Art behavior."
        Assert-True (@($manifest.execution.workflowBindings.inputs).Count -eq 4) "Workflow sample Art must expose all four workflow bindings."
        Assert-True ([string]$manifest.execution.workflowBindings.primaryOutput.nodeId -eq "compress") "Workflow sample Art must return the compression child result."
    }
    if ($entry.Key -eq "stock-monitor") {
        $mcpFrameworkManifestPath = Join-Path $repoRoot "framework-packages\mcp\framework.manifest.json"
        $mcpFrameworkManifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $mcpFrameworkManifestPath | ConvertFrom-Json
        $surface = $manifest.metadata.capabilities.surface
        $surfaceRuntimes = @($surface.variants | ForEach-Object { [string]$_.runtime })
        $surfaceActions = @($surface.actions | ForEach-Object { [string]$_.id } | Sort-Object)
        $expectedActions = @("stock_interval_commit", "stock_period_commit", "stock_refresh", "stock_symbol_commit", "stock_tick_refresh")
        Assert-True ([string]$surface.protocolVersion -eq "loom.surface.v1") "Stock Monitor Surface protocol is invalid."
        Assert-True ([string]$surface.apiVersion -eq "1.0") "Stock Monitor Surface API version is invalid."
        Assert-True ($surfaceRuntimes.Count -eq 2 -and $surfaceRuntimes[0] -eq "javascript" -and $surfaceRuntimes[1] -eq "declarative") "Stock Monitor must prefer JavaScript Surface and retain a declarative fallback."
        Assert-True ([string]$surface.fallbackScene -eq "surface/fallback.json") "Stock Monitor fallback scene is invalid."
        Assert-True (($surfaceActions -join ",") -eq ($expectedActions -join ",")) "Stock Monitor Surface action set is invalid."
        Assert-True ([string]$surface.defaultViewId -eq "full" -and @($surface.views).Count -eq 4) "Stock Monitor must declare four views and open the full view by default."
        Assert-True ([string]$manifest.metadata.marketData.providerId -eq "stock-api") "Stock Monitor provider id is invalid."
        Assert-True ([string]$manifest.metadata.marketData.upstreamVersion -eq "2.7.3") "Stock Monitor upstream version is invalid."
        Assert-True (-not [bool]$manifest.metadata.marketData.apiKeyRequired) "Stock Monitor must not require an API credential."
        Assert-True (-not [bool]$manifest.metadata.marketData.trading) "Stock Monitor must not advertise trading."
        $surfaceSourcePath = Join-Path $sourceDirectory "surface\main.js"
        Assert-True (Test-Path -LiteralPath $surfaceSourcePath -PathType Leaf) "Stock Monitor JavaScript Surface entry is missing."
        Assert-True (Test-Path -LiteralPath (Join-Path $sourceDirectory "surface\fallback.json") -PathType Leaf) "Stock Monitor declarative fallback is missing."
        $surfaceSource = Get-Content -Raw -Encoding UTF8 -LiteralPath $surfaceSourcePath
        Assert-True ($surfaceSource.Contains("PERIODS") -and $surfaceSource.Contains("stock_period_commit") -and $surfaceSource.Contains("isIntradayPeriod")) "Stock Monitor JavaScript Surface must implement all period controls and intraday rendering."
        Assert-True ($surfaceSource.Contains("downsampleRows") -and $surfaceSource.Contains("maxPoints")) "Stock Monitor JavaScript Surface must downsample long market series."
        Assert-True ($surfaceSource.Contains("averageValues") -and $surfaceSource.Contains("formatClock") -and $surfaceSource.Contains($automaticRefreshLabel)) "Stock Monitor JavaScript Surface must keep its price curve and automatic refresh recency visible."
        $secretParameters = @($manifest.params | Where-Object { [string]$_.id -match '(?i)key|secret|token|credential' })
        Assert-True ($secretParameters.Count -eq 0) "Stock Monitor must not own provider credentials."
        Assert-True (-not (@($surface.actions | Where-Object { [string]$_.id -match '(?i)buy|sell|trade|order|purchase' }).Count)) "Stock Monitor must not declare trading actions."
        $runtimeSource = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $sourceDirectory "runtime\main.ps1")
        Assert-True ($runtimeSource -match 'frameworkData' -and $runtimeSource -match 'Get-McpToolContent') "Stock Monitor runtime must consume MCP framework results."
        Assert-True ($runtimeSource.Contains('$script:MaxHistoryRows = 2000') -and $runtimeSource.Contains('Select-Object -Last $script:MaxHistoryRows')) "Stock Monitor runtime must bound provider history to 2000 rows."
        Assert-True ($runtimeSource -notmatch 'Invoke-RestMethod|push2\.eastmoney\.com|push2his\.eastmoney\.com') "Stock Monitor runtime must not bypass stock-api."
        Assert-True ([string]$manifest.metadata.mcp.serverId -eq "stock-api") "Stock Monitor MCP server id is invalid."
        Assert-True ([string]$manifest.metadata.mcp.packageId -eq "neuro.official/stock-api") "Stock Monitor MCP package id is invalid."
        Assert-True ([string]$manifest.metadata.mcp.version -eq "=2.9.0") "Stock Monitor MCP wrapper version must be exact."
        Assert-True ([version]$mcpFrameworkManifest.version -ge [version]"0.2.3") "Stock Monitor requires MCP framework 0.2.3 or newer for Surface action routing."
        Assert-True (@($manifest.metadata.mcp.calls).Count -eq 4) "Stock Monitor must declare quote, history, order-book, and favorites MCP calls."
        $historyCall = @($manifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "history" })[0]
        Assert-True ([string]$historyCall.toolName -eq "get_market_series") "Stock Monitor history must use the multi-period market-series tool."
        $orderBookCall = @($manifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "orderbook" })[0]
        Assert-True ([string]$orderBookCall.toolName -eq "get_order_book") "Stock Monitor depth must use the order-book tool."
        Assert-True ([string]$orderBookCall.arguments.source -eq "auto") "Stock Monitor order book must use the automatic pysnowball/Xueqiu realtime path."
        $favoritesCall = @($manifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "favorites" })[0]
        Assert-True ([string]$favoritesCall.toolName -eq "get_stocks" -and @($favoritesCall.arguments.codes).Count -eq 4) "Stock Monitor favorites must use the bounded aggregate stock API."
        Assert-True (@($manifest.metadata.mcp.calls | Where-Object { [string]$_.arguments.source -eq "eastmoney" }).Count -eq 3) "Stock Monitor MCP calls must use the bounded eastmoney provider path."
        Assert-True (@($manifest.metadata.mcp.surfaceActions.stock_interval_commit.calls).Count -eq 0) "Stock Monitor interval action must skip MCP calls."
        Assert-True ((@($manifest.metadata.mcp.surfaceActions.stock_period_commit.calls) -join ",") -eq "history") "Stock Monitor period action must request only history and reuse the current quote."
        Assert-True ((@($manifest.metadata.mcp.surfaceActions.stock_tick_refresh.calls) -join ",") -eq "orderbook") "Stock Monitor tick action must poll only live data and reuse the authoritative quote."
        $mcpDependencies = @($manifest.metadata.dependencies.mcpServers)
        Assert-True ($mcpDependencies.Count -eq 1) "Stock Monitor must declare exactly one MCP server dependency."
        Assert-True ([string]$mcpDependencies[0].id -eq "neuro.official/stock-api" -and [string]$mcpDependencies[0].version -eq "=2.9.0") "Stock Monitor MCP dependency is invalid."
        Assert-True ($runtimeSource -notmatch '(?i)random|fake price|placeholder quote') "Stock Monitor runtime must not fabricate quotes."
    }

    if ($entry.Value.executionType -eq "framework_art") {
        $runtime = Get-Content -Raw -Encoding UTF8 -LiteralPath $runtimePath | ConvertFrom-Json
        Assert-True ([string]$runtime.protocolVersion -eq "loom.art.runtime.v1") "Sample Art runtime protocol is invalid: $runtimePath"
        Assert-True ($null -ne $runtime.entry) "Sample Art runtime entry is required: $runtimePath"
        Assert-True (-not [string]::IsNullOrWhiteSpace([string]$runtime.entry.command)) "Sample Art runtime entry.command is required: $runtimePath"

        $runtimeCommand = ([string]$runtime.entry.command).Replace('/', '\')
        $runtimeCommandPath = Join-Path $sourceDirectory $runtimeCommand
        if ($runtimeCommand -match '\\|/') {
            Assert-True (Test-Path -LiteralPath $runtimeCommandPath -PathType Leaf) "Sample Art runtime entry is not bundled: $runtimeCommandPath"
        }
        else {
            $runtimeFile = @($runtime.entry.args | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^runtime[\\/]' } | Select-Object -First 1)
            Assert-True ($runtimeFile.Count -eq 1) "Sample Art runtime must reference a bundled runtime file: $runtimePath"
            Assert-True (Test-Path -LiteralPath (Join-Path $sourceDirectory ($runtimeFile[0] -replace '/', '\')) -PathType Leaf) "Sample Art runtime file is not bundled: $runtimeFile"
        }
    }
}

if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $artifactRootPath = if ([System.IO.Path]::IsPathRooted($ArtifactRoot)) {
        [System.IO.Path]::GetFullPath($ArtifactRoot)
    }
    else {
        [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ArtifactRoot))
    }
    Assert-True (Test-Path -LiteralPath $artifactRootPath -PathType Container) "Sample Art artifact root is required: $artifactRootPath"
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $expectedZipNames = @($expected.Values | ForEach-Object { "$($_.id).zip" })
    $zipFiles = @(Get-ChildItem -LiteralPath $artifactRootPath -Filter *.zip -File | Where-Object { $expectedZipNames -contains $_.Name })
    Assert-True ($zipFiles.Count -eq $expected.Count) "Expected all $($expected.Count) sample Art ZIPs, found $($zipFiles.Count)."
    $seenZipGlobalIds = @{}
    $certificationPath = Join-Path (Split-Path -Parent $artifactRootPath) "official-art-certifications.json"
    Assert-True (Test-Path -LiteralPath $certificationPath -PathType Leaf) "Official Art certification index is required: $certificationPath"
    $certificationIndex = Get-Content -Raw -Encoding UTF8 -LiteralPath $certificationPath | ConvertFrom-Json
    Assert-True ([int]$certificationIndex.schemaVersion -eq 1) "Official Art certification schema version must be 1."

    foreach ($entry in $expected.GetEnumerator()) {
        $zipPath = Join-Path $artifactRootPath "$($entry.Value.id).zip"
        $hashPath = "$zipPath.sha256"
        Assert-True (Test-Path -LiteralPath $zipPath -PathType Leaf) "Missing sample Art ZIP: $zipPath"
        Assert-True (Test-Path -LiteralPath $hashPath -PathType Leaf) "Missing sample Art ZIP hash: $hashPath"

        $archive = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
        try {
            Assert-True (-not @($archive.Entries | Where-Object {
                $_.FullName -match '(^|/)__pycache__/' -or $_.FullName -match '\.pyc$'
            }).Count) "Sample Art ZIP must not contain Python cache artifacts: $zipPath"
            $manifestEntry = $archive.Entries | Where-Object { $_.FullName -eq "manifest.json" } | Select-Object -First 1
            $runtimeEntry = $archive.Entries | Where-Object { $_.FullName -eq "art.runtime.json" } | Select-Object -First 1
            $workflowEntry = $archive.Entries | Where-Object { $_.FullName -eq "workflow.yaml" } | Select-Object -First 1
            Assert-True ($null -ne $manifestEntry) "Sample Art ZIP lacks manifest.json: $zipPath"
            if ($entry.Value.executionType -eq "framework_art") {
                Assert-True ($null -ne $runtimeEntry) "Sample Art ZIP lacks art.runtime.json: $zipPath"
            }
            else {
                Assert-True ($null -ne $workflowEntry) "Workflow sample Art ZIP lacks workflow.yaml: $zipPath"
                Assert-True ($null -eq $runtimeEntry) "Workflow sample Art ZIP must not contain art.runtime.json: $zipPath"
            }

            $reader = [System.IO.StreamReader]::new($manifestEntry.Open())
            try {
                $zipManifest = $reader.ReadToEnd() | ConvertFrom-Json
            }
            finally {
                $reader.Dispose()
            }
            Assert-True ([string]$zipManifest.execution.type -eq $entry.Value.executionType) "Sample Art ZIP execution type is invalid: $zipPath"
            Assert-True ([string]$zipManifest.id -eq $entry.Value.id) "Sample Art ZIP id mismatch: $zipPath"
            Assert-True ([string]$zipManifest.metadata.dependencies.framework -eq $entry.Value.framework) "Sample Art ZIP framework dependency mismatch: $zipPath"
            Assert-True ([string]$zipManifest.metadata.packageSecurity.version -eq $entry.Value.version) "Sample Art ZIP package version mismatch: $zipPath"
            Assert-ArtIdentityMetadata -Manifest $zipManifest -Context $zipPath
            $qualifiedId = [string]$zipManifest.metadata.art.qualifiedId
            $certifiedArt = $certificationIndex.certifications.PSObject.Properties[$qualifiedId]
            Assert-True ($null -ne $certifiedArt) "Official Art certification is missing: $qualifiedId"
            $certifiedVersion = $certifiedArt.Value.PSObject.Properties[[string]$entry.Value.version]
            Assert-True ($null -ne $certifiedVersion) "Official Art version certification is missing: $qualifiedId@$($entry.Value.version)"
            $actualDigest = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
            Assert-True ([string]$certifiedVersion.Value -eq $actualDigest) "Official Art certification digest mismatch: $qualifiedId@$($entry.Value.version)"
            $zipGlobalId = [string]$zipManifest.metadata.art.globalId
            Assert-True ($zipGlobalId -eq $entry.Value.globalId) "Sample Art ZIP global id mismatch: $zipPath"
            Assert-True (-not $seenZipGlobalIds.ContainsKey($zipGlobalId)) "Duplicate packaged Art global id: $zipGlobalId"
            $seenZipGlobalIds[$zipGlobalId] = $true
            if ($entry.Key -eq "image-search") {
                $zipSecret = @($zipManifest.params | Where-Object { [string]$_.id -eq "brave_api_key" }) | Select-Object -First 1
                Assert-True ($null -eq $zipSecret) "Packaged image search must not retain an MCP credential parameter."
                Assert-True ([string]$zipManifest.metadata.mcp.toolName -eq "brave_image_search") "Packaged image search MCP tool is invalid."
                Assert-True ([string]$zipManifest.metadata.mcp.packageId -eq "neuro.official/neuro-image-search") "Packaged image search MCP package id is invalid."
                $mcpServerEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq "runtime/image-search-mcp.ps1" } | Select-Object -First 1
                Assert-True ($null -eq $mcpServerEntry) "Packaged image search Art must not contain the independent MCP server: $zipPath"
            }
            if ($entry.Key -eq "color-transfer") {
                $packagedParameterIds = @($zipManifest.params | ForEach-Object { [string]$_.id })
                Assert-True ($packagedParameterIds.Count -eq 19) "Packaged Color Transfer Art must retain all 19 RBF-era parameters."
                foreach ($parameterId in @(
                    "strength", "gamma", "exposure", "contrast", "highlights", "shadows", "whites", "blacks",
                    "temperature", "tint", "saturation", "vibrance", "hue", "split_h_hue", "split_h_sat",
                    "split_s_hue", "split_s_sat", "split_balance", "skin_protection"
                )) {
                    Assert-True ($packagedParameterIds -contains $parameterId) "Packaged Color Transfer parameter is missing: $parameterId"
                }
            }
            if ($entry.Key -eq "stock-monitor") {
                $surfaceEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq "surface/main.js" } | Select-Object -First 1
                $fallbackEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq "surface/fallback.json" } | Select-Object -First 1
                Assert-True ($null -ne $surfaceEntry) "Packaged Stock Monitor JavaScript Surface entry is missing: $zipPath"
                Assert-True ($null -ne $fallbackEntry) "Packaged Stock Monitor fallback scene is missing: $zipPath"
                $surfaceReader = [System.IO.StreamReader]::new($surfaceEntry.Open())
                try {
                    $surfaceSource = $surfaceReader.ReadToEnd()
                }
                finally {
                    $surfaceReader.Dispose()
                }
                Assert-True ($surfaceSource.Contains("MAX_CANVAS_PIXELS")) "Packaged Stock Monitor must cap Canvas allocation: $zipPath"
                Assert-True ($surfaceSource.Contains("averageValues") -and $surfaceSource.Contains("formatClock") -and $surfaceSource.Contains($automaticRefreshLabel)) "Packaged Stock Monitor must retain its price curve and automatic refresh recency: $zipPath"
                Assert-True ($surfaceSource.Contains("position:absolute;inset:0")) "Packaged Stock Monitor Canvas must not contribute intrinsic Grid size: $zipPath"
                Assert-True (-not $surfaceSource.Contains("height:100%;min-height:145px")) "Packaged Stock Monitor must not retain the recursive Canvas sizing rule: $zipPath"
                Assert-True ($surfaceSource.Contains("new CSSStyleSheet()")) "Packaged Stock Monitor must use a CSP-compatible constructed stylesheet: $zipPath"
                Assert-True (-not $surfaceSource.Contains('document.createElement("style")')) "Packaged Stock Monitor must not inject a nonce-blocked inline style: $zipPath"
                $packagedActions = @($zipManifest.metadata.capabilities.surface.actions | ForEach-Object { [string]$_.id } | Sort-Object)
                Assert-True (($packagedActions -join ",") -eq "stock_interval_commit,stock_period_commit,stock_refresh,stock_symbol_commit,stock_tick_refresh") "Packaged Stock Monitor action set is invalid: $zipPath"
                Assert-True ([string]$zipManifest.metadata.marketData.providerId -eq "stock-api") "Packaged Stock Monitor provider is invalid: $zipPath"
                Assert-True ([string]$zipManifest.metadata.mcp.packageId -eq "neuro.official/stock-api") "Packaged Stock Monitor MCP package is invalid: $zipPath"
                Assert-True ([string]$zipManifest.metadata.mcp.version -eq "=2.9.0") "Packaged Stock Monitor MCP wrapper version is invalid: $zipPath"
                Assert-True (@($zipManifest.metadata.mcp.calls).Count -eq 4) "Packaged Stock Monitor MCP call set is invalid: $zipPath"
                $packagedHistoryCall = @($zipManifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "history" })[0]
                Assert-True ([string]$packagedHistoryCall.toolName -eq "get_market_series") "Packaged Stock Monitor history tool is invalid: $zipPath"
                $packagedOrderBookCall = @($zipManifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "orderbook" })[0]
                Assert-True ([string]$packagedOrderBookCall.toolName -eq "get_order_book" -and [string]$packagedOrderBookCall.arguments.source -eq "auto") "Packaged Stock Monitor order-book call is invalid: $zipPath"
                $packagedFavoritesCall = @($zipManifest.metadata.mcp.calls | Where-Object { [string]$_.id -eq "favorites" })[0]
                Assert-True ([string]$packagedFavoritesCall.toolName -eq "get_stocks" -and @($packagedFavoritesCall.arguments.codes).Count -eq 4) "Packaged Stock Monitor favorites call is invalid: $zipPath"
                Assert-True ([string]$zipManifest.metadata.capabilities.surface.defaultViewId -eq "full" -and @($zipManifest.metadata.capabilities.surface.views).Count -eq 4) "Packaged Stock Monitor view set is invalid: $zipPath"
                Assert-True ($surfaceSource.Contains("updateOrderBook") -and $surfaceSource.Contains("book-board")) "Packaged Stock Monitor must render the order-book panel: $zipPath"
                Assert-True ($surfaceSource.Contains("overflow:hidden")) "Packaged Stock Monitor root view must not scroll: $zipPath"
                Assert-True (@($zipManifest.metadata.mcp.calls | Where-Object { [string]$_.arguments.source -eq "eastmoney" }).Count -eq 3) "Packaged Stock Monitor MCP calls must use the bounded eastmoney provider path: $zipPath"
                Assert-True (-not [bool]$zipManifest.metadata.marketData.trading) "Packaged Stock Monitor must not advertise trading: $zipPath"
            }
            if ($entry.Value.executionType -eq "framework_art") {
                $runtimeReader = [System.IO.StreamReader]::new($runtimeEntry.Open())
                try {
                    $zipRuntime = $runtimeReader.ReadToEnd() | ConvertFrom-Json
                }
                finally {
                    $runtimeReader.Dispose()
                }
                $command = ([string]$zipRuntime.entry.command).Replace('\', '/')
                if ($command -match '/') {
                    $bundledRuntimeEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $command } | Select-Object -First 1
                    Assert-True ($null -ne $bundledRuntimeEntry) "Sample Art ZIP runtime entry is not bundled: $zipPath -> $command"
                }
                else {
                    $runtimeFile = @($zipRuntime.entry.args | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^runtime[\\/]' } | Select-Object -First 1)
                    Assert-True ($runtimeFile.Count -eq 1) "Sample Art ZIP runtime must reference a bundled runtime file: $zipPath"
                    $bundledRuntimeEntry = $archive.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $runtimeFile[0].Replace('\', '/') } | Select-Object -First 1
                    Assert-True ($null -ne $bundledRuntimeEntry) "Sample Art ZIP runtime file is not bundled: $zipPath -> $runtimeFile"
                }
            }
            else {
                $workflowReader = [System.IO.StreamReader]::new($workflowEntry.Open())
                try {
                    $zipWorkflow = $workflowReader.ReadToEnd()
                }
                finally {
                    $workflowReader.Dispose()
                }
                Assert-True ($zipWorkflow -match 'uses:\s*neuro\.official/custom-image-blend-script') "Packaged workflow must execute publisher-qualified image blend: $zipPath"
                Assert-True ($zipWorkflow -match 'uses:\s*neuro\.official/custom-1770146354922') "Packaged workflow must execute publisher-qualified image compression: $zipPath"
            }
        }
        finally {
            $archive.Dispose()
        }

        $actualHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $expectedHash = ((Get-Content -Raw -Encoding UTF8 -LiteralPath $hashPath).Trim() -split '\s+')[0].ToLowerInvariant()
        Assert-True ($actualHash -eq $expectedHash) "Sample Art ZIP hash mismatch: $zipPath"
    }
}

$imageSearchMcpContract = Join-Path $scriptRoot "Test-LoomImageSearchMcpServer.ps1"
Assert-True (Test-Path -LiteralPath $imageSearchMcpContract -PathType Leaf) "Image-search MCP contract test is required: $imageSearchMcpContract"
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $imageSearchMcpContract
Assert-True ($LASTEXITCODE -eq 0) "Independent image-search MCP server contract failed."

$stockApiMcpContract = Join-Path $scriptRoot "Test-LoomStockApiMcpServer.ps1"
Assert-True (Test-Path -LiteralPath $stockApiMcpContract -PathType Leaf) "stock-api MCP contract test is required: $stockApiMcpContract"
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $stockApiMcpContract
Assert-True ($LASTEXITCODE -eq 0) "Independent stock-api MCP server contract failed."

$stockMonitorContract = Join-Path $scriptRoot "Test-LoomStockMonitorArt.ps1"
Assert-True (Test-Path -LiteralPath $stockMonitorContract -PathType Leaf) "Stock Monitor Art contract test is required: $stockMonitorContract"
$stockMonitorArguments = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $stockMonitorContract)
if (-not [string]::IsNullOrWhiteSpace($ArtifactRoot)) {
    $stockMonitorArguments += @("-ArtifactRoot", $ArtifactRoot)
}
& powershell.exe @stockMonitorArguments
Assert-True ($LASTEXITCODE -eq 0) "Stock Monitor Art contract failed."

Write-Host "Sample Art package contract passed for $($expected.Count) packages."
