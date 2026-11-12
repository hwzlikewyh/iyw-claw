param([switch]$Unregister, [switch]$PurgeInbox, [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$packageName = 'IywAssistant.ShareTarget'
$identityTimeoutMs = 5000
function Test-ShareIdentity([string]$HelperPath) {
    # 接收器是 GUI 子系统程序；PowerShell 的 & 不保证等待，不能读取旧的 LASTEXITCODE。
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $HelperPath
    $start.Arguments = '--registration-check'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($start)
    try {
        if (-not $process.WaitForExit($identityTimeoutMs)) {
            $process.Kill()
            $process.WaitForExit()
            throw '分享接收程序身份检查超时。'
        }
        $script:identityError = $process.StandardError.ReadToEnd().Trim()
        return $process.ExitCode -eq 0
    } finally { $process.Dispose() }
}
function Test-ShareLocation($Package) {
    $null = [Windows.Management.Deployment.PackageManager, Windows.Management.Deployment, ContentType=WindowsRuntime]
    $manager = New-Object Windows.Management.Deployment.PackageManager
    $native = $manager.FindPackageForUser('', $Package.PackageFullName)
    if (-not $native) { return $false }
    $effective = [IO.Path]::GetFullPath($native.EffectiveLocation.Path).TrimEnd('\')
    $expected = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\')
    return [string]::Equals($effective, $expected, [StringComparison]::OrdinalIgnoreCase)
}
function Test-RegisteredShareResources($Package) {
    foreach ($name in Get-ShareResourceNames) {
        $installedFile = Join-Path $Package.InstallLocation $name
        if (-not (Test-Path -LiteralPath $installedFile -PathType Leaf)) { return $false }
        if ((Get-FileHash -LiteralPath $installedFile -Algorithm SHA256).Hash -ne
            (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $name) -Algorithm SHA256).Hash) {
            return $false
        }
    }
    return $true
}
if ($PurgeInbox) {
    $localData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
    $inbox = [IO.Path]::GetFullPath((Join-Path $localData 'app.iywclaw\share-inbox'))
    $expectedParent = [IO.Path]::GetFullPath((Join-Path $localData 'app.iywclaw'))
    if ([IO.Path]::GetDirectoryName($inbox) -ne $expectedParent) { throw '分享目录超出范围。' }
    if (Test-Path -LiteralPath $inbox) {
        foreach ($path in @($expectedParent, $inbox)) {
            $entry = Get-Item -LiteralPath $path -Force
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '拒绝删除链接形式的分享目录。' }
        }
        Remove-Item -LiteralPath $inbox -Recurse -Force
    }
    exit 0
}
if ([Environment]::OSVersion.Version.Build -lt 19041) {
    if ($Unregister) { exit 0 }
    if ($CheckOnly) { exit 1 }
    throw '系统分享需要 Windows 10 2004 或更新版本，可以使用手动导入。'
}
$installed = @(Get-AppxPackage -Name $packageName)
$helper = Join-Path $PSScriptRoot 'iyw-share-target.exe'
$owned = $false
if (Test-Path -LiteralPath $helper) {
    $owned = Test-ShareIdentity $helper
    $owned = $owned -and [bool]($installed | Where-Object { Test-ShareLocation $_ })
}
if ($Unregister) {
    if ($owned) {
        foreach ($package in $installed) { Remove-AppxPackage -Package $package.PackageFullName }
    }
    exit 0
}
$packagePath = Join-Path $PSScriptRoot 'IywAssistant.ShareTarget.msix'
if (-not (Test-Path -LiteralPath $packagePath)) {
    throw '当前安装包没有可注册的系统分享组件，请使用已签名的正式安装包。'
}
foreach ($name in @('iyw-share-target.exe', 'share-resources.ps1')) {
    if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $name) -PathType Leaf)) {
        throw "系统分享组件不完整：$name。请重新安装正式安装包。"
    }
}
. (Join-Path $PSScriptRoot 'share-resources.ps1')
Assert-ShareResources $PSScriptRoot
$configuration = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'config.json') -Encoding UTF8 -Raw | ConvertFrom-Json
if (-not $configuration.signed) { throw '开发包未签名，不能注册系统分享入口；可手动导入聊天 ZIP。' }
$expectedVersion = [version]$configuration.packageVersion
if ($owned -and ($installed | Where-Object {
    $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' -and (Test-RegisteredShareResources $_)
})) { exit 0 }
if ($CheckOnly) { exit 1 }
Write-Output "Updating native share registration for current user: version=$expectedVersion identity=$owned"
if ($installed | Where-Object { $_.Version -eq $expectedVersion }) {
    # 同版本换目录安装需要重建身份绑定，避免分享入口继续指向旧位置。
    foreach ($package in $installed) { Remove-AppxPackage -Package $package.PackageFullName }
}
Add-AppxPackage -Path $packagePath -ExternalLocation $PSScriptRoot -ForceApplicationShutdown
if (-not (Test-ShareIdentity $helper)) {
    throw "系统分享注册后未取得匹配的包身份：$script:identityError"
}
$registered = @(Get-AppxPackage -Name $packageName | Where-Object {
    $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' -and
        (Test-ShareLocation $_) -and (Test-RegisteredShareResources $_)
})
if ($registered.Count -eq 0) { throw '系统分享注册后包状态或版本不正确。' }
Write-Output "Native share registration verified: version=$expectedVersion status=Ok"
