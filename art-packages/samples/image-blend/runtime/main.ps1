. (Join-Path $PSScriptRoot "common.ps1")
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$workRoot = Get-RequestWorkRoot -Request $request

try {
    $sourcePath = Resolve-ImagePath -Value (Get-RequestInputValue -Request $request -Names @("input", "source", "image")) -Label "blend-source" -WorkRoot $workRoot
    $referencePath = Resolve-ImagePath -Value (Get-RequestInputValue -Request $request -Names @("reference", "referenceImage", "ref")) -Label "blend-reference" -WorkRoot $workRoot
    if ([string]::IsNullOrWhiteSpace($sourcePath) -or [string]::IsNullOrWhiteSpace($referencePath)) {
        throw "input and reference images are required"
    }

    $ratio = [double](Get-RequestParamValue -Request $request -Names @("mix_ratio", "mixRatio", "strength") -DefaultValue 50)
    $source = Load-BitmapArgb -Path $sourcePath
    $reference = Load-BitmapArgb -Path $referencePath
    try {
        $outputBitmap = Blend-Bitmaps -Source $source -Reference $reference -Alpha ($ratio / 100.0)
        try {
            $outputPath = Join-Path $workRoot "image-blend-output.png"
            Save-Png -Bitmap $outputBitmap -Path $outputPath
        }
        finally {
            $outputBitmap.Dispose()
        }
    }
    finally {
        $source.Dispose()
        $reference.Dispose()
    }

    $output = New-ImageOutput -Path $outputPath -Extra @{ mix_ratio = $ratio }
    Write-SuccessResponse -Output $output
}
catch {
    Write-ErrorResponse -Code "image_blend_failed" -Message $_.Exception.Message
}
