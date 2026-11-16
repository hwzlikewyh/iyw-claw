param([ValidateSet('Fresh', 'Legacy')][string]$Scenario)
$ErrorActionPreference = 'Stop'
$env:GH_FORCE_TTY = '0'
$env:CLICOLOR_FORCE = '0'
$env:TERM = 'dumb'
function ConvertFrom-GhJson([string[]]$Lines) {
    $pattern = [string][char]27 + [regex]::Escape('[') + '[0-?]*[ -/]*[@-~]'
    return [regex]::Replace(($Lines -join "`n"), $pattern, '').TrimStart([char]0xFEFF) | ConvertFrom-Json
}
if ($env:RUNNER_ENVIRONMENT -ne 'github-hosted') { throw 'Use a disposable GitHub hosted runner' }
$testRoot = Join-Path $env:RUNNER_TEMP ('iyw-direct-' + $Scenario)
if (Test-Path -LiteralPath $testRoot) { throw 'Direct verification root already exists' }
$dataRoot = Join-Path ([Environment]::GetFolderPath('UserProfile')) '.iyw-claw'
if ($Scenario -eq 'Fresh' -and (Test-Path -LiteralPath $dataRoot)) {
    throw 'Refusing to modify a pre-existing managed environment'
}
$tag = 'v' + $env:VERSION
$releaseJson = gh release view $tag --repo $env:GITHUB_REPOSITORY --json databaseId
if ($LASTEXITCODE -ne 0) { throw 'Draft release lookup failed' }
$releaseId = (ConvertFrom-GhJson $releaseJson).databaseId
$releaseJson = gh api ('repos/' + $env:GITHUB_REPOSITORY + '/releases/' + $releaseId)
if ($LASTEXITCODE -ne 0) { throw 'Draft assets lookup failed' }
$release = ConvertFrom-GhJson $releaseJson
if (-not $release.draft -or $release.target_commitish -ne $env:SOURCE_REF) { throw 'Draft release source mismatch' }
$inputRoot = Join-Path $testRoot 'inputs'
[IO.Directory]::CreateDirectory($inputRoot) | Out-Null
$payloadName = 'iyw-claw_' + $env:VERSION + '_x64-app-payload.zip'
$wizardName = 'iyw-claw_' + $env:VERSION + '_x64-bootstrapper.exe'
$nsisName = 'iyw-claw_' + $env:VERSION + '_x64-setup.exe'
gh release download $tag --repo $env:GITHUB_REPOSITORY --pattern $payloadName --pattern ($payloadName + '.sig') --pattern $wizardName --pattern ($wizardName + '.sig') --pattern $nsisName --dir $inputRoot
if ($LASTEXITCODE -ne 0) { throw 'Signed payload and installer download failed' }
$payload = Join-Path $inputRoot $payloadName
$wizard = Join-Path $inputRoot $wizardName
foreach ($name in @($payloadName, ($payloadName + '.sig'), $wizardName, ($wizardName + '.sig'), $nsisName)) {
    $asset = @($release.assets | Where-Object name -eq $name)
    $path = Join-Path $inputRoot $name
    if ($asset.Count -ne 1 -or -not (Test-Path -LiteralPath $path)) { throw ('Release asset missing: ' + $name) }
    if ((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $asset[0].digest.Substring(7)) { throw ('Asset digest mismatch: ' + $name) }
}
$archiveTool = (Get-Command 7z.exe -ErrorAction SilentlyContinue).Source
if (-not $archiveTool -and (Test-Path 'C:/Program Files/7-Zip/7z.exe')) { $archiveTool = 'C:/Program Files/7-Zip/7z.exe' }
if (-not $archiveTool) { throw 'NSIS archive inspection tool is unavailable' }
$nsisRoot = Join-Path $testRoot 'nsis-payload'
[IO.Directory]::CreateDirectory($nsisRoot) | Out-Null
& $archiveTool x (Join-Path $inputRoot $nsisName) ('-o' + $nsisRoot) iyw-claw.exe -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Could not inspect the signed NSIS package executable' }
$nsisApp = Get-ChildItem -LiteralPath $nsisRoot -Recurse -File -Filter iyw-claw.exe | Select-Object -First 1
if (-not $nsisApp) { throw 'Signed NSIS package does not contain the application' }
$nsisSignature = Get-AuthenticodeSignature -LiteralPath $nsisApp.FullName
if ($nsisSignature.Status -ne 'Valid' -or $nsisSignature.SignerCertificate.Thumbprint -ne $env:IYW_CLAW_SIGN_THUMBPRINT -or -not $nsisSignature.TimeStamperCertificate) {
    throw 'NSIS package main executable signature was invalidated by bundle finalization'
}
Write-Output 'PASS signed main executable inside legacy-compatible NSIS package'
$wizardSignature = Get-AuthenticodeSignature -LiteralPath $wizard
if ($wizardSignature.Status -ne 'Valid' -or $wizardSignature.SignerCertificate.Thumbprint -ne $env:IYW_CLAW_SIGN_THUMBPRINT -or -not $wizardSignature.TimeStamperCertificate) {
    throw 'Installer publisher signature or timestamp mismatch'
}
$archive = [IO.Compression.ZipFile]::OpenRead($payload)
try {
    $entry = $archive.GetEntry('payload-manifest.json')
    if (-not $entry -or $entry.Length -gt 32MB) { throw 'Payload manifest missing or oversized' }
    $reader = [IO.StreamReader]::new($entry.Open())
    try { $manifest = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
} finally { $archive.Dispose() }
if ($manifest.version -ne $env:VERSION -or $manifest.target -ne 'windows' -or $manifest.arch -ne 'x86_64') {
    throw 'Payload manifest does not match release version and target'
}
if ($manifest.sourceCommit -ne $env:SOURCE_REF) { throw 'Payload build source mismatch' }
if (@($manifest.files | Where-Object path -eq 'app/installer/原助理安装向导.exe').Count -ne 0) {
    throw 'Standalone installer must remain outside the app payload'
}
if ($Scenario -eq 'Fresh') {
    $installRoot = Join-Path $env:RUNNER_TEMP 'iyw-direct-fresh'
} else {
    $legacyTag = 'v0.1.333'
    $legacyJson = gh api ('repos/' + $env:GITHUB_REPOSITORY + '/releases/tags/' + $legacyTag)
    if ($LASTEXITCODE -ne 0) { throw 'Legacy NSIS lookup failed' }
    $legacyRelease = ConvertFrom-GhJson $legacyJson
    $legacyAsset = @($legacyRelease.assets | Where-Object name -match '_x64-setup[.]exe$')
    if ($legacyRelease.draft -or $legacyAsset.Count -ne 1) { throw 'Legacy NSIS artifact missing or ambiguous' }
    gh release download $legacyTag --repo $env:GITHUB_REPOSITORY --pattern $legacyAsset[0].name --dir $inputRoot
    if ($LASTEXITCODE -ne 0) { throw 'Legacy NSIS download failed' }
    $legacyPath = Join-Path $inputRoot $legacyAsset[0].name
    if ((Get-FileHash -LiteralPath $legacyPath).Hash.ToLowerInvariant() -ne $legacyAsset[0].digest.Substring(7)) { throw 'Legacy NSIS digest mismatch' }
    $installRoot = Join-Path $env:RUNNER_TEMP 'iyw-environment-live-install'
    $legacyProcess = Start-Process -FilePath $legacyPath -ArgumentList @('/S', ('/D=' + $installRoot)) -Wait -PassThru -WindowStyle Hidden
    if ($legacyProcess.ExitCode -ne 0) { throw ('Legacy NSIS setup failed: ' + $legacyProcess.ExitCode) }
    $sentinel = Join-Path $installRoot 'data/direct-update-sentinel.txt'
    [IO.Directory]::CreateDirectory((Split-Path $sentinel)) | Out-Null
    [IO.File]::WriteAllText($sentinel, 'preserve-existing-data')
}
$receipt = Join-Path $installRoot 'installer/direct-current.json'
$previous = if (Test-Path -LiteralPath $receipt) { (Get-Item -LiteralPath $receipt).LastWriteTimeUtc.Ticks } else { 0 }
$arguments = @(
    '--update-silent', '--update-install-dir', $installRoot, '--update-package-cache', $payload,
    '--update-version', $env:VERSION, '--update-url', 'https://gateway.iyw.cn/iyw-fusion-api/app-updates/artifacts/1/download',
    '--update-signature', ([IO.File]::ReadAllText(($payload + '.sig')).Trim()),
    '--update-size', [string](Get-Item -LiteralPath $payload).Length,
    '--update-sha256', (Get-FileHash -LiteralPath $payload).Hash.ToLowerInvariant(),
    '--update-package-kind', 'app_payload_zip', '--update-channel', 'stable', '--update-release-id', '1'
)
$start = [Diagnostics.ProcessStartInfo]::new($wizard)
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = 'Hidden'
foreach ($argument in $arguments) { $start.ArgumentList.Add($argument) }
$process = [Diagnostics.Process]::Start($start)
if (-not $process.WaitForExit(1800000)) {
    if (-not $process.HasExited) { $process.Kill($true) }
    throw 'Direct installer timed out'
}
$entryPoint = Join-Path $installRoot 'app/iyw-claw.exe'
if (-not (Test-Path -LiteralPath $entryPoint)) { throw ('Direct setup failed: ' + $process.ExitCode) }
if (-not (Test-Path -LiteralPath $receipt) -or (Get-Item -LiteralPath $receipt).LastWriteTimeUtc.Ticks -eq $previous) { throw 'Direct success receipt missing' }
$installed = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
$payloadDigest = (Get-FileHash -LiteralPath $payload).Hash.ToLowerInvariant()
if ($installed.version -ne $env:VERSION -or $installed.sha256 -ne $payloadDigest) { throw 'Direct success receipt does not match selected payload' }
if (Test-Path -LiteralPath (Join-Path $installRoot 'staging/direct-install.json')) { throw 'Direct install transaction remains uncommitted' }
$identity = & (Join-Path $installRoot 'app/iyw-environment.exe') --identity | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $identity.installerProtocol -ne 2) { throw 'Direct install did not activate the expected environment helper' }
Get-Process -Name iyw-claw -ErrorAction SilentlyContinue | Where-Object Path -eq $entryPoint | ForEach-Object {
    if (-not $_.CloseMainWindow() -or -not $_.WaitForExit(15000)) { $_.Kill(); $_.WaitForExit() }
}
if ($Scenario -eq 'Fresh') {
    & (Join-Path $installRoot 'app/iyw-environment.exe') diagnose --json
    if ($LASTEXITCODE -ne 0) { throw 'Fresh install environment diagnostic failed' }
    Write-Output 'PASS: downloaded app payload was directly deployed by the standalone installer'
} else {
    $sentinel = Join-Path $installRoot 'data/direct-update-sentinel.txt'
    if ([IO.File]::ReadAllText($sentinel) -ne 'preserve-existing-data') { throw 'Direct upgrade changed prior user data' }
    Write-Output 'PASS: legacy NSIS installation was upgraded by the standalone installer'
}
