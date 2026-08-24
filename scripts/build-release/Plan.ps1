<# Owns one release-script responsibility. #>

function New-Plan {
    param(
        [System.Collections.Specialized.OrderedDictionary]$Catalog,
        [string]$ResolvedVersionId,
        [string]$ResolvedOutputRoot,
        [string]$Destination
    )

    return [ordered]@{
        schemaVersion = 1
        mode = "dry-run"
        app = "Loom"
        versionId = $ResolvedVersionId
        outputRoot = $ResolvedOutputRoot
        destination = $Destination
        target = $targetName
        sourcePaths = @($Catalog.sourcePaths)
        commands = @($Catalog.commands | ForEach-Object {
            [ordered]@{
                executable = $_.executable
                arguments = @($_.arguments)
                workingDirectory = $_.workingDirectory
                display = $_.display
                logName = $_.logName
            }
        })
        exes = @($Catalog.exes | ForEach-Object {
            [ordered]@{
                name = $_.name
                source = $_.source
                destinationRelativePath = $_.destinationRelativePath
            }
        })
        supportFiles = @($Catalog.supportFiles | ForEach-Object {
            [ordered]@{
                source = $_.source
                destinationRelativePath = $_.destinationRelativePath
            }
        })
        cliArtifact = [ordered]@{
            name = $Catalog.cliArtifact.name
            entryName = $Catalog.cliArtifact.entryName
            source = $Catalog.cliArtifact.source
            zipNamePattern = $Catalog.cliArtifact.zipNamePattern
        }
        pluginSdkArtifact = [ordered]@{
            name = $Catalog.pluginSdkArtifact.name
            pluginCliSource = $Catalog.pluginSdkArtifact.pluginCliSource
            pluginCliEntryName = $Catalog.pluginSdkArtifact.pluginCliEntryName
            zipNamePattern = $Catalog.pluginSdkArtifact.zipNamePattern
            files = @($Catalog.pluginSdkArtifact.files | ForEach-Object {
                [ordered]@{
                    source = $_.source
                    destinationRelativePath = $_.destinationRelativePath
                }
            })
        }
        frameworkPackageCatalog = [ordered]@{
            outputRoot = $Catalog.frameworkPackageCatalog.outputRoot
            expectedIds = @($Catalog.frameworkPackageCatalog.expectedIds)
        }
        mcpServerPackageCatalog = [ordered]@{
            outputRoot = $Catalog.mcpServerPackageCatalog.outputRoot
            expectedIds = @($Catalog.mcpServerPackageCatalog.expectedIds)
        }
        sampleArtPackageCatalog = [ordered]@{
            outputRoot = $Catalog.sampleArtPackageCatalog.outputRoot
            expected = @($Catalog.sampleArtPackageCatalog.expected)
        }
        requireCleanSource = [bool]$RequireCleanSource
        zip = (-not $NoZip)
    }
}
