<# Owns modular release-script structure and security contract checks. #>

function Assert-ReleaseModuleContracts {
    param(
        [object[]]$ReleaseModules,
        [string[]]$CommonForbidden
    )

    foreach ($releaseModules in $ReleaseModules) {
        $actualNames = @(Get-ChildItem -LiteralPath $releaseModules.root -File -Filter "*.ps1" | Sort-Object Name | ForEach-Object Name)
        Assert-Equal `
            -Expected (@($releaseModules.names | Sort-Object) -join ",") `
            -Actual ($actualNames -join ",") `
            -Message "Release helper module set drifted: $($releaseModules.root)"
        $entryTokens = $null
        $entryParseErrors = $null
        $entryAst = [System.Management.Automation.Language.Parser]::ParseFile($releaseModules.entry, [ref]$entryTokens, [ref]$entryParseErrors)
        Assert-Equal -Expected 0 -Actual @($entryParseErrors).Count -Message "Release entry must parse before module load-order validation."
        $loadedModuleNames = @($entryAst.FindAll({
            param($node)
            $node -is [System.Management.Automation.Language.CommandAst] -and
                $node.InvocationOperator -eq [System.Management.Automation.Language.TokenKind]::Dot -and
                $node.Extent.Text -match 'Join-Path\s+\$\w*moduleRoot\s+"([^"]+\.ps1)"'
        }, $true) | ForEach-Object { $_.Extent.Text -replace '^.*"([^"]+\.ps1)".*$', '$1' })
        Assert-Equal `
            -Expected (@($releaseModules.names) -join ",") `
            -Actual ($loadedModuleNames -join ",") `
            -Message "Release helper AST load order drifted."
        foreach ($moduleName in $releaseModules.names) {
            $modulePath = Join-Path $releaseModules.root $moduleName
            Assert-ScriptContract -Path $modulePath -RequiredText @('<# Owns') -ForbiddenText $CommonForbidden
        }
    }
}

function Assert-CapturedPowerShellContract {
    param([string]$VerifyCommonPath)

    $verifyTokens = $null
    $verifyParseErrors = $null
    $verifyAst = [System.Management.Automation.Language.Parser]::ParseFile($VerifyCommonPath, [ref]$verifyTokens, [ref]$verifyParseErrors)
    Assert-Equal -Expected 0 -Actual @($verifyParseErrors).Count -Message "Verifier must parse before captured-process tests."
    . (Get-ScriptFunctionDefinition -Ast $verifyAst -Name "Invoke-CapturedPowerShell")

    $fixtureRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-verify-capture-" + [Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null
    try {
        $fixturePath = Join-Path $fixtureRoot "capture-fixture.ps1"
        [System.IO.File]::WriteAllText(
            $fixturePath,
            '[Console]::Out.WriteLine("fixture stdout"); [Console]::Error.WriteLine("fixture stderr"); exit 7',
            [System.Text.ASCIIEncoding]::new()
        )
        $captureResult = Invoke-CapturedPowerShell -Arguments @(
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", $fixturePath
        )
        $captureText = @($captureResult.output | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
        Assert-Equal -Expected 7 -Actual ([int]$captureResult.exitCode) -Message "Captured PowerShell helper lost the child exit code."
        Assert-True -Condition $captureText.Contains("fixture stdout") -Message "Captured PowerShell helper lost child stdout."
        Assert-True -Condition $captureText.Contains("fixture stderr") -Message "Captured PowerShell helper lost child stderr."
    }
    finally {
        Remove-Item -LiteralPath $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Assert-ReleaseSecurityScriptContracts {
    param(
        [string]$LayoutPath,
        [string]$TamperPath,
        [string]$PathSafetyPath,
        [string[]]$CommonForbidden
    )

    Assert-ScriptContract -Path $LayoutPath -RequiredText @(
        'function Get-LoomReleaseLayout',
        'function Get-LoomArchiveFileEntries',
        'function Assert-LoomPathHasNoReparsePoints',
        'function Assert-LoomDesktopRootExecutableBoundary',
        'function Test-LoomArtifactKind',
        'Loom.exe',
        '$runtimeRoot = Join-Path $packageFullPath "runtime"',
        '$daemonExe = Join-Path $runtimeRoot "loom-daemon.exe"',
        'Loom-CLI-',
        'Loom CLI artifact metadata mismatch.',
        'Loom CLI ZIP must contain exactly one loom.exe entry.',
        'Loom CLI extraction destination must be empty:',
        'Invalid Loom archive entry:',
        'Loom release paths must not contain reparse points:',
        '$entry.Name.Length -eq 0',
        '[System.StringComparison]::Ordinal',
        'manifest.json',
        '[System.IO.FileMode]::CreateNew',
        '$entry.Open()'
    ) -ForbiddenText $CommonForbidden

    Assert-ScriptContract -Path $TamperPath -RequiredText @(
        'function New-IntegrityFixture',
        'function New-TraversalZip',
        'function New-WhitespaceEntryZip',
        'ExtraRootExecutable',
        'ExtraCliEntry',
        'CliMetadataMismatch',
        'CliEntryCaseMismatch',
        'CliKindCaseMismatch',
        'ForwardSlashPaths',
        'no-newline',
        'extra-line',
        'Traversal archive unexpectedly passed shared entry validation.',
        'Non-empty CLI extraction destination unexpectedly passed validation.',
        'Runtime reparse point unexpectedly passed shared layout validation.',
        'ArtifactNamingMismatch',
        'PluginSdkPathMismatch',
        'desktop-wrong',
        'cli-wrong',
        'Loom release integrity tamper contract passed.'
    ) -ForbiddenText $CommonForbidden

    Assert-ScriptContract -Path $PathSafetyPath -RequiredText @(
        'Assert-LoomSafeRelativePath',
        'Get-LoomSafeDescendantFiles',
        'Assert-LoomBuildOutputRoot',
        'Read-LoomBoundedFileBytes',
        'Get-LoomVerifiedFileDigest',
        'Get-LoomArchiveFileEntries',
        'mklink /J',
        'Loom release path safety contract passed.'
    ) -ForbiddenText $CommonForbidden
}
