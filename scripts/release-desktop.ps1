param([string]$Version, [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$releaseRoot = Split-Path -Parent $PSScriptRoot
foreach ($name in @('IYW_FUSION_ADMIN_TOKEN', 'IYW_FUSION_GATEWAY_TOKEN', 'HTTPS_PROXY', 'HTTP_PROXY', 'NO_PROXY')) {
    if (![Environment]::GetEnvironmentVariable($name, 'Process')) {
        $value = [Environment]::GetEnvironmentVariable($name, 'User')
        if ($value) { [Environment]::SetEnvironmentVariable($name, $value, 'Process') }
    }
}
Get-Command node, gh -ErrorAction Stop | Out-Null
$releaseArgs = @()
if ($Version) { $releaseArgs += $Version }
if ($CheckOnly) { $releaseArgs += '--check' }
Push-Location $releaseRoot
try {
    & node (Join-Path $PSScriptRoot 'release-desktop.mjs') @releaseArgs
    if ($LASTEXITCODE -ne 0) { throw '发布未完成，请根据上面的错误处理后用同一版本号重试。' }
} finally { Pop-Location }
