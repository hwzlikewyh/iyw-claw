param(
  [Parameter(Mandatory = $true)][string]$FilePath,
  [Parameter(Mandatory = $true)][string]$Thumbprint,
  [Parameter(Mandatory = $true)][string]$Digest
)

$ErrorActionPreference = 'Stop'

try {
  if ([string]::IsNullOrWhiteSpace($env:IYW_CLAW_SAFENET_PIN)) {
    throw 'IYW_CLAW_SAFENET_PIN is required; interactive signing is disabled'
  }
  Add-Type -Path (Join-Path $PSScriptRoot 'windows-token-signer.cs')
  [IywWindowsTokenSigner]::Sign($FilePath, $Thumbprint, $Digest)
} catch {
  [Console]::Error.WriteLine('[signing-token][ERROR] ' + $_.Exception.GetBaseException().Message)
  exit 1
}
