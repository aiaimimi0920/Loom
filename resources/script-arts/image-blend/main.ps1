[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$PayloadJson
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing
if (-not ("LoomImageBlendNative" -as [type])) {
    Add-Type -ReferencedAssemblies @([System.Drawing.Bitmap].Assembly.Location) -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public static class LoomImageBlendNative
{
    public static void Blend(string inputPath, string referencePath, double blendAlpha, string outputPath)
    {
        using (var source = LoadBitmapArgb(inputPath))
        using (var reference = ResizeBitmapArgb(referencePath, source.Width, source.Height))
        using (var output = new Bitmap(source.Width, source.Height, PixelFormat.Format32bppArgb))
        {
            BlendBitmaps(source, reference, output, blendAlpha);
            output.Save(outputPath, ImageFormat.Png);
        }
    }

    private static Bitmap LoadBitmapArgb(string path)
    {
        using (var loaded = new Bitmap(path))
        {
            var converted = new Bitmap(loaded.Width, loaded.Height, PixelFormat.Format32bppArgb);
            using (var graphics = Graphics.FromImage(converted))
            {
                ConfigureGraphics(graphics);
                graphics.DrawImage(loaded, 0, 0, loaded.Width, loaded.Height);
            }

            return converted;
        }
    }

    private static Bitmap ResizeBitmapArgb(string path, int width, int height)
    {
        using (var loaded = new Bitmap(path))
        {
            var resized = new Bitmap(width, height, PixelFormat.Format32bppArgb);
            using (var graphics = Graphics.FromImage(resized))
            {
                ConfigureGraphics(graphics);
                graphics.DrawImage(loaded, 0, 0, width, height);
            }

            return resized;
        }
    }

    private static void ConfigureGraphics(Graphics graphics)
    {
        graphics.CompositingMode = CompositingMode.SourceCopy;
        graphics.CompositingQuality = CompositingQuality.HighQuality;
        graphics.InterpolationMode = InterpolationMode.HighQualityBicubic;
        graphics.PixelOffsetMode = PixelOffsetMode.HighQuality;
        graphics.SmoothingMode = SmoothingMode.HighQuality;
    }

    private static void BlendBitmaps(Bitmap source, Bitmap reference, Bitmap output, double blendAlpha)
    {
        double clampedAlpha = Math.Max(0.0, Math.Min(1.0, blendAlpha));
        double inverseAlpha = 1.0 - clampedAlpha;
        var rect = new Rectangle(0, 0, source.Width, source.Height);
        var sourceData = source.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        var referenceData = reference.LockBits(rect, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
        var outputData = output.LockBits(rect, ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);

        try
        {
            int sourceStride = Math.Abs(sourceData.Stride);
            int referenceStride = Math.Abs(referenceData.Stride);
            int outputStride = Math.Abs(outputData.Stride);
            byte[] sourceBytes = new byte[sourceStride * source.Height];
            byte[] referenceBytes = new byte[referenceStride * reference.Height];
            byte[] outputBytes = new byte[outputStride * output.Height];

            Marshal.Copy(sourceData.Scan0, sourceBytes, 0, sourceBytes.Length);
            Marshal.Copy(referenceData.Scan0, referenceBytes, 0, referenceBytes.Length);

            for (int y = 0; y < source.Height; y++)
            {
                int sourceRow = RowOffset(y, sourceData.Stride, source.Height);
                int referenceRow = RowOffset(y, referenceData.Stride, reference.Height);
                int outputRow = RowOffset(y, outputData.Stride, output.Height);

                for (int x = 0; x < source.Width; x++)
                {
                    int sourceIndex = sourceRow + (x * 4);
                    int referenceIndex = referenceRow + (x * 4);
                    int outputIndex = outputRow + (x * 4);

                    outputBytes[outputIndex] = BlendChannel(sourceBytes[sourceIndex], referenceBytes[referenceIndex], inverseAlpha, clampedAlpha);
                    outputBytes[outputIndex + 1] = BlendChannel(sourceBytes[sourceIndex + 1], referenceBytes[referenceIndex + 1], inverseAlpha, clampedAlpha);
                    outputBytes[outputIndex + 2] = BlendChannel(sourceBytes[sourceIndex + 2], referenceBytes[referenceIndex + 2], inverseAlpha, clampedAlpha);
                    outputBytes[outputIndex + 3] = BlendChannel(sourceBytes[sourceIndex + 3], referenceBytes[referenceIndex + 3], inverseAlpha, clampedAlpha);
                }
            }

            Marshal.Copy(outputBytes, 0, outputData.Scan0, outputBytes.Length);
        }
        finally
        {
            source.UnlockBits(sourceData);
            reference.UnlockBits(referenceData);
            output.UnlockBits(outputData);
        }
    }

    private static int RowOffset(int y, int stride, int height)
    {
        return stride >= 0 ? y * stride : (height - 1 - y) * Math.Abs(stride);
    }

    private static byte BlendChannel(byte source, byte reference, double inverseAlpha, double blendAlpha)
    {
        double blended = Math.Round(
            (source * inverseAlpha) + (reference * blendAlpha),
            MidpointRounding.AwayFromZero
        );
        if (blended <= 0.0)
        {
            return 0;
        }

        if (blended >= 255.0)
        {
            return 255;
        }

        return (byte)blended;
    }
}
"@
}

function Get-JsonPropertyValue {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Object,
        [Parameter(Mandatory = $true)]
        [string[]]$Names
    )

    foreach ($name in $Names) {
        if ($Object -is [System.Collections.IDictionary] -and $Object.Contains($name)) {
            return $Object[$name]
        }
        $property = $Object.PSObject.Properties[$name]
        if ($null -ne $property) {
            return $property.Value
        }
    }
    return $null
}

function Resolve-AssetOrFileUrlPath {
    param([string]$Value)

    try {
        $uri = [System.Uri]$Value
    }
    catch {
        return $null
    }

    if ($uri.Scheme -ieq "file") {
        return $uri.LocalPath
    }

    if (
        ($uri.Scheme -ieq "http" -or $uri.Scheme -ieq "asset") -and
        ($uri.Host -ieq "asset.localhost" -or $uri.Host -ieq "localhost")
    ) {
        $decoded = [System.Uri]::UnescapeDataString($uri.AbsolutePath.TrimStart('/'))
        if (-not [string]::IsNullOrWhiteSpace($decoded)) {
            return $decoded
        }
    }

    return $null
}

function Write-DataUrlImage {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value,
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [string]$WorkRoot
    )

    if ($Value -notmatch '^data:image\/[A-Za-z0-9.+-]+;base64,(?<data>.+)$') {
        return $null
    }

    $path = Join-Path $WorkRoot "$Label.png"
    $bytes = [Convert]::FromBase64String($Matches["data"])
    [System.IO.File]::WriteAllBytes($path, $bytes)
    return $path
}

