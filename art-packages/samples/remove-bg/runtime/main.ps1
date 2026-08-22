$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "common.ps1")
$request = [Console]::In.ReadToEnd() | ConvertFrom-Json
$workRoot = Get-RequestWorkRoot -Request $request
$allowedRoots = Get-RequestImageRoots -Request $request -WorkRoot $workRoot

try {
    $inputValue = Get-RequestInputValue -Request $request -Names @("input", "image", "source")
    $inputPath = Resolve-ImagePath `
        -Value $inputValue `
        -Label "remove-bg" `
        -WorkRoot $workRoot `
        -AllowedRoots $allowedRoots
    if ([string]::IsNullOrWhiteSpace($inputPath)) {
        throw "input image is required"
    }

    $outputPath = Join-Path $workRoot (New-WorkRootFileName -Stem "remove-bg-output")
    $bitmap = Load-BitmapArgb -Path $inputPath
    try {
        # The package remains deterministic when no cloud credential is present:
        # near-white pixels are made transparent, while all other pixels remain.
        $bitmap.MakeTransparent([System.Drawing.Color]::White)
        Save-Png -Bitmap $bitmap -Path $outputPath
    }
    finally {
        $bitmap.Dispose()
    }

    $output = New-ImageOutput -Path $outputPath -Extra @{
        backgroundRemoved = $true
        provider = "package-local-adapter"
    }
    Write-SuccessResponse -Output $output
}
catch {
    Write-ErrorResponse -Code "remove_bg_failed" -Message $_.Exception.Message
}
