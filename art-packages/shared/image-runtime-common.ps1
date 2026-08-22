# This file is dot-sourced, so whatever it does at file scope it does to the caller's scope. It
# therefore declares no preference variables of its own: an entry point that wants terminating errors
# sets `$ErrorActionPreference` itself, before dot-sourcing this file.

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

function Get-ArtRuntimeInstanceId {
    # A value unique to this process, used to keep the fallback work root and the filenames written
    # into it clear of a concurrent execution of the same Art.
    #
    # Process id plus start time rather than a GUID: a GUID would have to be generated once and then
    # remembered, and the only scope a dot-sourced file can remember anything in is its caller's,
    # which is the coupling this file otherwise avoids. The pair is unique among live processes - only
    # one process holds a given id at a time - and stays unique across id reuse, because a reused id
    # necessarily has a later start time. It is derived on each call and cached nowhere.
    $process = [System.Diagnostics.Process]::GetCurrentProcess()
    return "{0}-{1}" -f $process.Id, $process.StartTime.ToString("HHmmssfff")
}

function New-WorkRootFileName {
    param(
        [Parameter(Mandatory = $true)][string]$Stem,
        [string]$Extension = ".png"
    )

    # Every filename an Art generates inside the work root carries the instance id, because the
    # fallback root is shared rather than per request: two executions that both wrote
    # `image-blend-output.png` would otherwise hand each other's pixels back to their callers.
    return "{0}-{1}{2}" -f $Stem, (Get-ArtRuntimeInstanceId), $Extension
}

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
        if ($null -eq $value) {
            continue
        }
        # A present-but-blank string counts as absent, so a request carrying
        # `{ "path": "", "url": "https://..." }` resolves to the URL instead of stopping at the empty
        # `path` and reporting a missing input while holding a usable value. Callers already read a
        # blank string that way once they receive it - `Get-RequestWorkRoot` and `Resolve-ImagePath`
        # both reject whitespace - and rejecting it here lets the remaining names have their turn.
        if ($value -is [string] -and [string]::IsNullOrWhiteSpace($value)) {
            continue
        }
        return $value
    }
    return $null
}

function Get-RequestWorkRoot {
    param([Parameter(Mandatory = $true)][object]$Request)

    $context = Get-JsonPropertyValue -Object $Request -Name "context"
    $requested = Get-JsonPropertyFromNames -Object $context -Names @("tempDir", "cacheDir")
    $root = if ($requested -is [string]) {
        # A requested root normally comes from the host, but this script also runs from smoke harnesses
        # and by hand, so the value is checked the way the host checks a path before it touches the
        # filesystem on behalf of request content: `loom_security::network::validate_local_path`
        # refuses UNC and device spellings, and `New-Item -Force` below would otherwise bring whatever
        # the request named into existence. A work root additionally has to be absolute, because a
        # relative one resolves against whichever directory this process happened to be spawned in.
        if (Test-RemoteOrDevicePath -Path $requested) {
            throw "requested work root '$requested' names a remote or device path"
        }
        if (-not [System.IO.Path]::IsPathRooted($requested)) {
            throw "requested work root '$requested' must be an absolute path"
        }
        [string]$requested
    }
    else {
        # No requested root. Production never lands here - `framework_process.rs` always supplies a
        # per-request temp directory and deletes it afterwards - so this branch belongs to smoke
        # harnesses, manual runs, and any future caller that omits `context`. Those callers get a
        # directory of their own per process instead of one directory shared by every Art, so
        # concurrent runs cannot overwrite each other's inputs and outputs.
        Join-Path ([System.IO.Path]::GetTempPath()) (
            Join-Path "loom-art-package-runtime" (Get-ArtRuntimeInstanceId)
        )
    }
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    return $root
}

