param([switch]$FreshOnly)
$ErrorActionPreference='Stop'
if($env:RUNNER_ENVIRONMENT -ne 'github-hosted'){throw 'Direct installation verification requires a disposable hosted runner'}
$version=(Get-Content package.json -Raw|ConvertFrom-Json).version
$bundle=Join-Path $PWD 'src-tauri/target/x86_64-pc-windows-msvc/release/bundle/direct'
$wizard=Join-Path $bundle "iyw-claw_$($version)_x64-bootstrapper.exe"
$payload=Join-Path $bundle "iyw-claw_$($version)_x64-app-payload.zip"
$signature=[IO.File]::ReadAllText("$payload.sig").Trim()
$digest=(Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash.ToLowerInvariant()
$bytes=(Get-Item -LiteralPath $payload).Length
$environment=Join-Path ([Environment]::GetFolderPath('UserProfile')) '.iyw-claw'
if($FreshOnly -and (Test-Path -LiteralPath $environment)){throw 'Fresh verification refuses an existing user environment'}
$url='https://gateway.iyw.cn/iyw-fusion-api/app-updates/artifacts/1/download'

function Stop-OwnedApp([string]$Root){
    Get-Process -Name iyw-claw -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq (Join-Path $Root 'app/iyw-claw.exe')} | ForEach-Object {
        if(-not $_.CloseMainWindow() -or -not $_.WaitForExit(15000)){$_.Kill();$_.WaitForExit()}
    }
}
function Run-Direct([string]$Root){
    $receipt=Join-Path $Root 'installer/direct-current.json'
    $previous=if(Test-Path -LiteralPath $receipt){(Get-Item -LiteralPath $receipt).LastWriteTimeUtc.Ticks}else{0}
    $arguments=@('--update-silent','--update-install-dir',"`"$Root`"",'--update-package-cache',"`"$payload`"",
        '--update-version',$version,'--update-url',$url,'--update-signature',$signature,'--update-size',"$bytes",
        '--update-sha256',$digest,'--update-package-kind','app_payload_zip','--update-channel','stable','--update-release-id','1')
    $child=Start-Process -FilePath $wizard -ArgumentList $arguments -WindowStyle Hidden -PassThru
    if(-not $child.WaitForExit(900000)){throw 'Direct wizard timed out; diagnostic runner will be retained'}
    $entry=Join-Path $Root 'app/iyw-claw.exe'
    if(-not(Test-Path -LiteralPath $entry)){throw 'Direct wizard did not install application'}
    $identity=& (Join-Path $Root 'app/iyw-environment.exe') --identity | ConvertFrom-Json
    if($identity.installerProtocol -ne 2){throw 'Direct installation selected a legacy helper'}
    if(Test-Path -LiteralPath (Join-Path $Root 'staging/direct-install.json')){throw 'Direct deployment journal was not committed'}
    if(-not(Test-Path -LiteralPath $receipt) -or (Get-Item -LiteralPath $receipt).LastWriteTimeUtc.Ticks -eq $previous){throw 'Direct wizard did not publish a fresh success receipt'}
    $installed=Get-Content -LiteralPath $receipt -Raw|ConvertFrom-Json
    if($installed.version -ne $version -or $installed.sha256 -ne $digest){throw 'Direct receipt does not match the signed payload'}
    $snapshot=Get-Content (Join-Path $environment 'inventory/environment-current.json') -Raw | ConvertFrom-Json
    if($snapshot.pcVersion -ne $version){throw 'Direct deployment environment version mismatch'}
    $uninstall=Get-ItemProperty 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/原助理'
    if($uninstall.InstallLocation -ne $Root -or $uninstall.UninstallString -notlike '*--uninstall-root*'){
        throw 'Direct wizard did not take over uninstall registration'
    }
    Stop-OwnedApp $Root
}

$fresh=Join-Path $env:RUNNER_TEMP 'iyw-direct-fresh'
if(Test-Path -LiteralPath $fresh){throw 'Fresh direct root already exists'}
Run-Direct $fresh
Write-Output 'PASS signed direct fresh application deployment with existing healthy environment'
if($FreshOnly){
    & (Join-Path $fresh 'app/iyw-environment.exe') diagnose --json
    if($LASTEXITCODE -ne 0){throw 'Fresh direct environment integrity verification failed'}
    Write-Output 'PASS clean direct installation including managed environment bootstrap'
    exit 0
}
$legacy=Join-Path $env:RUNNER_TEMP 'iyw-environment-live-install'
if(-not(Test-Path -LiteralPath (Join-Path $legacy 'app/iyw-claw.exe'))){throw 'Missing legacy installation verification input'}
$marker=Join-Path $legacy 'direct-user-data-sentinel.txt'
[IO.File]::WriteAllText($marker,'preserve-user-data')
Run-Direct $legacy
if([IO.File]::ReadAllText($marker) -ne 'preserve-user-data'){throw 'Migration modified user data'}
Write-Output 'PASS legacy NSIS root migration, current-version reuse and user data preservation'
Run-Direct $legacy
Write-Output 'PASS repeated direct update'
$inventory=Get-Content (Join-Path $environment 'inventory/environment-current.json') -Raw | ConvertFrom-Json
$uv=@($inventory.components|Where-Object componentId -eq 'uv')
if($uv.Count -ne 1){throw 'Expected one managed uv component'}
$entry=Join-Path (Join-Path $environment $uv[0].relativePath) $uv[0].entrypoints.uv
$resolved=[IO.Path]::GetFullPath($entry)
if(-not $resolved.StartsWith([IO.Path]::GetFullPath((Join-Path $environment 'runtime/uv/')),[StringComparison]::OrdinalIgnoreCase)){throw 'uv entry escaped the isolated managed component'}
Move-Item -LiteralPath $resolved -Destination "$resolved.verification-missing"
Run-Direct $legacy
if(-not(Test-Path -LiteralPath $resolved)){throw 'Missing managed component was not repaired'}
Write-Output 'PASS missing managed uv entry repaired from verified cache'
