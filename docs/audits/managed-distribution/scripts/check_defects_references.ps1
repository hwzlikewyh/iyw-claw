<#
check_defects_references.ps1 — 校验 defects.yaml 证据引用与 ID 完整性（Audit A 补充 / Audit B 复用）
用法：
  powershell -File docs/audits/managed-distribution/scripts/check_defects_references.ps1
  [-DefectsPath <defects.yaml>] [-ReposRoot <F:\projects\iyw>] [-ReportOnlyMissing]
输出：逐条引用状态（OK / MISSING / AMBIGUOUS / LINE-OUT-OF-RANGE）与 P0/P1 字段完整性检查。
说明：路径解析规则——`iyw-claw/...`、`iyw-fusion-api/...`、`skill/...` 前缀直接映射三仓；
裸文件名在三个仓库内按 basename 检索，命中多处标 AMBIGUOUS（需要人工确认目标）。
#>
param(
  [string]$DefectsPath = (Join-Path $PSScriptRoot '..\defects.yaml'),
  [string]$ReposRoot = 'F:\projects\iyw',
  [switch]$ReportOnlyMissing
)
$ErrorActionPreference = 'Continue'
$defects = Resolve-Path $DefectsPath
$repos = @{ 'iyw-claw' = Join-Path $ReposRoot 'iyw-claw'; 'iyw-fusion-api' = Join-Path $ReposRoot 'iyw-fusion-api'; 'skill' = Join-Path $ReposRoot 'skill' }

$lines = Get-Content $defects
$inEvidence = $false
$currentId = $null
$ids = [System.Collections.Generic.HashSet[string]]::new()
$results = New-Object System.Collections.Generic.List[string]
$missingFields = New-Object System.Collections.Generic.List[string]
$severity = $null
$hasOwner = $false; $hasRepro = $false; $hasRoot = $false

function Resolve-Reference([string]$ref) {
  # 剥离尾部中文/半角注释（如 （未跟踪）、（externalBin/resources））
  $clean = ($ref -replace '\s*[（(][^）)]*[）)]\s*$','').Trim()
  # 剥离行号说明（:12 / :14-31 / :62,110 / :14-31,43-44）
  $clean = ($clean -replace ':\d+(-\d+)?(,\d+(-\d+)?)*\s*$','').Trim()
  $path = $clean
  if ($path -eq '') { return @('SKIP','') }
  # 去文档/evidence 类引用
  if ($path -match '^(docs/|evidence/|skill/experts\.toml|git -C|F:\\|C:\\|/skills|#)') {
    if ($path -like 'skill/*') {
      $cand = Join-Path $repos['skill'] ($path.Substring('skill/'.Length))
      if (Test-Path $cand) { return @('OK', $cand) }
      return @('MISSING', $cand)
    }
    return @('SKIP', $path)
  }
  foreach ($k in $repos.Keys) {
    $prefix = "$k/"
    if ($path.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      $cand = Join-Path $repos[$k] ($path.Substring($prefix.Length))
      if (Test-Path $cand) { return @('OK', $cand) }
      return @('MISSING', $cand)
    }
  }
  # 裸文件名：三仓检索
  $base = Split-Path $path -Leaf
  $hits = @()
  foreach ($k in $repos.Keys) {
    $hits += Get-ChildItem -Path $repos[$k] -Recurse -Filter $base -File -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -notmatch '\\node_modules\\|\\target\\|\\\.git\\|\\experts\\skills\\' } |
      ForEach-Object { $_.FullName }
  }
  if ($hits.Count -eq 0) { return @('MISSING', $path) }
  if ($hits.Count -gt 1) { return @('AMBIGUOUS', ($hits -join ' | ')) }
  return @('OK', $hits[0])
}

foreach ($line in $lines) {
  if ($line -match '^\s+- id: (IYW-[A-Z0-9-]+)') {
    $currentId = $Matches[1]
    if (-not $ids.Add($currentId)) { $results.Add("DUPLICATE-ID`t$currentId") }
    $severity = $null; $hasOwner = $false; $hasRepro = $false; $hasRoot = $false
    continue
  }
  if ($line -match '^\s+severity: (P[0-3])') { $severity = $Matches[1]; continue }
  if ($line -match '^\s+owner_task:') { $hasOwner = $true; continue }
  if ($line -match '^\s+reproduction:') { $hasRepro = $true; continue }
  if ($line -match '^\s+root_cause:') { $hasRoot = $true; continue }
  if ($line -match '^\s+evidence:') { $inEvidence = $true; continue }
  if ($line -match '^\s+[a-z_]+:') { $inEvidence = $false; continue }
  if ($inEvidence -and $line -match '^\s+- (.+)$') {
    $ref = $Matches[1]
    if ($currentId) {
      $r = Resolve-Reference $ref
      if ($r[0] -eq 'OK' -and $ref -match ':\d+') {
        $m2 = [regex]::Match($ref, '(\d+)(-\d+)?(,\d+(-\d+)?)*')
        $ln = [int]$m2.Groups[1].Value
        $fc = (Get-Content $r[1] -ErrorAction SilentlyContinue).Count
        if ($fc -gt 0 -and $ln -gt $fc) { $results.Add("LINE-OUT-OF-RANGE`t$currentId`t$ref`t(has $fc lines)") }
      }
      if ($r[0] -ne 'OK' -and $r[0] -ne 'SKIP') { $results.Add("$($r[0])`t$currentId`t$ref`t$($r[1])") }
    }
    continue
  }
  if ($line -match '^\s*- id:' -or $line -match '^defects:') { }
}
# P0/P1 字段完整性
$lines2 = Get-Content $defects
for ($i=0; $i -lt $lines2.Count; $i++) {
  if ($lines2[$i] -match '^\s+- id: (IYW-[A-Z0-9-]+)') {
    $cid = $Matches[1]; $sev=''; $o=$false; $r=$false; $rc=$false
    for ($j=$i+1; $j -lt $lines2.Count; $j++) {
      if ($lines2[$j] -match '^\s+- id:') { break }
      if ($lines2[$j] -match '^\s+severity: (P[0-3])') { $sev = $Matches[1] }
      if ($lines2[$j] -match '^\s+owner_task:') { $o = $true }
      if ($lines2[$j] -match '^\s+reproduction:') { $r = $true }
      if ($lines2[$j] -match '^\s+root_cause:') { $rc = $true }
    }
    if (($sev -eq 'P0' -or $sev -eq 'P1') -and (-not $o -or -not $r -or -not $rc)) {
      $missingFields.Add("MISSING-FIELDS`t$cid`towner=$o repro=$r root_cause=$rc")
    }
  }
}

Write-Host "== defects.yaml: $DefectsPath =="
Write-Host "唯一 ID 数：$($ids.Count)"
foreach ($r in $results) { Write-Host $r }
foreach ($m in $missingFields) { Write-Host $m }
Write-Host "== 完成 =="