function Get-RequestImageRoots {
    param(
        [Parameter(Mandatory = $true)][object]$Request,
        [Parameter(Mandatory = $true)][string]$WorkRoot
    )

    # The roots an image input is allowed to name. These mirror the roots the host itself accepts
    # for an image *output* (`normalize_framework_image_output` in framework_process.rs): the
    # per-request temp directory, the package cache, the optional state and output directories, and
    # the package directory that ships the Art's own assets. Anything else is a path the request has
    # no business reading.
    $context = Get-JsonPropertyValue -Object $Request -Name "context"
    $roots = [System.Collections.Generic.List[string]]::new()
    foreach ($candidate in @(
            $WorkRoot,
            (Get-JsonPropertyFromNames -Object $Request -Names @("artDir", "art_dir")),
            (Get-JsonPropertyFromNames -Object $context -Names @("tempDir", "temp_dir")),
            (Get-JsonPropertyFromNames -Object $context -Names @("cacheDir", "cache_dir")),
            (Get-JsonPropertyFromNames -Object $context -Names @("stateDir", "state_dir")),
            (Get-JsonPropertyFromNames -Object $context -Names @("outputDir", "output_dir"))
        )) {
        if ($candidate -is [string] -and -not [string]::IsNullOrWhiteSpace($candidate)) {
            $roots.Add([string]$candidate)
        }
    }
    return $roots.ToArray()
}

