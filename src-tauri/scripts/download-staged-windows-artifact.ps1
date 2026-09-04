$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$requiredEnvironment = @(
  'GH_TOKEN',
  'GITHUB_REPOSITORY',
  'GITHUB_RUN_ID',
  'RUNNER_TEMP',
  'STAGING_ARTIFACT_NAME'
)
foreach ($name in $requiredEnvironment) {
  if (![Environment]::GetEnvironmentVariable($name)) {
    throw "Required environment variable is missing: $name"
  }
}

$headers = @{
  Authorization = "Bearer $env:GH_TOKEN"
  Accept = 'application/vnd.github+json'
  'X-GitHub-Api-Version' = '2022-11-28'
}
$artifactsApi = "https://api.github.com/repos/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID/artifacts?per_page=100"
$artifacts = Invoke-RestMethod -Uri $artifactsApi -Headers $headers -TimeoutSec 30
$matches = @($artifacts.artifacts | Where-Object name -eq $env:STAGING_ARTIFACT_NAME)
if ($matches.Count -ne 1) {
  throw "Expected one staging artifact, found $($matches.Count)"
}
$artifact = $matches[0]
if ($artifact.expired) {
  throw 'Staging artifact is expired'
}
if ($artifact.digest -notmatch '^sha256:[0-9a-fA-F]{64}$') {
  throw 'Staging artifact has no valid SHA-256 digest'
}

$downloadEndpoint = "https://api.github.com/repos/$env:GITHUB_REPOSITORY/actions/artifacts/$($artifact.id)/zip"
$downloadDirectory = Join-Path $env:RUNNER_TEMP 'iyw-windows-staging-download'
$artifactArchive = Join-Path $env:RUNNER_TEMP 'iyw-windows-staging-artifact.zip'
Remove-Item -LiteralPath $downloadDirectory -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -LiteralPath $artifactArchive -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $downloadDirectory | Out-Null

for ($attempt = 1; $attempt -le 12; $attempt++) {
  $downloadUrl = $null
  try {
    Invoke-WebRequest -Uri $downloadEndpoint -Headers $headers -Method Get `
      -MaximumRedirection 0 -TimeoutSec 30 -ErrorAction Stop | Out-Null
    throw 'GitHub artifact endpoint did not redirect'
  } catch {
    $response = $_.Exception.Response
    if (!$response -or [int]$response.StatusCode -notin 302, 303) {
      throw
    }
    $downloadUrl = $response.Headers.Location
  }
  if (!$downloadUrl) {
    throw 'GitHub did not return an artifact download URL'
  }

  Write-Host "Downloading staging artifact, attempt $attempt/12"
  $curlArgs = @(
    '--proxy', 'http://127.0.0.1:7890',
    '--output', $artifactArchive,
    '--continue-at', '-',
    '--location',
    '--fail',
    '--silent',
    '--show-error',
    '--connect-timeout', '30',
    '--speed-limit', '1024',
    '--speed-time', '60',
    '--max-time', '480',
    $downloadUrl
  )
  & curl.exe @curlArgs
  $downloaded = if (Test-Path -LiteralPath $artifactArchive) {
    (Get-Item -LiteralPath $artifactArchive).Length
  } else {
    0
  }
  if ($downloaded -eq [int64]$artifact.size_in_bytes) {
    break
  }
  if ($attempt -eq 12) {
    throw "Staging artifact download incomplete: $downloaded/$($artifact.size_in_bytes)"
  }
  Write-Warning "Download interrupted at $downloaded/$($artifact.size_in_bytes); refreshing URL"
  Start-Sleep -Seconds 5
}

$actualHash = (Get-FileHash -LiteralPath $artifactArchive -Algorithm SHA256).Hash.ToLowerInvariant()
$expectedHash = ($artifact.digest -replace '^sha256:', '').ToLowerInvariant()
if ($actualHash -ne $expectedHash) {
  throw 'Staging artifact digest mismatch'
}
Expand-Archive -LiteralPath $artifactArchive -DestinationPath $downloadDirectory -Force
Remove-Item -LiteralPath $artifactArchive -Force
$innerArchive = Join-Path $downloadDirectory 'iyw-windows-staging.zip'
if (!(Test-Path -LiteralPath $innerArchive -PathType Leaf)) {
  throw 'Downloaded artifact did not contain iyw-windows-staging.zip'
}
Write-Host "Downloaded verified staging artifact: sha256=$actualHash"
