#Requires -Version 5.1
<#
.SYNOPSIS
    GeoEngine CLI Installer for Windows

.DESCRIPTION
    Installs the GeoEngine CLI tool on Windows.
    Supports both online download and offline installation.

.PARAMETER InstallDir
    Installation directory (default: C:\Program Files\GeoEngine)

.PARAMETER LocalBinary
    Path to local binary for offline installation

.EXAMPLE
    # Online installation
    irm https://raw.githubusercontent.com/NikaGeospatial/geoengine/main/install/install.ps1 | iex

.EXAMPLE
    # Offline installation
    .\install.ps1 -LocalBinary .\geoengine.exe
#>

[CmdletBinding()]
param(
    [string]$InstallDir = "$env:ProgramFiles\GeoEngine",
    [string]$LocalBinary
)

$ErrorActionPreference = "Stop"

# Configuration
$RepoUrl = "https://github.com/NikaGeospatial/geoengine"
$BinaryName = "geoengine.exe"
$ConfigDir = "$env:USERPROFILE\.geoengine"

function Write-Info {
    param([string]$Message)
    Write-Host "==> " -ForegroundColor Blue -NoNewline
    Write-Host $Message
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] " -ForegroundColor Green -NoNewline
    Write-Host $Message
}

function Write-Warn {
    param([string]$Message)
    Write-Host "[!] " -ForegroundColor Yellow -NoNewline
    Write-Host $Message
}

function Write-Err {
    param([string]$Message)
    Write-Host "[X] " -ForegroundColor Red -NoNewline
    Write-Host $Message
    exit 1
}

function Test-Administrator {
    $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($currentUser)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Test-Dependencies {
    Write-Info "Checking dependencies..."

    # Check for Docker
    $docker = Get-Command docker -ErrorAction SilentlyContinue
    if ($docker) {
        $dockerVersion = docker --version
        Write-Success "Docker found: $dockerVersion"

        # Check if Docker is running
        try {
            docker info | Out-Null
            Write-Success "Docker daemon is running"
        }
        catch {
            Write-Warn "Docker daemon is not running. Start Docker Desktop."
        }
    }
    else {
        Write-Warn "Docker not found. GeoEngine requires Docker."
        Write-Host "  Install Docker Desktop: https://docs.docker.com/desktop/install/windows-install/"
    }

    # Check for WSL2 (recommended for GPU support)
    $wsl = Get-Command wsl -ErrorAction SilentlyContinue
    if ($wsl) {
        Write-Success "WSL2 available"
    }
    else {
        Write-Warn "WSL2 not found. WSL2 is recommended for GPU passthrough."
    }

    # Check for NVIDIA GPU
    $nvidia = Get-Command nvidia-smi -ErrorAction SilentlyContinue
    if ($nvidia) {
        Write-Success "NVIDIA GPU detected"
    }
}

function Get-Architecture {
    $arch = [System.Environment]::GetEnvironmentVariable("PROCESSOR_ARCHITECTURE")
    switch ($arch) {
        "AMD64" { return "x86_64" }
        "ARM64" { return "aarch64" }
        default { Write-Err "Unsupported architecture: $arch" }
    }
}

function Install-FromDownload {
    Write-Info "Downloading GeoEngine..."

    $arch = Get-Architecture
    $platform = "windows-$arch"
    $downloadUrl = "$RepoUrl/releases/latest/download/geoengine-$platform.zip"

    $tempDir = Join-Path $env:TEMP "geoengine-install"
    $zipPath = Join-Path $tempDir "geoengine.zip"

    # Create temp directory
    New-Item -ItemType Directory -Force -Path $tempDir | Out-Null

    try {
        # Download
        Write-Info "Downloading from $downloadUrl..."
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $downloadUrl -OutFile $zipPath -UseBasicParsing

        # Extract
        Write-Info "Extracting..."
        Expand-Archive -Path $zipPath -DestinationPath $tempDir -Force

        # Install
        Install-Binary (Join-Path $tempDir $BinaryName)
    }
    finally {
        # Cleanup
        Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
    }
}

function Install-Binary {
    param([string]$BinaryPath)

    if (-not (Test-Path $BinaryPath)) {
        Write-Err "Binary not found: $BinaryPath"
    }

    Write-Info "Installing to $InstallDir..."

    # Create install directory
    if (-not (Test-Path $InstallDir)) {
        if (Test-Administrator) {
            New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        }
        else {
            Write-Err "Administrator privileges required to create $InstallDir. Run as Administrator."
        }
    }

    # Copy binary
    $destPath = Join-Path $InstallDir $BinaryName
    Copy-Item -Path $BinaryPath -Destination $destPath -Force

    Write-Success "Installed to $destPath"

    # Add to PATH
    Add-ToPath $InstallDir
}

function Add-ToPath {
    param([string]$Directory)

    $currentPath = [Environment]::GetEnvironmentVariable("Path", "Machine")

    if ($currentPath -notlike "*$Directory*") {
        Write-Info "Adding to system PATH..."

        if (Test-Administrator) {
            $newPath = "$currentPath;$Directory"
            [Environment]::SetEnvironmentVariable("Path", $newPath, "Machine")
            Write-Success "Added to PATH. Restart your terminal to use 'geoengine' command."
        }
        else {
            Write-Warn "Run as Administrator to add to system PATH, or add manually:"
            Write-Host "  $Directory"
        }
    }
    else {
        Write-Success "Already in PATH"
    }
}

function Initialize-Config {
    Write-Info "Setting up configuration directory..."

    $dirs = @(
        $ConfigDir,
        "$ConfigDir\logs",
        "$ConfigDir\jobs"
    )

    foreach ($dir in $dirs) {
        if (-not (Test-Path $dir)) {
            New-Item -ItemType Directory -Force -Path $dir | Out-Null
        }
    }

    Write-Success "Config directory: $ConfigDir"
}

function Show-Success {
    Write-Host ""
    Write-Host "+==========================================+" -ForegroundColor Green
    Write-Host "|   GeoEngine installed successfully!      |" -ForegroundColor Green
    Write-Host "+==========================================+" -ForegroundColor Green
    Write-Host ""
    Write-Host "Get started:"
    Write-Host "  geoengine --help              " -ForegroundColor Cyan -NoNewline
    Write-Host "Show all commands"
    Write-Host "  geoengine project init        " -ForegroundColor Cyan -NoNewline
    Write-Host "Create a new project"
    Write-Host "  geoengine service start       " -ForegroundColor Cyan -NoNewline
    Write-Host "Start the proxy service"
    Write-Host ""
    Write-Host "For GIS integration:"
    Write-Host "  geoengine service register arcgis  " -ForegroundColor Cyan -NoNewline
    Write-Host "Register with ArcGIS Pro"
    Write-Host "  geoengine service register qgis    " -ForegroundColor Cyan -NoNewline
    Write-Host "Register with QGIS"
    Write-Host ""
    Write-Host "Documentation: $RepoUrl"
    Write-Host ""
}

# Main
function Main {
    Write-Host ""
    Write-Host "GeoEngine CLI Installer" -ForegroundColor Blue
    Write-Host "========================"
    Write-Host ""

    # Check dependencies
    Test-Dependencies

    # Install
    if ($LocalBinary) {
        Write-Info "Installing from local binary..."
        Install-Binary $LocalBinary
    }
    else {
        Install-FromDownload
    }

    # Setup config
    Initialize-Config

    # Success message
    Show-Success
}

Main
