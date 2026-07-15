param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('x86', 'x64', 'arm64')]
    [string]$Architecture,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string]$RuntimeRoot,

    [Parameter(Mandatory = $true)]
    [string]$ArchivePath,

    [Parameter(Mandatory = $true)]
    [string]$ChecksumPath,

    [string]$LogPath = ''
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = `
    [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

function Get-NodeArchitecture {
    $architecture = if ($env:PROCESSOR_ARCHITEW6432) {
        $env:PROCESSOR_ARCHITEW6432
    } else {
        $env:PROCESSOR_ARCHITECTURE
    }

    switch ($architecture.ToUpperInvariant()) {
        'AMD64' { return 'x64' }
        'ARM64' { return 'arm64' }
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
        $line = '{0} [managed-node] {1}' -f [DateTimeOffset]::Now.ToString('O'), $Message
        Add-Content -LiteralPath $logFullPath -Value $line -Encoding UTF8
    }
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
$platform = "win-$Architecture"
$archiveName = "node-v$Version-$platform.zip"
$targetDirectory = Join-Path $runtimeRootPath "node\$Version\$platform"
$nodeRoot = Join-Path $runtimeRootPath 'node'
$stagingDirectory = Join-Path $runtimeRootPath "staging\node-$([Guid]::NewGuid().ToString('N'))"
$bundledArchivePath = [IO.Path]::GetFullPath($ArchivePath)
$bundledChecksumPath = [IO.Path]::GetFullPath($ChecksumPath)
$currentPath = Join-Path $nodeRoot 'current.json'

Assert-ChildPath -Parent $runtimeRootPath -Child $targetDirectory
Assert-ChildPath -Parent $runtimeRootPath -Child $stagingDirectory

if (Test-NodeRuntime -Directory $targetDirectory -ExpectedVersion $Version) {
    Write-InstallMessage "Node.js v$Version 已安装：$targetDirectory"
    exit 0
}

New-Item -ItemType Directory -Force -Path $stagingDirectory | Out-Null

try {
    if (-not (Test-Path -LiteralPath $bundledArchivePath -PathType Leaf)) {
        throw "内置 Node.js 压缩包不存在：$bundledArchivePath"
    }
    if (-not (Test-Path -LiteralPath $bundledChecksumPath -PathType Leaf)) {
        throw "内置 Node.js 校验清单不存在：$bundledChecksumPath"
    }
    Write-InstallMessage '正在验证内置 Node.js/npm 运行环境...'

    $checksumLine = Get-Content -LiteralPath $bundledChecksumPath |
        Where-Object { $_ -match "^[a-fA-F0-9]{64}\s+$([regex]::Escape($archiveName))$" } |
        Select-Object -First 1
    if (-not $checksumLine) {
        throw "Node.js 校验清单中不存在 $archiveName"
    }

    $expectedHash = ($checksumLine -split '\s+')[0].ToUpperInvariant()
    $actualHash = Get-FileSha256 -Path $bundledArchivePath
    if ($actualHash -ne $expectedHash) {
        throw 'Node.js 压缩包 SHA-256 校验失败'
    }

    $extractDirectory = Join-Path $stagingDirectory 'extract'
    New-Item -ItemType Directory -Force -Path $extractDirectory | Out-Null
    & tar.exe -xf $bundledArchivePath -C $extractDirectory
    if ($LASTEXITCODE -ne 0) {
        throw "Node.js 压缩包解压失败，tar.exe 错误码：$LASTEXITCODE"
    }
    $expandedRoot = Join-Path $extractDirectory "node-v$Version-$platform"
    if (-not (Test-NodeRuntime -Directory $expandedRoot -ExpectedVersion $Version)) {
        throw '内置 Node.js 运行环境未通过版本验证'
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
    Write-Utf8Json -Value $currentState -Path $currentTemporaryPath
    Move-Item -LiteralPath $currentTemporaryPath -Destination $currentPath -Force

    Write-InstallMessage "Node.js v$Version 与 npm 安装完成：$targetDirectory"
} catch {
    Write-InstallMessage "Node.js/npm 安装失败：$($_.Exception.Message)"
    throw
} finally {
    if (Test-Path -LiteralPath $stagingDirectory) {
        Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
    }
}
