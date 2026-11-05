param([switch]$Unregister, [switch]$PurgeInbox)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$packageName = 'IywAssistant.ShareTarget'
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
        if (-not $process.WaitForExit(5000)) {
            $process.Kill()
            $process.WaitForExit()
            throw '分享接收程序身份检查超时。'
        }
        $script:identityError = $process.StandardError.ReadToEnd().Trim()
        return $process.ExitCode -eq 0
    } finally { $process.Dispose() }
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
    throw '系统分享需要 Windows 10 2004 或更新版本，可以使用手动导入。'
}
$installed = @(Get-AppxPackage -Name $packageName)
$helper = Join-Path $PSScriptRoot 'iyw-share-target.exe'
$owned = $false
if (Test-Path -LiteralPath $helper) {
    $owned = Test-ShareIdentity $helper
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
foreach ($name in @('iyw-share-target.exe', 'resources.pri', 'Assets\Square44x44Logo.png', 'Assets\Square150x150Logo.png')) {
    if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $name) -PathType Leaf)) {
        throw "系统分享组件不完整：$name。请重新安装正式安装包。"
    }
}
$configuration = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'config.json') -Encoding UTF8 -Raw | ConvertFrom-Json
if (-not $configuration.signed) { throw '开发包未签名，不能注册系统分享入口；可手动导入聊天 ZIP。' }
$expectedVersion = [version]$configuration.packageVersion
if ($owned -and ($installed | Where-Object { $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' })) { exit 0 }
if ($installed | Where-Object { $_.Version -eq $expectedVersion }) {
    # 同版本换目录安装需要重建身份绑定，避免分享入口继续指向旧位置。
    foreach ($package in $installed) { Remove-AppxPackage -Package $package.PackageFullName }
}
Add-AppxPackage -Path $packagePath -ExternalLocation $PSScriptRoot -ForceApplicationShutdown
if (-not (Test-ShareIdentity $helper)) {
    throw "系统分享注册后未取得匹配的包身份：$script:identityError"
}
$registered = @(Get-AppxPackage -Name $packageName | Where-Object { $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' })
if ($registered.Count -eq 0) { throw '系统分享注册后包状态或版本不正确。' }
