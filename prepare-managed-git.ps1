param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86', 'x64', 'arm64')]
    [string]$Architecture,

    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = `
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$version = '2.55.0.2'
$releaseTag = 'v2.55.0.windows.2'
$releaseBaseUrl = "https://github.com/git-for-windows/git/releases/download/$releaseTag"
$target = switch ($Architecture) {
    'x86' {
        [pscustomobject]@{
            Asset = "MinGit-$version-32-bit.zip"
            Sha256 = '04009f6150c1cec2d6779c51406c8c6a3f0133e57fa91c91eb8a030b93e68ccb'
        }
    }
    'x64' {
        [pscustomobject]@{
            Asset = "MinGit-$version-64-bit.zip"
            Sha256 = 'e3ea2944cea4b3fabcd69c7c1669ef69b1b66c05ac7806d81224d0abad2dec31'
        }
    }
    'arm64' {
        [pscustomobject]@{
            Asset = "MinGit-$version-arm64.zip"
            Sha256 = '0b2b81fdce284efd174cbb51b886ccea2fd271679c4b5c21f07d9e03bae51413'
        }
    }
}

function Test-ExpectedHash {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Expected
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }

    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $actual = [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '')
        return $actual -eq $Expected
    } finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

function Download-Archive {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Uri,

        [Parameter(Mandatory = $true)]
        [string]$Destination
    )

    if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
        & curl.exe --location --fail --retry 3 --connect-timeout 30 `
            --silent --show-error --output $Destination $Uri
        if ($LASTEXITCODE -eq 0) {
            return
        }
        Remove-Item -LiteralPath $Destination -Force -ErrorAction SilentlyContinue
    }
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
}

$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$destination = Join-Path $outputRoot $target.Asset
$temporary = "$destination.download-$([Guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null

if (Test-ExpectedHash -Path $destination -Expected $target.Sha256) {
    Write-Output $destination
    exit 0
}

if (Test-Path -LiteralPath $destination) {
    Remove-Item -LiteralPath $destination -Force
}

try {
    Download-Archive -Uri "$releaseBaseUrl/$($target.Asset)" -Destination $temporary
    if (-not (Test-ExpectedHash -Path $temporary -Expected $target.Sha256)) {
        throw "Downloaded MinGit archive failed SHA-256 verification"
    }
    Move-Item -LiteralPath $temporary -Destination $destination -Force
    Write-Output $destination
} finally {
    if (Test-Path -LiteralPath $temporary) {
        Remove-Item -LiteralPath $temporary -Force
    }
}
