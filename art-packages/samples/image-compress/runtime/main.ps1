. (Join-Path $PSScriptRoot "common.ps1")
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$workRoot = Get-RequestWorkRoot -Request $request

try {
    $inputValue = Get-RequestInputValue -Request $request -Names @("input", "image", "source")
    $inputPath = Resolve-ImagePath -Value $inputValue -Label "compress" -WorkRoot $workRoot
    if ([string]::IsNullOrWhiteSpace($inputPath)) {
        throw "input image is required"
    }

    $outputPath = Join-Path $workRoot "image-compress-output.png"
    $bitmap = Load-BitmapArgb -Path $inputPath
    try {
        Save-Png -Bitmap $bitmap -Path $outputPath
    }
    finally {
        $bitmap.Dispose()
    }

    $quality = [double](Get-RequestParamValue -Request $request -Names @("quality_num", "quality") -DefaultValue 90)
    $output = New-ImagePathOutput -Path $outputPath -Extra @{
        quality_num = $quality
        lossless = [bool](Get-RequestParamValue -Request $request -Names @("lossless") -DefaultValue $true)
        compression = "png-reencode"
    }
    Write-SuccessResponse -Output $output
}
catch {
    Write-ErrorResponse -Code "image_compress_failed" -Message $_.Exception.Message
}
