param([switch]$Unregister, [switch]$PurgeInbox)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$packageName = 'IywAssistant.ShareTarget'
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
    & $helper --registration-check
    $owned = $LASTEXITCODE -eq 0
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
$configuration = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'config.json') -Encoding UTF8 -Raw | ConvertFrom-Json
if (-not $configuration.signed) { throw '开发包未签名，不能注册系统分享入口；可手动导入聊天 ZIP。' }
$expectedVersion = [version]$configuration.packageVersion
if ($owned -and ($installed | Where-Object { $_.Version -ge $expectedVersion -and $_.Status -eq 'Ok' })) { exit 0 }
if ($installed | Where-Object { $_.Version -eq $expectedVersion }) {
    # 同版本换目录安装需要重建身份绑定，避免分享入口继续指向旧位置。
    foreach ($package in $installed) { Remove-AppxPackage -Package $package.PackageFullName }
}
Add-AppxPackage -Path $packagePath -ExternalLocation $PSScriptRoot -ForceApplicationShutdown
