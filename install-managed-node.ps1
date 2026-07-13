param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string]$RuntimeRoot
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Get-NodeArchitecture {
    $architecture = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($architecture.ToUpperInvariant()) {
        'AMD64' { return 'x64' }
        'ARM64' { return 'arm64' }
        default { throw "Unsupported Windows architecture: $architecture" }
    }
}

function Assert-ChildPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Parent,

        [Parameter(Mandatory = $true)]
        [string]$Child
    )

    $parentPath = [IO.Path]::GetFullPath($Parent).TrimEnd('\') + '\'
    $childPath = [IO.Path]::GetFullPath($Child)
    if (-not $childPath.StartsWith($parentPath, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to modify a path outside the runtime root: $childPath"
    }
}

function Test-NodeRuntime {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Directory,

        [Parameter(Mandatory = $true)]
        [string]$ExpectedVersion
    )

    $node = Join-Path $Directory 'node.exe'
    $npm = Join-Path $Directory 'npm.cmd'
    if (-not (Test-Path -LiteralPath $node -PathType Leaf) -or
        -not (Test-Path -LiteralPath $npm -PathType Leaf)) {
        return $false
    }

    $actualVersion = (& $node --version 2>$null).Trim()
    return $LASTEXITCODE -eq 0 -and $actualVersion -eq "v$ExpectedVersion"
}

$runtimeRootPath = [IO.Path]::GetFullPath($RuntimeRoot)
$architecture = Get-NodeArchitecture
$platform = "win-$architecture"
$archiveName = "node-v$Version-$platform.zip"
$releaseBaseUrl = "https://nodejs.org/dist/v$Version"
$targetDirectory = Join-Path $runtimeRootPath "node\$Version\$platform"
$nodeRoot = Join-Path $runtimeRootPath 'node'
$downloadsDirectory = Join-Path $runtimeRootPath 'downloads'
$stagingDirectory = Join-Path $runtimeRootPath "staging\node-$([Guid]::NewGuid().ToString('N'))"
$archivePath = Join-Path $downloadsDirectory $archiveName
$checksumPath = Join-Path $downloadsDirectory "SHASUMS256-$Version.txt"
$currentPath = Join-Path $nodeRoot 'current.json'

Assert-ChildPath -Parent $runtimeRootPath -Child $targetDirectory
Assert-ChildPath -Parent $runtimeRootPath -Child $downloadsDirectory
Assert-ChildPath -Parent $runtimeRootPath -Child $stagingDirectory

if (Test-NodeRuntime -Directory $targetDirectory -ExpectedVersion $Version) {
    Write-Host "Node.js v$Version is already installed at $targetDirectory"
    exit 0
}

New-Item -ItemType Directory -Force -Path $downloadsDirectory | Out-Null
New-Item -ItemType Directory -Force -Path $stagingDirectory | Out-Null

try {
    Write-Host "Downloading $archiveName..."
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseBaseUrl/$archiveName" -OutFile $archivePath
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseBaseUrl/SHASUMS256.txt" -OutFile $checksumPath

    $checksumLine = Get-Content -LiteralPath $checksumPath |
        Where-Object { $_ -match "^[a-fA-F0-9]{64}\s+$([regex]::Escape($archiveName))$" } |
        Select-Object -First 1
    if (-not $checksumLine) {
        throw "The Node.js checksum manifest does not contain $archiveName"
    }

    $expectedHash = ($checksumLine -split '\s+')[0].ToUpperInvariant()
    $actualHash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToUpperInvariant()
    if ($actualHash -ne $expectedHash) {
        throw 'Node.js archive checksum mismatch'
    }

    $extractDirectory = Join-Path $stagingDirectory 'extract'
    New-Item -ItemType Directory -Force -Path $extractDirectory | Out-Null
    & tar.exe -xf $archivePath -C $extractDirectory
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to extract the Node.js archive: tar.exe exited with $LASTEXITCODE"
    }
    $expandedRoot = Join-Path $extractDirectory "node-v$Version-$platform"
    if (-not (Test-NodeRuntime -Directory $expandedRoot -ExpectedVersion $Version)) {
        throw 'The downloaded Node.js runtime failed validation'
    }

    New-Item -ItemType Directory -Force -Path (Split-Path $targetDirectory -Parent) | Out-Null
    if (Test-Path -LiteralPath $targetDirectory) {
        Remove-Item -LiteralPath $targetDirectory -Recurse -Force
    }
    Move-Item -LiteralPath $expandedRoot -Destination $targetDirectory

    $currentState = [ordered]@{
        version = $Version
        platform = $platform
        path = $targetDirectory
        installedAt = [DateTimeOffset]::UtcNow.ToString('O')
    }
    $currentTemporaryPath = "$currentPath.tmp"
    $currentState | ConvertTo-Json | Set-Content -LiteralPath $currentTemporaryPath -Encoding UTF8
    Move-Item -LiteralPath $currentTemporaryPath -Destination $currentPath -Force

    Write-Host "Installed Node.js v$Version with npm at $targetDirectory"
} finally {
    if (Test-Path -LiteralPath $stagingDirectory) {
        Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
    }
}
