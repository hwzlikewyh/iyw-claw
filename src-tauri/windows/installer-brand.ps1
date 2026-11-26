param([Parameter(Mandatory=$true)][string]$Root,
      [Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
$brandPath = Join-Path $Root 'config/branding.json'
$brandId = 'iyw'
$brandName = '原助理'
$brandIcon = Join-Path $Root 'app/iyw-claw.exe'
if (Test-Path -LiteralPath $brandPath -PathType Leaf) {
    if ((Get-Item -LiteralPath $brandPath).Length -gt 65536) { throw '品牌配置过大' }
    $brand = Get-Content -LiteralPath $brandPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $brandId = [string]$brand.brandId
    $brandName = [string]$brand.displayName
    if ($brandId -notmatch '^[a-z][a-z0-9]{1,31}$' -or
        [string]::IsNullOrWhiteSpace($brandName) -or $brandName.Length -gt 64 -or
        $brandName -match '[\x00-\x1f<>:"/\\|?*]') { throw '品牌配置无效' }
    if ($brand.iconFile) {
        $configRoot = [IO.Path]::GetFullPath((Join-Path $Root 'config'))
        $brandIcon = [IO.Path]::GetFullPath((Join-Path $configRoot ([string]$brand.iconFile)))
        if (!$brandIcon.StartsWith($configRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw '品牌图标路径超出配置目录'
        }
    }
}
$suffix = if ($brandId -eq 'iyw') { '' } else { "-$brandId" }
$identifier = if ($brandId -eq 'iyw') { 'app.iywclaw' } else { "app.iywclaw.$brandId" }
$uninstallName = if ($brandId -eq 'iyw') { '原助理' } else { $identifier }
$values = @('[Brand]', "Id=$brandId", "Name=$brandName", "Identifier=$identifier",
    "Scheme=iyw-assistant$suffix", "UserDirectory=.iyw-claw$suffix", "Icon=$brandIcon",
    "InstallKey=Software\iywclaw\iyw-claw$suffix",
    "UninstallKey=Software\Microsoft\Windows\CurrentVersion\Uninstall\$uninstallName")
[IO.File]::WriteAllLines($Output, $values, [Text.Encoding]::Unicode)
