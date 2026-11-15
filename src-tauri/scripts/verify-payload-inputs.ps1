param(
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$WebView,
    [Parameter(Mandatory=$true)][string]$Application,
    [Parameter(Mandatory=$true)][string]$Helper
)
$ErrorActionPreference='Stop'
$expected=($env:IYW_CLAW_SIGN_THUMBPRINT -replace '[\s:]','').ToUpperInvariant()
if($expected -notmatch '^[A-F0-9]{40}$'){throw 'Missing publisher certificate identity'}
foreach($file in @($Installer,$Application,$Helper)){
    $signature=Get-AuthenticodeSignature -LiteralPath $file
    if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $expected -or -not $signature.TimeStamperCertificate){
        throw "Payload input publisher or timestamp mismatch: $file"
    }
}
$signature=Get-AuthenticodeSignature -LiteralPath $WebView
if($signature.Status -ne 'Valid' -or $signature.SignerCertificate.GetNameInfo([Security.Cryptography.X509Certificates.X509NameType]::SimpleName,$false) -ne 'Microsoft Corporation'){
    throw 'Payload WebView input is not signed by Microsoft'
}
function Read-Identity([string]$File,[string]$Argument){
    $start=[Diagnostics.ProcessStartInfo]::new($File,$Argument)
    $start.UseShellExecute=$false
    $start.CreateNoWindow=$true
    $start.RedirectStandardOutput=$true
    $start.RedirectStandardError=$true
    $child=[Diagnostics.Process]::Start($start)
    try{
        if(-not $child.WaitForExit(15000)){$child.Kill();throw 'Payload input identity timed out'}
        if($child.ExitCode -ne 0){throw 'Payload input identity check failed'}
        return $child.StandardOutput.ReadToEnd() | ConvertFrom-Json
    }finally{$child.Dispose()}
}
$capabilities=Read-Identity $Installer '--capabilities'
if($capabilities.protocol -ne 2 -or $capabilities.payload -ne 'app_payload_zip' -or $capabilities.target -ne 'windows' -or $capabilities.arch -ne 'x86_64'){
    throw 'Installer does not implement Windows x64 direct deployment protocol 2'
}
$identity=Read-Identity $Helper '--identity'
if($identity.installerProtocol -ne 2 -or $identity.distributionTarget -ne 'windows' -or $identity.target -ne 'x86_64-pc-windows-msvc'){
    throw 'Environment helper does not implement direct installer recovery protocol'
}