function Resolve-ImageInput {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Value,
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [string]$WorkRoot
    )

    $candidate = if ($Value -is [string]) {
        $Value.Trim()
    }
    else {
        $nested = Get-JsonPropertyValue -Object $Value -Names @("data", "path", "src", "value")
        if ($nested -is [string]) { $nested.Trim() } else { "" }
    }

    if ([string]::IsNullOrWhiteSpace($candidate)) {
        throw "$Label image is required"
    }

    $dataUrlPath = Write-DataUrlImage -Value $candidate -Label $Label -WorkRoot $WorkRoot
    if ($dataUrlPath) {
        return $dataUrlPath
    }

    $urlPath = Resolve-AssetOrFileUrlPath -Value $candidate
    if ($urlPath -and (Test-Path -LiteralPath $urlPath -PathType Leaf)) {
        return (Resolve-Path -LiteralPath $urlPath).Path
    }

    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
        return (Resolve-Path -LiteralPath $candidate).Path
    }

    throw "$Label image could not be resolved: $candidate"
}

function Clamp-Double {
    param(
        [double]$Value,
        [double]$Min,
        [double]$Max
    )

    return [Math]::Max($Min, [Math]::Min($Max, $Value))
}

function Convert-ImagePathToDataUrl {
    param([Parameter(Mandatory = $true)][string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    return "data:image/png;base64," + [Convert]::ToBase64String($bytes)
}

function Invoke-BlendImages {
    param(
        [Parameter(Mandatory = $true)]
        [string]$InputPath,
        [Parameter(Mandatory = $true)]
        [string]$ReferencePath,
        [Parameter(Mandatory = $true)]
        [double]$BlendAlpha,
        [Parameter(Mandatory = $true)]
        [string]$OutputPath
    )

    [LoomImageBlendNative]::Blend($InputPath, $ReferencePath, $BlendAlpha, $OutputPath)
}

$payload = $PayloadJson | ConvertFrom-Json
$arguments = if ($null -ne $payload.arguments) { $payload.arguments } else { [pscustomobject]@{} }

$workRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("loom-script-image-blend-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $workRoot | Out-Null

try {
    $inputValue = Get-JsonPropertyValue -Object $arguments -Names @("input", "input_base64", "image")
    if ($null -eq $inputValue) {
        throw "input image is required"
    }
    $referenceValue = Get-JsonPropertyValue -Object $arguments -Names @("reference", "reference_image")
    if ($null -eq $referenceValue) {
        throw "reference image is required"
    }

    $inputPath = Resolve-ImageInput -Value $inputValue -Label "input" -WorkRoot $workRoot
    $referencePath = Resolve-ImageInput -Value $referenceValue -Label "reference" -WorkRoot $workRoot

    $ratioValue = Get-JsonPropertyValue -Object $arguments -Names @("mix_ratio", "ratio", "blend_ratio")
    $ratio = 50.0
    if ($null -ne $ratioValue -and -not [string]::IsNullOrWhiteSpace([string]$ratioValue)) {
        $ratio = [double]$ratioValue
    }
    $ratio = Clamp-Double -Value $ratio -Min 0.0 -Max 100.0
    $blendAlpha = $ratio / 100.0

    $requestedOutputPath = Get-JsonPropertyValue -Object $arguments -Names @("output_path", "output")
    $outputPath = if ($requestedOutputPath -is [string] -and -not [string]::IsNullOrWhiteSpace($requestedOutputPath)) {
        $requestedOutputPath
    }
    else {
        Join-Path $workRoot "output.png"
    }

    $outputParent = Split-Path -Parent $outputPath
    if (-not [string]::IsNullOrWhiteSpace($outputParent)) {
        New-Item -ItemType Directory -Force -Path $outputParent | Out-Null
    }

    Invoke-BlendImages -InputPath $inputPath -ReferencePath $referencePath -BlendAlpha $blendAlpha -OutputPath $outputPath

    $outputDataUrl = Convert-ImagePathToDataUrl -Path $outputPath
    $response = [ordered]@{
        content = @(
            [ordered]@{
                type = "image"
                data = $outputDataUrl
                mimeType = "image/png"
            }
        )
        output_path = $outputPath
        output_base64 = $outputDataUrl
        mix_ratio = $ratio
    }
    [Console]::Out.WriteLine(($response | ConvertTo-Json -Depth 20 -Compress))
}
finally {
    $defaultOutputPath = Join-Path $workRoot "output.png"
    if (-not (Test-Path -LiteralPath $defaultOutputPath -PathType Leaf)) {
        Remove-Item -Recurse -Force -LiteralPath $workRoot -ErrorAction SilentlyContinue
    }
}
