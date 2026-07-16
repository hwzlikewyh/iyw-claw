param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86', 'x64', 'arm64')]
    [string]$Architecture,

    [Parameter(Mandatory = $true)]
    [string]$RuntimeRoot,

    [string]$ArchivePath = '',

    [string]$LogPath = ''
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = `
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$Version = '2.55.0.2'
$ReleaseTag = 'v2.55.0.windows.2'
$ReleaseBaseUrl = "https://github.com/git-for-windows/git/releases/download/$ReleaseTag"

function Get-GitTarget {
    switch ($Architecture) {
        'x86' {
            return [pscustomobject]@{
                Platform = 'win-x86'
                Asset = "MinGit-$Version-32-bit.zip"
                Sha256 = '04009f6150c1cec2d6779c51406c8c6a3f0133e57fa91c91eb8a030b93e68ccb'
            }
        }
        'x64' {
            return [pscustomobject]@{
                Platform = 'win-x64'
                Asset = "MinGit-$Version-64-bit.zip"
                Sha256 = 'e3ea2944cea4b3fabcd69c7c1669ef69b1b66c05ac7806d81224d0abad2dec31'
            }
        }
        'arm64' {
            return [pscustomobject]@{
                Platform = 'win-arm64'
                Asset = "MinGit-$Version-arm64.zip"
                Sha256 = '0b2b81fdce284efd174cbb51b886ccea2fd271679c4b5c21f07d9e03bae51413'
            }
        }
        default { throw "不支持的 Windows 处理器架构：$architecture" }
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
        throw "拒绝修改运行目录之外的路径：$childPath"
    }
}

function Write-InstallMessage {
    param([Parameter(Mandatory = $true)][string]$Message)

    Write-Host $Message
    if (-not [string]::IsNullOrWhiteSpace($LogPath)) {
        $logFullPath = [IO.Path]::GetFullPath($LogPath)
        New-Item -ItemType Directory -Force -Path (Split-Path $logFullPath -Parent) | Out-Null
        $line = '{0} [managed-git] {1}' -f [DateTimeOffset]::Now.ToString('O'), $Message
        Add-Content -LiteralPath $logFullPath -Value $line -Encoding UTF8
    }
}

function Test-GitRuntime {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Directory
    )

    $git = Join-Path $Directory 'cmd\git.exe'
    if (-not (Test-Path -LiteralPath $git -PathType Leaf)) {
        return $false
    }
    $actualVersion = (& $git --version 2>$null).Trim()
    return $LASTEXITCODE -eq 0 -and $actualVersion -eq 'git version 2.55.0.windows.2'
}

function Write-Utf8Json {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Value,

        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $json = $Value | ConvertTo-Json
    [IO.File]::WriteAllText($Path, "$json`r`n", [Text.UTF8Encoding]::new($false))
}

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [IO.File]::OpenRead($Path)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace('-', '').ToUpperInvariant()
    } finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
}

$runtimeRootPath = [IO.Path]::GetFullPath($RuntimeRoot)
$target = Get-GitTarget
$gitRoot = Join-Path $runtimeRootPath 'git'
$targetDirectory = Join-Path $gitRoot "$Version\$($target.Platform)"
$downloadsDirectory = Join-Path $runtimeRootPath 'downloads'
$stagingDirectory = Join-Path $runtimeRootPath "staging\git-$([Guid]::NewGuid().ToString('N'))"
$downloadArchivePath = Join-Path $downloadsDirectory $target.Asset
$currentPath = Join-Path $gitRoot 'current.json'

Assert-ChildPath -Parent $runtimeRootPath -Child $targetDirectory
Assert-ChildPath -Parent $runtimeRootPath -Child $downloadsDirectory
Assert-ChildPath -Parent $runtimeRootPath -Child $stagingDirectory

if (Test-GitRuntime -Directory $targetDirectory) {
    Write-InstallMessage "Git $Version 已安装：$targetDirectory"
    exit 0
}

New-Item -ItemType Directory -Force -Path $downloadsDirectory | Out-Null
New-Item -ItemType Directory -Force -Path $stagingDirectory | Out-Null

try {
    $archivePathToUse = if ([string]::IsNullOrWhiteSpace($ArchivePath)) {
        Write-InstallMessage "正在下载 $($target.Asset)..."
        Invoke-WebRequest -UseBasicParsing -Uri "$ReleaseBaseUrl/$($target.Asset)" `
            -OutFile $downloadArchivePath
        $downloadArchivePath
    } else {
        $bundled = [IO.Path]::GetFullPath($ArchivePath)
        if (-not (Test-Path -LiteralPath $bundled -PathType Leaf)) {
            throw "内置 Git 压缩包不存在：$bundled"
        }
        Write-InstallMessage '正在验证内置 Git 运行环境...'
        $bundled
    }

    $actualHash = Get-FileSha256 -Path $archivePathToUse
    if ($actualHash -ne $target.Sha256) {
        throw 'Git 压缩包 SHA-256 校验失败'
    }

    $extractDirectory = Join-Path $stagingDirectory 'extract'
    Expand-Archive -LiteralPath $archivePathToUse -DestinationPath $extractDirectory -Force
    if (-not (Test-GitRuntime -Directory $extractDirectory)) {
        throw '下载的 Git 运行环境未通过版本验证'
    }

    New-Item -ItemType Directory -Force -Path (Split-Path $targetDirectory -Parent) | Out-Null
    if (Test-Path -LiteralPath $targetDirectory) {
        Remove-Item -LiteralPath $targetDirectory -Recurse -Force
    }
    Move-Item -LiteralPath $extractDirectory -Destination $targetDirectory

    $currentState = [ordered]@{
        version = $Version
        platform = $target.Platform
        path = $targetDirectory
        installedAt = [DateTimeOffset]::UtcNow.ToString('O')
    }
    $currentTemporaryPath = "$currentPath.tmp"
    Write-Utf8Json -Value $currentState -Path $currentTemporaryPath
    Move-Item -LiteralPath $currentTemporaryPath -Destination $currentPath -Force
    Write-InstallMessage "Git $Version 安装完成：$targetDirectory"
} catch {
    Write-InstallMessage "Git 安装失败：$($_.Exception.Message)"
    throw
} finally {
    if (Test-Path -LiteralPath $stagingDirectory) {
        Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
    }
}
