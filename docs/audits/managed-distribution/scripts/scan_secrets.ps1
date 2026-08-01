<#
scan_secrets.ps1 — 三仓凭据扫描（脱敏输出：只报路径/类型/行号，不回显值）
用法：
  powershell -File docs/audits/managed-distribution/scripts/scan_secrets.ps1
    [-ReposRoot F:\projects\iyw] [-OutFile <evidence 输出路径>] [-IncludeHistory]
说明：
  - 基于 rg 扫描，模式：password/secret/api_key/access_token 赋值、GitHub PAT(ghp_)、GitLab PAT(glpat-)、URL userinfo。
  - 排除：.git、node_modules、target、experts/skills（第三方 Skill 目录）、__pycache__。
  - 命中值一律不回显；-IncludeHistory 时对 iyw-claw 做 git log -S 历史扫描（结果同样脱敏）。
#>
param(
  [string]$ReposRoot = 'F:\projects\iyw',
  [string]$OutFile = '',
  [switch]$IncludeHistory
)
$ErrorActionPreference = 'Continue'
$repos = @('iyw-claw','iyw-fusion-api','skill')
$excludes = @('-g','!**/.git/**','-g','!**/node_modules/**','-g','!**/target/**','-g','!**/experts/skills/**','-g','!**/__pycache__/**','-g','!**/out/**')
$globs = @('-g','*.rs','-g','*.ts','-g','*.tsx','-g','*.js','-g','*.go','-g','*.toml','-g','*.yaml','-g','*.yml','-g','*.mjs','-g','*.ps1','-g','*.html','-g','*.sql')
$patterns = @(
  @{ Name='assignment';   Regex='(password|passwd|secret|api[_-]?key|access[_-]?token)\s*[:=]\s*["''"][^"''"]{6,}["'']' },
  @{ Name='github-pat';   Regex='ghp_[A-Za-z0-9]{20,}' },
  @{ Name='gitlab-pat';   Regex='glpat-[A-Za-z0-9_-]{20,}' },
  @{ Name='url-userinfo'; Regex='https?://[^\s/@]+:[^\s/@]+@' }
)
$rows = New-Object System.Collections.Generic.List[string]
foreach ($repo in $repos) {
  $root = Join-Path $ReposRoot $repo
  if (-not (Test-Path $root)) { Write-Host "跳过（不存在）：$root"; continue }
  foreach ($p in $patterns) {
    $raw = & rg -n --no-heading $excludes $globs -e $p.Regex $root 2>$null
    if (-not $raw) { continue }
    foreach ($line in $raw) {
      # rg 输出：<path>:<line>:<content>；路径可能含 \ 前缀
      if ($line -match '^(.+?):(\d+):') {
        $full = $Matches[1]; $ln = $Matches[2]
        $rel = $full.Substring($root.Length).TrimStart('\')
        $rows.Add("$repo`t$rel`t$($p.Name)`tline=$ln`tMASKED")
      }
    }
  }
}
$out = $rows | Sort-Object -Unique
Write-Host "== secret scan（脱敏）: $($out.Count) 处命中 =="
$out | ForEach-Object { Write-Host $_ }
if ($IncludeHistory) {
  Write-Host '== 历史扫描（iyw-claw git log -S，仅报告 commit/路径）=='
  Push-Location (Join-Path $ReposRoot 'iyw-claw')
  git log --all --oneline -S 'iyw_lq' -- src-tauri/src/system_skills 2>$null | Select-Object -First 10 | ForEach-Object { Write-Host "HISTORY`t$_" }
  Pop-Location
}
if ($OutFile -ne '') {
  $dir = Split-Path $OutFile -Parent
  if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  @("# secret scan（脱敏）$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')", "命中数：$($out.Count)") + $out | Set-Content -Path $OutFile -Encoding UTF8
  Write-Host "已写入：$OutFile"
}

