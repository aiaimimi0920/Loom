<# Owns the native one-pixel PNG fixture and image-helper conversion assertions. #>

function New-LoomNativeImageSmokePngDataUrl {
    Add-Type -AssemblyName System.Drawing

    $bitmap = $null
    $stream = $null
    try {
        $bitmap = [System.Drawing.Bitmap]::new(1, 1)
        $bitmap.SetPixel(0, 0, [System.Drawing.Color]::FromArgb(255, 10, 20, 30))
        $stream = [System.IO.MemoryStream]::new()
        $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
        return "data:image/png;base64,$([Convert]::ToBase64String($stream.ToArray()))"
    } finally {
        if ($null -ne $stream) {
            $stream.Dispose()
        }
        if ($null -ne $bitmap) {
            $bitmap.Dispose()
        }
    }
}











function Test-LoomImageHelperConvert {
    param(
        [string]$BaseUrl
    )

    $imageData = New-LoomNativeImageSmokePngDataUrl
    $buffer = Invoke-JsonPost -Uri "$BaseUrl/v1/image-helpers/convert" -Body @{
        sourceType = "image_base64"
        targetType = "image_buffer"
        data = $imageData
    }
    Assert-Equal 1 ([int]$buffer.image.width) "Loom image helper buffer width mismatch."
    Assert-Equal 1 ([int]$buffer.image.height) "Loom image helper buffer height mismatch."
    Assert-Equal "rgba8" ([string]$buffer.image.format) "Loom image helper buffer format mismatch."
    Assert-Equal 4 ([int]$buffer.image.size) "Loom image helper buffer size mismatch."
    $rgba = @($buffer.data | ForEach-Object { [int]$_ })
    Assert-Equal "10,20,30,255" ($rgba -join ",") "Loom image helper RGBA output mismatch."

    $base64 = Invoke-JsonPost -Uri "$BaseUrl/v1/image-helpers/convert" -Body @{
        sourceType = "image_buffer"
        targetType = "image_base64"
        width = 1
        height = 1
        data = @(10, 20, 30, 255)
    }
    $dataBase64 = [string]$base64.dataBase64
    if (-not $dataBase64.StartsWith("data:image/png;base64,", [System.StringComparison]::Ordinal)) {
        throw "Loom image helper image_buffer to image_base64 did not return a PNG data URL."
    }

    return [ordered]@{
        inputType = "image_base64"
        outputType = "image_buffer"
        width = [int]$buffer.image.width
        height = [int]$buffer.image.height
        format = [string]$buffer.image.format
        size = [int]$buffer.image.size
        outputRgba = ($rgba -join ",")
        roundtripType = "image_base64"
        roundtripLength = [int]$dataBase64.Length
    }
}
