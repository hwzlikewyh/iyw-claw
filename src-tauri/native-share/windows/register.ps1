param([switch]$Unregister, [switch]$PurgeInbox, [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$packageName = 'IywAssistant.Desktop'
$legacyPackageName = 'IywAssistant.ShareTarget'
$identityTimeoutMs = 5000
$appDirectory = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
function Test-ShareIdentity([string]$HelperPath) {
    $start = New-Object Diagnostics.ProcessStartInfo
    $start.FileName = $HelperPath
    $start.Arguments = '--native-share-registration-check'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardError = $true
    $process = [Diagnostics.Process]::Start($start)
    try {
        if (-not $process.WaitForExit($identityTimeoutMs)) { $process.Kill(); $process.WaitForExit(); throw '分享接收程序身份检查超时。' }
        $script:identityError = $process.StandardError.ReadToEnd().Trim()
        return $process.ExitCode -eq 0
    } finally { $process.Dispose() }
}
function Test-ShareLocation($Package, [switch]$IncludeLegacy) {
    $null = [Windows.Management.Deployment.PackageManager, Windows.Management.Deployment, ContentType=WindowsRuntime]
    $manager = New-Object Windows.Management.Deployment.PackageManager
    $native = $manager.FindPackageForUser('', $Package.PackageFullName)
    if (-not $native) { return $false }
    $effective = [IO.Path]::GetFullPath($native.EffectiveLocation.Path).TrimEnd('\')
    $expected = $appDirectory.TrimEnd('\')
    if ([string]::Equals($effective, $expected, [StringComparison]::OrdinalIgnoreCase)) { return $true }
    return $IncludeLegacy -and [string]::Equals($effective, $PSScriptRoot.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
}
function Test-ShareManifest($Package) {
    $manifest = Get-AppxPackageManifest -Package $Package
    $application = $manifest.Package.Applications.Application
    return $application.Id -eq 'Share.Hub' -and $application.Executable -eq 'iyw-claw.exe'
}
function Test-RegisteredShareResources($Package) {
    foreach ($name in Get-ShareResourceNames) {
        $installedFile = Join-Path $Package.InstallLocation $name
        if (-not (Test-Path -LiteralPath $installedFile -PathType Leaf)) { return $false }
        if ((Get-FileHash -LiteralPath $installedFile -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $name) -Algorithm SHA256).Hash) { return $false }
    }
    return $true
}
if ($PurgeInbox) {
    $localData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
    $inbox = [IO.Path]::GetFullPath((Join-Path $localData 'app.iywclaw\share-inbox'))
    $expectedParent = [IO.Path]::GetFullPath((Join-Path $localData 'app.iywclaw'))
    if ([IO.Path]::GetDirectoryName($inbox) -ne $expectedParent) { throw '分享目录超出范围。' }
    if (Test-Path -LiteralPath $inbox) {
        foreach ($path in @($expectedParent, $inbox)) { $entry = Get-Item -LiteralPath $path -Force; if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw '拒绝删除链接形式的分享目录。' } }
        Remove-Item -LiteralPath $inbox -Recurse -Force
    }
    exit 0
}
if ([Environment]::OSVersion.Version.Build -lt 19041) {
    if ($Unregister) { exit 0 }; if ($CheckOnly) { exit 1 }
    throw '系统分享需要 Windows 10 2004 或更新版本，可以使用手动导入。'
}
$installed = @(Get-AppxPackage -Name $packageName)
$legacyPackages = @(Get-AppxPackage -Name $legacyPackageName | Where-Object { Test-ShareLocation $_ -IncludeLegacy })
$helper = Join-Path $appDirectory 'iyw-claw.exe'
if ($Unregister) {
    $ownedPackages = @($installed) + $legacyPackages
    foreach ($package in $ownedPackages | Where-Object { Test-ShareLocation $_ -IncludeLegacy }) {
        Remove-AppxPackage -Package $package.PackageFullName
    }
    exit 0
}
$owned = $false
if ((Test-Path -LiteralPath $helper) -and ($installed | Where-Object { (Test-ShareLocation $_) -and (Test-ShareManifest $_) })) {
    $owned = Test-ShareIdentity $helper
}
$packagePath = Join-Path $PSScriptRoot 'IywAssistant.ShareTarget.msix'
if (-not (Test-Path -LiteralPath $packagePath)) { throw '当前安装包没有可注册的系统分享组件，请使用已签名的正式安装包。' }
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) { throw '原助理主程序缺失，请重新安装正式安装包。' }
if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'share-resources.ps1') -PathType Leaf)) { throw '分享资源校验脚本缺失，请重新安装正式安装包。' }
. (Join-Path $PSScriptRoot 'share-resources.ps1')
Assert-ShareResources $PSScriptRoot
$configuration = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'config.json') -Encoding UTF8 -Raw | ConvertFrom-Json
if (-not $configuration.signed -or $configuration.receiver -ne 'main') { throw '当前构建没有可注册的主程序分享身份包。' }
$expectedVersion = [version]$configuration.packageVersion
if ($owned -and ($installed | Where-Object { $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' -and (Test-ShareManifest $_) -and (Test-RegisteredShareResources $_) })) {
    if ($CheckOnly -and $legacyPackages.Count -gt 0) { exit 1 }
    if (-not $CheckOnly) { foreach ($package in $legacyPackages) { Remove-AppxPackage -Package $package.PackageFullName } }
    exit 0
}
if ($CheckOnly) { exit 1 }
Write-Output "Updating native share registration for current user: version=$expectedVersion identity=$owned"
if ($installed | Where-Object { $_.Version -eq $expectedVersion }) {
    $running = @(Get-Process -Name 'iyw-claw' -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $helper })
    if ($running.Count -gt 0) { throw '正在运行的原助理占用了分享身份，请使用正常安装器修复，安装器会安全关闭旧实例。' }
    foreach ($package in $installed) { Remove-AppxPackage -Package $package.PackageFullName }
}
Add-AppxPackage -Path $packagePath -ExternalLocation $appDirectory
if (-not (Test-ShareIdentity $helper)) { throw "系统分享注册后未取得匹配的包身份：$script:identityError" }
$registered = @(Get-AppxPackage -Name $packageName | Where-Object { $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' -and (Test-ShareLocation $_) -and (Test-ShareManifest $_) -and (Test-RegisteredShareResources $_) })
if ($registered.Count -eq 0) { throw '系统分享注册后包状态或版本不正确。' }
foreach ($package in $legacyPackages) {
    Remove-AppxPackage -Package $package.PackageFullName
}
Write-Output "Native share registration verified: version=$expectedVersion status=Ok"
