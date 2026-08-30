<# Shared validation for paired Hook/Loom candidate evidence. #>

function Read-LoomExtensionCompatibility {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolved = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw "Extension compatibility evidence is missing: $resolved"
    }
    Assert-LoomPathHasNoReparsePoints -RootPath (Split-Path -Parent $resolved) -Path $resolved
    $document = Read-LoomBoundedJsonFile -Path $resolved -MaxBytes 256KB
    if ([int]$document.schemaVersion -ne 1 -or
        [string]$document.protocol -ne "loom.extension.v1" -or
        [string]$document.target -ne "windows-x64" -or
        [string]$document.packageSchema.id -ne "loom.capability.package.v1" -or
        [int]$document.packageSchema.version -ne 1) {
        throw "Extension compatibility evidence uses an unsupported contract."
    }
    foreach ($api in @($document.hook.extensionApi, $document.loom.extensionApi)) {
        if ([string]$api.minimum -ne "1.0" -or [string]$api.maximum -ne "1.0") {
            throw "Extension compatibility evidence has an unsupported API range."
        }
    }
    if (@($document.surfaceFeatures) -notcontains "declarative.v1") {
        throw "Extension compatibility evidence omits the declarative Surface feature."
    }
    return $document
}

function Assert-LoomExtensionCompatibility {
    param(
        [Parameter(Mandatory = $true)][object]$Document,
        [Parameter(Mandatory = $true)][string]$GitHead,
        [Parameter(Mandatory = $true)][object[]]$ExecutableRecords
    )

    if ([string]$Document.loom.commit -cne $GitHead) {
        throw "Extension compatibility evidence does not match the Loom commit."
    }
    $declared = @($Document.loom.executables)
    if ($declared.Count -ne $ExecutableRecords.Count) {
        throw "Extension compatibility evidence has the wrong Loom executable count."
    }
    foreach ($record in $ExecutableRecords) {
        $path = ([string]$record.path).Replace('\', '/')
        $matches = @($declared | Where-Object { ([string]$_.path).Replace('\', '/') -ceq $path })
        if ($matches.Count -ne 1 -or
            [string]$matches[0].name -cne [string]$record.name -or
            [int64]$matches[0].bytes -ne [int64]$record.bytes -or
            [string]$matches[0].sha256 -cne [string]$record.sha256) {
            throw "Extension compatibility evidence does not match Loom executable $path."
        }
    }
}
