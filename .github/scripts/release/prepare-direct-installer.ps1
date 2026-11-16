param([Parameter(Mandatory=$true)][string]$Tag)
$ErrorActionPreference='Stop'
if($Tag -notmatch '^v\d+\.\d+\.\d+$'){throw 'Invalid installer release tag'}
$env:GH_FORCE_TTY='0';$env:CLICOLOR_FORCE='0';$env:TERM='dumb'
$releaseJson=gh release view $Tag --repo hwzlikewyh/iyw-claw-installer --json isDraft,isPrerelease,assets
if($LASTEXITCODE -ne 0){throw 'Published direct installer lookup failed'}
$ansiPattern=[string][char]27+[regex]::Escape('[')+'[0-?]*[ -/]*[@-~]'
$plain=[regex]::Replace(($releaseJson -join "`n"),$ansiPattern,'')
$release=$plain | ConvertFrom-Json
if($release.isDraft -or $release.isPrerelease){throw 'A published stable direct installer release is required'}
$asset=@($release.assets | Where-Object name -eq 'YuanAssistant-Setup-windows.exe')
if($asset.Count -ne 1 -or $asset[0].digest -notmatch '^sha256:[a-f0-9]{64}$'){throw 'Installer artifact digest is missing'}
$directory=Join-Path $env:RUNNER_TEMP "direct-installer-$env:GITHUB_RUN_ID-$env:GITHUB_RUN_ATTEMPT"
New-Item -ItemType Directory -Path $directory -Force | Out-Null
gh release download $Tag --repo hwzlikewyh/iyw-claw-installer --pattern YuanAssistant-Setup-windows.exe --dir $directory
if($LASTEXITCODE -ne 0){throw 'Installer download failed'}
$file=Join-Path $directory 'YuanAssistant-Setup-windows.exe'
if((Get-FileHash -LiteralPath $file).Hash -ne $asset[0].digest.Substring(7)){throw 'Installer digest mismatch'}
$sig=Get-AuthenticodeSignature -LiteralPath $file
if($sig.Status -ne 'Valid' -or $sig.SignerCertificate.Thumbprint -ne $env:IYW_CLAW_SIGN_THUMBPRINT -or -not $sig.TimeStamperCertificate){throw 'Installer publisher or timestamp mismatch'}
$start=[Diagnostics.ProcessStartInfo]::new($file,'--capabilities')
$start.UseShellExecute=$false
$start.CreateNoWindow=$true
$start.RedirectStandardOutput=$true
$start.RedirectStandardError=$true
$child=[Diagnostics.Process]::Start($start)
try{
    if(-not $child.WaitForExit(15000)){$child.Kill();throw 'Installer capabilities timed out'}
    if($child.ExitCode -ne 0){throw 'Installer capabilities check failed'}
    $info=$child.StandardOutput.ReadToEnd() | ConvertFrom-Json
}finally{$child.Dispose()}
if($info.protocol -ne 2 -or $info.payload -ne 'app_payload_zip' -or $info.target -ne 'windows' -or $info.arch -ne 'x86_64'){throw 'Installer lacks direct deployment support'}
"IYW_UNIFIED_INSTALLER=$file" >> $env:GITHUB_ENV
