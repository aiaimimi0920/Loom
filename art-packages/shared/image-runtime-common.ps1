$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing

function Get-JsonPropertyValue {
    param(
        [AllowNull()][object]$Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }
    if ($Object -is [System.Collections.IDictionary] -and $Object.Contains($Name)) {
        return $Object[$Name]
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -ne $property) {
        return $property.Value
    }
    return $null
}

function Get-JsonPropertyFromNames {
    param(
        [AllowNull()][object]$Object,
        [Parameter(Mandatory = $true)][string[]]$Names
    )

    foreach ($name in $Names) {
        $value = Get-JsonPropertyValue -Object $Object -Name $name
        if ($null -ne $value) {
            return $value
        }
    }
    return $null
}

function Get-RequestWorkRoot {
    param([Parameter(Mandatory = $true)][object]$Request)

    $context = Get-JsonPropertyValue -Object $Request -Name "context"
    $requested = Get-JsonPropertyFromNames -Object $context -Names @("tempDir", "cacheDir")
    $root = if ($requested -is [string] -and -not [string]::IsNullOrWhiteSpace($requested)) {
        [string]$requested
    }
    else {
        Join-Path ([System.IO.Path]::GetTempPath()) "loom-art-package-runtime"
    }
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    return $root
}

function Get-RequestInputValue {
    param(
        [Parameter(Mandatory = $true)][object]$Request,
        [Parameter(Mandatory = $true)][string[]]$Names
    )

    $inputs = Get-JsonPropertyValue -Object $Request -Name "inputs"
    $value = Get-JsonPropertyFromNames -Object $inputs -Names $Names
    if ($null -ne $value) {
        return $value
    }
    if ($inputs -is [System.Collections.IDictionary]) {
        foreach ($item in $inputs.GetEnumerator()) {
            if ($null -ne $item.Value) {
                return $item.Value
            }
        }
    }
    return $null
}

function Get-RequestParamValue {
    param(
        [Parameter(Mandatory = $true)][object]$Request,
        [Parameter(Mandatory = $true)][string[]]$Names,
        [AllowNull()][object]$DefaultValue
    )

    $params = Get-JsonPropertyValue -Object $Request -Name "params"
    $value = Get-JsonPropertyFromNames -Object $params -Names $Names
    if ($null -eq $value) {
        return $DefaultValue
    }
    return $value
}

function Resolve-ImagePath {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$WorkRoot
    )

    if ($null -eq $Value) {
        return $null
    }
    if ($Value -is [System.Array]) {
        foreach ($item in $Value) {
            $resolved = Resolve-ImagePath -Value $item -Label $Label -WorkRoot $WorkRoot
            if (-not [string]::IsNullOrWhiteSpace($resolved)) {
                return $resolved
            }
        }
        return $null
    }
    if ($Value -is [string]) {
        $text = $Value.Trim()
        if ([string]::IsNullOrWhiteSpace($text)) {
            return $null
        }
        if ($text -match '^data:image\/[A-Za-z0-9.+-]+;base64,(?<data>.+)$') {
            $path = Join-Path $WorkRoot "$Label-input.png"
            [System.IO.File]::WriteAllBytes($path, [Convert]::FromBase64String($Matches["data"]))
            return $path
        }
        if ($text.StartsWith("file://", [System.StringComparison]::OrdinalIgnoreCase)) {
            try {
                $text = ([System.Uri]$text).LocalPath
            }
            catch {
                return $null
            }
        }
        if (Test-Path -LiteralPath $text -PathType Leaf) {
            return [System.IO.Path]::GetFullPath($text)
        }
        return $null
    }

    $nested = Get-JsonPropertyFromNames -Object $Value -Names @(
        "path", "filePath", "imagePath", "url", "source", "value", "data", "base64", "imageBase64"
    )
    if ($null -ne $nested) {
        return Resolve-ImagePath -Value $nested -Label $Label -WorkRoot $WorkRoot
    }
    $content = Get-JsonPropertyValue -Object $Value -Name "content"
    if ($null -ne $content) {
        return Resolve-ImagePath -Value $content -Label $Label -WorkRoot $WorkRoot
    }
    return $null
}

function Load-BitmapArgb {
    param([Parameter(Mandatory = $true)][string]$Path)

    $loaded = [System.Drawing.Bitmap]::new($Path)
    try {
        $bitmap = [System.Drawing.Bitmap]::new(
            $loaded.Width,
            $loaded.Height,
            [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
        )
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.DrawImage($loaded, 0, 0, $loaded.Width, $loaded.Height)
        }
        finally {
            $graphics.Dispose()
        }
        return $bitmap
    }
    finally {
        $loaded.Dispose()
    }
}

