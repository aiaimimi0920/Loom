function Build-ExternalFrameworkRuntime {
    param(
        [string]$Version,
        [string]$SourcePath,
        [string]$Destination
    )

    $runtimeSource = @'
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn extract_json_string(input: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = input.find(&needle)? + needle.len();
    let mut value = String::new();
    let mut escaped = false;
    for ch in input[start..].chars() {
        if escaped {
            match ch {
                '\\' => value.push('\\'),
                '"' => value.push('"'),
                '/' => value.push('/'),
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                other => {
                    value.push('\\');
                    value.push(other);
                }
            }
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(value);
        } else {
            value.push(ch);
        }
    }
    None
}

fn main() {
    let mut request = String::new();
    std::io::stdin()
        .read_to_string(&mut request)
        .expect("read Loom framework request");
    let art_dir = extract_json_string(&request, "artDir").expect("request artDir");
    let script = PathBuf::from(art_dir).join("runtime").join("main.ps1");
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .env("THIRD_PARTY_FRAMEWORK_VERSION", "__FRAMEWORK_VERSION__")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("start Art package runtime");
    child
        .stdin
        .take()
        .expect("Art runtime stdin")
        .write_all(request.as_bytes())
        .expect("write Art runtime request");
    let output = child.wait_with_output().expect("wait for Art runtime");
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }
    std::io::stdout()
        .write_all(&output.stdout)
        .expect("relay Art runtime response");
}
'@
    $runtimeSource = $runtimeSource.Replace("__FRAMEWORK_VERSION__", $Version)
    Write-Utf8NoBomFile -Path $SourcePath -Content $runtimeSource
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
    $rustc = Get-Command rustc.exe -ErrorAction SilentlyContinue
    if ($null -eq $rustc) {
        $rustc = Get-Command rustc -ErrorAction Stop
    }
    & $rustc.Path --edition=2021 -C opt-level=1 -C debuginfo=0 -o $Destination $SourcePath
    if ($LASTEXITCODE -ne 0) {
        throw "Independent third-party framework compilation failed for version $Version."
    }
    Assert-True (Test-Path -LiteralPath $Destination -PathType Leaf) "Independent framework runtime was not created: $Destination"
}

function New-ThirdPartyFrameworkPackage {
    param(
        [string]$Version,
        [string]$StageRoot,
        [string]$SourcePath,
        [string]$ZipPath,
        [string]$FrameworkId,
        [string]$PublisherId
    )

    New-Item -ItemType Directory -Force -Path $StageRoot | Out-Null
    $frameworkRuntime = Join-Path $StageRoot "runtime\loom-framework-third-party.exe"
    Build-ExternalFrameworkRuntime -Version $Version -SourcePath $SourcePath -Destination $frameworkRuntime
    $frameworkManifest = [ordered]@{
        id = $FrameworkId
        name = "Third-party Echo Framework"
        description = "Framework package compiled outside the Loom source tree."
        version = $Version
        publisher = [ordered]@{ id = $PublisherId; name = "Third Party" }
        protocolVersion = "loom.framework.v1"
        platforms = @("windows-x64")
        entry = [ordered]@{
            kind = "process"
            command = "runtime/loom-framework-third-party.exe"
            args = @()
            processModel = "per_execution"
        }
        permissions = @("process.spawn", "file.read")
        artExecution = [ordered]@{
            requestSchema = "loom.art.execute.v1"
            responseSchema = "loom.art.result.v1"
        }
    }
    Write-Utf8NoBomFile -Path (Join-Path $StageRoot "framework.manifest.json") -Content (($frameworkManifest | ConvertTo-Json -Depth 30) + [Environment]::NewLine)
    Compress-Archive -Path (Join-Path $StageRoot "*") -DestinationPath $ZipPath -CompressionLevel Optimal -Force
}

