function Write-LoomNativePackageArchive {
    param(
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Add-Type -AssemblyName System.IO.Compression
    $root = [System.IO.Path]::GetFullPath($Stage).TrimEnd('\', '/')
    $target = [System.IO.Path]::GetFullPath($Destination)
    if ($target.StartsWith($root + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "PACKAGE_OUTPUT_INSIDE_STAGE"
    }
    $pending = [System.Collections.Generic.Stack[string]]::new()
    $pending.Push($root)
    $files = [System.Collections.Generic.List[object]]::new()
    $names = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    while ($pending.Count -gt 0) {
        $directory = Get-Item -LiteralPath $pending.Pop() -Force
        if (-not $directory.PSIsContainer -or ($directory.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            throw "PACKAGE_STAGE_LINK_OR_NOT_DIRECTORY"
        }
        foreach ($entry in Get-ChildItem -LiteralPath $directory.FullName -Force) {
            if ($entry.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
                throw "PACKAGE_STAGE_LINK"
            }
            if ($entry.PSIsContainer) {
                $pending.Push($entry.FullName)
                continue
            }
            $relative = $entry.FullName.Substring($root.Length + 1).Replace([char]0x5c, [char]0x2f)
            if (-not $names.Add($relative)) { throw "PACKAGE_PATH_COLLISION" }
            $files.Add([pscustomobject]@{ File = $entry; Name = $relative })
        }
    }
    $output = [System.IO.File]::Open($target, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    $archive = $null
    try {
        $archive = [System.IO.Compression.ZipArchive]::new($output, [System.IO.Compression.ZipArchiveMode]::Create, $false)
        foreach ($record in $files | Sort-Object Name) {
            $entry = $archive.CreateEntry($record.Name, [System.IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $input = $null
            $entryStream = $null
            try {
                $input = $record.File.OpenRead()
                $entryStream = $entry.Open()
                $input.CopyTo($entryStream)
            } finally {
                if ($null -ne $entryStream) { $entryStream.Dispose() }
                if ($null -ne $input) { $input.Dispose() }
            }
        }
    } finally {
        if ($null -ne $archive) { $archive.Dispose() }
        $output.Dispose()
    }
}