function Resize-BitmapArgb {
    param(
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Bitmap,
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height
    )

    $resized = [System.Drawing.Bitmap]::new(
        $Width,
        $Height,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    $graphics = [System.Drawing.Graphics]::FromImage($resized)
    try {
        $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.DrawImage($Bitmap, 0, 0, $Width, $Height)
    }
    finally {
        $graphics.Dispose()
    }
    return $resized
}

function Blend-Bitmaps {
    param(
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Source,
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Reference,
        [Parameter(Mandatory = $true)][double]$Alpha
    )

    $referenceSized = Resize-BitmapArgb -Bitmap $Reference -Width $Source.Width -Height $Source.Height
    $output = [System.Drawing.Bitmap]::new(
        $Source.Width,
        $Source.Height,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    $clamped = [Math]::Max(0.0, [Math]::Min(1.0, $Alpha))
    try {
        for ($y = 0; $y -lt $Source.Height; $y++) {
            for ($x = 0; $x -lt $Source.Width; $x++) {
                $sourcePixel = $Source.GetPixel($x, $y)
                $referencePixel = $referenceSized.GetPixel($x, $y)
                $red = [int][Math]::Round(($sourcePixel.R * (1.0 - $clamped)) + ($referencePixel.R * $clamped))
                $green = [int][Math]::Round(($sourcePixel.G * (1.0 - $clamped)) + ($referencePixel.G * $clamped))
                $blue = [int][Math]::Round(($sourcePixel.B * (1.0 - $clamped)) + ($referencePixel.B * $clamped))
                $alpha = [int][Math]::Round(($sourcePixel.A * (1.0 - $clamped)) + ($referencePixel.A * $clamped))
                $output.SetPixel($x, $y, [System.Drawing.Color]::FromArgb($alpha, $red, $green, $blue))
            }
        }
        return $output
    }
    finally {
        $referenceSized.Dispose()
    }
}

function Save-Png {
    param(
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Bitmap,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $parent = Split-Path -Parent $Path
    New-Item -ItemType Directory -Force -Path $parent | Out-Null
    $Bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
}

function Convert-ImagePathToDataUrl {
    param([Parameter(Mandatory = $true)][string]$Path)

    return "data:image/png;base64,$([Convert]::ToBase64String([System.IO.File]::ReadAllBytes($Path)))"
}

function New-ImageOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [AllowNull()][hashtable]$Extra
    )

    $bitmap = [System.Drawing.Bitmap]::new($Path)
    try {
        $data = Convert-ImagePathToDataUrl -Path $Path
        $output = [ordered]@{
            output_base64 = $data
            output_path = $Path
            width = $bitmap.Width
            height = $bitmap.Height
            content = @(
                [ordered]@{
                    type = "image"
                    data = $data
                    mimeType = "image/png"
                }
            )
        }
        if ($null -ne $Extra) {
            foreach ($key in $Extra.Keys) {
                $output[$key] = $Extra[$key]
            }
        }
        return $output
    }
    finally {
        $bitmap.Dispose()
    }
}

function New-ImagePathOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [AllowNull()][hashtable]$Extra
    )

    $bitmap = [System.Drawing.Bitmap]::new($Path)
    try {
        $output = [ordered]@{
            output_path = $Path
            width = $bitmap.Width
            height = $bitmap.Height
        }
        if ($null -ne $Extra) {
            foreach ($key in $Extra.Keys) {
                $output[$key] = $Extra[$key]
            }
        }
        return $output
    }
    finally {
        $bitmap.Dispose()
    }
}

function New-PlaceholderImage {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$Red,
        [Parameter(Mandatory = $true)][int]$Green,
        [Parameter(Mandatory = $true)][int]$Blue,
        [string]$Label = "Loom Art"
    )

    $bitmap = [System.Drawing.Bitmap]::new(256, 160, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.Clear([System.Drawing.Color]::FromArgb(255, $Red, $Green, $Blue))
        $brush = [System.Drawing.Brushes]::White
        $font = [System.Drawing.Font]::new("Segoe UI", 16)
        try {
            $graphics.DrawString($Label, $font, $brush, 12, 68)
        }
        finally {
            $font.Dispose()
        }
    }
    finally {
        $graphics.Dispose()
    }
    try {
        Save-Png -Bitmap $bitmap -Path $Path
    }
    finally {
        $bitmap.Dispose()
    }
}

function Write-SuccessResponse {
    param(
        [Parameter(Mandatory = $true)][object]$Output,
        [AllowNull()][object[]]$Candidates
    )

    $response = [ordered]@{
        status = "success"
        output = $Output
    }
    if ($null -ne $Candidates -and $Candidates.Count -gt 0) {
        $response.candidates = @($Candidates)
    }
    [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 40 -Compress))
}

function Write-ErrorResponse {
    param(
        [Parameter(Mandatory = $true)][string]$Code,
        [Parameter(Mandatory = $true)][string]$Message,
        [AllowNull()][string]$Detail
    )

    $response = [ordered]@{
        status = "error"
        error = [ordered]@{
            code = $Code
            message = $Message
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($Detail)) {
        $response.error.detail = $Detail
    }
    [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
}
