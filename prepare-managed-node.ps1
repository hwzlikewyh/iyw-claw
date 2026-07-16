param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86', 'x64', 'arm64')]
    [string]$Architecture,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '')
    } finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

function Get-ExpectedHash {
    param(
        [Parameter(Mandatory = $true)][string]$ChecksumPath,
        [Parameter(Mandatory = $true)][string]$ArchiveName
    )

    $line = Get-Content -LiteralPath $ChecksumPath |
        Where-Object { $_ -match "^[a-fA-F0-9]{64}\s+$([regex]::Escape($ArchiveName))$" } |
        Select-Object -First 1
    if (-not $line) {
        throw "Node.js 校验清单中不存在 $ArchiveName"
    }
    return ($line -split '\s+')[0].ToUpperInvariant()
}

function Get-RemoteFile {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
        & curl.exe --location --fail --retry 3 --connect-timeout 30 --silent --show-error --output $Destination $Uri
        if ($LASTEXITCODE -eq 0) {
            return
        }
        Remove-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue
    }
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
}

$platform = "win-$Architecture"
$archiveName = "node-v$Version-$platform.zip"
$releaseBaseUrl = "https://nodejs.org/dist/v$Version"
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$archivePath = Join-Path $outputRoot $archiveName
$checksumPath = Join-Path $outputRoot 'SHASUMS256.txt'
$archiveTemporary = "$archivePath.download-$([Guid]::NewGuid().ToString('N'))"
$checksumTemporary = "$checksumPath.download-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

$cacheValid = $false
if ((Test-Path -LiteralPath $archivePath -PathType Leaf) -and
    (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
    try {
        $expected = Get-ExpectedHash -ChecksumPath $checksumPath -ArchiveName $archiveName
        $cacheValid = (Get-FileSha256 -Path $archivePath) -eq $expected
    } catch {
        $cacheValid = $false
    }
}
if ($cacheValid) {
    Write-Output $archivePath
    exit 0
}

try {
    Get-RemoteFile -Uri "$releaseBaseUrl/SHASUMS256.txt" -Destination $checksumTemporary
    $expected = Get-ExpectedHash -ChecksumPath $checksumTemporary -ArchiveName $archiveName
    Get-RemoteFile -Uri "$releaseBaseUrl/$archiveName" -Destination $archiveTemporary
    if ((Get-FileSha256 -Path $archiveTemporary) -ne $expected) {
        throw 'Node.js 压缩包 SHA-256 校验失败'
    }

    Move-Item -LiteralPath $archiveTemporary -Destination $archivePath -Force
    Move-Item -LiteralPath $checksumTemporary -Destination $checksumPath -Force
    Write-Output $archivePath
} finally {
    Remove-Item -LiteralPath $archiveTemporary -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $checksumTemporary -Force -ErrorAction SilentlyContinue
}
