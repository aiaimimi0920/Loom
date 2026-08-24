<# Owns one release-script responsibility. #>

function Get-LoomCatalog {
    param(
        [string]$FrameworkPackageOutputRoot,
        [string]$McpServerPackageOutputRoot,
        [string]$SampleArtPackageOutputRoot
    )

    $ocrRoot = Join-Path $repoRoot "resources\ocr"

    $exes = @(
        New-ExeSpec -Name "Loom.exe" -Source (Join-Path $repoRoot "apps\desktop\src-tauri\target\release\loom-desktop.exe")
        New-ExeSpec -Name "loom-daemon.exe" -Source (Join-Path $repoRoot "target\release\loom-daemon.exe") -DestinationRelativePath "runtime\loom-daemon.exe"
    )

    $support = @(
        New-SupportSpec -Source (Join-Path $ocrRoot "README.txt") -DestinationRelativePath "runtime\resources\ocr\README.txt"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_PP-OCRv4_det_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_PP-OCRv4_det_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_ppocr_mobile_v2.0_cls_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_ppocr_mobile_v2.0_cls_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_PP-OCRv4_rec_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_PP-OCRv4_rec_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "ch_PP-OCRv5_rec_mobile_infer.onnx") -DestinationRelativePath "runtime\resources\ocr\ch_PP-OCRv5_rec_mobile_infer.onnx"
        New-SupportSpec -Source (Join-Path $ocrRoot "fixtures\test_1.png") -DestinationRelativePath "runtime\resources\ocr\fixtures\test_1.png"
        New-SupportSpec -Source (Join-Path $ocrRoot "onnxruntime.dll") -DestinationRelativePath "runtime\resources\ocr\onnxruntime.dll"
        New-SupportSpec -Source (Join-Path $ocrRoot "onnxruntime_providers_shared.dll") -DestinationRelativePath "runtime\resources\ocr\onnxruntime_providers_shared.dll"
    )

    $cliArtifact = [ordered]@{
        name = "loom-cli"
        source = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target\release\loom.exe"))
        entryName = "loom.exe"
        zipNamePattern = "Loom-CLI-{versionId}-windows-x64.zip"
    }

    $pluginSdkArtifact = [ordered]@{
        name = "loom-plugin-sdk"
        pluginCliSource = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target\release\loom-plugin.exe"))
        pluginCliEntryName = "loom-plugin.exe"
        zipNamePattern = "Loom-Plugin-SDK-{versionId}-windows-x64.zip"
        files = @(
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\README.md") -DestinationRelativePath "protocol\README.md"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-manifest.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-manifest.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-execute-request.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-execute-request.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-execute-response.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-execute-response.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\framework-authoring.v1.schema.json") -DestinationRelativePath "protocol\schemas\framework-authoring.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\art-runtime.v1.schema.json") -DestinationRelativePath "protocol\schemas\art-runtime.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\surface-manifest.v1.schema.json") -DestinationRelativePath "protocol\schemas\surface-manifest.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\surface-message.v1.schema.json") -DestinationRelativePath "protocol\schemas\surface-message.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\surface-scene.v1.schema.json") -DestinationRelativePath "protocol\schemas\surface-scene.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\surface-stream.v1.schema.json") -DestinationRelativePath "protocol\schemas\surface-stream.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\device-session.v1.schema.json") -DestinationRelativePath "protocol\schemas\device-session.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "protocol\schemas\hook-message.v1.schema.json") -DestinationRelativePath "protocol\schemas\hook-message.v1.schema.json"
            New-SupportSpec -Source (Join-Path $repoRoot "sdk\surface\README.md") -DestinationRelativePath "sdk\surface\README.md"
            New-SupportSpec -Source (Join-Path $repoRoot "sdk\surface\neuro-surface.d.ts") -DestinationRelativePath "sdk\surface\neuro-surface.d.ts"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-development.md") -DestinationRelativePath "docs\plugin-development.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-security.md") -DestinationRelativePath "docs\plugin-security.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-permissions.md") -DestinationRelativePath "docs\plugin-permissions.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-signing-and-trust.md") -DestinationRelativePath "docs\plugin-signing-and-trust.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\plugin-migration.md") -DestinationRelativePath "docs\plugin-migration.md"
            New-SupportSpec -Source (Join-Path $repoRoot "docs\release-provenance.md") -DestinationRelativePath "docs\release-provenance.md"
        )
    }

    $frameworkPackageCatalog = [ordered]@{
        outputRoot = [System.IO.Path]::GetFullPath($FrameworkPackageOutputRoot)
        expectedIds = @(
            "process",
            "cloud_api",
            "mcp",
            "workflow"
        )
    }

    $sampleArtPackageCatalog = [ordered]@{
        outputRoot = [System.IO.Path]::GetFullPath($SampleArtPackageOutputRoot)
        expected = @(
            [ordered]@{ id = "custom-1770146354922"; framework = "process" }
            [ordered]@{ id = "custom-remove-bg-cloud"; framework = "cloud_api" }
            [ordered]@{ id = "custom-image-search"; framework = "mcp" }
            [ordered]@{ id = "custom-1770131241684"; framework = "process" }
            [ordered]@{ id = "custom-image-blend-script"; framework = "process" }
            [ordered]@{ id = "custom-image-blend-compress-workflow"; framework = "workflow" }
            [ordered]@{ id = "custom-stock-monitor"; framework = "mcp" }
        )
    }

    $mcpServerPackageCatalog = [ordered]@{
        outputRoot = [System.IO.Path]::GetFullPath($McpServerPackageOutputRoot)
        expectedIds = @("neuro-image-search", "stock-api")
    }

    return [ordered]@{
        app = "Loom"
        sourceProject = "Loom"
        sourcePaths = @(".")
        exes = $exes
        supportFiles = $support
        cliArtifact = $cliArtifact
        pluginSdkArtifact = $pluginSdkArtifact
        frameworkPackageCatalog = $frameworkPackageCatalog
        mcpServerPackageCatalog = $mcpServerPackageCatalog
        sampleArtPackageCatalog = $sampleArtPackageCatalog
        commands = @(
            New-CommandSpec -Executable "cargo" `
                -Arguments @("build", "--locked", "--release", "-p", "loom-daemon", "-p", "loom-cli", "-p", "loom-plugin-cli") `
                -WorkingDirectory $repoRoot `
                -Display "cargo build --locked --release -p loom-daemon -p loom-cli -p loom-plugin-cli" `
                -LogName "build-01.log"
            New-CommandSpec -Executable "cmd.exe" `
                -Arguments @("/d", "/c", "npm ci") `
                -WorkingDirectory (Join-Path $repoRoot "apps\desktop") `
                -Display "npm ci" `
                -LogName "build-02.log"
            New-CommandSpec -Executable "cmd.exe" `
                -Arguments @("/d", "/c", "npm run tauri build -- --no-bundle") `
                -WorkingDirectory (Join-Path $repoRoot "apps\desktop") `
                -Display "npm run tauri build -- --no-bundle" `
                -LogName "build-03.log"
            New-CommandSpec -Executable "powershell.exe" `
                -Arguments @(
                    "-NoProfile",
                    "-ExecutionPolicy", "Bypass",
                    "-File", (Join-Path $repoRoot "scripts\Build-LoomArtFrameworkPackages.ps1"),
                    "-OutputRoot", $frameworkPackageCatalog.outputRoot,
                    "-Configuration", "Release"
                ) `
                -WorkingDirectory $repoRoot `
                -Display "Build-LoomArtFrameworkPackages.ps1 -OutputRoot packages\frameworks -Configuration Release" `
                -LogName "build-04.log"
            New-CommandSpec -Executable "powershell.exe" `
                -Arguments @(
                    "-NoProfile",
                    "-ExecutionPolicy", "Bypass",
                    "-File", (Join-Path $repoRoot "scripts\Build-LoomMcpServerPackages.ps1"),
                    "-OutputRoot", $mcpServerPackageCatalog.outputRoot
                ) `
                -WorkingDirectory $repoRoot `
                -Display "Build-LoomMcpServerPackages.ps1 -OutputRoot packages\mcp-servers" `
                -LogName "build-05.log"
            New-CommandSpec -Executable "powershell.exe" `
                -Arguments @(
                    "-NoProfile",
                    "-ExecutionPolicy", "Bypass",
                    "-File", (Join-Path $repoRoot "scripts\Build-LoomSampleArtPackages.ps1"),
                    "-OutputRoot", $sampleArtPackageCatalog.outputRoot,
                    "-Configuration", "Release"
                ) `
                -WorkingDirectory $repoRoot `
                -Display "Build-LoomSampleArtPackages.ps1 -OutputRoot packages\arts -Configuration Release" `
                -LogName "build-06.log"
        )
    }
}
