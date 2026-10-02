# rcompose installer for Windows.
#
#   irm https://ricardoborges.github.io/rcompose/install.ps1 | iex
#
# Downloads the latest release binary from GitHub, installs it into a per-user
# folder and adds that folder to the user PATH. No administrator rights needed.
#
# Optional environment variables:
#   RCOMPOSE_VERSION      Release tag to install (e.g. v0.1.0). Default: latest.
#   RCOMPOSE_INSTALL_DIR  Install folder. Default: %LOCALAPPDATA%\Programs\rcompose.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

& {
    $repo = 'ricardoborges/rcompose'

    # A 32-bit PowerShell on a 64-bit OS reports x86 in PROCESSOR_ARCHITECTURE;
    # PROCESSOR_ARCHITEW6432 then holds the real OS architecture.
    $osArch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $arch = switch ($osArch) {
        'AMD64' { 'x86_64' }
        'ARM64' { 'aarch64' }
        default { throw "Unsupported architecture: $osArch" }
    }

    $asset = "rcompose-$arch-pc-windows-msvc.zip"

    $version = $env:RCOMPOSE_VERSION
    if ($version) {
        $url = "https://github.com/$repo/releases/download/$version/$asset"
    } else {
        $version = 'latest'
        $url = "https://github.com/$repo/releases/latest/download/$asset"
    }

    $installDir = $env:RCOMPOSE_INSTALL_DIR
    if (-not $installDir) {
        $installDir = Join-Path $env:LOCALAPPDATA 'Programs\rcompose'
    }

    # Older Windows PowerShell builds default to TLS 1.0, which GitHub rejects.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("rcompose-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "Downloading rcompose ($version, $arch)..."
        $zip = Join-Path $tmp $asset
        Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing

        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        $exe = Get-ChildItem -Path $tmp -Filter 'rcompose.exe' -Recurse | Select-Object -First 1
        if (-not $exe) { throw "rcompose.exe not found in $asset" }

        New-Item -ItemType Directory -Path $installDir -Force | Out-Null
        Copy-Item -Path $exe.FullName -Destination (Join-Path $installDir 'rcompose.exe') -Force
    } finally {
        Remove-Item -Path $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-Host "Installed rcompose to $installDir"

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { $_ })
    if ($entries -notcontains $installDir) {
        [Environment]::SetEnvironmentVariable('Path', (($entries + $installDir) -join ';'), 'User')
        Write-Host "Added $installDir to your user PATH."
    }
    if (($env:Path -split ';') -notcontains $installDir) {
        $env:Path = "$env:Path;$installDir"
    }

    if (-not (Get-Command wslc.exe -ErrorAction SilentlyContinue) -and
        -not (Test-Path 'C:\Program Files\WSL\wslc.exe')) {
        Write-Warning 'wslc.exe was not found. rcompose needs the WSL Container preview installed.'
    }

    Write-Host ''
    Write-Host 'Run "rcompose --help" to get started. Open a new terminal if the command is not found.'
}
