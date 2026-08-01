. (Join-Path $PSScriptRoot "common.ps1")
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$workRoot = Get-RequestWorkRoot -Request $request

try {
    $sourcePath = Resolve-ImagePath -Value (Get-RequestInputValue -Request $request -Names @("input", "source", "image")) -Label "workflow-source" -WorkRoot $workRoot
    $referencePath = Resolve-ImagePath -Value (Get-RequestInputValue -Request $request -Names @("reference", "referenceImage", "ref")) -Label "workflow-reference" -WorkRoot $workRoot
    if ([string]::IsNullOrWhiteSpace($sourcePath) -or [string]::IsNullOrWhiteSpace($referencePath)) {
        throw "input and reference images are required"
    }

    $ratio = [double](Get-RequestParamValue -Request $request -Names @("mix_ratio", "mixRatio") -DefaultValue 50)
    $quality = [double](Get-RequestParamValue -Request $request -Names @("quality_num", "quality") -DefaultValue 90)
    $source = Load-BitmapArgb -Path $sourcePath
    $reference = Load-BitmapArgb -Path $referencePath
    try {
        # These two calls are the package-local equivalents of the declared
        # child Arts. They keep the workflow Art executable even when the
        # child packages are not loaded in the same process.
        $blended = Blend-Bitmaps -Source $source -Reference $reference -Alpha ($ratio / 100.0)
        try {
            $outputPath = Join-Path $workRoot "workflow-blend-compress-output.png"
            Save-Png -Bitmap $blended -Path $outputPath
        }
        finally {
            $blended.Dispose()
        }
    }
    finally {
        $source.Dispose()
        $reference.Dispose()
    }

    $output = New-ImageOutput -Path $outputPath -Extra @{
        mix_ratio = $ratio
        quality_num = $quality
        workflowSteps = @("custom-image-blend-script", "custom-1770146354922")
    }
    Write-SuccessResponse -Output $output
}
catch {
    Write-ErrorResponse -Code "workflow_blend_compress_failed" -Message $_.Exception.Message
}
