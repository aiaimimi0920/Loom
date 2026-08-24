<# Owns Windows argument quoting, hidden process launch, captured execution, text capture, and path readiness. #>

function ConvertTo-ProcessArgument {
    param([AllowEmptyString()][string]$Argument)

    if (($Argument.Length -gt 0) -and ($Argument -notmatch '[\s"]')) {
        return $Argument
    }

    $builder = [System.Text.StringBuilder]::new()
    [void]$builder.Append('"')
    $backslashCount = 0
    foreach ($character in $Argument.ToCharArray()) {
        if ($character -eq [char]0x5c) {
            $backslashCount += 1
            continue
        }

        if ($character -eq [char]0x22) {
            if ($backslashCount -gt 0) {
                [void]$builder.Append("\" * (($backslashCount * 2) + 1))
            } else {
                [void]$builder.Append("\")
            }
            [void]$builder.Append('"')
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            [void]$builder.Append("\" * $backslashCount)
            $backslashCount = 0
        }
        [void]$builder.Append($character)
    }

    if ($backslashCount -gt 0) {
        [void]$builder.Append("\" * ($backslashCount * 2))
    }
    [void]$builder.Append('"')

    return $builder.ToString()
}

function Start-SmokeProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList = @(),
        [string]$StdoutPath = "",
        [string]$StderrPath = ""
    )

    $argumentLine = (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument -Argument $_ }) -join " "
    $parameters = @{
        FilePath = $FilePath
        ArgumentList = $argumentLine
        PassThru = $true
        WindowStyle = "Hidden"
    }

    if (-not [string]::IsNullOrWhiteSpace($StdoutPath)) {
        $parameters.RedirectStandardOutput = $StdoutPath
    }
    if (-not [string]::IsNullOrWhiteSpace($StderrPath)) {
        $parameters.RedirectStandardError = $StderrPath
    }

    return Start-Process @parameters
}

function Invoke-ProcessCapture {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [int]$TimeoutSeconds = 60
    )

    $argumentLine = (@($ArgumentList) | ForEach-Object { ConvertTo-ProcessArgument -Argument $_ }) -join " "
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.Arguments = $argumentLine
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $started = $false
    try {
        if (-not $process.Start()) {
            throw "Failed to start process '$FilePath'."
        }
        $started = $true
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()

        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            Stop-SpawnedProcess -Process $process
            $timeoutStdout = if ($stdoutTask.IsCompleted) { [string]$stdoutTask.Result } else { "<stdout capture incomplete>" }
            $timeoutStderr = if ($stderrTask.IsCompleted) { [string]$stderrTask.Result } else { "<stderr capture incomplete>" }
            $timeoutOutput = (($timeoutStdout, $timeoutStderr) -join "`n").Trim()
            throw "Timed out waiting for process '$FilePath' after $TimeoutSeconds seconds. output=$(Redact-SmokeFailureText -Text $timeoutOutput)"
        }
        $process.WaitForExit()
        $stdout = [string]$stdoutTask.Result
        $stderr = [string]$stderrTask.Result

        return [ordered]@{
            exitCode = [int]$process.ExitCode
            stdout = [string]$stdout
            stderr = [string]$stderr
            output = (($stdout, $stderr) -join "`n").Trim()
        }
    } finally {
        if ($started -and -not $process.HasExited) {
            Stop-SpawnedProcess -Process $process
        }
        $process.Dispose()
    }
}

function Wait-ForPath {
    param(
        [string]$Path,
        [int]$TimeoutSeconds = 20
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        if (Test-Path -LiteralPath $Path) {
            return
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    throw "Timed out waiting for path: $Path"
}
