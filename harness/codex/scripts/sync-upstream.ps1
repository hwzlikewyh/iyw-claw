param(
  [switch]$CheckOnly,
  [switch]$RunProbe
)

$ErrorActionPreference = "Stop"
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$harnessRoot = Split-Path -Parent $scriptRoot
$lockPath = Join-Path $harnessRoot "upstream.lock"
$identityPath = Join-Path $harnessRoot "src/upstream.rs"
$harnessManifestPath = Join-Path $harnessRoot "Cargo.toml"
$probeManifestPath = Join-Path $harnessRoot "upstream-probe/Cargo.toml"
$ptyPatchPath = Join-Path $harnessRoot "patches/codex-utils-pty/Cargo.toml"
$lock = Get-Content -Raw $lockPath | ConvertFrom-Json

if ($lock.repository -notmatch '^https://github\.com/[^/]+/[^/]+\.git$' -or
    $lock.ref -match '\s' -or
    $lock.tagObject -notmatch '^[0-9a-f]{40}$' -or
    $lock.commit -notmatch '^[0-9a-f]{40}$' -or
    $lock.localPatches.Count -ne 1 -or
    $lock.localPatches[0].crate -ne 'codex-utils-pty' -or
    $lock.localPatches[0].basePath -ne 'codex-rs/utils/pty' -or
    [string]::IsNullOrWhiteSpace($lock.repository) -or
    [string]::IsNullOrWhiteSpace($lock.ref) -or
    [string]::IsNullOrWhiteSpace($lock.commit)) {
    throw "upstream.lock must contain repository, ref, and commit"
}
$identity = Get-Content -Raw $identityPath
foreach ($value in @($lock.repository, $lock.ref, $lock.tagObject, $lock.commit)) {
  if ($identity.IndexOf($value, [StringComparison]::Ordinal) -lt 0) {
    throw "src/upstream.rs is out of sync with upstream.lock"
  }
}
$probeManifest = Get-Content -Raw $probeManifestPath
$harnessManifest = Get-Content -Raw $harnessManifestPath
foreach ($value in @($lock.repository, $lock.commit)) {
  if ($harnessManifest.IndexOf($value, [StringComparison]::Ordinal) -lt 0) {
    throw "harness/Cargo.toml is out of sync with upstream.lock"
  }
}
if (-not (Test-Path -LiteralPath $ptyPatchPath)) {
  throw "harness Windows PTY compatibility patch is missing"
}
foreach ($file in @($lock.localPatches[0].files)) {
  if ([string]::IsNullOrWhiteSpace($file) -or
      -not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $ptyPatchPath) $file))) {
    throw "harness Windows PTY compatibility patch file is missing: $file"
  }
}
foreach ($patch in @($lock.cargoPatches)) {
  foreach ($value in @($patch.repository, $patch.commit)) {
    if ($probeManifest.IndexOf($value, [StringComparison]::Ordinal) -lt 0 -or
        $harnessManifest.IndexOf($value, [StringComparison]::Ordinal) -lt 0) {
      throw "Codex Cargo patch pins are out of sync with upstream.lock"
    }
  }
}

$remoteTags = @(git ls-remote $lock.repository "refs/tags/$($lock.ref)" "refs/tags/$($lock.ref)^{}")
if ($LASTEXITCODE -ne 0) {
  throw "Unable to resolve Codex upstream tag $($lock.ref)"
}

$peeled = $remoteTags |
  Where-Object { $_ -match "\srefs/tags/$([regex]::Escape($lock.ref))\^\{\}$" } |
  Select-Object -First 1
$tagLine = $remoteTags |
  Where-Object { $_ -match "\srefs/tags/$([regex]::Escape($lock.ref))$" } |
  Select-Object -First 1
$actualTagObject = if ($tagLine) { ($tagLine -split "\s+")[0] } else { "" }
$actualCommit = if ($peeled) { ($peeled -split "\s+")[0] } else { $actualTagObject }
if ($actualTagObject -ne $lock.tagObject) {
  throw "Codex tag object $($lock.ref) moved from $($lock.tagObject) to $actualTagObject"
}
if ($actualCommit -ne $lock.commit) {
  throw "Codex tag $($lock.ref) moved from $($lock.commit) to $actualCommit"
}

Write-Output "Codex upstream verified: $($lock.ref) ($actualCommit)"
if ($CheckOnly) {
  exit 0
}

if ($RunProbe) {
  cargo check --manifest-path $probeManifestPath
  if ($LASTEXITCODE -ne 0) {
    throw "Codex upstream compile probe failed"
  }
}

Write-Output "No files were changed. Update upstream.lock explicitly when adopting a new release."
