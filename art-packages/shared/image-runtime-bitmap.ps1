# Bitmap transforms and bounded thumbnails; callers own returned bitmap disposal.

function Initialize-ImageRuntime {
    # GDI+ is loaded the first time a caller actually needs a bitmap, rather than when this file is
    # dot-sourced, because a file-scope `Add-Type` charges every framework Art for the load whether or
    # not it ever decodes an image. `image-search` reads JSON and forwards data URLs it already has,
    # and only reaches `System.Drawing` when it is asked to downscale one of them.
    if ('System.Drawing.Bitmap' -as [type]) {
        return
    }
    Add-Type -AssemblyName System.Drawing
}

function Load-BitmapArgb {
    param([Parameter(Mandatory = $true)][string]$Path)

    Initialize-ImageRuntime
    # Both bitmaps are allocated inside the `try`, and the one this function is building is disposed on
    # the way out of the `catch`. Anything between its allocation and the `return` can fail - an
    # out-of-memory `DrawImage`, a GDI+ error, the host's timeout kill landing mid-draw - and a
    # full-size 32bpp surface would otherwise be abandoned. The `catch` cannot run on the success path,
    # so the caller receives a bitmap nobody else has disposed.
    $loaded = $null
    $bitmap = $null
    try {
        $loaded = [System.Drawing.Bitmap]::new($Path)
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
    catch {
        if ($null -ne $bitmap) {
            $bitmap.Dispose()
        }
        throw
    }
    finally {
        if ($null -ne $loaded) {
            $loaded.Dispose()
        }
    }
}

function Resize-BitmapArgb {
    param(
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Bitmap,
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height
    )

    $resized = $null
    try {
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
    catch {
        if ($null -ne $resized) {
            $resized.Dispose()
        }
        throw
    }
}

function Blend-Bitmaps {
    param(
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Source,
        [Parameter(Mandatory = $true)][System.Drawing.Bitmap]$Reference,
        [Parameter(Mandatory = $true)][double]$Alpha
    )

    # Two GDI+ draws rather than a PowerShell loop over every pixel. The loop this replaced walked
    # `Height * Width` and paid two `GetPixel` calls, four roundings, and a `SetPixel` per pixel, and
    # each pixel accessor locks and unlocks the bitmap's bits on its own: a 1920x1080 blend was over
    # six million interop calls, and a 4000x3000 one ran past the host's framework process timeout, so
    # the caller saw a timeout instead of a slow blend.
    $clamped = [Math]::Max(0.0, [Math]::Min(1.0, $Alpha))
    $referenceSized = Resize-BitmapArgb -Bitmap $Reference -Width $Source.Width -Height $Source.Height
    $output = $null
    try {
        $output = [System.Drawing.Bitmap]::new(
            $Source.Width,
            $Source.Height,
            [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
        )
        $graphics = [System.Drawing.Graphics]::FromImage($output)
        $attributes = $null
        try {
            # Both draws are 1:1, so nearest-neighbour sampling keeps them straight copies. The
            # reference draw also pins the wrap mode, because GDI+ otherwise samples past the edge of
            # the source rectangle and leaves the outermost row and column of the composite unblended.
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
            $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.DrawImage($Source, 0, 0, $Source.Width, $Source.Height)
            if ($clamped -gt 0.0) {
                # The source is copied verbatim and the reference composited over it at the mix ratio,
                # which is `source * (1 - ratio) + reference * ratio` per colour channel wherever both
                # layers are opaque — the same result the loop produced. The two differ only where a
                # layer is transparent, and the composite is the sane one: the loop read a transparent
                # pixel's colour as black and darkened the other layer with it, and it also made an
                # opaque source semi-transparent when the reference was not there at all.
                $graphics.CompositingMode = [System.Drawing.Drawing2D.CompositingMode]::SourceOver
                $matrix = [System.Drawing.Imaging.ColorMatrix]::new()
                $matrix.Matrix33 = [float]$clamped
                $attributes = [System.Drawing.Imaging.ImageAttributes]::new()
                $attributes.SetColorMatrix($matrix)
                $attributes.SetWrapMode([System.Drawing.Drawing2D.WrapMode]::TileFlipXY)
                $graphics.DrawImage(
                    $referenceSized,
                    [System.Drawing.Rectangle]::new(0, 0, $Source.Width, $Source.Height),
                    0,
                    0,
                    $referenceSized.Width,
                    $referenceSized.Height,
                    [System.Drawing.GraphicsUnit]::Pixel,
                    $attributes
                )
            }
        }
        finally {
            if ($null -ne $attributes) {
                $attributes.Dispose()
            }
            $graphics.Dispose()
        }
        return $output
    }
    catch {
        # The composite is disposed if any of the work between its allocation and the `return` fails,
        # for the same reason `Load-BitmapArgb` disposes its own: this is a full-size 32bpp surface and
        # the two draws below are exactly where a GDI+ or out-of-memory failure lands.
        if ($null -ne $output) {
            $output.Dispose()
        }
        throw
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

function Get-ImageDimensions {
    param([Parameter(Mandatory = $true)][string]$Path)

    # Dimensions come from the file's header, not from its pixels. The output builders used to construct a
    # `System.Drawing.Bitmap` from the path just to reach `.Width`/`.Height`, and that constructor decodes
    # the whole image: the surface it allocates is width * height * 4 bytes and has no relation to the
    # compressed size, so a 4000x3000 PNG cost 48 MiB to answer two integers. `FromStream` with
    # `validateImageData` false reads the header and leaves the pixel data alone.
    #
    # The stream is opened with `FileShare.Read` and closed here, which also ends the second problem: the
    # `Bitmap` path constructor holds an exclusive lock on the file for the lifetime of the object, so the
    # builder held its own output open while base64-encoding it.
    #
    # Skipping validation means a truncated or corrupt file is no longer rejected at this point. That is
    # the right trade here: every caller has just written this file itself with GDI+, so a decode would be
    # re-validating the runtime's own output, and a caller that hands over a file it did not produce has
    # already resolved it through `Resolve-ImagePath`.
    Initialize-ImageRuntime
    $stream = [System.IO.File]::Open(
        $Path,
        [System.IO.FileMode]::Open,
        [System.IO.FileAccess]::Read,
        [System.IO.FileShare]::Read
    )
    try {
        $image = [System.Drawing.Image]::FromStream($stream, $false, $false)
        try {
            return [pscustomobject]@{
                Width = $image.Width
                Height = $image.Height
            }
        }
        finally {
            $image.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}
function Convert-ImagePathToDataUrl {
    param([Parameter(Mandatory = $true)][string]$Path)

    return "data:image/png;base64,$([Convert]::ToBase64String([System.IO.File]::ReadAllBytes($Path)))"
}

function New-ImageThumbnailDataUrl {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$DataUrl,
        [int]$MaxEdge = 320
    )

    # A candidate grid paints its thumbnails at gallery size, so a candidate that carries the
    # full-resolution image under `thumbnail` pays for every pixel twice and shows none of them. The
    # downscale is an optimization and never a hard requirement: an input that cannot be decoded, or
    # that is already small enough, is returned unchanged.
    if ([string]::IsNullOrWhiteSpace($DataUrl) -or $MaxEdge -lt 1) {
        return $DataUrl
    }
    $payload = $DataUrl
    $separator = $DataUrl.IndexOf(";base64,")
    if ($separator -ge 0) {
        $payload = $DataUrl.Substring($separator + 8)
    }
    try {
        $bytes = [Convert]::FromBase64String($payload)
    }
    catch {
        return $DataUrl
    }
    Initialize-ImageRuntime
    $stream = [System.IO.MemoryStream]::new($bytes)
    try {
        $source = $null
        try {
            $source = [System.Drawing.Bitmap]::new($stream)
        }
        catch {
            return $DataUrl
        }
        try {
            $longestEdge = [Math]::Max($source.Width, $source.Height)
            if ($longestEdge -le $MaxEdge) {
                return $DataUrl
            }
            $scale = $MaxEdge / [double]$longestEdge
            $width = [Math]::Max(1, [int][Math]::Round($source.Width * $scale))
            $height = [Math]::Max(1, [int][Math]::Round($source.Height * $scale))
            $thumbnail = Resize-BitmapArgb -Bitmap $source -Width $width -Height $height
            try {
                $encoded = [System.IO.MemoryStream]::new()
                try {
                    $thumbnail.Save($encoded, [System.Drawing.Imaging.ImageFormat]::Png)
                    return "data:image/png;base64,$([Convert]::ToBase64String($encoded.ToArray()))"
                }
                finally {
                    $encoded.Dispose()
                }
            }
            finally {
                $thumbnail.Dispose()
            }
        }
        finally {
            $source.Dispose()
        }
    }
    finally {
        $stream.Dispose()
    }
}
