param([ValidateSet('resources','installer','upload')][string]$Action)
$ErrorActionPreference = 'Stop'

function Get-Draft {
    $release = gh api "repos/$env:GITHUB_REPOSITORY/releases/$env:RELEASE_ID" | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or !$release.draft -or $release.tag_name -ne "v$env:RELEASE_VERSION") { throw 'Expected matching draft release' }
    return $release
}

if ($Action -eq 'resources') {
    $name = if ($env:BUILD_ARCH -eq 'x64') { 'iyw-windows-staging.zip' } else { 'iyw-windows-x86-staging.zip' }
    $root = Join-Path $env:RUNNER_TOOL_CACHE "iyw-staging-cache/$env:SOURCE_STAGING_RUN_ID"
    $archive = Join-Path $root $name
    $release = Get-Draft
    $asset = @($release.assets | Where-Object name -eq $name)
    if ($asset.Count -ne 1 -or !(Test-Path -LiteralPath $archive)) { throw 'Verified staging cache is unavailable' }
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($asset[0].digest -ne "sha256:$hash") { throw 'Staging cache SHA-256 mismatch' }
    $destination = Join-Path $env:RUNNER_TEMP "finish-$env:GITHUB_RUN_ID/$env:BUILD_ARCH-input"
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    if ($env:BUILD_ARCH -eq 'x64') { Copy-Item -LiteralPath $archive -Destination (Join-Path $destination $name) }
    else { Expand-Archive -LiteralPath $archive -DestinationPath $destination }
    Write-Output "Restored verified $env:BUILD_ARCH staging cache: $hash"
}

if ($Action -eq 'installer') {
    $root = Join-Path $env:RUNNER_TOOL_CACHE "iyw-staging-cache/$env:SOURCE_SIGNING_RUN_ID"
    $manifest = Get-Content -LiteralPath (Join-Path $root 'signed-installer.json') -Raw | ConvertFrom-Json
    if ($manifest.sourceCommit -ne (git rev-parse HEAD).Trim() -or $manifest.version -ne $env:RELEASE_VERSION -or $manifest.sourceRunId -ne $env:SOURCE_SIGNING_RUN_ID) { throw 'Signed installer source mismatch' }
    $name = "iyw-claw_$env:RELEASE_VERSION`_x64-setup.exe"
    $path = Join-Path $root $name
    if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.sha256) { throw 'Signed installer cache changed' }
    $directory = "src-tauri/target/$env:BUILD_TARGET/release/bundle/nsis"
    if (Test-Path -LiteralPath $directory) { Remove-Item -LiteralPath $directory -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $directory | Out-Null
    Copy-Item -LiteralPath $path -Destination (Join-Path $directory $name)
}

if ($Action -eq 'upload') {
    $release = Get-Draft
    foreach ($file in Get-ChildItem -LiteralPath $env:RELEASE_DIRECTORY -File) {
        $existing = @($release.assets | Where-Object name -eq $file.Name)
        if ($existing.Count -gt 0) {
            $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($existing.Count -ne 1 -or $existing[0].digest -ne "sha256:$hash") { throw 'Existing release asset differs from signed bytes' }
            continue
        }
        gh release upload "v$env:RELEASE_VERSION" --repo $env:GITHUB_REPOSITORY $file.FullName
        if ($LASTEXITCODE -ne 0) { throw 'Draft asset upload failed' }
    }
}
