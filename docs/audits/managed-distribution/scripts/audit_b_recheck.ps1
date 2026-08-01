<#
audit_b_recheck.ps1 — Audit B 回归重跑：重跑 Audit A 自动检查并与基线证据对比
用法：
  powershell -File docs/audits/managed-distribution/scripts/audit_b_recheck.ps1
    [-ReposRoot F:\projects\iyw] [-ReportDir <audits/managed-distribution>]
输出：
  1. 三仓当前 HEAD 与基线提交（evidence/00-git-baselines.txt）对比，报告漂移。
  2. Rust 风险模式 / 外部 URL / include_dir / 凭据命中数量，与基线 evidence 对比。
  3. 引用校验（复用 check_defects_references.ps1）。
  新增告警必须人工分类，不能直接忽略。
#>
param(
  [string]$ReposRoot = 'F:\projects\iyw',
  [string]$ReportDir = (Join-Path $PSScriptRoot '..')
)
$ErrorActionPreference = 'Continue'
$evidence = Join-Path $ReportDir 'evidence'
function Get-Count([string]$file, [string]$pattern) {
  if (-not (Test-Path $file)) { return -1 }
  return (Select-String -Path $file -Pattern $pattern -AllMatches | Measure-Object).Count
}
Write-Host '== 1. 三仓 HEAD vs 基线 =='
$baselineFile = Join-Path $evidence '00-git-baselines.txt'
if (Test-Path $baselineFile) {
  Get-Content $baselineFile | Select-Object -First 6 | ForEach-Object { Write-Host "基线: $_" }
} else { Write-Host '基线文件缺失' }
foreach ($repo in @('iyw-claw','iyw-fusion-api','skill')) {
  $root = Join-Path $ReposRoot $repo
  Push-Location $root
  $head = git log --oneline -1 2>$null
  $branch = git rev-parse --abbrev-ref HEAD 2>$null
  $dirty = (git status --porcelain 2>$null | Measure-Object).Count
  Write-Host "$repo`tbranch=$branch`thead=$head`tuncommitted=$dirty"
  Pop-Location
}
Write-Host '== 2. 自动检查计数（对比基线 evidence）=='
$base = Join-Path $ReposRoot 'iyw-claw\src-tauri\src'
$unwrapNow = (rg -c '\.unwrap\(\)|\.expect\(|panic!|unreachable!' $base -g '*.rs' --glob '!**/experts/skills/**' 2>$null | Measure-Object).Count
$unwrapBase = Get-Count (Join-Path $evidence '01-rust-unwrap-expect.txt') 'unwrap|expect|panic|unreachable'
$includeNow = (Select-String -Path (Join-Path $base 'commands\experts.rs') -Pattern 'include_dir!' -ErrorAction SilentlyContinue | Measure-Object).Count
Write-Host "unwrap/expect/panic 命中文件数：now=$unwrapNow baseline=$unwrapBase"
Write-Host "experts.rs include_dir!：now=$includeNow baseline=14（Audit A）"
Write-Host '== 3. 凭据扫描（脱敏）=='
$tmp = Join-Path $env:TEMP "audit_b_secrets_$(Get-Date -Format 'yyyyMMddHHmmss').txt"
& (Join-Path $PSScriptRoot 'scan_secrets.ps1') -ReposRoot $ReposRoot -OutFile $tmp | Out-Null
$secretsNow = (Get-Content $tmp -ErrorAction SilentlyContinue | Where-Object { $_ -match 'MASKED' } | Measure-Object).Count
Write-Host "secret 命中：now=$secretsNow baseline=1（IYW-SEC-001 基线；工作区已移除，若 now>1 需人工分类）"
Write-Host '== 4. 引用校验（复用 check_defects_references.ps1）=='
& (Join-Path $PSScriptRoot 'check_defects_references.ps1') -ReposRoot $ReposRoot
Write-Host '== 完成：新增告警必须人工分类，不能直接忽略 =='
