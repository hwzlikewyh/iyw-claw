param([Parameter(Mandatory=$true)][string]$Directory)
$ErrorActionPreference='Stop'
$file=Join-Path $Directory 'YuanAssistant-Setup-windows.exe'
$manifest=Get-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt')
$entry=@($manifest | Where-Object { $_ -match '^[a-f0-9]{64}  YuanAssistant-Setup-windows\.exe$' })
if($entry.Count -ne 1){throw 'Common installer checksum is missing'}
if((Get-FileHash -LiteralPath $file).Hash -ne $entry[0].Substring(0,64)){throw 'Common installer checksum mismatch'}
$signature=Get-AuthenticodeSignature -LiteralPath $file
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $env:IYW_CLAW_SIGN_THUMBPRINT -or -not $signature.TimeStamperCertificate){throw 'Common installer signature or timestamp mismatch'}
$start=[Diagnostics.ProcessStartInfo]::new($file,'--capabilities')
$start.UseShellExecute=$false
$start.CreateNoWindow=$true
$start.RedirectStandardOutput=$true
$child=[Diagnostics.Process]::Start($start)
try {
    if(-not $child.WaitForExit(15000)){$child.Kill();throw 'Installer capability check timed out'}
    if($child.ExitCode -ne 0){throw 'Installer capability check failed'}
    $info=$child.StandardOutput.ReadToEnd() | ConvertFrom-Json
} finally {$child.Dispose()}
if($info.protocol -ne 2 -or $info.payload -ne 'app_payload_zip' -or $info.runtime_branding -ne 1){throw 'Installer must preserve runtime branding'}
"IYW_UNIFIED_INSTALLER=$file" >> $env:GITHUB_ENV