function Test-RemoteOrDevicePath {
    param([Parameter(Mandatory = $true)][string]$Path)

    # `\\host\share\x.png`, `//host/share/x.png`, `\\?\UNC\host\share` and `\\.\PhysicalDrive0` all
    # reach the network or a raw device without ever looking like a URL, and merely asking
    # `Test-Path` about one opens an SMB session that offers the caller's NTLM credentials to
    # whoever answers. They are rejected on the spelling, before any filesystem call, which is the
    # same rule `loom_security::network::validate_local_path` applies on the host side.
    return $Path.Replace('/', '\').StartsWith('\\', [System.StringComparison]::Ordinal)
}

function Get-NormalizedPathRoots {
    param(
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [AllowNull()][string[]]$AllowedRoots
    )

    $candidates = [System.Collections.Generic.List[string]]::new()
    $candidates.Add($WorkRoot)
    if ($null -ne $AllowedRoots) {
        foreach ($root in $AllowedRoots) {
            if ($root -is [string] -and -not [string]::IsNullOrWhiteSpace($root)) {
                $candidates.Add([string]$root)
            }
        }
    }
    $normalized = [System.Collections.Generic.List[string]]::new()
    foreach ($candidate in $candidates) {
        if (Test-RemoteOrDevicePath -Path $candidate) {
            continue
        }
        try {
            $full = [System.IO.Path]::GetFullPath($candidate)
        }
        catch {
            continue
        }
        $full = $full.TrimEnd(
            [System.IO.Path]::DirectorySeparatorChar,
            [System.IO.Path]::AltDirectorySeparatorChar
        )
        if (-not [string]::IsNullOrWhiteSpace($full) -and -not $normalized.Contains($full)) {
            $normalized.Add($full)
        }
    }
    return $normalized.ToArray()
}

function Test-PathUnderRoots {
    param(
        [Parameter(Mandatory = $true)][string]$FullPath,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Roots
    )

    foreach ($root in $Roots) {
        if ($FullPath.Equals($root, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
        $prefix = $root + [System.IO.Path]::DirectorySeparatorChar
        if ($FullPath.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            return $true
        }
    }
    return $false
}

function Convert-FileUrlToLocalPath {
    param([Parameter(Mandatory = $true)][string]$Value)

    # Only the host-less spelling `file:///C:/dir/x.png` names a local file. `file://localhost/...`
    # and `file://attacker.example/share/...` both come back from .NET as UNC paths
    # (`\\localhost\C:\...`, `\\attacker.example\share\...`), so they are refused here rather than
    # handed to `Test-Path`, which would be the call that reaches out over SMB.
    $uri = $null
    if (-not [System.Uri]::TryCreate($Value, [System.UriKind]::Absolute, [ref]$uri)) {
        return $null
    }
    if (-not $uri.IsFile -or $uri.IsUnc -or -not [string]::IsNullOrEmpty($uri.Host)) {
        return $null
    }
    return $uri.LocalPath
}

function Resolve-ConfinedImagePath {
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Roots
    )

    $text = $Value
    if ($text.StartsWith("file://", [System.StringComparison]::OrdinalIgnoreCase)) {
        $text = Convert-FileUrlToLocalPath -Value $text
        if ([string]::IsNullOrWhiteSpace($text)) {
            return $null
        }
    }
    if (Test-RemoteOrDevicePath -Path $text) {
        return $null
    }
    $fullPaths = [System.Collections.Generic.List[string]]::new()
    if ([System.IO.Path]::IsPathRooted($text)) {
        try {
            $fullPaths.Add([System.IO.Path]::GetFullPath($text))
        }
        catch {
            return $null
        }
    }
    else {
        # A relative input is resolved against the granted roots instead of the process working
        # directory, so it cannot escape by walking up from wherever the runtime happens to run.
        foreach ($root in $Roots) {
            try {
                $fullPaths.Add([System.IO.Path]::GetFullPath((Join-Path $root $text)))
            }
            catch {
            }
        }
    }
    foreach ($fullPath in $fullPaths) {
        if (-not (Test-PathUnderRoots -FullPath $fullPath -Roots $Roots)) {
            continue
        }
        if (Test-Path -LiteralPath $fullPath -PathType Leaf) {
            return $fullPath
        }
    }
    return $null
}

function Get-RequestInputValue {
    param(
        [Parameter(Mandatory = $true)][object]$Request,
        [Parameter(Mandatory = $true)][string[]]$Names
    )

    # Only the names the caller declared are read. There used to be a fallback that returned the first
    # non-null entry of `inputs` when none of the names matched; it never ran, because
    # `ConvertFrom-Json` yields a `PSCustomObject` rather than a dictionary, and on the one shape where
    # it would have run it would have picked an entry by hash order and handed an Art whichever input
    # happened to come first. A request that names none of `$Names` is missing its input, and the
    # caller says so.
    $inputs = Get-JsonPropertyValue -Object $Request -Name "inputs"
    return Get-JsonPropertyFromNames -Object $inputs -Names $Names
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

function Get-ImageDataUrlMaxEncodedLength {
    # 32 MiB of decoded image, expressed in base64 characters (4 characters per 3 bytes). That is the
    # same ceiling the host already applies on the two paths that hand image bytes to a framework Art:
    # `MAX_FRAMEWORK_CANDIDATE_BYTES` in `crates/loom_tool_registry/src/framework_process.rs` and
    # `MAX_MCP_IMAGE_BYTES` in `crates/loom_tool_registry/src/lib.rs`, and the same number
    # `image-search` enforces on a download.
    return [int][math]::Ceiling(32MB / 3.0) * 4
}

function Resolve-ImageDataUrlExtension {
    param([Parameter(Mandatory = $true)][string]$Subtype)

    # The declared subtype decides the extension, rather than every payload being written under `.png`
    # whatever it actually contains. A subtype outside this list is refused here, where the message can
    # name it, instead of failing two frames later inside `Bitmap`'s constructor with a generic "the
    # parameter is not valid". `svg+xml` is the case that motivated the check: it matches the data-URL
    # pattern this runtime accepts, is not a raster format, and GDI+ cannot decode it.
    switch ($Subtype.ToLowerInvariant()) {
        "png" { return ".png" }
        "jpeg" { return ".jpg" }
        "jpg" { return ".jpg" }
        "pjpeg" { return ".jpg" }
        "bmp" { return ".bmp" }
        "x-ms-bmp" { return ".bmp" }
        "gif" { return ".gif" }
        "tiff" { return ".tif" }
        "x-icon" { return ".ico" }
        "vnd.microsoft.icon" { return ".ico" }
        default {
            throw "data URL image subtype '$Subtype' is not a format this runtime can decode"
        }
    }
}

function Resolve-ImagePath {
    param(
        [AllowNull()][object]$Value,
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [AllowNull()][string[]]$AllowedRoots
    )

    if ($null -eq $Value) {
        return $null
    }
    if ($Value -is [System.Array]) {
        foreach ($item in $Value) {
            $resolved = Resolve-ImagePath `
                -Value $item `
                -Label $Label `
                -WorkRoot $WorkRoot `
                -AllowedRoots $AllowedRoots
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
        if ($text.StartsWith("data:", [System.StringComparison]::OrdinalIgnoreCase) -and
            $text.Length -gt (Get-ImageDataUrlMaxEncodedLength)) {
            # Decided on `$text.Length`, before the pattern below runs: the capture group would copy the
            # encoded payload, so the value is rejected while there is still only one copy of it.
            throw "data URL image exceeds the 32 MiB this runtime accepts"
        }
        if ($text -match '^data:image\/(?<subtype>[A-Za-z0-9.+-]+);base64,(?<data>.+)$') {
            $subtype = $Matches["subtype"]
            $encoded = $Matches["data"]
            $extension = Resolve-ImageDataUrlExtension -Subtype $subtype
            $path = Join-Path $WorkRoot (
                New-WorkRootFileName -Stem "$Label-input" -Extension $extension
            )
            [System.IO.File]::WriteAllBytes($path, [Convert]::FromBase64String($encoded))
            return $path
        }
        # Anything else is a path naming a file this process did not produce, and the value comes
        # from `request.inputs`. Without confinement the runtime would read any image on the machine
        # and hand it back as base64, and a `file://` host would turn the read into an outbound SMB
        # connection. Both are refused before the first filesystem call.
        $roots = Get-NormalizedPathRoots -WorkRoot $WorkRoot -AllowedRoots $AllowedRoots
        return Resolve-ConfinedImagePath -Value $text -Roots $roots
    }

    $nested = Get-JsonPropertyFromNames -Object $Value -Names @(
        "path", "filePath", "imagePath", "url", "source", "value", "data", "base64", "imageBase64"
    )
    if ($null -ne $nested) {
        return Resolve-ImagePath `
            -Value $nested `
            -Label $Label `
            -WorkRoot $WorkRoot `
            -AllowedRoots $AllowedRoots
    }
    $content = Get-JsonPropertyValue -Object $Value -Name "content"
    if ($null -ne $content) {
        return Resolve-ImagePath `
            -Value $content `
            -Label $Label `
            -WorkRoot $WorkRoot `
            -AllowedRoots $AllowedRoots
    }
    return $null
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

function New-ImageOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [AllowNull()][hashtable]$Extra
    )

    $dimensions = Get-ImageDimensions -Path $Path
    $data = Convert-ImagePathToDataUrl -Path $Path
    # The image travels once, inside `content`. An `output_base64` beside it would be a second
    # full copy of the same bytes in the same response, and no consumer needs it: the host's
    # `normalize_framework_image_output` strips the key anyway once `output_path` is present, and
    # every reader in the workspace (the workflow runtime's `extract_image_output`, its
    # `extract_default_output`, its `extract_named_output` arm for a port literally named
    # `output_base64`, and the daemon's `extract_art_image_data_url`) falls back to
    # `content[0].data`.
    $output = [ordered]@{
        output_path = $Path
        width = $dimensions.Width
        height = $dimensions.Height
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

function New-ImagePathOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [AllowNull()][hashtable]$Extra
    )

    $dimensions = Get-ImageDimensions -Path $Path
    $output = [ordered]@{
        output_path = $Path
        width = $dimensions.Width
        height = $dimensions.Height
    }
    if ($null -ne $Extra) {
        foreach ($key in $Extra.Keys) {
            $output[$key] = $Extra[$key]
        }
    }
    return $output
}

function New-PlaceholderImage {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$Red,
        [Parameter(Mandatory = $true)][int]$Green,
        [Parameter(Mandatory = $true)][int]$Blue,
        [string]$Label = "Loom Art"
    )

    Initialize-ImageRuntime
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
