function Get-GitStateFingerprint {
    param([string]$Repository)
    $head = (& git -C $Repository rev-parse HEAD 2>$null | Out-String).Trim()
    $status = (& git -C $Repository status --porcelain=v1 --untracked-files=all 2>$null | Out-String).Trim()
    $sourcePatterns = @("*.rs", "*.toml", "*.ps1", "*.ts", "*.tsx", "*.json", "*.yaml", "*.yml")
    $trackedSources = @(& git -C $Repository ls-files -- $sourcePatterns 2>$null | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $sourceHashes = foreach ($relativePath in $trackedSources) {
        $sourcePath = Join-Path $Repository $relativePath
        if (Test-Path -LiteralPath $sourcePath -PathType Leaf) {
            "$relativePath|$((Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash.ToLowerInvariant())"
        }
    }
    $sourceBytes = [System.Text.Encoding]::UTF8.GetBytes(($sourceHashes -join "`n"))
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $sourceHash = ([BitConverter]::ToString($sha256.ComputeHash($sourceBytes))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
    return "$head|$status|$sourceHash"
}

